use std::collections::HashSet;
use std::path::Path;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

pub type DbResult<T> = Result<T, rusqlite::Error>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookRow {
    pub id: i64,
    pub weread_book_id: String,
    pub title: String,
    pub author: String,
    pub cover: String,
    pub reading_progress: i64,
    pub note_count: i64,
    pub review_count: i64,
    #[serde(default = "default_true")]
    pub sync_reviews: bool,
    pub synced_at: Option<i64>,
}

#[allow(dead_code)] // serde(default) 引用，前端反序列化兜底用
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CardRow {
    pub id: i64,
    pub kind: String,
    pub book_id: Option<i64>,
    pub remote_id: Option<String>,
    pub chapter_uid: Option<i64>,
    pub chapter_title: Option<String>,
    pub text: String,
    pub abstract_text: Option<String>,
    pub range_str: Option<String>,
    pub color_style: i64,
    pub note: String,
    pub starred: bool,
    pub excluded_from_review: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted: bool,
    /// JOIN 出的所属书名（self 卡为空字符串）
    pub book_title: String,
    /// 关联标签
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CardFilter {
    pub book_id: Option<i64>,
    #[serde(default)]
    pub tag_ids: Vec<i64>,
    #[serde(default)]
    pub starred_only: bool,
    #[serde(default)]
    pub kinds: Vec<String>,
}

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS books (
  id INTEGER PRIMARY KEY,
  weread_book_id TEXT NOT NULL UNIQUE,
  title TEXT NOT NULL,
  author TEXT DEFAULT '',
  cover TEXT DEFAULT '',
  reading_progress INTEGER DEFAULT 0,
  note_count INTEGER DEFAULT 0,
  review_count INTEGER DEFAULT 0,
  sync_reviews INTEGER DEFAULT 1,
  synced_at INTEGER
);
CREATE TABLE IF NOT EXISTS cards (
  id INTEGER PRIMARY KEY,
  kind TEXT NOT NULL CHECK (kind IN ('highlight','thought','self')),
  book_id INTEGER REFERENCES books(id),
  remote_id TEXT UNIQUE,
  chapter_uid INTEGER,
  chapter_title TEXT,
  text TEXT NOT NULL,
  abstract_text TEXT,
  range_str TEXT,
  color_style INTEGER DEFAULT 0,
  note TEXT DEFAULT '',
  starred INTEGER DEFAULT 0,
  excluded_from_review INTEGER DEFAULT 0,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  deleted INTEGER DEFAULT 0
);
CREATE TABLE IF NOT EXISTS tags (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE COLLATE NOCASE);
CREATE TABLE IF NOT EXISTS card_tags (
  card_id INTEGER REFERENCES cards(id) ON DELETE CASCADE,
  tag_id INTEGER REFERENCES tags(id) ON DELETE CASCADE,
  UNIQUE(card_id, tag_id)
);
CREATE TABLE IF NOT EXISTS review_state (
  card_id INTEGER PRIMARY KEY REFERENCES cards(id) ON DELETE CASCADE,
  due_at INTEGER NOT NULL,
  interval_days REAL NOT NULL DEFAULT 0,
  ease REAL NOT NULL DEFAULT 2.5,
  reps INTEGER DEFAULT 0,
  lapses INTEGER DEFAULT 0
);
CREATE TABLE IF NOT EXISTS sync_meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
CREATE VIRTUAL TABLE IF NOT EXISTS cards_fts USING fts5(
  text, note, chapter_title, content='cards', content_rowid='id',
  tokenize='trigram'
);
CREATE TRIGGER IF NOT EXISTS cards_ai AFTER INSERT ON cards BEGIN
  INSERT INTO cards_fts(rowid, text, note, chapter_title) VALUES (new.id, new.text, new.note, COALESCE(new.chapter_title,''));
END;
CREATE TRIGGER IF NOT EXISTS cards_ad AFTER DELETE ON cards BEGIN
  INSERT INTO cards_fts(cards_fts, rowid, text, note, chapter_title) VALUES ('delete', old.id, old.text, old.note, COALESCE(old.chapter_title,''));
END;
CREATE TRIGGER IF NOT EXISTS cards_au AFTER UPDATE OF text, note ON cards BEGIN
  INSERT INTO cards_fts(cards_fts, rowid, text, note, chapter_title) VALUES ('delete', old.id, old.text, old.note, COALESCE(old.chapter_title,''));
  INSERT INTO cards_fts(rowid, text, note, chapter_title) VALUES (new.id, new.text, new.note, COALESCE(new.chapter_title,''));
END;
CREATE INDEX IF NOT EXISTS idx_cards_book ON cards(book_id);
CREATE INDEX IF NOT EXISTS idx_cards_remote ON cards(remote_id);
CREATE INDEX IF NOT EXISTS idx_review_due ON review_state(due_at);
"#;

/// 当前 schema 版本。SCHEMA_SQL 永远保持 v0.1 形态；
/// 之后所有 schema 变更只允许以幂等迁移追加（PRD 10.3）。
pub const LATEST_VERSION: i64 = 2;

/// v0 → v1（v0.2）：每书同步基线列 + 用户隐藏墓碑。
/// - 基线列与远端最新计数分开存，供增量同步判断（R0）；
/// - hidden_by_user 是「用户本地隐藏」墓碑，与远端 reconcile 删除分开（R4）；
/// - 存量软删同步卡无法区分用户删除/远端删除，默认按用户隐藏，避免内容复活。
pub const MIGRATION_V1_SQL: &str = "
ALTER TABLE books ADD COLUMN synced_note_count INTEGER;
ALTER TABLE books ADD COLUMN synced_review_count INTEGER;
ALTER TABLE cards ADD COLUMN hidden_by_user INTEGER NOT NULL DEFAULT 0;
UPDATE cards SET hidden_by_user=1 WHERE deleted=1;
";

/// v1 → v2：AI 派生数据与本地向量。原文仍在 cards；问题面/支架/向量独立生命周期。
pub const MIGRATION_V2_SQL: &str = "
CREATE TABLE IF NOT EXISTS card_embeddings (
  card_id INTEGER PRIMARY KEY REFERENCES cards(id) ON DELETE CASCADE,
  provider TEXT NOT NULL,
  model TEXT NOT NULL,
  dimensions INTEGER NOT NULL,
  content_hash TEXT NOT NULL,
  vector BLOB NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS ai_artifacts (
  id INTEGER PRIMARY KEY,
  artifact_type TEXT NOT NULL CHECK (artifact_type IN ('question_face','review_scaffold','topic_brief')),
  primary_card_id INTEGER NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
  source_card_ids TEXT NOT NULL,
  input_hash TEXT NOT NULL,
  provider TEXT NOT NULL,
  model TEXT NOT NULL,
  prompt_version TEXT NOT NULL,
  content_json TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('proposed','accepted','rejected','stale')),
  user_edited INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ai_artifacts_card ON ai_artifacts(primary_card_id, artifact_type, status);
CREATE INDEX IF NOT EXISTS idx_card_embeddings_hash ON card_embeddings(content_hash);
";

/// 迁移清单：(目标版本, SQL)。按序执行，每个迁移在独立事务中恰好跑一次。
const MIGRATIONS: &[(i64, &str)] = &[(1, MIGRATION_V1_SQL), (2, MIGRATION_V2_SQL)];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaPlan {
    /// 全新空库
    Fresh,
    /// 已是最新
    Current,
    /// 旧库需要从该版本升级
    UpgradeFrom(i64),
}

pub fn user_version(conn: &Connection) -> DbResult<i64> {
    conn.query_row("PRAGMA user_version", [], |r| r.get(0))
}

fn set_user_version(conn: &Connection, v: i64) -> DbResult<()> {
    conn.execute_batch(&format!("PRAGMA user_version={v};"))?;
    Ok(())
}

fn table_exists(conn: &Connection, name: &str) -> DbResult<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
        [name],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// 判断该连接需要哪种 schema 处理。库版本高于应用时直接报错，绝不打开覆盖。
pub fn plan_schema(conn: &Connection) -> DbResult<SchemaPlan> {
    let version = user_version(conn)?;
    if version > LATEST_VERSION {
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "数据库版本 v{version} 高于当前应用支持的 v{LATEST_VERSION}，请先升级应用；原库未被改动"
        )));
    }
    if !table_exists(conn, "books")? {
        return Ok(SchemaPlan::Fresh);
    }
    if version < LATEST_VERSION {
        return Ok(SchemaPlan::UpgradeFrom(version));
    }
    Ok(SchemaPlan::Current)
}

/// 执行 schema 建立/迁移。幂等：Fresh 全量建表后跑全部迁移；UpgradeFrom 只跑缺的迁移。
/// 每个迁移在独立事务中执行，失败即整体回滚，绝不留下半套列。
pub fn apply_schema(conn: &Connection, plan: SchemaPlan) -> DbResult<()> {
    match plan {
        SchemaPlan::Current => return Ok(()),
        SchemaPlan::Fresh => conn.execute_batch(SCHEMA_SQL)?,
        SchemaPlan::UpgradeFrom(_) => conn.execute_batch(SCHEMA_SQL)?, // 幂等补齐缺失对象
    }
    let from = user_version(conn)?;
    for (to, sql) in MIGRATIONS {
        if *to <= from {
            continue;
        }
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?; // 出错时 tx 被 drop → 自动回滚
        tx.commit()?;
    }
    set_user_version(conn, LATEST_VERSION)?;
    Ok(())
}

fn io_err(e: std::io::Error) -> rusqlite::Error {
    rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
        Some(e.to_string()),
    )
}

/// 升级前把原库连同 WAL 一并备份到同目录，文件名带来源版本与时间戳。
/// 备份失败则中止迁移，原库保持原样。
fn backup_before_upgrade(conn: &Connection, db_path: &Path, from_version: i64) -> DbResult<()> {
    // 尽力把 WAL 折回主文件，保证单文件拷贝完整；失败不阻断（下面对 -wal 再兜底拷贝）
    let _ = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let backup = db_path.with_file_name(format!("mudflat.db.bak-v{from_version}-{stamp}"));
    std::fs::copy(db_path, &backup).map_err(io_err)?;
    let wal = db_path.with_extension("db-wal");
    if wal.exists() {
        let _ = std::fs::copy(&wal, backup.with_extension("db-wal"));
    }
    Ok(())
}

/// 打开数据库连接并确保 schema 就绪。
/// - 库版本高于应用：报错拒绝打开（不覆盖、不降级）；
/// - 旧库升级前自动备份，迁移失败保留原库并向上抛错（阻止启动覆盖）。
pub fn open_db(app_data_dir: &Path) -> DbResult<Connection> {
    let db_path = app_data_dir.join("mudflat.db");
    let conn = Connection::open(&db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    let plan = plan_schema(&conn)?;
    if let SchemaPlan::UpgradeFrom(v) = plan {
        backup_before_upgrade(&conn, &db_path, v)?;
    }
    apply_schema(&conn, plan)?;
    Ok(conn)
}

// ---------- books ----------

pub fn upsert_book(conn: &Connection, b: &NewBook) -> DbResult<()> {
    conn.execute(
        "INSERT INTO books (weread_book_id, title, author, cover, reading_progress, note_count, review_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(weread_book_id) DO UPDATE SET
           title=excluded.title, author=excluded.author, cover=excluded.cover,
           reading_progress=excluded.reading_progress,
           note_count=excluded.note_count, review_count=excluded.review_count",
        rusqlite::params![b.weread_book_id, b.title, b.author, b.cover, b.reading_progress, b.note_count, b.review_count],
    )?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct NewBook {
    pub weread_book_id: String,
    pub title: String,
    pub author: String,
    pub cover: String,
    pub reading_progress: i64,
    pub note_count: i64,
    pub review_count: i64,
}

pub fn list_books(conn: &Connection) -> DbResult<Vec<BookRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, weread_book_id, title, author, cover, reading_progress, note_count, review_count, sync_reviews, synced_at
         FROM books ORDER BY note_count + review_count DESC, title",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(BookRow {
            id: r.get(0)?,
            weread_book_id: r.get(1)?,
            title: r.get(2)?,
            author: r.get(3)?,
            cover: r.get(4)?,
            reading_progress: r.get(5)?,
            note_count: r.get(6)?,
            review_count: r.get(7)?,
            sync_reviews: r.get::<_, i64>(8)? != 0,
            synced_at: r.get(9)?,
        })
    })?;
    rows.collect()
}

pub fn set_book_sync_reviews(conn: &Connection, book_row_id: i64, enabled: bool) -> DbResult<()> {
    conn.execute(
        "UPDATE books SET sync_reviews=?2 WHERE id=?1",
        rusqlite::params![book_row_id, enabled as i64],
    )?;
    Ok(())
}

fn book_by_weread_id(conn: &Connection, weread_book_id: &str) -> DbResult<Option<(i64, i64, i64)>> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, note_count, review_count FROM books WHERE weread_book_id=?1",
    )?;
    let mut rows = stmt.query_map([weread_book_id], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?))
    })?;
    match rows.next() {
        Some(v) => v.map(Some),
        None => Ok(None),
    }
}

/// 返回该书的本地行 id；不存在时返回 None。
pub fn find_book_row(conn: &Connection, weread_book_id: &str) -> DbResult<Option<i64>> {
    Ok(book_by_weread_id(conn, weread_book_id)?.map(|(id, _, _)| id))
}

pub fn touch_book_synced(conn: &Connection, book_row_id: i64, now: i64) -> DbResult<()> {
    conn.execute("UPDATE books SET synced_at=?2 WHERE id=?1", rusqlite::params![book_row_id, now])?;
    Ok(())
}

/// 记录某书「上次成功内容同步」的远端划线/想法计数（成功基线，R0）。
pub fn set_book_sync_baseline(conn: &Connection, book_row_id: i64, note: i64, review: i64) -> DbResult<()> {
    conn.execute(
        "UPDATE books SET synced_note_count=?2, synced_review_count=?3 WHERE id=?1",
        rusqlite::params![book_row_id, note, review],
    )?;
    Ok(())
}

/// 读取成功基线；从未成功同步过时为 (None, None)。
pub fn book_sync_baseline(conn: &Connection, book_row_id: i64) -> DbResult<(Option<i64>, Option<i64>)> {
    conn.query_row(
        "SELECT synced_note_count, synced_review_count FROM books WHERE id=?1",
        [book_row_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
}

/// 一次性 created_at 修复：清空全部书的同步基线（置 NULL）并返回受影响书数。
/// 基线缺失的语义是「一律拉取」（见 sync::needs_pull），于是下次同步强制全量重拉，
/// 让 upsert_card 用远端 createTime 修回被旧版钳成同步时刻的 created_at。
pub fn clear_book_sync_baselines(conn: &Connection) -> DbResult<usize> {
    conn.execute(
        "UPDATE books SET synced_note_count=NULL, synced_review_count=NULL",
        [],
    )
}

// ---------- cards ----------

#[derive(Debug, Clone)]
pub struct UpsertCard<'a> {
    pub kind: &'a str,
    pub book_row_id: i64,
    pub remote_id: &'a str,
    pub chapter_uid: Option<i64>,
    pub chapter_title: Option<&'a str>,
    pub text: &'a str,
    pub abstract_text: Option<&'a str>,
    pub range_str: Option<&'a str>,
    pub color_style: i64,
    /// 远端内容创建时间（划线/想法的 createTime，unix 秒）——卡片上展示的「划线时间」
    pub created_at: i64,
}

/// 插入或按 remote_id 更新同步卡。返回 (行 id, 是否新插入)。
/// 用户隐藏墓碑（hidden_by_user=1）优先于远端内容：不复活为可见（R4）。
/// 不触碰 excluded_from_review / note / starred 等用户字段（R2）。
/// created_at 始终写远端 createTime（真实划线时间），冲突时同样覆盖：
/// 远端是权威源，重复同步能自愈旧库把 created_at 污染成同步时刻的数据。
pub fn upsert_card(conn: &Connection, c: &UpsertCard, now: i64) -> DbResult<(i64, bool)> {
    let existed: Option<i64> = conn
        .query_row(
            "SELECT id FROM cards WHERE remote_id=?1",
            [c.remote_id],
            |r| r.get(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    // created_at 语义 = 远端内容创建时间（划线/想法的 createTime），即「当时的划线时间」，
    // 不是同步时刻。只防远端时钟跑到本地未来：钳到 now；过去的真实时间必须原样保留。
    // 远端异常缺省（<=0）时回退为同步时刻，避免出现 1970 卡。
    let created_at = if c.created_at > 0 { c.created_at.min(now) } else { now };
    conn.execute(
        "INSERT INTO cards (kind, book_id, remote_id, chapter_uid, chapter_title, text, abstract_text, range_str, color_style, created_at, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
         ON CONFLICT(remote_id) DO UPDATE SET
           kind=excluded.kind, book_id=excluded.book_id,
           chapter_uid=excluded.chapter_uid, chapter_title=excluded.chapter_title,
           text=excluded.text, abstract_text=excluded.abstract_text, range_str=excluded.range_str,
           color_style=excluded.color_style,
           deleted=CASE WHEN cards.hidden_by_user=1 THEN 1 ELSE 0 END,
           created_at=excluded.created_at,
           updated_at=excluded.updated_at",
        rusqlite::params![
            c.kind, c.book_row_id, c.remote_id, c.chapter_uid, c.chapter_title,
            c.text, c.abstract_text, c.range_str, c.color_style, created_at, now
        ],
    )?;
    let row_id = match existed {
        Some(id) => id, // 走的是 DO UPDATE 路径：last_insert_rowid 不会刷新，必须用预查的 id
        None => conn.last_insert_rowid(),
    };
    ensure_review_row(conn, row_id, now)?;
    Ok((row_id, existed.is_none()))
}

const CARD_SELECT_COLS: &str =
    "c.id, c.kind, c.book_id, c.remote_id, c.chapter_uid, c.chapter_title, c.text, c.abstract_text, c.range_str, \
     c.color_style, c.note, c.starred, c.excluded_from_review, c.created_at, c.updated_at, c.deleted";

struct CardParts {
    id: i64,
    kind: String,
    book_id: Option<i64>,
    remote_id: Option<String>,
    chapter_uid: Option<i64>,
    chapter_title: Option<String>,
    text: String,
    abstract_text: Option<String>,
    range_str: Option<String>,
    color_style: i64,
    note: String,
    starred: bool,
    excluded_from_review: bool,
    created_at: i64,
    updated_at: i64,
    deleted: bool,
    book_title: String,
}

impl CardParts {
    fn finalize(self, tags: Vec<String>) -> CardRow {
        CardRow {
            id: self.id,
            kind: self.kind,
            book_id: self.book_id,
            remote_id: self.remote_id,
            chapter_uid: self.chapter_uid,
            chapter_title: self.chapter_title,
            text: self.text,
            abstract_text: self.abstract_text,
            range_str: self.range_str,
            color_style: self.color_style,
            note: self.note,
            starred: self.starred,
            excluded_from_review: self.excluded_from_review,
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted: self.deleted,
            book_title: self.book_title,
            tags,
        }
    }
}

fn parse_card(r: &rusqlite::Row) -> rusqlite::Result<CardParts> {
    Ok(CardParts {
        id: r.get(0)?,
        kind: r.get(1)?,
        book_id: r.get(2)?,
        remote_id: r.get(3)?,
        chapter_uid: r.get(4)?,
        chapter_title: r.get(5)?,
        text: r.get(6)?,
        abstract_text: r.get(7)?,
        range_str: r.get(8)?,
        color_style: r.get(9)?,
        note: r.get(10)?,
        starred: r.get::<_, i64>(11)? != 0,
        excluded_from_review: r.get::<_, i64>(12)? != 0,
        created_at: r.get(13)?,
        updated_at: r.get(14)?,
        deleted: r.get::<_, i64>(15)? != 0,
        book_title: r.get(16)?,
    })
}

fn card_select_sql(where_clause: &str) -> String {
    format!(
        "SELECT {CARD_SELECT_COLS}, COALESCE(b.title,'')
         FROM cards c LEFT JOIN books b ON c.book_id=b.id WHERE {where_clause}"
    )
}

fn load_tags_for_cards(conn: &Connection, ids: &[i64]) -> DbResult<std::collections::HashMap<i64, Vec<String>>> {
    let mut map: std::collections::HashMap<i64, Vec<String>> = std::collections::HashMap::new();
    if ids.is_empty() {
        return Ok(map);
    }
    let in_list = ids.iter().enumerate().map(|(i, _)| if i == 0 { "?1".into() } else { format!("?{}", i + 1) }).collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT ct.card_id, t.name FROM card_tags ct JOIN tags t ON ct.tag_id=t.id WHERE ct.card_id IN ({in_list})"
    );
    let params_iter = rusqlite::params_from_iter(ids.iter());
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_iter, |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (card_id, name) = row?;
        map.entry(card_id).or_default().push(name);
    }
    Ok(map)
}

fn collect_cards(conn: &Connection, sql: &str, params: &[&dyn rusqlite::ToSql]) -> DbResult<Vec<CardRow>> {
    let mut stmt = conn.prepare(sql)?;
    let parts: Vec<CardParts> = stmt
        .query_map(params, parse_card)?
        .collect::<rusqlite::Result<_>>()?;
    let ids: Vec<i64> = parts.iter().map(|p| p.id).collect();
    let tag_map = load_tags_for_cards(conn, &ids)?;
    Ok(parts
        .into_iter()
        .map(|p| {
            let tags = tag_map.get(&p.id).cloned().unwrap_or_default();
            p.finalize(tags)
        })
        .collect())
}

fn apply_card_filter(filter: &CardFilter, conds: &mut Vec<String>, params: &mut Vec<Box<dyn rusqlite::ToSql>>) {
    if let Some(bid) = filter.book_id {
        conds.push(format!("c.book_id=?{}", push_param(params, bid)));
    }
    if filter.starred_only {
        conds.push("c.starred=1".into());
    }
    if !filter.kinds.is_empty() {
        let placeholders = filter.kinds.iter().map(|k| { let _ = push_param(params, k.clone()); format!("?{}", params.len()) }).collect::<Vec<_>>().join(",");
        conds.push(format!("c.kind IN ({placeholders})"));
    }
    if !filter.tag_ids.is_empty() {
        // OR 语义：卡片有任一选中标签即命中
        let placeholders = filter.tag_ids.iter().map(|t| { let _ = push_param(params, *t); format!("?{}", params.len()) }).collect::<Vec<_>>().join(",");
        conds.push(format!(
            "c.id IN (SELECT card_id FROM card_tags WHERE tag_id IN ({placeholders}))"
        ));
    }
}

/// 当前筛选下全部可见卡 id，无条数上限。语义检索用它，而不是墙的最近 2000 张。
pub fn query_card_ids(conn: &Connection, filter: &CardFilter) -> DbResult<HashSet<i64>> {
    let mut conds: Vec<String> = vec!["c.deleted=0".into()];
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![];
    apply_card_filter(filter, &mut conds, &mut params);
    let sql = format!("SELECT c.id FROM cards c WHERE {}", conds.join(" AND "));
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    let ids = stmt
        .query_map(refs.as_slice(), |r| r.get::<_, i64>(0))?
        .collect::<rusqlite::Result<HashSet<i64>>>()?;
    Ok(ids)
}

pub fn query_cards(conn: &Connection, filter: &CardFilter, limit: i64, offset: i64) -> DbResult<Vec<CardRow>> {
    let mut conds: Vec<String> = vec!["c.deleted=0".into()];
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![];
    apply_card_filter(filter, &mut conds, &mut params);
    // 占位符从 ?1 起连续编号：LIMIT/OFFSET 取当前长度+1
    let limit_ph = params.len() + 1;
    params.push(Box::new(limit));
    let offset_ph = params.len() + 1;
    params.push(Box::new(offset));
    let sql = format!(
        "{} ORDER BY c.created_at DESC LIMIT ?{} OFFSET ?{}",
        card_select_sql(&conds.join(" AND ")),
        limit_ph,
        offset_ph
    );
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
    collect_cards(conn, &sql, &refs)
}

pub fn count_cards(conn: &Connection, filter: &CardFilter) -> DbResult<i64> {
    let mut conds: Vec<String> = vec!["c.deleted=0".into()];
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![];
    apply_card_filter(filter, &mut conds, &mut params);
    let sql = format!("SELECT COUNT(*) FROM cards c WHERE {}", conds.join(" AND "));
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
    let mut stmt = conn.prepare(&sql)?;
    stmt.query_row(refs.as_slice(), |r| r.get(0))
}

fn push_param(params: &mut Vec<Box<dyn rusqlite::ToSql>>, v: impl rusqlite::ToSql + 'static) -> usize {
    params.push(Box::new(v));
    params.len()
}

pub fn get_card(conn: &Connection, id: i64) -> DbResult<Option<CardRow>> {
    let sql = card_select_sql("c.id=?1 AND c.deleted=0");
    let rows = collect_cards(conn, &sql, &[&id])?;
    Ok(rows.into_iter().next())
}

/// 按 id 取可见卡，返回顺序与传入 ids 一致；缺卡/已隐藏的跳过。
pub fn cards_by_ids(conn: &Connection, ids: &[i64]) -> DbResult<Vec<CardRow>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let mut conds: Vec<String> = vec!["c.deleted=0".into()];
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![];
    let placeholders = ids
        .iter()
        .map(|id| format!("?{}", push_param(&mut params, *id)))
        .collect::<Vec<_>>()
        .join(",");
    conds.push(format!("c.id IN ({placeholders})"));
    let sql = card_select_sql(&conds.join(" AND "));
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
    let rows = collect_cards(conn, &sql, &refs)?;
    let pos: std::collections::HashMap<i64, usize> =
        ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let mut ordered = rows;
    ordered.sort_by_key(|c| pos.get(&c.id).copied().unwrap_or(usize::MAX));
    Ok(ordered)
}

pub fn search_cards(conn: &Connection, q: &str, filter: &CardFilter, limit: i64) -> DbResult<Vec<CardRow>> {
    let trimmed = q.trim();
    // trigram 需要 ≥3 字符；短词回退 LIKE 全表扫描（cards 量级数千，可接受）
    let use_fts = trimmed.chars().count() >= 3;
    let mut conds: Vec<String> = vec!["c.deleted=0".into()];
    // PRD 11.8：FTS 路径把整句包成短语查询并转义内嵌引号，避免语法字符（" - OR 等）导致报错
    let match_arg = if use_fts {
        format!("\"{}\"", trimmed.replace('"', "\"\""))
    } else {
        trimmed.to_string()
    };
    if use_fts {
        conds.push("c.id IN (SELECT rowid FROM cards_fts WHERE cards_fts MATCH ?1)".into());
    } else {
        conds.push("(c.text LIKE '%'||?1||'%' OR c.note LIKE '%'||?1||'%')".into());
    }
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(match_arg)];
    if let Some(bid) = filter.book_id {
        conds.push(format!("c.book_id=?{}", push_param(&mut params, bid)));
    }
    if filter.starred_only {
        conds.push("c.starred=1".into());
    }
    if !filter.kinds.is_empty() {
        let placeholders = filter.kinds.iter().map(|k| { let _ = push_param(&mut params, k.clone()); format!("?{}", params.len()) }).collect::<Vec<_>>().join(",");
        conds.push(format!("c.kind IN ({placeholders})"));
    }
    if !filter.tag_ids.is_empty() {
        let placeholders = filter.tag_ids.iter().map(|t| { let _ = push_param(&mut params, *t); format!("?{}", params.len()) }).collect::<Vec<_>>().join(",");
        conds.push(format!("c.id IN (SELECT card_id FROM card_tags WHERE tag_id IN ({placeholders}))"));
    }
    let limit_ph = params.len() + 1;
    params.push(Box::new(limit));
    let sql = format!(
        "{} ORDER BY c.created_at DESC LIMIT ?{}",
        card_select_sql(&conds.join(" AND ")),
        limit_ph
    );
    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
    collect_cards(conn, &sql, &refs)
}


pub fn create_self_card(conn: &Connection, text: &str, now: i64) -> DbResult<i64> {
    conn.execute(
        "INSERT INTO cards (kind, text, created_at, updated_at) VALUES ('self', ?1, ?2, ?2)",
        rusqlite::params![text, now],
    )?;
    let id = conn.last_insert_rowid();
    ensure_review_row(conn, id, now)?;
    Ok(id)
}

pub fn update_card_note(conn: &Connection, card_id: i64, note: &str, now: i64) -> DbResult<()> {
    conn.execute(
        "UPDATE cards SET note=?2, updated_at=?3 WHERE id=?1",
        rusqlite::params![card_id, note, now],
    )?;
    Ok(())
}

pub fn update_card_text(conn: &Connection, card_id: i64, text: &str, now: i64) -> DbResult<()> {
    conn.execute(
        "UPDATE cards SET text=?2, updated_at=?3 WHERE id=?1 AND kind='self'",
        rusqlite::params![card_id, text, now],
    )?;
    Ok(())
}

pub fn set_starred(conn: &Connection, card_id: i64, starred: bool) -> DbResult<()> {
    conn.execute(
        "UPDATE cards SET starred=?2 WHERE id=?1",
        rusqlite::params![card_id, starred as i64],
    )?;
    Ok(())
}

/// 用户对同步卡执行「隐藏」：写入用户墓碑并从所有视图移除。
/// 与远端 reconcile 删除语义分开；同步 upsert 尊重该墓碑，不复活（R4）。
pub fn hide_card_from_user(conn: &Connection, card_id: i64) -> DbResult<()> {
    conn.execute(
        "UPDATE cards SET deleted=1, hidden_by_user=1 WHERE id=?1",
        [card_id],
    )?;
    Ok(())
}

pub fn hard_delete_card(conn: &Connection, card_id: i64) -> DbResult<()> {
    conn.execute("DELETE FROM card_tags WHERE card_id=?1", [card_id])?;
    conn.execute("DELETE FROM review_state WHERE card_id=?1", [card_id])?;
    conn.execute("DELETE FROM cards WHERE id=?1 AND kind='self'", [card_id])?;
    Ok(())
}

/// 移出/恢复回顾。恢复（excluded=false）时立即回到待回顾状态：due_at=now（R2）。
pub fn set_excluded_from_review(conn: &Connection, card_id: i64, excluded: bool, now: i64) -> DbResult<()> {
    conn.execute(
        "UPDATE cards SET excluded_from_review=?2 WHERE id=?1",
        rusqlite::params![card_id, excluded as i64],
    )?;
    if !excluded {
        ensure_review_row(conn, card_id, now)?;
        conn.execute(
            "UPDATE review_state SET due_at=?2 WHERE card_id=?1",
            rusqlite::params![card_id, now],
        )?;
    }
    Ok(())
}

/// reconcile：本次同步结果中不存在的远程卡按「远端删除」软删。
/// 只动 deleted/hidden_by_user，note/星标/标签/排除状态全部保留（R0/R2）。
pub fn reconcile_cards(conn: &Connection, book_row_id: i64, present_remote_ids: &[String]) -> DbResult<usize> {
    let present: HashSet<&String> = present_remote_ids.iter().collect();
    let mut stmt = conn.prepare_cached(
        "SELECT c.id, c.remote_id FROM cards c
         JOIN books b ON c.book_id=b.id
         WHERE c.book_id=?1 AND c.deleted=0 AND c.remote_id IS NOT NULL",
    )?;
    let rows: Vec<(i64, String)> = stmt
        .query_map([book_row_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut n = 0;
    for (id, remote_id) in rows {
        if !present.contains(&remote_id) {
            conn.execute(
                "UPDATE cards SET deleted=1, hidden_by_user=0 WHERE id=?1",
                [id],
            )?;
            n += 1;
        }
    }
    Ok(n)
}

// ---------- tags ----------

#[derive(Debug, Clone, Serialize)]
pub struct TagRow {
    pub id: i64,
    pub name: String,
}

pub fn list_tags(conn: &Connection) -> DbResult<Vec<TagRow>> {
    let mut stmt = conn.prepare_cached(
        "SELECT t.id, t.name, COUNT(ct.card_id) AS cnt FROM tags t
         LEFT JOIN card_tags ct ON ct.tag_id=t.id
         GROUP BY t.id ORDER BY t.name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(TagRow { id: r.get(0)?, name: r.get(1)? })
    })?;
    rows.collect()
}

fn get_or_create_tag(conn: &Connection, name: &str) -> DbResult<i64> {
    conn.execute("INSERT OR IGNORE INTO tags(name) VALUES (?1)", [name])?;
    let mut stmt = conn.prepare_cached("SELECT id FROM tags WHERE name=?1")?;
    stmt.query_row([name], |r| r.get(0))
}

pub fn add_tag_to_card(conn: &Connection, card_id: i64, tag_name: &str) -> DbResult<i64> {
    let tag_id = get_or_create_tag(conn, tag_name)?;
    conn.execute(
        "INSERT OR IGNORE INTO card_tags(card_id, tag_id) VALUES (?1, ?2)",
        rusqlite::params![card_id, tag_id],
    )?;
    Ok(tag_id)
}

pub fn remove_tag_from_card(conn: &Connection, card_id: i64, tag_name: &str) -> DbResult<()> {
    conn.execute(
        "DELETE FROM card_tags WHERE card_id=?1 AND tag_id=(SELECT id FROM tags WHERE name=?2)",
        rusqlite::params![card_id, tag_name],
    )?;
    // 清理孤儿标签
    conn.execute(
        "DELETE FROM tags WHERE id NOT IN (SELECT DISTINCT tag_id FROM card_tags)",
        [],
    )?;
    Ok(())
}

pub fn delete_tag(conn: &Connection, tag_id: i64) -> DbResult<()> {
    conn.execute("DELETE FROM card_tags WHERE tag_id=?1", [tag_id])?;
    conn.execute("DELETE FROM tags WHERE id=?1", [tag_id])?;
    Ok(())
}

// ---------- review (SRS) ----------

pub fn ensure_review_row(conn: &Connection, card_id: i64, due_now: i64) -> DbResult<()> {
    conn.execute(
        "INSERT OR IGNORE INTO review_state(card_id, due_at) VALUES (?1, ?2)",
        rusqlite::params![card_id, due_now],
    )?;
    Ok(())
}

/// 到期卡片。book_id 传入时只取该书（本书翻牌）；None 为整馆。
pub fn due_cards(conn: &Connection, now: i64, limit: i64, book_id: Option<i64>) -> DbResult<Vec<CardRow>> {
    // book_id 是服务端 i64，走 format! 与既有 {now}/{limit} 同一约束
    let scope = book_id.map(|id| format!(" AND c.book_id={id}")).unwrap_or_default();
    let sql = format!(
        "SELECT {}, COALESCE(b.title,'') FROM cards c \
         LEFT JOIN books b ON c.book_id=b.id \
         JOIN review_state rs ON rs.card_id=c.id \
         WHERE c.deleted=0 AND c.excluded_from_review=0 AND rs.due_at<={now} \
         AND (b.id IS NULL OR b.sync_reviews<>0){scope} \
         ORDER BY rs.due_at LIMIT {limit}",
        CARD_SELECT_COLS
    );
    collect_cards(conn, &sql, &[])
}

/// 到期计数。book_id 传入时只数该书；None 为整馆。
pub fn due_count(conn: &Connection, now: i64, book_id: Option<i64>) -> DbResult<i64> {
    let scope = book_id.map(|id| format!(" AND c.book_id={id}")).unwrap_or_default();
    conn.query_row(
        &format!(
            "SELECT COUNT(*) FROM cards c \
             LEFT JOIN books b ON c.book_id=b.id \
             JOIN review_state rs ON rs.card_id=c.id \
             WHERE c.deleted=0 AND c.excluded_from_review=0 AND rs.due_at<=?1 \
             AND (b.id IS NULL OR b.sync_reviews<>0){scope}"
        ),
        [now],
        |r| r.get(0),
    )
}

pub fn save_review_state(conn: &Connection, card_id: i64, st: &crate::srs::SrsState) -> DbResult<()> {
    conn.execute(
        "INSERT INTO review_state(card_id, due_at, interval_days, ease, reps, lapses)
         VALUES (?1,?2,?3,?4,?5,?6)
         ON CONFLICT(card_id) DO UPDATE SET
           due_at=excluded.due_at, interval_days=excluded.interval_days,
           ease=excluded.ease, reps=excluded.reps, lapses=excluded.lapses",
        rusqlite::params![card_id, st.due_at, st.interval_days, st.ease, st.reps, st.lapses],
    )?;
    Ok(())
}

pub fn load_review_state(conn: &Connection, card_id: i64) -> DbResult<Option<crate::srs::SrsState>> {
    let mut stmt = conn.prepare_cached(
        "SELECT due_at, interval_days, ease, reps, lapses FROM review_state WHERE card_id=?1",
    )?;
    let mut rows = stmt.query_map([card_id], |r| {
        Ok(crate::srs::SrsState {
            due_at: r.get(0)?,
            interval_days: r.get(1)?,
            ease: r.get(2)?,
            reps: r.get(3)?,
            lapses: r.get(4)?,
        })
    })?;
    match rows.next() {
        Some(v) => v.map(Some),
        None => Ok(None),
    }
}

// ---------- sync meta / settings ----------

/// 回顾批次大小在 sync_meta 中的键。合法值 10/20/30，默认 20（R3）。
pub const KEY_REVIEW_BATCH: &str = "review_batch_size";

/// 一次性修复标记：旧版把同步卡的 created_at 钳成了同步时刻。
/// 本键缺失时，sync 在拉取前清空全部基线强制全量重拉一次，跑完后写入本键。
pub const KEY_CREATED_AT_REPAIR: &str = "created_at_repair_v1";
pub const REVIEW_BATCH_OPTIONS: [i64; 3] = [10, 20, 30];
pub const DEFAULT_REVIEW_BATCH: i64 = 20;

pub fn get_sync_meta(conn: &Connection, key: &str) -> DbResult<Option<String>> {
    let mut stmt = conn.prepare_cached("SELECT value FROM sync_meta WHERE key=?1")?;
    let mut rows = stmt.query_map([key], |r| r.get::<_, String>(0))?;
    match rows.next() {
        Some(v) => v.map(Some),
        None => Ok(None),
    }
}

pub fn set_sync_meta(conn: &Connection, key: &str, value: &str) -> DbResult<()> {
    conn.execute(
        "INSERT INTO sync_meta(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        rusqlite::params![key, value],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_700_000_000;

    fn mem() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        apply_schema(&conn, SchemaPlan::Fresh).unwrap();
        conn
    }

    fn book(conn: &Connection, wid: &str, sync_reviews: bool) -> i64 {
        upsert_book(
            conn,
            &NewBook {
                weread_book_id: wid.into(),
                title: wid.into(),
                author: String::new(),
                cover: String::new(),
                reading_progress: 0,
                note_count: 3,
                review_count: 1,
            },
        )
        .unwrap();
        let id = find_book_row(conn, wid).unwrap().unwrap();
        set_book_sync_reviews(conn, id, sync_reviews).unwrap();
        id
    }

    /// 插入一张同步卡并把到期时间拨到 now + due_in
    fn card(conn: &Connection, book_row_id: i64, remote: &str, due_in: i64) -> i64 {
        let (id, _) = upsert_card(
            conn,
            &UpsertCard {
                kind: "highlight",
                book_row_id,
                remote_id: remote,
                chapter_uid: None,
                chapter_title: None,
                text: remote,
                abstract_text: None,
                range_str: None,
                color_style: 0,
                created_at: NOW,
            },
            NOW,
        )
        .unwrap();
        conn.execute(
            "UPDATE review_state SET due_at=?2 WHERE card_id=?1",
            rusqlite::params![id, NOW + due_in],
        )
        .unwrap();
        id
    }

    #[test]
    fn due_scope_filters_by_book() {
        let conn = mem();
        let b1 = book(&conn, "b1", true);
        let b2 = book(&conn, "b2", true);
        card(&conn, b1, "r-1", 0);
        card(&conn, b1, "r-2", 0);
        card(&conn, b2, "r-3", 0);
        card(&conn, b2, "r-4", 3600); // 未到期，两边都不该出现

        assert_eq!(due_count(&conn, NOW, None).unwrap(), 3);
        assert_eq!(due_count(&conn, NOW, Some(b1)).unwrap(), 2);
        assert_eq!(due_count(&conn, NOW, Some(b2)).unwrap(), 1);

        let rows = due_cards(&conn, NOW, 30, Some(b1)).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|c| c.book_id == Some(b1)), "本书翻牌不得混入其他书");

        let all = due_cards(&conn, NOW, 30, None).unwrap();
        assert_eq!(all.len(), 3, "None 仍是整馆队列");
    }

    #[test]
    fn get_card_and_cards_by_ids_preserve_order() {
        let conn = mem();
        let b1 = book(&conn, "b1", true);
        let a = card(&conn, b1, "r-a", 0);
        let b = card(&conn, b1, "r-b", 0);
        let c = card(&conn, b1, "r-c", 0);
        assert_eq!(get_card(&conn, a).unwrap().unwrap().text, "r-a");
        let rows = cards_by_ids(&conn, &[c, a, b]).unwrap();
        assert_eq!(
            rows.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![c, a, b]
        );
        hide_card_from_user(&conn, b).unwrap();
        assert!(get_card(&conn, b).unwrap().is_none());
        assert_eq!(cards_by_ids(&conn, &[b, a]).unwrap().len(), 1);
    }

    #[test]
    fn due_scope_respects_sync_reviews_toggle() {
        let conn = mem();
        let off = book(&conn, "b-off", false);
        let on = book(&conn, "b-on", true);
        card(&conn, off, "r-off", 0);
        card(&conn, on, "r-on", 0);

        // 关掉回顾同步的书：无论整馆还是按书，到期队列都不可见
        assert_eq!(due_count(&conn, NOW, None).unwrap(), 1);
        assert_eq!(due_count(&conn, NOW, Some(off)).unwrap(), 0);
        assert_eq!(due_count(&conn, NOW, Some(on)).unwrap(), 1);
    }
}
