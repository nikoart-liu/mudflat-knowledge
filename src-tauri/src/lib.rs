pub mod db;
mod gateway;
mod keychain;
pub mod srs;
mod sync;

use std::sync::Mutex;
use tauri::{Manager, State};

use db::{BookRow, CardFilter, CardRow, TagRow};
use srs::{Rating, SrsState};
use sync::SyncEvent;

type Db = Mutex<rusqlite::Connection>;

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn db_err(e: rusqlite::Error) -> String {
    e.to_string()
}

fn init_db(app: &tauri::AppHandle) -> Result<Mutex<rusqlite::Connection>, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("数据目录解析失败: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("数据目录创建失败: {e}"))?;
    let conn = db::open_db(&dir).map_err(db_err)?;
    Ok(Mutex::new(conn))
}

// ---------- setup / settings ----------

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupStatus {
    pub has_key: bool,
    pub has_books: bool,
}

#[tauri::command]
fn get_setup_status(state: State<'_, Db>) -> Result<SetupStatus, String> {
    let has_books = {
        let conn = state.lock().map_err(|e| e.to_string())?;
        conn.query_row("SELECT COUNT(*) FROM books", [], |r| r.get::<_, i64>(0))
            .map_err(db_err)?
            > 0
    };
    Ok(SetupStatus { has_key: keychain::has_key(), has_books })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub last_full_sync: Option<i64>,
    pub data_dir: Option<String>,
}

#[tauri::command]
fn get_settings(app: tauri::AppHandle, state: State<'_, Db>) -> Result<Settings, String> {
    let last = {
        let conn = state.lock().map_err(|e| e.to_string())?;
        db::get_sync_meta(&conn, "last_full_sync").map_err(db_err)?
    };
    let data_dir = app.path().app_data_dir().ok().map(|p| p.to_string_lossy().into_owned());
    Ok(Settings { last_full_sync: last.and_then(|v| v.parse().ok()), data_dir })
}

#[tauri::command]
async fn save_api_key(key: String) -> Result<(), String> {
    tokio::task::block_in_place(|| keychain::set_key(key.trim()).map_err(|e| e.to_string()))
}

#[tauri::command]
async fn clear_api_key() -> Result<(), String> {
    tokio::task::block_in_place(|| keychain::clear_key().map_err(|e| e.to_string()))
}

/// 用输入框中的 key 临时测试：调一次 notebooks{count:1}，成功返回书本总数。
#[tauri::command]
async fn test_connection(key: String) -> Result<i64, String> {
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err("API Key 不能为空".into());
    }
    gateway::test_connection(&key)
        .await
        .map_err(|e| {
            if e.is_auth() {
                e.to_string()
            } else {
                format!("{e}")
            }
        })
}


// ---------- sync ----------

#[tauri::command]
async fn sync_all(on_progress: tauri::ipc::Channel<SyncEvent>, state: State<'_, Db>) -> Result<sync::SyncSummary, String> {
    let api_key = match keychain::get_key() {
        Ok(k) => k,
        Err(keychain::KeyError::NotFound) => {
            return Err("尚未保存 API Key，请先到设置页填写并保存".into())
        }
        Err(e) => return Err(format!("读取 API Key 失败：{e}（可到设置页重新保存）")),
    };
    // State<'_, Db> 即 &Mutex<Connection>；run_sync 每次锁短临界区，跨 await 持引用安全
    // （State 由 Tauri 管理，生命周期覆盖整个命令调用）。
    let conn = state.inner();
    sync::run_sync(conn, &api_key, on_progress).await
}



// ---------- books ----------

#[tauri::command]
fn list_books(state: State<'_, Db>) -> Result<Vec<BookRow>, String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    db::list_books(&conn).map_err(db_err)
}

#[tauri::command]
fn set_book_sync_reviews(state: State<'_, Db>, book_id: i64, enabled: bool) -> Result<(), String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    db::set_book_sync_reviews(&conn, book_id, enabled).map_err(db_err)
}

// ---------- cards ----------

#[tauri::command]
fn query_cards(state: State<'_, Db>, filter: CardFilter, limit: Option<i64>, offset: Option<i64>) -> Result<Vec<CardRow>, String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    db::query_cards(&conn, &filter, limit.unwrap_or(500), offset.unwrap_or(0)).map_err(db_err)
}

#[tauri::command]
fn count_cards(state: State<'_, Db>, filter: CardFilter) -> Result<i64, String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    db::count_cards(&conn, &filter).map_err(db_err)
}

#[tauri::command]
fn search_cards(state: State<'_, Db>, q: String, filter: CardFilter) -> Result<Vec<CardRow>, String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    db::search_cards(&conn, &q, &filter, 200).map_err(db_err)
}

#[tauri::command]
fn create_card(state: State<'_, Db>, text: String, tag_names: Vec<String>) -> Result<CardRow, String> {
    let mut conn = state.lock().map_err(|e| e.to_string())?;
    let now = now_secs();
    let id = {
        let tx = conn.transaction().map_err(db_err)?;
        let card_id = db::create_self_card(&tx, text.trim(), now).map_err(db_err)?;
        for tag in &tag_names {
            if !tag.trim().is_empty() {
                db::add_tag_to_card(&tx, card_id, tag.trim()).map_err(db_err)?;
            }
        }
        tx.commit().map_err(db_err)?;
        card_id
    };
    let filter = CardFilter::default();
    // 单卡回读以返回完整行（含标签/书名）
    let all = db::query_cards(&conn, &filter, 1_000_000, 0).map_err(db_err)?;
    all.into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| "卡片创建后读取失败".into())
}

#[tauri::command]
fn update_card(state: State<'_, Db>, id: i64, note: Option<String>, text: Option<String>) -> Result<(), String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    let now = now_secs();
    if let Some(n) = note {
        db::update_card_note(&conn, id, &n, now).map_err(db_err)?;
    }
    if let Some(t) = text {
        db::update_card_text(&conn, id, &t, now).map_err(db_err)?;
    }
    Ok(())
}

#[tauri::command]
fn toggle_starred(state: State<'_, Db>, id: i64, starred: bool) -> Result<(), String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    db::set_starred(&conn, id, starred).map_err(db_err)
}

#[tauri::command]
fn set_excluded_from_review(state: State<'_, Db>, id: i64, excluded: bool) -> Result<(), String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    db::set_excluded_from_review(&conn, id, excluded).map_err(db_err)
}

#[tauri::command]
fn delete_card(state: State<'_, Db>, id: i64) -> Result<(), String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    // self 卡硬删；同步来的划线/想法软删（下次同步若仍存在会复活）
    let kind: String = conn
        .query_row("SELECT kind FROM cards WHERE id=?1", [id], |r| r.get(0))
        .map_err(db_err)?;
    if kind == "self" {
        db::hard_delete_card(&conn, id).map_err(db_err)
    } else {
        db::soft_delete_card(&conn, id).map_err(db_err)
    }
}

// ---------- tags ----------

#[tauri::command]
fn list_tags(state: State<'_, Db>) -> Result<Vec<TagRow>, String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    db::list_tags(&conn).map_err(db_err)
}

#[tauri::command]
fn add_tag(state: State<'_, Db>, card_id: i64, name: String) -> Result<(), String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    db::add_tag_to_card(&conn, card_id, name.trim()).map_err(db_err)?;
    Ok(())
}

#[tauri::command]
fn remove_tag(state: State<'_, Db>, card_id: i64, name: String) -> Result<(), String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    db::remove_tag_from_card(&conn, card_id, name.trim()).map_err(db_err)
}

#[tauri::command]
fn delete_tag(state: State<'_, Db>, tag_id: i64) -> Result<(), String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    db::delete_tag(&conn, tag_id).map_err(db_err)
}

// ---------- review (SRS) ----------

#[tauri::command]
fn get_due_cards(state: State<'_, Db>, limit: Option<i64>) -> Result<Vec<CardRow>, String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    db::due_cards(&conn, now_secs(), limit.unwrap_or(30)).map_err(db_err)
}

#[tauri::command]
fn get_due_count(state: State<'_, Db>) -> Result<i64, String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    db::due_count(&conn, now_secs()).map_err(db_err)
}

#[tauri::command]
fn grade_review(state: State<'_, Db>, card_id: i64, rating: Rating) -> Result<SrsState, String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    let now = now_secs();
    let current = db::load_review_state(&conn, card_id).map_err(db_err)?.unwrap_or_default();
    let next = srs::schedule(&current, rating, now);
    db::save_review_state(&conn, card_id, &next).map_err(db_err)?;
    Ok(next)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let conn = init_db(app.handle()).map_err(std::io::Error::other)?;
            app.manage::<Db>(conn);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_setup_status,
            get_settings,
            save_api_key,
            clear_api_key,
            test_connection,
            sync_all,
            list_books,
            set_book_sync_reviews,
            query_cards,
            count_cards,
            search_cards,
            create_card,
            update_card,
            toggle_starred,
            set_excluded_from_review,
            delete_card,
            list_tags,
            add_tag,
            remove_tag,
            delete_tag,
            get_due_cards,
            grade_review,
            get_due_count,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
