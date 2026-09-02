pub mod db;
mod gateway;
mod keystore;
mod llm;
pub mod srs;
mod sync;

use std::path::PathBuf;
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

fn data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|e| format!("数据目录解析失败: {e}"))
}

fn init_db(app: &tauri::AppHandle) -> Result<Mutex<rusqlite::Connection>, String> {
    let dir = data_dir(app)?;
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
fn get_setup_status(app: tauri::AppHandle, state: State<'_, Db>) -> Result<SetupStatus, String> {
    let has_books = {
        let conn = state.lock().map_err(|e| e.to_string())?;
        conn.query_row("SELECT COUNT(*) FROM books", [], |r| r.get::<_, i64>(0))
            .map_err(db_err)?
            > 0
    };
    let has_key = keystore::has_key(&data_dir(&app)?);
    Ok(SetupStatus { has_key, has_books })
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
async fn save_api_key(app: tauri::AppHandle, key: String) -> Result<(), String> {
    let dir = data_dir(&app)?;
    tokio::task::block_in_place(|| keystore::set_key(&dir, key.trim()).map_err(|e| e.to_string()))
}

#[tauri::command]
async fn clear_api_key(app: tauri::AppHandle) -> Result<(), String> {
    let dir = data_dir(&app)?;
    tokio::task::block_in_place(|| keystore::clear_key(&dir).map_err(|e| e.to_string()))
}

// ---------- LLM provider (BYOK) ----------

#[tauri::command]
fn get_llm_settings(app: tauri::AppHandle) -> Result<llm::LlmSettings, String> {
    let dir = data_dir(&app)?;
    llm::load_settings(&dir).map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_llm_settings(app: tauri::AppHandle, draft: llm::LlmDraft) -> Result<llm::LlmSettings, String> {
    let dir = data_dir(&app)?;
    tokio::task::block_in_place(|| {
        let existing = llm::get_key(&dir).ok();
        llm::save(&dir, &draft, existing.as_deref()).map_err(|e| e.to_string())
    })
}

#[tauri::command]
async fn clear_llm_settings(app: tauri::AppHandle) -> Result<(), String> {
    let dir = data_dir(&app)?;
    tokio::task::block_in_place(|| llm::clear(&dir).map_err(|e| e.to_string()))
}

/// 用 OpenAI 兼容的 GET /models 探测供应商。不落盘。
#[tauri::command]
async fn test_llm_connection(app: tauri::AppHandle, draft: llm::LlmDraft) -> Result<String, String> {
    let dir = data_dir(&app)?;
    let existing = llm::get_key(&dir).ok();
    let normalized = llm::normalize_draft(&draft, existing.as_deref()).map_err(|e| e.to_string())?;
    if normalized.config.provider == llm::Provider::Off {
        return Err("请先选择供应商".into());
    }
    let key = normalized
        .key
        .as_deref()
        .or(existing.as_deref())
        .unwrap_or("");
    let url = llm::models_url(&normalized.config.base_url);
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("无法创建连接: {e}"))?;
    let mut req = http.get(&url);
    if !key.is_empty() {
        req = req.bearer_auth(key);
    }
    let resp = req.send().await.map_err(|e| format!("连不上供应商: {e}"))?;
    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err("API Key 无效或没有权限".into());
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        let snippet: String = body.chars().take(120).collect();
        return Err(format!("供应商返回 {status}: {snippet}"));
    }
    let wanted = normalized.config.model;
    let parsed: serde_json::Value = resp.json().await.unwrap_or(serde_json::Value::Null);
    let names = parse_model_ids(&parsed);
    if names.is_empty() {
        return Ok(format!("已接通 {wanted}（未返回模型列表）"));
    }
    if names.iter().any(|n| n == &wanted) {
        return Ok(format!("连接成功：已找到模型 {wanted}"));
    }
    Ok(format!(
        "已接通，但列表里没有「{wanted}」。可用 {} 个模型，请核对模型名。",
        names.len()
    ))
}

fn parse_model_ids(v: &serde_json::Value) -> Vec<String> {
    v.get("data")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("id").and_then(|id| id.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
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

/// 在系统默认浏览器里打开外部链接（如设置页的微信读书 Skills 开通页）。
/// 只放行 http/https 且不含空白/控制字符的 URL；子进程由独立线程收尸，不阻塞命令。
#[tauri::command]
fn open_external(url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("仅支持打开 http/https 链接".into());
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("URL 含非法字符".into());
    }
    match open_with_system_browser(&url).spawn() {
        Ok(mut child) => {
            // wait 交给独立线程，避免留下僵尸进程；open/xdg-open 都会立即返回。
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            Ok(())
        }
        Err(e) => Err(format!("打开浏览器失败: {e}")),
    }
}

#[cfg(target_os = "macos")]
fn open_with_system_browser(url: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new("open");
    cmd.arg(url);
    cmd
}

#[cfg(target_os = "linux")]
fn open_with_system_browser(url: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new("xdg-open");
    cmd.arg(url);
    cmd
}

#[cfg(target_os = "windows")]
fn open_with_system_browser(url: &str) -> std::process::Command {
    // explorer.exe 会把 URL 交给默认浏览器，且不经过 cmd 转义。
    let mut cmd = std::process::Command::new("explorer");
    cmd.arg(url);
    cmd
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn open_with_system_browser(_url: &str) -> std::process::Command {
    panic!("当前平台不支持打开外部浏览器");
}


// ---------- sync ----------

#[tauri::command]
async fn sync_all(app: tauri::AppHandle, on_progress: tauri::ipc::Channel<SyncEvent>, state: State<'_, Db>) -> Result<sync::SyncSummary, String> {
    let dir = data_dir(&app)?;
    let api_key = match keystore::get_key(&dir) {
        Ok(k) => k,
        Err(keystore::KeyError::NotFound) => {
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
    // 恢复（excluded=false）时立即回到待回顾状态（due_at=now，R2）
    db::set_excluded_from_review(&conn, id, excluded, now_secs()).map_err(db_err)
}

/// 删除语义（R4）：自建卡物理删除；同步卡写入「用户本地隐藏」墓碑，
/// 之后同步也不会复活。确认文案由前端按卡别区分。
#[tauri::command]
fn delete_card(state: State<'_, Db>, id: i64) -> Result<(), String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    let kind: String = conn
        .query_row("SELECT kind FROM cards WHERE id=?1", [id], |r| r.get(0))
        .map_err(db_err)?;
    if kind == "self" {
        db::hard_delete_card(&conn, id).map_err(db_err)
    } else {
        db::hide_card_from_user(&conn, id).map_err(db_err)
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
fn get_due_cards(state: State<'_, Db>, limit: Option<i64>, book_id: Option<i64>) -> Result<Vec<CardRow>, String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    db::due_cards(&conn, now_secs(), limit.unwrap_or(30), book_id).map_err(db_err)
}

#[tauri::command]
fn get_due_count(state: State<'_, Db>, book_id: Option<i64>) -> Result<i64, String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    db::due_count(&conn, now_secs(), book_id).map_err(db_err)
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

// ---------- review settings (R3) ----------

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSettings {
    pub batch_size: i64,
}

#[tauri::command]
fn get_review_settings(state: State<'_, Db>) -> Result<ReviewSettings, String> {
    let conn = state.lock().map_err(|e| e.to_string())?;
    let size = db::get_sync_meta(&conn, db::KEY_REVIEW_BATCH)
        .map_err(db_err)?
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|v| db::REVIEW_BATCH_OPTIONS.contains(v))
        .unwrap_or(db::DEFAULT_REVIEW_BATCH);
    Ok(ReviewSettings { batch_size: size })
}

#[tauri::command]
fn set_review_batch_size(state: State<'_, Db>, size: i64) -> Result<(), String> {
    if !db::REVIEW_BATCH_OPTIONS.contains(&size) {
        return Err("每批张数仅支持 10 / 20 / 30".into());
    }
    let conn = state.lock().map_err(|e| e.to_string())?;
    db::set_sync_meta(&conn, db::KEY_REVIEW_BATCH, &size.to_string()).map_err(db_err)
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
            get_llm_settings,
            save_llm_settings,
            clear_llm_settings,
            test_llm_connection,
            test_connection,
            open_external,
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
            get_review_settings,
            set_review_batch_size,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
