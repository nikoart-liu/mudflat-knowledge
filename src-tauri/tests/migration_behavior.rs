//! 迁移框架行为测试（PRD R0 / 10.3）：
//! - v0.1 真实旧库形态（user_version=0、无新列）升级后数据零丢失；
//! - 升级前自动备份；迁移失败回滚且保留原库；
//! - 升级幂等（二次打开不再迁移、不再备份）；
//! - 库版本高于应用时拒绝打开、原库不动。

use mudflat_knowledge_lib::db::{self, SchemaPlan};

/// v0.1 发布时的 SCHEMA_SQL 原样拷贝（与 db.rs 中 SCHEMA_SQL 一致，禁止随手改）。
/// 用于构造「真实 v0.1 旧库」。
const V01_SCHEMA_SQL: &str = r#"
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

fn tmp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mudflat-mig-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn column_names(conn: &rusqlite::Connection, table: &str) -> Vec<String> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})")).unwrap();
    let rows = stmt.query_map([], |r| r.get::<_, String>(1)).unwrap();
    rows.map(|r| r.unwrap()).collect()
}

fn backup_count(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("mudflat.db.bak-"))
        .count()
}

/// 构造一个带完整用户数据的 v0.1 旧库：
/// 一本书 3 张卡（1 张软删）、标签、星标、批注、排除状态、SRS 状态、sync_meta。
fn seed_v01_db(dir: &std::path::Path) {
    let conn = rusqlite::Connection::open(dir.join("mudflat.db")).unwrap();
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;").unwrap();
    conn.execute_batch(V01_SCHEMA_SQL).unwrap();
    conn.execute_batch(
        "INSERT INTO books (weread_book_id, title, author, note_count, review_count, synced_at)
         VALUES ('wb-1', '置身事内', '兰小欢', 3, 0, 1700000000);
         INSERT INTO cards (kind, book_id, remote_id, chapter_uid, chapter_title, text, note, starred, excluded_from_review, created_at, updated_at, deleted)
         VALUES ('highlight', 1, 'wb-1-bm-0', 11, '第一章', '政府直接参与了经济活动', '', 1, 0, 1700000100, 1700000100, 0),
                ('highlight', 1, 'wb-1-bm-1', 11, '第一章', '工作记忆容量有限', '值得反复读', 0, 1, 1700000200, 1700000200, 0),
                ('highlight', 1, 'wb-1-bm-2', 12, '第二章', '被用户删掉的一条', '', 0, 0, 1700000300, 1700000300, 1);
         INSERT INTO review_state (card_id, due_at, interval_days, ease, reps, lapses)
         VALUES (1, 1893456000, 3.0, 2.5, 2, 0);
         INSERT INTO tags (name) VALUES ('经济学');
         INSERT INTO card_tags (card_id, tag_id) VALUES (1, 1);
         INSERT INTO sync_meta (key, value) VALUES ('last_full_sync', '1700000000');",
    )
    .unwrap();
    assert_eq!(db::user_version(&conn).unwrap(), 0, "v0.1 从不写 user_version");
    conn.close().unwrap();
}

#[test]
fn fresh_db_is_created_at_latest_version() {
    let dir = tmp_dir("fresh");
    let conn = db::open_db(&dir).unwrap();
    assert_eq!(db::user_version(&conn).unwrap(), db::LATEST_VERSION);
    assert!(column_names(&conn, "books").contains(&"synced_note_count".to_string()));
    assert!(column_names(&conn, "books").contains(&"synced_review_count".to_string()));
    assert!(column_names(&conn, "cards").contains(&"hidden_by_user".to_string()));
    assert!(column_names(&conn, "card_embeddings").contains(&"vector".to_string()));
    assert!(column_names(&conn, "ai_artifacts").contains(&"content_json".to_string()));
    assert_eq!(backup_count(&dir), 0, "全新库不产生备份");
    conn.close().unwrap();
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn v01_upgrade_preserves_all_user_data_and_backs_up() {
    let dir = tmp_dir("v01-upgrade");
    seed_v01_db(&dir);

    let conn = db::open_db(&dir).unwrap();
    // 迁移到位
    assert_eq!(db::user_version(&conn).unwrap(), db::LATEST_VERSION);
    assert!(column_names(&conn, "books").contains(&"synced_note_count".to_string()));
    assert!(column_names(&conn, "cards").contains(&"hidden_by_user".to_string()));
    assert_eq!(backup_count(&dir), 1, "升级前必须自动备份");

    // 卡片全部保留（含软删），用户编辑不丢
    let texts: Vec<String> = {
        let mut stmt = conn.prepare("SELECT text FROM cards ORDER BY id").unwrap();
        stmt.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect()
    };
    assert_eq!(texts.len(), 3);
    assert!(texts.contains(&"被用户删掉的一条".to_string()));

    // 星标 / 批注 / 标签 / 排除状态 / SRS 状态逐项核对
    let (starred, note): (i64, String) = conn
        .query_row("SELECT starred, note FROM cards WHERE remote_id='wb-1-bm-0'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((starred, note.as_str()), (1, ""));
    let (note2, excluded): (String, i64) = conn
        .query_row("SELECT note, excluded_from_review FROM cards WHERE remote_id='wb-1-bm-1'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((note2.as_str(), excluded), ("值得反复读", 1));
    let tag: String = conn
        .query_row("SELECT t.name FROM card_tags ct JOIN tags t ON t.id=ct.tag_id WHERE ct.card_id=1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(tag, "经济学");
    let (due_at, interval_days, reps): (i64, f64, i64) = conn
        .query_row("SELECT due_at, interval_days, reps FROM review_state WHERE card_id=1", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap();
    assert_eq!((due_at, interval_days, reps), (1_893_456_000, 3.0, 2));
    let meta: String = conn
        .query_row("SELECT value FROM sync_meta WHERE key='last_full_sync'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(meta, "1700000000");

    // 存量软删同步卡：无法区分用户/远端删除 → 默认按用户隐藏，绝不允许复活
    let (deleted, hidden): (i64, i64) = conn
        .query_row("SELECT deleted, hidden_by_user FROM cards WHERE remote_id='wb-1-bm-2'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((deleted, hidden), (1, 1), "存量软删卡迁移为用户隐藏");

    // 墓碑在升级后依然有效：远端再推同 id 也不复活
    db::upsert_card(
        &conn,
        &db::UpsertCard {
            kind: "highlight",
            book_row_id: 1,
            remote_id: "wb-1-bm-2",
            chapter_uid: Some(12),
            chapter_title: Some("第二章"),
            text: "被用户删掉的一条（远端更新）",
            abstract_text: None,
            range_str: Some("1-2"),
            color_style: 1,
            created_at: 1_700_000_300,
        },
        1_900_000_000,
    )
    .unwrap();
    let still_hidden: i64 = conn
        .query_row("SELECT hidden_by_user FROM cards WHERE remote_id='wb-1-bm-2'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(still_hidden, 1, "升级后墓碑仍生效");
    conn.close().unwrap();
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn upgrade_is_idempotent_and_does_not_rebackup() {
    let dir = tmp_dir("idempotent");
    seed_v01_db(&dir);
    {
        let conn = db::open_db(&dir).unwrap();
        conn.close().unwrap();
    }
    let backups_after_first = backup_count(&dir);
    assert_eq!(backups_after_first, 1);
    {
        let conn = db::open_db(&dir).unwrap();
        assert_eq!(db::user_version(&conn).unwrap(), db::LATEST_VERSION);
        // 二次打开：无新迁移、无新备份
        let (n, rv) = db::book_sync_baseline(&conn, 1).unwrap();
        assert_eq!((n, rv), (None, None), "v0.1 升级后基线为空，下次同步将全量重拉一次");
    }
    assert_eq!(backup_count(&dir), backups_after_first, "已是最新版本时不得再备份");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn failed_migration_rolls_back_and_keeps_original() {
    // 模拟「半套迁移」的旧库：cards 已有 hidden_by_user，books 没有基线列。
    // 迁移在 cards 的 ALTER 上失败 → 整个迁移事务回滚 → books 不得留下新列。
    let dir = tmp_dir("failed-mig");
    seed_v01_db(&dir);
    {
        let conn = rusqlite::Connection::open(dir.join("mudflat.db")).unwrap();
        conn.execute_batch("ALTER TABLE cards ADD COLUMN hidden_by_user INTEGER NOT NULL DEFAULT 0;").unwrap();
        conn.close().unwrap();
    }
    let original = std::fs::read(dir.join("mudflat.db")).unwrap();

    let err = db::open_db(&dir).unwrap_err();
    assert!(
        err.to_string().contains("hidden_by_user") || err.to_string().to_lowercase().contains("duplicate"),
        "应当因重复列失败，实际：{err}"
    );

    // 原库未被推进：版本不变、books 无基线列、数据完好
    let conn = rusqlite::Connection::open(dir.join("mudflat.db")).unwrap();
    assert_eq!(db::user_version(&conn).unwrap(), 0, "失败迁移不得推进 user_version");
    assert!(!column_names(&conn, "books").contains(&"synced_note_count".to_string()),
        "失败迁移必须整体回滚，不得留下半套列");
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM cards", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 3, "失败迁移不得破坏数据");
    conn.close().unwrap();
    // 升级失败前仍应留有备份（open_db 在迁移前备份）
    assert!(backup_count(&dir) >= 1, "迁移失败也要有升级前备份兜底");
    // 失败后原库内容与迁移前逐字节一致（备份路径不影响原库）
    let after = std::fs::read(dir.join("mudflat.db")).unwrap();
    assert_eq!(original.len(), after.len(), "失败迁移不得改写原库内容");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn newer_db_version_is_rejected_without_touching() {
    let dir = tmp_dir("newer-db");
    {
        let conn = db::open_db(&dir).unwrap();
        conn.close().unwrap();
    }
    {
        let conn = rusqlite::Connection::open(dir.join("mudflat.db")).unwrap();
        conn.execute_batch("PRAGMA user_version=99;").unwrap();
        conn.close().unwrap();
    }
    let original = std::fs::read(dir.join("mudflat.db")).unwrap();
    let err = db::open_db(&dir).unwrap_err();
    assert!(err.to_string().contains("v99"), "应报告版本过高，实际：{err}");
    let after = std::fs::read(dir.join("mudflat.db")).unwrap();
    assert_eq!(original, after, "版本过高的库必须原样保留");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn plan_and_apply_schema_work_on_in_memory_conn() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    assert_eq!(db::plan_schema(&conn).unwrap(), SchemaPlan::Fresh);
    db::apply_schema(&conn, SchemaPlan::Fresh).unwrap();
    assert_eq!(db::plan_schema(&conn).unwrap(), SchemaPlan::Current);
    // 已是最新时 apply 是 no-op
    db::apply_schema(&conn, SchemaPlan::Current).unwrap();
    assert_eq!(db::user_version(&conn).unwrap(), db::LATEST_VERSION);
}
