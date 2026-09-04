//! 同步引擎：增量拉取笔记本/划线/想法并 reconcile 进本地库。
//!
//! v0.2 增量语义（PRD R0）：
//! - 每书持久化「上次成功同步」的划线/想法计数基线，与远端最新计数分开存；
//! - 用本次远端计数与基线比较决定是否拉取，绝不先覆盖再判断；
//! - 单书内容事务成功后才更新该书基线；失败书不更新、不阻断其他书，下次重试。
//!
//! 全程经 Channel<SyncEvent> 向前端发进度；每本书一个事务；
//! 请求串行且已由 gateway 层 300ms 节流。

use serde::Serialize;
use std::sync::Mutex;
use tauri::ipc::Channel;

use crate::db::{self, UpsertCard};
use crate::gateway::{self, NotebookBook};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncEvent {
    pub stage: String,
    pub current: i64,
    pub total: i64,
    pub book_title: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FailedBook {
    pub book_id: String,
    pub title: String,
    pub error: String,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub books_total: usize,
    pub books_synced: usize,
    pub books_failed: usize,
    pub failures: Vec<FailedBook>,
    /// 本次拉取的划线总数（含更新）
    pub highlights: usize,
    /// 本次拉取的想法总数（含更新）
    pub thoughts: usize,
    /// 新插入的卡片数
    pub added: usize,
    /// 远端消失而被 reconcile 软删的卡片数
    pub removed: usize,
}

fn emit(chan: &Channel<SyncEvent>, stage: &str, current: i64, total: i64, title: &str) {
    let _ = chan.send(SyncEvent {
        stage: stage.into(),
        current,
        total,
        book_title: title.into(),
    });
}

/// 用「上次成功同步基线」与本次远端计数比较，决定是否拉取（R0）。
/// 基线缺失（首同步、v0.1 旧库升级后）一律拉取，成功后写入基线。
fn needs_pull(
    conn: &rusqlite::Connection,
    book_row_id: i64,
    remote_note: i64,
    remote_review: i64,
) -> rusqlite::Result<bool> {
    match db::book_sync_baseline(conn, book_row_id) {
        Ok((Some(n), Some(rv))) => Ok(n != remote_note || rv != remote_review),
        _ => Ok(true),
    }
}

/// 章节标题映射：chapterUid -> title。
fn chapter_map(chapters: &[gateway::ChapterInfo]) -> std::collections::HashMap<i64, String> {
    chapters.iter().map(|c| (c.chapter_uid, c.title.clone())).collect()
}

/// 一次性修复（KEY_CREATED_AT_REPAIR）：旧版入库把 created_at 钳成了同步时刻，
/// 而远端 createTime 才是真实划线时间。修复标记缺失时返回 true，
/// 由调用方清空全部基线强制本次全量重拉，让 upsert_card 用远端值自愈。
fn needs_created_at_repair(conn: &rusqlite::Connection) -> rusqlite::Result<bool> {
    Ok(db::get_sync_meta(conn, db::KEY_CREATED_AT_REPAIR)?.is_none())
}

pub async fn run_sync(
    state: &Mutex<rusqlite::Connection>,
    api_key: &str,
    on_progress: Channel<SyncEvent>,
) -> Result<SyncSummary, String> {
    let http = gateway::client().map_err(|e| e.to_string())?;

    // 1. 分页拉全部 notebooks，upsert 全部书籍（仅展示元数据，不影响增量判断）
    let notebooks: Vec<NotebookBook> =
        gateway::fetch_notebooks(&http, api_key, 50).await.map_err(|e| e.to_string())?;
    {
        let conn = state.lock().map_err(|e| e.to_string())?;
        for nb in &notebooks {
            db::upsert_book(
                &conn,
                &db::NewBook {
                    weread_book_id: nb.book.book_id.clone(),
                    title: nb.book.title.clone(),
                    author: nb.book.author.clone(),
                    cover: nb.book.cover.clone(),
                    reading_progress: nb.reading_progress,
                    note_count: nb.note_count,
                    review_count: nb.review_count,
                    // 远端「最近笔记时间」；0 视为未知，不覆盖旧值
                    wr_sort: (nb.sort > 0).then_some(nb.sort),
                    category: nb.first_category().to_string(),
                },
            )
            .map_err(|e| e.to_string())?;
        }
    }
    emit(&on_progress, "books", notebooks.len() as i64, notebooks.len() as i64, "");

    // 1.5 一次性修复：created_at 曾被旧版钳成同步时刻。清空基线强制本次全量重拉，
    // 用远端 createTime 修回真实划线时间；跑完才写修复标记，失败书靠基线缺失下次重试。
    {
        let repair_needed = {
            let conn = state.lock().map_err(|e| e.to_string())?;
            needs_created_at_repair(&conn).map_err(|e| e.to_string())?
        };
        if repair_needed {
            let conn = state.lock().map_err(|e| e.to_string())?;
            let cleared = db::clear_book_sync_baselines(&conn).map_err(|e| e.to_string())?;
            if cleared > 0 {
                emit(&on_progress, "repair", 0, cleared as i64, "");
            }
        }
    }

    let now = chrono_now();
    let mut summary = SyncSummary { books_total: notebooks.len(), ..Default::default() };

    // 2. 逐本书：sync_reviews=1 且基线有变化才拉内容；单书失败不阻断其他书
    let total = notebooks.len() as i64;
    for (idx, nb) in notebooks.iter().enumerate() {
        let row_id = {
            let conn = state.lock().map_err(|e| e.to_string())?;
            match db::find_book_row(&conn, &nb.book.book_id) {
                Ok(Some(id)) => {
                    let sync_on: bool = conn
                        .query_row("SELECT sync_reviews FROM books WHERE id=?1", [id], |r| {
                            r.get::<_, i64>(0)
                        })
                        .map(|v| v != 0)
                        .unwrap_or(true);
                    if !sync_on {
                        None
                    } else {
                        Some(id)
                    }
                }
                _ => None,
            }
        };
        let Some(row_id) = row_id else { continue };

        let should = {
            let conn = state.lock().map_err(|e| e.to_string())?;
            needs_pull(&conn, row_id, nb.note_count, nb.review_count).map_err(|e| e.to_string())?
        };
        if !should {
            continue;
        }

        emit(&on_progress, "pulling", idx as i64 + 1, total, &nb.book.title);

        // 单书全程容错：任何一步失败都记入失败清单并继续下一本（R0.6）
        let outcome =
            sync_one_book(state, &http, api_key, nb, row_id, now).await;
        match outcome {
            Ok((hi_n, th_n, added, removed)) => {
                summary.books_synced += 1;
                summary.highlights += hi_n;
                summary.thoughts += th_n;
                summary.added += added;
                summary.removed += removed;
            }
            Err(err) => {
                summary.books_failed += 1;
                summary.failures.push(FailedBook {
                    book_id: nb.book.book_id.clone(),
                    title: nb.book.title.clone(),
                    error: err,
                });
                emit(&on_progress, "book_failed", idx as i64 + 1, total, &nb.book.title);
            }
        }
    }

    // 3. 写 sync_meta：本轮已跑完（失败书基线缺失，下次自动重试），
    //    落 created_at 修复标记，避免每次同步都全量重拉。
    {
        let conn = state.lock().map_err(|e| e.to_string())?;
        db::set_sync_meta(&conn, "last_full_sync", &now.to_string())
            .map_err(|e| e.to_string())?;
        db::set_sync_meta(&conn, db::KEY_CREATED_AT_REPAIR, &now.to_string())
            .map_err(|e| e.to_string())?;
    }

    emit(&on_progress, "done", total, total, "");
    Ok(summary)
}

/// 同步单本书：拉划线/想法 → 事务入库 + reconcile → 成功后写基线。
/// 返回 (划线数, 想法数, 新增数, 移除数)。任何失败返回 Err，且不更新基线。
async fn sync_one_book(
    state: &Mutex<rusqlite::Connection>,
    http: &reqwest::Client,
    api_key: &str,
    nb: &NotebookBook,
    row_id: i64,
    now: i64,
) -> Result<(usize, usize, usize, usize), String> {
    let bookmarks = gateway::fetch_bookmarks(http, api_key, &nb.book.book_id)
        .await
        .map_err(|e| e.to_string())?;
    let reviews = gateway::fetch_reviews_all(http, api_key, &nb.book.book_id)
        .await
        .map_err(|e| e.to_string())?;

    let (mut hi_n, mut th_n, mut added) = (0usize, 0usize, 0usize);
    {
        let mut conn = state.lock().map_err(|e| e.to_string())?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let book_row_id = row_id;
        let cmap = chapter_map(&bookmarks.chapters);

        let mut present_ids: Vec<String> = Vec::new();
        for bm in &bookmarks.updated {
            let (_, inserted) = db::upsert_card(
                &tx,
                &UpsertCard {
                    kind: "highlight",
                    book_row_id,
                    remote_id: &bm.bookmark_id,
                    chapter_uid: Some(bm.chapter_uid),
                    chapter_title: cmap.get(&bm.chapter_uid).map(|s| s.as_str()),
                    text: &bm.mark_text,
                    abstract_text: None,
                    range_str: Some(&bm.range),
                    color_style: bm.color_style,
                    created_at: bm.create_time.max(1),
                },
                now,
            )
            .map_err(|e| e.to_string())?;
            if inserted {
                added += 1;
            }
            present_ids.push(bm.bookmark_id.clone());
            hi_n += 1;
        }

        for wrap in &reviews {
            let rv = &wrap.review;
            if rv.review_id.is_empty() {
                continue;
            }
            // 想法通过 abstract+range 与该书某条 highlight 的 (chapter_uid, range) 共享值关联，不加外键
            let (_, inserted) = db::upsert_card(
                &tx,
                &UpsertCard {
                    kind: "thought",
                    book_row_id,
                    remote_id: &rv.review_id,
                    chapter_uid: Some(rv.chapter_uid).filter(|v| *v != 0),
                    chapter_title: if rv.chapter_name.is_empty() { None } else { Some(&rv.chapter_name) },
                    text: &rv.content,
                    abstract_text: if rv.abstract_text.is_empty() { None } else { Some(&rv.abstract_text) },
                    range_str: if rv.range.is_empty() { None } else { Some(&rv.range) },
                    color_style: 0,
                    created_at: rv.create_time.max(1),
                },
                now,
            )
            .map_err(|e| e.to_string())?;
            if inserted {
                added += 1;
            }
            present_ids.push(rv.review_id.clone());
            th_n += 1;
        }

        // reconcile：本次结果中消失的远程卡按远端删除软删
        let removed = db::reconcile_cards(&tx, book_row_id, &present_ids).map_err(|e| e.to_string())?;

        // 事务成功路径的最后一步：写成功基线 + synced_at，与内容同事务原子生效
        db::set_book_sync_baseline(&tx, book_row_id, nb.note_count, nb.review_count)
            .map_err(|e| e.to_string())?;
        db::touch_book_synced(&tx, book_row_id, now).map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok((hi_n, th_n, added, removed))
    }
}

/// unix 秒。避免直接依赖 chrono：用 SystemTime。
fn chrono_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{self, NewBook};

    fn mem() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        db::apply_schema(&conn, db::SchemaPlan::Fresh).unwrap();
        conn
    }

    fn book(conn: &rusqlite::Connection, wid: &str) -> i64 {
        db::upsert_book(
            conn,
            &NewBook {
                weread_book_id: wid.into(),
                title: wid.into(),
                author: String::new(),
                cover: String::new(),
                reading_progress: 0,
                note_count: 3,
                review_count: 1,
                wr_sort: None,
                category: String::new(),
            },
        )
        .unwrap();
        db::find_book_row(conn, wid).unwrap().unwrap()
    }

    #[test]
    fn no_baseline_means_pull() {
        let conn = mem();
        let id = book(&conn, "b1");
        assert!(needs_pull(&conn, id, 3, 1).unwrap(), "基线缺失（首同步/旧库升级）必须拉取");
    }

    #[test]
    fn unchanged_baseline_skips_pull() {
        let conn = mem();
        let id = book(&conn, "b1");
        db::set_book_sync_baseline(&conn, id, 3, 1).unwrap();
        assert!(!needs_pull(&conn, id, 3, 1).unwrap(), "远端计数与基线一致时跳过");
    }

    #[test]
    fn changed_remote_count_pulls() {
        let conn = mem();
        let id = book(&conn, "b1");
        db::set_book_sync_baseline(&conn, id, 3, 1).unwrap();
        for (n, rv) in [(4, 1), (3, 2), (2, 0)] {
            assert!(needs_pull(&conn, id, n, rv).unwrap(), "任一计数变化都要拉取 ({n},{rv})");
        }
    }

    #[test]
    fn created_at_repair_clears_baselines_once() {
        let conn = mem();
        let id = book(&conn, "b1");
        db::set_book_sync_baseline(&conn, id, 3, 1).unwrap();
        assert!(!needs_pull(&conn, id, 3, 1).unwrap(), "前置：基线一致时本应跳过");

        // 修复标记缺失：判定需要修复，清基线后即使计数一致也必须全量重拉
        assert!(needs_created_at_repair(&conn).unwrap(), "无修复标记时需要全量重拉");
        assert_eq!(db::clear_book_sync_baselines(&conn).unwrap(), 1);
        assert!(needs_pull(&conn, id, 3, 1).unwrap(), "基线被清空后必须拉取");

        // 跑完写入标记后：不再触发修复，基线恢复后回到正常增量
        db::set_sync_meta(&conn, db::KEY_CREATED_AT_REPAIR, "1").unwrap();
        assert!(!needs_created_at_repair(&conn).unwrap(), "修复只做一次");
        db::set_book_sync_baseline(&conn, id, 3, 1).unwrap();
        assert!(!needs_pull(&conn, id, 3, 1).unwrap());
    }

    #[test]
    fn baseline_survives_remote_count_overwrite() {
        // R0 核心回归：upsert_book 会用远端最新计数覆盖 note_count/review_count，
        // 但 needs_pull 只看基线列，不能再出现「先覆盖再比较 → 恒不拉取」。
        let conn = mem();
        let id = book(&conn, "b1");
        db::set_book_sync_baseline(&conn, id, 3, 1).unwrap();
        db::upsert_book(
            &conn,
            &NewBook {
                weread_book_id: "b1".into(),
                title: "b1".into(),
                author: String::new(),
                cover: String::new(),
                reading_progress: 0,
                note_count: 9, // 远端涨了
                review_count: 1,
                wr_sort: None,
                category: String::new(),
            },
        )
        .unwrap();
        assert!(needs_pull(&conn, id, 9, 1).unwrap(), "远端新增后必须拉取，即使展示列已被覆盖");
    }
}
