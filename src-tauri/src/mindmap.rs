//! 按书划线生成概要脑图：模型只输出主题节点，划线挂在 source_card_ids 上。
//! 校验/清洗是纯函数，生成失败不得把单卡画成叶子。

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::db::{self, CardFilter, CardRow};
use crate::llm;

pub const PROMPT_VERSION: &str = "mindmap-theme-v1";
const LABEL_MIN: usize = 8;
const LABEL_MAX: usize = 24;
const SUMMARY_MAX: usize = 40;
const MAX_TOP: usize = 12;
const MAX_SECONDARY: usize = 3;
const MIN_EVIDENCE: usize = 2;
const CARD_LINE_CHARS: usize = 72;
const ONESHOT_LIMIT: usize = 40;

#[derive(Debug, thiserror::Error)]
pub enum MindmapError {
    #[error("{0}")]
    Msg(String),
}

pub type MindmapResult<T> = Result<T, MindmapError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Mindmap {
    pub book_id: i64,
    pub title: String,
    pub mode: String,
    pub input_hash: String,
    pub prompt_version: String,
    pub stats: MindmapStats,
    pub root: MindmapNode,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MindmapStats {
    pub cards: i64,
    pub chapters: i64,
    pub themes: i64,
    pub unplaced: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MindmapNode {
    pub id: String,
    pub label: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, alias = "source_card_ids")]
    pub source_card_ids: Vec<i64>,
    #[serde(default)]
    pub children: Vec<MindmapNode>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MindmapStatus {
    pub available: bool,
    pub provider_off: bool,
    pub card_count: i64,
    pub cached: Option<Mindmap>,
    pub stale: bool,
    /// 生成时会 POST 的地址；未启用时为空。
    #[serde(default)]
    pub chat_endpoint: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

pub fn input_hash(cards: &[CardRow]) -> String {
    let mut parts: Vec<String> = cards
        .iter()
        .map(|c| format!("{}:{}", c.id, c.updated_at))
        .collect();
    parts.sort();
    parts.push(PROMPT_VERSION.into());
    format!("v1:{}", simple_hash(&parts.join("|")))
}

fn simple_hash(s: &str) -> String {
    // 稳定、短、非密码学：只用于缓存失效。
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{h:016x}")
}

pub fn pack_card_line(c: &CardRow) -> String {
    let chapter = c.chapter_title.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or("未分章");
    let kind = match c.kind.as_str() {
        "thought" => "想法",
        "self" => "自建",
        _ => "划线",
    };
    let star = if c.starred { " [星]" } else { "" };
    let text = truncate_chars(&c.text, CARD_LINE_CHARS);
    let mut line = format!("#{} [章:{}] [{}]{} {}", c.id, chapter, kind, star, text);
    let note = c.note.trim();
    if !note.is_empty() {
        line.push_str(" | 注: ");
        line.push_str(&truncate_chars(note, 40));
    }
    line
}

fn truncate_chars(s: &str, max: usize) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= max {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

fn char_len(s: &str) -> usize {
    s.chars().count()
}

fn prefix_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// 丢掉非法节点、不足证据的主题、原文截断当标题的节点。
pub fn sanitize(
    mut tree: Mindmap,
    cards: &[CardRow],
    book_id: i64,
    title: &str,
    hash: &str,
) -> MindmapResult<Mindmap> {
    let by_id: HashMap<i64, &CardRow> = cards.iter().map(|c| (c.id, c)).collect();
    let allowed: HashSet<i64> = by_id.keys().copied().collect();
    let mut warnings = tree.warnings;

    tree.root.kind = "book".into();
    tree.root.label = format!("{title} · 我的划线");
    tree.root.source_card_ids.clear();
    tree.root.children = sanitize_themes(
        std::mem::take(&mut tree.root.children),
        &by_id,
        &allowed,
        &mut warnings,
        1,
    );

    if tree.root.children.len() > MAX_TOP {
        tree.root.children.truncate(MAX_TOP);
        warnings.push(format!("一级概要超过 {MAX_TOP} 个，已截断"));
    }

    let assigned: HashSet<i64> = tree.root.children.iter().flat_map(collect_ids).collect();
    let unplaced: HashSet<i64> = allowed.difference(&assigned).copied().collect();

    if tree.root.children.is_empty() {
        return Err(MindmapError::Msg("没有得到可用的概要节点。请重试，或换一个模型。".into()));
    }

    let chapters = cards
        .iter()
        .filter_map(|c| c.chapter_uid)
        .collect::<HashSet<_>>()
        .len() as i64;

    tree.book_id = book_id;
    tree.title = title.to_string();
    tree.mode = "theme".into();
    tree.input_hash = hash.to_string();
    tree.prompt_version = PROMPT_VERSION.into();
    tree.stats = MindmapStats {
        cards: cards.len() as i64,
        chapters,
        themes: count_themes(&tree.root) as i64,
        unplaced: unplaced.len() as i64,
    };
    if !unplaced.is_empty() {
        warnings.push(format!("有 {} 张未归入主题", unplaced.len()));
    }
    tree.warnings = warnings;
    Ok(tree)
}

fn sanitize_themes(
    nodes: Vec<MindmapNode>,
    by_id: &HashMap<i64, &CardRow>,
    allowed: &HashSet<i64>,
    warnings: &mut Vec<String>,
    depth: usize,
) -> Vec<MindmapNode> {
    let cap = if depth <= 1 { MAX_TOP } else { MAX_SECONDARY };
    let mut out = Vec::new();
    for mut n in nodes {
        if out.len() >= cap {
            continue;
        }
        if n.kind == "card" {
            warnings.push("已丢掉卡片叶子，划线不能当节点".into());
            continue;
        }
        n.kind = "theme".into();
        n.label = n.label.trim().to_string();
        if let Some(s) = n.summary.as_mut() {
            let t = s.trim().to_string();
            if char_len(&t) > SUMMARY_MAX {
                *s = prefix_chars(&t, SUMMARY_MAX);
            } else if t.is_empty() {
                n.summary = None;
            } else {
                *s = t;
            }
        }
        let mut ids: Vec<i64> = Vec::new();
        for id in n.source_card_ids.iter().chain(collect_ids_ref(&n.children).iter()) {
            if allowed.contains(id) && !ids.contains(id) {
                ids.push(*id);
            }
        }
        n.children = if depth >= 2 {
            n.children.clear();
            Vec::new()
        } else {
            sanitize_themes(n.children, by_id, allowed, warnings, depth + 1)
        };
        for child in &n.children {
            for id in &child.source_card_ids {
                if !ids.contains(id) {
                    ids.push(*id);
                }
            }
        }
        n.source_card_ids = ids;
        if n.source_card_ids.len() < MIN_EVIDENCE {
            continue;
        }
        let label_len = char_len(&n.label);
        if label_len < LABEL_MIN || label_len > LABEL_MAX {
            warnings.push(format!("丢掉标题长度不合规的节点「{}」", n.label));
            continue;
        }
        if is_excerpt(&n.label, &n.source_card_ids, by_id) {
            warnings.push(format!("丢掉原文截断节点「{}」", n.label));
            continue;
        }
        out.push(n);
    }
    out
}

fn is_excerpt(label: &str, ids: &[i64], by_id: &HashMap<i64, &CardRow>) -> bool {
    let needle = normalize_cmp(label);
    if needle.is_empty() {
        return true;
    }
    for id in ids {
        let Some(c) = by_id.get(id) else { continue };
        let prefix = normalize_cmp(&prefix_chars(c.text.trim(), LABEL_MAX));
        if !prefix.is_empty() && (needle == prefix || prefix.starts_with(&needle) || needle.starts_with(&prefix)) {
            return true;
        }
        let note = c.note.trim();
        if !note.is_empty() {
            let np = normalize_cmp(&prefix_chars(note, LABEL_MAX));
            if needle == np {
                return true;
            }
        }
    }
    false
}

fn normalize_cmp(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace() && *c != '…' && *c != '.' && *c != '。').collect()
}

fn collect_ids(n: &MindmapNode) -> Vec<i64> {
    let mut ids = n.source_card_ids.clone();
    for c in &n.children {
        ids.extend(collect_ids(c));
    }
    ids
}

fn collect_ids_ref(nodes: &[MindmapNode]) -> Vec<i64> {
    nodes.iter().flat_map(collect_ids).collect()
}

fn count_themes(n: &MindmapNode) -> usize {
    let self_n = if n.kind == "theme" { 1 } else { 0 };
    self_n + n.children.iter().map(count_themes).sum::<usize>()
}

pub fn system_prompt() -> &'static str {
    "你是编辑，不是作者。只根据用户提供的划线/想法归纳一张主题脑图。\n\
规则：\n\
1. 输出一个 JSON 对象，不要 Markdown。字段：root（节点）。\n\
2. 节点 kind 只能是 book 或 theme。禁止 kind=card。划线只能出现在 sourceCardIds。\n\
3. 根节点 kind=book，children 是 一级主题（5 到 12 个，卡少则可更少）。\n\
4. 每个 theme 的 label 是 8 到 24 个字的归纳句，不是摘录截断，不能与任何划线原文前 24 字相同。\n\
5. 每个 theme 至少 2 个 sourceCardIds（整数，来自输入的 #id）。\n\
6. 一级下最多 3 个二级 theme；不要第三层。\n\
7. 没出现的论点不要补。同义合并，冲突保留为两个主题或一个一级下的两个二级。\n\
8. summary 可选，不超过 40 字。"
}

pub fn user_prompt(title: &str, lines: &[String]) -> String {
    format!(
        "书名：{title}\n共 {} 条用户划线/想法。请压缩成主题脑图。\n\n{}",
        lines.len(),
        lines.join("\n")
    )
}

pub fn parse_model_json(raw: &str) -> MindmapResult<Mindmap> {
    let trimmed = raw.trim();
    let json = if let Some(start) = trimmed.find('{') {
        let end = trimmed.rfind('}').ok_or_else(|| MindmapError::Msg("模型没有返回 JSON 对象".into()))?;
        &trimmed[start..=end]
    } else {
        return Err(MindmapError::Msg("模型没有返回 JSON 对象".into()));
    };
    let mut v: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| MindmapError::Msg(format!("无法解析模型 JSON: {e}")))?;
    // 允许模型只返回 root，或缺顶层字段
    if v.get("root").is_none() {
        if v.get("children").is_some() || v.get("kind").is_some() {
            v = serde_json::json!({ "root": v });
        }
    }
    if v.get("title").is_none() {
        v["title"] = serde_json::json!("");
    }
    if v.get("bookId").is_none() {
        v["bookId"] = serde_json::json!(0);
    }
    if v.get("mode").is_none() {
        v["mode"] = serde_json::json!("theme");
    }
    if v.get("inputHash").is_none() {
        v["inputHash"] = serde_json::json!("");
    }
    if v.get("promptVersion").is_none() {
        v["promptVersion"] = serde_json::json!(PROMPT_VERSION);
    }
    if v.get("stats").is_none() {
        v["stats"] = serde_json::json!({ "cards": 0, "chapters": 0, "themes": 0, "unplaced": 0 });
    }
    serde_json::from_value(v).map_err(|e| MindmapError::Msg(format!("脑图结构不对: {e}")))
}

fn cache_path(dir: &Path, book_id: i64) -> std::path::PathBuf {
    dir.join("mindmaps").join(format!("{book_id}.json"))
}

pub fn load_cache(dir: &Path, book_id: i64) -> Option<Mindmap> {
    let raw = std::fs::read_to_string(cache_path(dir, book_id)).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn save_cache(dir: &Path, map: &Mindmap) -> MindmapResult<()> {
    let folder = dir.join("mindmaps");
    std::fs::create_dir_all(&folder).map_err(|e| MindmapError::Msg(e.to_string()))?;
    let path = cache_path(dir, map.book_id);
    let tmp = folder.join(format!("{}.tmp", map.book_id));
    let body = serde_json::to_vec_pretty(map).map_err(|e| MindmapError::Msg(e.to_string()))?;
    std::fs::write(&tmp, body).map_err(|e| MindmapError::Msg(e.to_string()))?;
    #[cfg(windows)]
    let _ = std::fs::remove_file(&path);
    std::fs::rename(&tmp, &path).map_err(|e| MindmapError::Msg(e.to_string()))?;
    Ok(())
}

pub fn load_book_cards(conn: &rusqlite::Connection, book_id: i64) -> rusqlite::Result<(db::BookRow, Vec<CardRow>)> {
    let books = db::list_books(conn)?;
    let book = books
        .into_iter()
        .find(|b| b.id == book_id)
        .ok_or_else(|| rusqlite::Error::QueryReturnedNoRows)?;
    let filter = CardFilter {
        book_id: Some(book_id),
        ..CardFilter::default()
    };
    let cards = db::query_cards(conn, &filter, 10_000, 0)?;
    Ok((book, cards))
}

pub fn status_for(dir: &Path, book_id: i64, cards: &[CardRow], provider_off: bool) -> MindmapStatus {
    let hash = input_hash(cards);
    let cached = load_cache(dir, book_id);
    let stale = cached.as_ref().map(|c| c.input_hash != hash).unwrap_or(false);
    let runtime = llm::load_runtime(dir).ok();
    MindmapStatus {
        available: !provider_off && cards.len() >= MIN_EVIDENCE,
        provider_off,
        card_count: cards.len() as i64,
        cached,
        stale,
        chat_endpoint: runtime.as_ref().map(|(cfg, _)| llm::chat_url(&cfg.base_url)),
        model: runtime.as_ref().map(|(cfg, _)| cfg.model.clone()),
    }
}

pub async fn generate(
    dir: &Path,
    book: &db::BookRow,
    cards: &[CardRow],
) -> MindmapResult<Mindmap> {
    if cards.len() < MIN_EVIDENCE {
        return Err(MindmapError::Msg("至少两张卡片才能归纳脑图。".into()));
    }
    let (cfg, key) = llm::load_runtime(dir).map_err(|e| MindmapError::Msg(e.to_string()))?;

    let hash = input_hash(cards);
    let packed = pack_for_prompt(cards);
    let raw = chat_complete(
        &cfg.base_url,
        &cfg.model,
        &key,
        system_prompt(),
        &user_prompt(&book.title, &packed),
    )
    .await?;
    let parsed = parse_model_json(&raw)?;
    let clean = sanitize(parsed, cards, book.id, &book.title, &hash)?;
    save_cache(dir, &clean)?;
    Ok(clean)
}

fn pack_for_prompt(cards: &[CardRow]) -> Vec<String> {
    // 一次请求只送高权重的一小撮，避免 OpenCode Go 把整书 POST 拖过超时。
    let mut scored: Vec<(i32, &CardRow)> = cards
        .iter()
        .map(|c| {
            let mut s = 0;
            if c.starred { s += 4; }
            if c.kind == "thought" { s += 3; }
            if !c.note.trim().is_empty() { s += 2; }
            (s, c)
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
    scored
        .into_iter()
        .take(ONESHOT_LIMIT)
        .map(|(_, c)| pack_card_line(c))
        .collect()
}

async fn chat_complete(
    base_url: &str,
    model: &str,
    key: &str,
    system: &str,
    user: &str,
) -> MindmapResult<String> {
    let url = llm::chat_url(base_url);
    let http = llm::http_client(Duration::from_secs(180)).map_err(MindmapError::Msg)?;
    let body = serde_json::json!({
        "model": model,
        "temperature": 0.2,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user }
        ]
    });
    let mut last_err = None;
    let mut resp = None;
    for attempt in 0..3u32 {
        let mut req = http.post(&url).json(&body);
        if !key.is_empty() {
            req = req.bearer_auth(key);
        }
        match req.send().await {
            Ok(r) => {
                resp = Some(r);
                break;
            }
            Err(e) => {
                let detail = llm::format_reqwest(e);
                let transient = detail.contains("reset by peer")
                    || detail.contains("os error 54")
                    || detail.contains("timed out")
                    || detail.contains("connection error");
                last_err = Some(detail);
                if !transient || attempt == 2 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(400 * (attempt as u64 + 1))).await;
            }
        }
    }
    let resp = match resp {
        Some(r) => r,
        None => {
            let detail = last_err.unwrap_or_else(|| "未知网络错误".into());
            return Err(MindmapError::Msg(if detail.contains("reset by peer") || detail.contains("os error 54") {
                format!("生成线索时连接被对端断开。请再试一次；若反复出现，换直连供应商。({detail})")
            } else if detail.contains("timed out") {
                "生成线索超时。请再试一次；若反复超时，换更快的模型。".into()
            } else {
                format!("生成线索失败。{detail}")
            }));
        }
    };
    let status = resp.status();
    let text = resp.text().await.map_err(|e| MindmapError::Msg(format!("读取响应失败: {e}")))?;
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(MindmapError::Msg("语言模型 API Key 无效或没有权限".into()));
    }
    if !status.is_success() {
        let snippet: String = text.chars().take(160).collect();
        return Err(MindmapError::Msg(format!("语言模型返回 {status}: {snippet}")));
    }
    let v: serde_json::Value = serde_json::from_str(&text)
        .map_err(|_| MindmapError::Msg("语言模型响应不是 JSON".into()))?;
    let content = v
        .pointer("/choices/0/message/content")
        .and_then(|c| c.as_str())
        .ok_or_else(|| MindmapError::Msg("语言模型没有返回内容".into()))?;
    Ok(content.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: i64, chapter: &str, kind: &str, text: &str, note: &str, starred: bool) -> CardRow {
        CardRow {
            id,
            kind: kind.into(),
            book_id: Some(1),
            remote_id: Some(format!("r{id}")),
            chapter_uid: Some(1),
            chapter_title: Some(chapter.into()),
            text: text.into(),
            abstract_text: None,
            range_str: None,
            color_style: 0,
            note: note.into(),
            starred,
            excluded_from_review: false,
            created_at: 1,
            updated_at: 10 + id,
            deleted: false,
            book_title: "原子习惯".into(),
            tags: vec![],
        }
    }

    fn sample_cards() -> Vec<CardRow> {
        vec![
            card(1, "习惯的复利", "highlight", "人不是拥有习惯，而是由习惯塑造。", "", false),
            card(2, "习惯的复利", "thought", "身份不是想出来的，是重复出来的。", "", false),
            card(3, "习惯的复利", "highlight", "每做一个 1% 的改进，你都在把自己变成那种人。", "", false),
            card(4, "习惯的复利", "highlight", "你高估了一天的意义，低估了两年的意义。", "", false),
            card(5, "如何建立习惯", "highlight", "让它显而易见、有吸引力、简便易行、令人愉悦。", "", false),
            card(6, "如何建立习惯", "highlight", "把新习惯叠在旧习惯后面。", "", false),
            card(7, "如何建立习惯", "highlight", "早饭后立刻写。", "我试过早饭后立刻写 20 分钟，比定闹钟有用。", false),
            card(8, "环境", "highlight", "环境是无形的手。", "", false),
            card(9, "环境", "highlight", "想多读书，就把书放在沙发上，把遥控器藏起来。", "", false),
            card(10, "环境", "highlight", "自控力是高估的，设计才是可重复的。", "", true),
            card(11, "坏习惯", "highlight", "让它看不见、没有吸引力、难以实行、令人不满。", "", false),
            card(12, "坏习惯", "highlight", "戒手机不是靠意志，是把充电器放到另一间房。", "", false),
            card(13, "坏习惯", "thought", "和 #9 是同一件事的反面。", "", false),
            card(14, "习惯的复利", "highlight", "你的结果是滞后指标，身份才是先行指标。", "", false),
            card(15, "如何建立习惯", "highlight", "两分钟规则：入门必须短到不可能失败。", "", false),
            card(16, "如何建立习惯", "highlight", "先出现，再优化。", "", false),
            card(17, "身份", "highlight", "不要以结果为目标，要以身份为目标。", "", false),
            card(18, "身份", "highlight", "「我是跑步的人」比「我要跑完马拉松」更稳。", "", false),
        ]
    }

    fn theme(id: &str, label: &str, ids: &[i64], children: Vec<MindmapNode>) -> MindmapNode {
        MindmapNode {
            id: id.into(),
            label: label.into(),
            kind: "theme".into(),
            summary: None,
            source_card_ids: ids.to_vec(),
            children,
        }
    }

    fn raw_tree(children: Vec<MindmapNode>) -> Mindmap {
        Mindmap {
            book_id: 0,
            title: String::new(),
            mode: "theme".into(),
            input_hash: String::new(),
            prompt_version: String::new(),
            stats: MindmapStats { cards: 0, chapters: 0, themes: 0, unplaced: 0 },
            root: MindmapNode {
                id: "root".into(),
                label: "x".into(),
                kind: "book".into(),
                summary: None,
                source_card_ids: vec![],
                children,
            },
            warnings: vec![],
        }
    }

    #[test]
    fn drops_card_leaves_and_excerpt_titles() {
        let cards = sample_cards();
        let tree = raw_tree(vec![
            theme("bad-leaf", "身份由重复塑造", &[1, 2, 3, 14, 17, 18], vec![
                MindmapNode {
                    id: "c-1".into(),
                    label: "人不是拥有习惯，而是由习惯塑造。".into(),
                    kind: "card".into(),
                    summary: None,
                    source_card_ids: vec![1],
                    children: vec![],
                },
            ]),
            theme("excerpt", "人不是拥有习惯，而是由习惯塑造。", &[1, 2], vec![]),
            theme("env", "环境在替你做决定", &[8, 9, 10, 12], vec![]),
            theme("compound", "复利只在两年后显现", &[4, 3], vec![]),
            theme("method", "先出现再优化动作", &[15, 16], vec![]),
            theme("four", "习惯四法则成对出现", &[5, 11, 13], vec![]),
        ]);
        let hash = input_hash(&cards);
        let out = sanitize(tree, &cards, 1, "原子习惯", &hash).unwrap();
        assert!(out.root.children.iter().all(|n| n.kind == "theme"));
        assert!(out.root.children.iter().all(|n| n.children.iter().all(|c| c.kind != "card")));
        assert!(out.root.children.iter().any(|n| n.label == "环境在替你做决定"));
        assert!(!out.root.children.iter().any(|n| n.label.contains("人不是拥有习惯")));
        assert!(out.stats.themes >= 4);
        assert_eq!(out.input_hash, hash);
    }

    #[test]
    fn drops_theme_with_one_card_or_unknown_id() {
        let cards = sample_cards();
        let tree = raw_tree(vec![
            theme("ok", "身份由重复行为塑造", &[1, 2], vec![]),
            theme("one", "单独一条不能成题", &[4], vec![]),
            theme("ghost", "库外节点必须丢掉啊", &[999, 998], vec![]),
        ]);
        let out = sanitize(tree, &cards, 1, "原子习惯", "h").unwrap();
        assert_eq!(out.root.children.len(), 1);
        assert_eq!(out.root.children[0].label, "身份由重复行为塑造");
        assert!(out.stats.unplaced >= 1);
    }

    #[test]
    fn pack_line_includes_id_chapter_and_note() {
        let c = card(7, "如何建立习惯", "highlight", "早饭后立刻写。", "我试过早饭后立刻写 20 分钟", true);
        let line = pack_card_line(&c);
        assert!(line.contains("#7"));
        assert!(line.contains("如何建立习惯"));
        assert!(line.contains("[星]"));
        assert!(line.contains("注:"));
    }

    #[test]
    fn pack_for_prompt_caps_at_oneshot_limit() {
        let cards: Vec<CardRow> = (1..=50)
            .map(|i| card(i, "章", "highlight", "一段用来凑数的划线原文。", "", false))
            .collect();
        let packed = pack_for_prompt(&cards);
        assert_eq!(packed.len(), ONESHOT_LIMIT);
    }

    #[test]
    fn parse_accepts_fenced_json_and_root_only() {
        let raw = "```json\n{\"root\":{\"id\":\"root\",\"label\":\"书\",\"kind\":\"book\",\"children\":[]}}\n```";
        let m = parse_model_json(raw).unwrap();
        assert_eq!(m.root.kind, "book");
    }

    #[test]
    fn hash_changes_when_card_updates() {
        let mut cards = sample_cards();
        let a = input_hash(&cards);
        cards[0].updated_at += 1;
        let b = input_hash(&cards);
        assert_ne!(a, b);
    }
}
