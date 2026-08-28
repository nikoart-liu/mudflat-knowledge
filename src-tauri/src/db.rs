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

#[derive(Debug, Clone, Serialize)]
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

/// 打开数据库连接并确保 schema 就绪。
pub fn open_db(app_data_dir: &Path) -> DbResult<Connection> {
    let conn = Connection::open(app_data_dir.join("mudflat.db"))?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    conn.execute_batch(SCHEMA_SQL)?;
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
    pub created_at: i64,
}

pub fn upsert_card(conn: &Connection, c: &UpsertCard, now: i64) -> DbResult<i64> {
    conn.execute(
        "INSERT INTO cards (kind, book_id, remote_id, chapter_uid, chapter_title, text, abstract_text, range_str, color_style, created_at, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10)
         ON CONFLICT(remote_id) DO UPDATE SET
           kind=excluded.kind, book_id=excluded.book_id,
           chapter_uid=excluded.chapter_uid, chapter_title=excluded.chapter_title,
           text=excluded.text, abstract_text=excluded.abstract_text, range_str=excluded.range_str,
           color_style=excluded.color_style, deleted=0, updated_at=excluded.updated_at",
        rusqlite::params![
            c.kind, c.book_row_id, c.remote_id, c.chapter_uid, c.chapter_title,
            c.text, c.abstract_text, c.range_str, c.color_style, c.created_at.max(now.saturating_sub(1))
        ],
    )?;
    let row_id = conn.last_insert_rowid();
    ensure_review_row(conn, row_id, now)?;
    Ok(row_id)
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

pub fn query_cards(conn: &Connection, filter: &CardFilter, limit: i64, offset: i64) -> DbResult<Vec<CardRow>> {
    let mut conds: Vec<String> = vec!["c.deleted=0".into()];
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![];
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
        // OR 语义：卡片有任一选中标签即命中
        let placeholders = filter.tag_ids.iter().map(|t| { let _ = push_param(&mut params, *t); format!("?{}", params.len()) }).collect::<Vec<_>>().join(",");
        conds.push(format!(
            "c.id IN (SELECT card_id FROM card_tags WHERE tag_id IN ({placeholders}))"
        ));
    }
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

fn push_param(params: &mut Vec<Box<dyn rusqlite::ToSql>>, v: impl rusqlite::ToSql + 'static) -> usize {
    params.push(Box::new(v));
    params.len()
}

pub fn search_cards(conn: &Connection, q: &str, filter: &CardFilter, limit: i64) -> DbResult<Vec<CardRow>> {
    let trimmed = q.trim();
    // trigram 需要 ≥3 字符；短词回退 LIKE 全表扫描（cards 量级数千，可接受）
    let use_fts = trimmed.chars().count() >= 3;
    let mut conds: Vec<String> = vec!["c.deleted=0".into()];
    if use_fts {
        conds.push("c.id IN (SELECT rowid FROM cards_fts WHERE cards_fts MATCH ?1)".into());
    } else {
        conds.push("(c.text LIKE '%'||?1||'%' OR c.note LIKE '%'||?1||'%')".into());
    }
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(trimmed.to_string())];
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

/// 软删：从列表过滤但不破坏用户编辑；同步卡下次同步若仍存在会被 upsert 复活。
pub fn soft_delete_card(conn: &Connection, card_id: i64) -> DbResult<()> {
    conn.execute("UPDATE cards SET deleted=1 WHERE id=?1", [card_id])?;
    Ok(())
}

pub fn hard_delete_card(conn: &Connection, card_id: i64) -> DbResult<()> {
    conn.execute("DELETE FROM card_tags WHERE card_id=?1", [card_id])?;
    conn.execute("DELETE FROM review_state WHERE card_id=?1", [card_id])?;
    conn.execute("DELETE FROM cards WHERE id=?1 AND kind='self'", [card_id])?;
    Ok(())
}

pub fn set_excluded_from_review(conn: &Connection, card_id: i64, excluded: bool) -> DbResult<()> {
    conn.execute(
        "UPDATE cards SET excluded_from_review=?2 WHERE id=?1",
        rusqlite::params![card_id, excluded as i64],
    )?;
    Ok(())
}

/// reconcile：本次同步结果中不存在的远程卡软删。
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
            conn.execute("UPDATE cards SET deleted=1 WHERE id=?1", [id])?;
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

pub fn due_cards(conn: &Connection, now: i64, limit: i64) -> DbResult<Vec<CardRow>> {
    let sql = format!(
        "SELECT {}, COALESCE(b.title,'') FROM cards c \
         LEFT JOIN books b ON c.book_id=b.id \
         JOIN review_state rs ON rs.card_id=c.id \
         WHERE c.deleted=0 AND c.excluded_from_review=0 AND rs.due_at<={now} \
         AND (b.id IS NULL OR b.sync_reviews<>0) \
         ORDER BY rs.due_at LIMIT {limit}",
        CARD_SELECT_COLS
    );
    collect_cards(conn, &sql, &[])
}

pub fn due_count(conn: &Connection, now: i64) -> DbResult<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM cards c \
         LEFT JOIN books b ON c.book_id=b.id \
         JOIN review_state rs ON rs.card_id=c.id \
         WHERE c.deleted=0 AND c.excluded_from_review=0 AND rs.due_at<=?1 \
         AND (b.id IS NULL OR b.sync_reviews<>0)",
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
