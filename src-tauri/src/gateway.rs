//! 微信读书官方 Agent Gateway HTTP 客户端。
//!
//! 契约要点（docs/weread-skills.md）：
//! - POST https://i.weread.qq.com/api/agent/gateway
//! - 业务参数平铺在 body 顶层，与 `api_name`、`skill_version` 同级
//! - 禁止 params 包裹；禁止 offset/limit
//! - `/review/list/mine` 的参数名是小写 `bookid`（不是 bookId）

use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const GATEWAY_URL: &str = "https://i.weread.qq.com/api/agent/gateway";
pub const SKILL_VERSION: &str = "1.0.4";
const THROTTLE: Duration = Duration::from_millis(300);

#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("API Key 无效或已过期")]
    Unauthorized,
    #[error("网关错误 {status}: {body}")]
    Http { status: u16, body: String },
    #[error("网络错误: {0}")]
    Network(String),
}

impl GatewayError {
    pub fn is_auth(&self) -> bool {
        matches!(self, GatewayError::Unauthorized)
    }
}

pub type GatewayResult<T, E = GatewayError> = std::result::Result<T, E>;

pub fn client() -> GatewayResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| GatewayError::Network(e.to_string()))
}

async fn call<T: for<'de> Deserialize<'de>>(
    http: &reqwest::Client,
    api_key: &str,
    mut body: serde_json::Value,
) -> GatewayResult<T> {
    // 平铺约束：skill_version 与业务参数、api_name 同级
    let obj = body
        .as_object_mut()
        .ok_or_else(|| GatewayError::Network("request body must be an object".into()))?;
    obj.insert("skill_version".into(), serde_json::Value::String(SKILL_VERSION.into()));
    debug_assert!(obj.contains_key("api_name"), "every request must carry api_name");

    let resp = http
        .post(GATEWAY_URL)
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&body)
        .send()
        .await
        .map_err(|e| GatewayError::Network(e.to_string()))?;

    let status = resp.status().as_u16();
    let text = resp.text().await.map_err(|e| GatewayError::Network(e.to_string()))?;
    if status == 401 || status == 403 {
        return Err(GatewayError::Unauthorized);
    }
    if !(200..300).contains(&status) {
        return Err(GatewayError::Http { status, body: truncate(&text, 200) });
    }
    // 鉴权失败可能藏在 200 + code 字段里
    let parsed: serde_json::Value = serde_json::from_str(&text).map_err(|e| GatewayError::Http {
        status,
        body: truncate(&format!("非 JSON 响应: {e}; body={text}"), 200),
    })?;
    if let Some(code) = parsed.get("code").and_then(|v| v.as_i64()) {
        if code != 0 {
            return Err(classify_code_error(code, &parsed));
        }
    }
    serde_json::from_value(parsed).map_err(|e| GatewayError::Http {
        status,
        body: truncate(&format!("响应结构解析失败: {e}"), 200),
    })
}

fn classify_code_error(code: i64, parsed: &serde_json::Value) -> GatewayError {
    let msg = parsed
        .get("msg")
        .or_else(|| parsed.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if code == 401 || msg.contains("auth") || msg.contains("token") || msg.contains("key") {
        GatewayError::Unauthorized
    } else {
        GatewayError::Http { status: 200, body: truncate(msg, 200) }
    }
}

fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

// ---------- /user/notebooks ----------

/// 书店分类。实测回包 book.categories = [{categoryId, subCategoryId, categoryType, title}]，
/// title 是「大类-子类」合并串（如「经济理财-财经」）；其余字段不需要，serde 默认忽略。
/// 实测证据见 docs/research/book-visibility-api-probe.md。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BookCategory {
    #[serde(default)]
    pub title: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BookInfo {
    #[serde(rename = "bookId")]
    pub book_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub cover: String,
    #[serde(default)]
    pub categories: Vec<BookCategory>,
}

/// 计划契约导出类型：NotebookBook { book, note_count, review_count, sort }（字段名全小写下划线）
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotebookBook {
    pub book: BookInfo,
    #[serde(rename = "noteCount", default)]
    pub note_count: i64,
    #[serde(rename = "reviewCount", default)]
    pub review_count: i64,
    #[serde(default)]
    pub sort: i64,
    /// 阅读进度百分比（0–100）。实测回包有值；契约文档遗漏，勿按文档以为不存在。
    #[serde(rename = "readingProgress", default)]
    pub reading_progress: i64,
}

impl NotebookBook {
    /// 首分类标题；无分类返回空串。多分类取第一个（产品决策：只做元数据展示）。
    pub fn first_category(&self) -> &str {
        self.book.categories.first().map(|c| c.title.as_str()).unwrap_or("")
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotebooksResp {
    #[serde(rename = "totalBookCount", default)]
    pub total_book_count: i64,
    #[serde(default)]
    pub books: Vec<NotebookBook>,
}

/// hasMore===1 表示还有下一页
fn has_more(raw: &serde_json::Value) -> bool {
    raw.get("hasMore").and_then(|v| v.as_i64()).unwrap_or(0) == 1
}

/// 分页拉取全部笔记本概览。串行 + 300ms 节流，游标为上页最后一本的 sort。
pub async fn fetch_notebooks(
    http: &reqwest::Client,
    api_key: &str,
    count: i64,
) -> GatewayResult<Vec<NotebookBook>> {
    let mut all: Vec<NotebookBook> = Vec::new();
    let mut last_sort: Option<i64> = None;
    loop {
        let raw: serde_json::Value = call(
            http,
            api_key,
            match last_sort {
                Some(ls) => serde_json::json!({ "api_name": "/user/notebooks", "count": count, "lastSort": ls }),
                None => serde_json::json!({ "api_name": "/user/notebooks", "count": count }),
            },
        )
        .await?;
        // hasMore 判断走原始 JSON（结构体不带 hasMore 的页也安全）
        let more = has_more(&raw);
        let resp: NotebooksResp = serde_json::from_value(raw.clone()).map_err(|e| {
            GatewayError::Http { status: 200, body: truncate(&format!("notebooks 解析失败: {e}"), 200) }
        })?;
        let next_sort = resp.books.last().map(|b| b.sort);
        all.extend(resp.books);
        if !more {
            break;
        }
        match next_sort {
            Some(s) => last_sort = Some(s),
            None => break, // 空页但 hasMore=1：防御性退出避免死循环
        }
        tokio::time::sleep(THROTTLE).await;
    }
    Ok(all)
}

/// 测试连接：拉一页 notebook 概览，返回 totalBookCount。
pub async fn test_connection(api_key: &str) -> GatewayResult<i64> {
    let http = client()?;
    let raw: serde_json::Value = call(
        &http,
        api_key,
        serde_json::json!({ "api_name": "/user/notebooks", "count": 1 }),
    )
    .await?;
    let resp: NotebooksResp = serde_json::from_value(raw).map_err(|e| GatewayError::Http {
        status: 200,
        body: truncate(&format!("响应解析失败: {e}"), 200),
    })?;
    Ok(resp.total_book_count)
}

// ---------- /book/bookmarklist ----------

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BookmarkItem {
    #[serde(rename = "bookmarkId")]
    pub bookmark_id: String,
    #[serde(rename = "bookId", default)]
    pub book_id: String,
    #[serde(rename = "chapterUid", default)]
    pub chapter_uid: i64,
    #[serde(rename = "markText", default)]
    pub mark_text: String,
    #[serde(rename = "createTime", default)]
    pub create_time: i64,
    #[serde(rename = "range", default)]
    pub range: String,
    #[serde(rename = "colorStyle", default)]
    pub color_style: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChapterInfo {
    #[serde(rename = "chapterUid", default)]
    pub chapter_uid: i64,
    #[serde(rename = "chapterIdx", default)]
    pub chapter_idx: i64,
    #[serde(default)]
    pub title: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BookmarkListResp {
    #[serde(default)]
    pub updated: Vec<BookmarkItem>,
    #[serde(default)]
    pub chapters: Vec<ChapterInfo>,
}

/// 单本书全部划线。无分页。
pub async fn fetch_bookmarks(
    http: &reqwest::Client,
    api_key: &str,
    book_id: &str,
) -> GatewayResult<BookmarkListResp> {
    call(http, api_key, serde_json::json!({
        "api_name": "/book/bookmarklist",
        "bookId": book_id,
    }))
    .await
}

// ---------- /review/list/mine ----------


#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ReviewDetail {
    #[serde(rename = "reviewId", default)]
    pub review_id: String,
    #[serde(default)]
    pub content: String,
    /// 想法对应的划线原文；仅能定位原文的划线想法有值
    #[serde(rename = "abstract", default)]
    pub abstract_text: String,
    #[serde(default)]
    pub range: String,
    #[serde(rename = "chapterUid", default)]
    pub chapter_uid: i64,
    #[serde(rename = "chapterIdx", default)]
    pub chapter_idx: i64,
    #[serde(rename = "createTime", default)]
    pub create_time: i64,
    #[serde(default)]
    pub star: i64,
    #[serde(rename = "chapterName", default)]
    pub chapter_name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReviewWrapper {
    #[serde(default)]
    pub review: ReviewDetail,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReviewListResp {
    #[serde(default)]
    pub reviews: Vec<ReviewWrapper>,
    #[serde(default)]
    pub has_more: i64,
    #[serde(default)]
    pub synckey: i64,
}


/// 注意参数名是小写 bookid！
pub async fn fetch_reviews_all(
    http: &reqwest::Client,
    api_key: &str,
    bookid: &str,
) -> GatewayResult<Vec<ReviewWrapper>> {
    let mut all: Vec<ReviewWrapper> = Vec::new();
    let mut synckey: i64 = 0;
    loop {
        let body = serde_json::json!({
            "api_name": "/review/list/mine",
            "bookid": bookid, // 小写 d —— 文档明示陷阱
            "synckey": synckey,
        });
        let raw: serde_json::Value = call(http, api_key, body).await?;
        let more = has_more(&raw);
        let resp: ReviewListResp = serde_json::from_value(raw.clone()).map_err(|e| {
            GatewayError::Http { status: 200, body: truncate(&format!("reviews 解析失败: {e}"), 200) }
        })?;
        let next_synckey = resp.synckey;
        all.extend(resp.reviews);
        if !more {
            break;
        }
        if next_synckey == synckey && synckey != 0 {
            break; // 游标未推进：防御性退出
        }
        synckey = next_synckey;
        tokio::time::sleep(THROTTLE).await;
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_notebooks_fixture() {
        let j = r#"{
            "totalBookCount": 2,
            "hasMore": 0,
            "books": [
                {"book": {"bookId": "3300067488", "title": "置身事内", "author": "兰小欢", "cover": "https://x/y.jpg",
                          "categories": [{"categoryId": 1100000, "subCategoryId": 1100001, "categoryType": 0, "title": "经济理财-财经"}]},
                 "noteCount": 120, "reviewCount": 5, "sort": 1778312777, "readingProgress": 64},
                {"book": {"bookId": "912345"}, "noteCount": 3, "reviewCount": 0}
            ]
        }"#;
        let v: NotebooksResp = serde_json::from_str(j).expect("fixture parse");
        assert_eq!(v.total_book_count, 2);
        assert_eq!(v.books.len(), 2);
        assert_eq!(v.books[0].book.title, "置身事内");
        assert_eq!(v.books[0].note_count, 120);
        assert_eq!(v.books[0].sort, 1778312777);
        assert_eq!(v.books[0].reading_progress, 64, "实测回包含阅读进度百分比");
        assert_eq!(v.books[0].first_category(), "经济理财-财经");
        // 无分类的书：categories 缺省为空，first_category 是空串
        assert_eq!(v.books[1].sort, 0);
        assert_eq!(v.books[1].reading_progress, 0);
        assert_eq!(v.books[1].first_category(), "");
    }

    #[test]
    fn parse_bookmarklist_fixture() {
        let j = r#"{
            "updated": [{
                "bookmarkId": "bm-1", "bookId": "3300067488", "chapterUid": 11,
                "markText": "政府直接参与了经济活动", "createTime": 1700000000,
                "type": 1, "range": "2959-3007", "colorStyle": 1
            }],
            "chapters": [{"chapterUid": 11, "chapterIdx": 2, "title": "第二章 土地与融资"}]
        }"#;
        let v: BookmarkListResp = serde_json::from_str(j).expect("fixture parse");
        assert_eq!(v.updated.len(), 1);
        assert_eq!(v.updated[0].mark_text, "政府直接参与了经济活动");
        assert_eq!(v.chapters[0].title, "第二章 土地与融资");
    }

    #[test]
    fn parse_review_mine_fixture() {
        let j = r#"{
            "totalCount": 2, "hasMore": 1, "synckey": 42,
            "reviews": [
                {"review": {
                    "reviewId": "rv-1", "content": "这一段对我理解城投债很关键",
                    "abstract": "政府直接参与了经济活动", "range": "2959-3007",
                    "chapterUid": 11, "chapterIdx": 2, "createTime": 1700100000,
                    "star": -1, "chapterName": ""
                }},
                {"review": {"reviewId": "rv-2", "content": "整本书评", "chapterUid": 0}}
            ]
        }"#;
        let v: ReviewListResp = serde_json::from_str(j).expect("fixture parse");
        assert_eq!(v.reviews.len(), 2);
        assert_eq!(v.synckey, 42);
        assert_eq!(v.reviews[0].review.abstract_text, "政府直接参与了经济活动");
        assert_eq!(v.reviews[1].review.content, "整本书评");
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let j = r#"{"review": {"reviewId": "rv-x", "futureField": {"a": 1}, "anotherNew": 7}}"#;
        let v: ReviewWrapper = serde_json::from_str(j).expect("unknown ignored");
        assert_eq!(v.review.review_id, "rv-x");
    }
}
