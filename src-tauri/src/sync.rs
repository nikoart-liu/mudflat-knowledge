//! 同步引擎：增量拉取笔记本/划线/想法并 reconcile 进本地库。
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

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub books_total: usize,
    pub books_synced: usize,
    pub highlights: usize,
    pub thoughts: usize,
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

/// 首次全量或数量有变化时返回 true（存量以本地数为基准）。
fn needs_pull(conn: &rusqlite::Connection, weread_id: &str, remote_note: i64, remote_review: i64) -> bool {
    let row_id = match db::find_book_row(conn, weread_id) {
        Ok(Some(id)) => id,
        Ok(None) => return true,
        Err(_) => return true,
    };
    // 首次全量：从未成功同步过内容（synced_at 为空）
    let synced_at: Option<i64> = conn
        .query_row("SELECT synced_at FROM books WHERE id=?1", [row_id], |r| r.get(0))
        .unwrap_or(None);
    if synced_at.is_none() {
        return true;
    }
    let (note, review): (i64, i64) = conn
        .query_row(
            "SELECT note_count, review_count FROM books WHERE id=?1",
            [row_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap_or((remote_note + 1, remote_review + 1)); // 查询失败按需拉取
    note != remote_note || review != remote_review
}

/// 章节标题映射：chapterUid -> title。
fn chapter_map(chapters: &[gateway::ChapterInfo]) -> std::collections::HashMap<i64, String> {
    chapters.iter().map(|c| (c.chapter_uid, c.title.clone())).collect()
}

pub async fn run_sync(
    state: &Mutex<rusqlite::Connection>,
    api_key: &str,
    on_progress: Channel<SyncEvent>,
) -> Result<SyncSummary, String> {
    let http = gateway::client().map_err(|e| e.to_string())?;

    // 1. 分页拉全部 notebooks，upsert 全部书籍
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
                    reading_progress: 0,
                    note_count: nb.note_count,
                    review_count: nb.review_count,
                },
            )
            .map_err(|e| e.to_string())?;
        }
    }
    emit(&on_progress, "books", notebooks.len() as i64, notebooks.len() as i64, "");

    let now = chrono_now();
    let mut summary = SyncSummary { books_total: notebooks.len(), ..Default::default() };

    // 2. 逐本书：sync_reviews=1 且数量有变化才拉内容
    let total = notebooks.len() as i64;
    for (idx, nb) in notebooks.iter().enumerate() {
        let should = {
            let conn = state.lock().map_err(|e| e.to_string())?;
            let sync_on: bool = db::find_book_row(&conn, &nb.book.book_id)
                .map(|id| {
                    conn.query_row("SELECT sync_reviews FROM books WHERE id=?1", [id], |r| r.get::<_, i64>(0))
                        .unwrap_or(1)
                        != 0
                })
                .unwrap_or(true);
            sync_on && needs_pull(&conn, &nb.book.book_id, nb.note_count, nb.review_count)
        };
        if !should {
            continue;
        }

        emit(&on_progress, "pulling", idx as i64 + 1, total, &nb.book.title);

        let bookmarks = gateway::fetch_bookmarks(&http, api_key, &nb.book.book_id)
            .await
            .map_err(|e| e.to_string())?;
        let reviews = gateway::fetch_reviews_all(&http, api_key, &nb.book.book_id)
            .await
            .map_err(|e| e.to_string())?;

        let (mut hi_n, mut th_n) = (0usize, 0usize);
        {
            let mut conn = state.lock().map_err(|e| e.to_string())?;
            let tx = conn.transaction().map_err(|e| e.to_string())?;
            let book_row_id = db::find_book_row(&tx, &nb.book.book_id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("书籍 {} 未入库", nb.book.title))?;
            let cmap = chapter_map(&bookmarks.chapters);

            let mut present_ids: Vec<String> = Vec::new();
            for bm in &bookmarks.updated {
                db::upsert_card(
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
                present_ids.push(bm.bookmark_id.clone());
                hi_n += 1;
            }

            for wrap in &reviews {
                let rv = &wrap.review;
                if rv.review_id.is_empty() {
                    continue;
                }
                // 想法通过 abstract+range 与该书某条 highlight 的 (chapter_uid, range) 共享值关联，不加外键
                db::upsert_card(
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
                present_ids.push(rv.review_id.clone());
                th_n += 1;
            }

            // 4. reconcile：本次结果中消失的远程卡软删
            let removed = db::reconcile_cards(&tx, book_row_id, &present_ids).map_err(|e| e.to_string())?;
            summary.removed += removed;

            db::touch_book_synced(&tx, book_row_id, now).map_err(|e| e.to_string())?;
            tx.commit().map_err(|e| e.to_string())?;
        }
        summary.books_synced += 1;
        summary.highlights += hi_n;
        summary.thoughts += th_n;
    }

    // 6. 写 sync_meta
    {
        let conn = state.lock().map_err(|e| e.to_string())?;
        db::set_sync_meta(&conn, "last_full_sync", &now.to_string())
            .map_err(|e| e.to_string())?;
    }

    emit(&on_progress, "done", total, total, "");
    Ok(summary)
}

/// unix 秒。避免直接依赖 chrono：用 SystemTime。
fn chrono_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
