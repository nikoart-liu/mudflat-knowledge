//! 按书划线生成线索：模型只输出概要节点，划线挂在 source_card_ids 上。
//! 校验/清洗是纯函数，生成失败不得把单卡画成叶子。

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;

use crate::db::{self, CardFilter, CardRow};
use crate::llm;

pub const PROMPT_VERSION: &str = "mindmap-theme-v3";
const LABEL_MIN: usize = 8;
const LABEL_MAX: usize = 24;
const SUMMARY_MAX: usize = 40;
const MAX_TOP: usize = 12;
const MAX_SECONDARY: usize = 3;
const MIN_EVIDENCE: usize = 2;
const CARD_LINE_CHARS: usize = 72;
const SMALL_MAX: usize = 80;
const MEDIUM_MAX: usize = 400;
const SHORT_QUOTE: usize = 12;
const CLUSTER_PREFIX: usize = 16;

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MindmapEvent {
    pub stage: String,
    pub current: i64,
    pub total: i64,
    pub title: String,
    pub message: String,
}

fn emit(chan: &Channel<MindmapEvent>, stage: &str, current: i64, total: i64, title: &str, message: &str) {
    let _ = chan.send(MindmapEvent {
        stage: stage.into(),
        current,
        total,
        title: title.into(),
        message: message.into(),
    });
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
        return Err(empty_clue_error(&warnings));
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
            let name = warn_label(&n.label);
            if n.source_card_ids.is_empty() {
                warnings.push(format!("丢掉「{name}」：没有有效证据卡号"));
            } else {
                warnings.push(format!(
                    "丢掉「{name}」：只有 {} 张证据，至少要 2 张",
                    n.source_card_ids.len()
                ));
            }
            continue;
        }
        let label_len = char_len(&n.label);
        if label_len < LABEL_MIN || label_len > LABEL_MAX {
            warnings.push(format!("丢掉标题长度不合规的节点「{}」", warn_label(&n.label)));
            continue;
        }
        if is_excerpt(&n.label, &n.source_card_ids, by_id) {
            warnings.push(format!("丢掉原文截断节点「{}」", warn_label(&n.label)));
            continue;
        }
        out.push(n);
    }
    out
}

fn warn_label(s: &str) -> String {
    let t = s.trim();
    if char_len(t) <= LABEL_MAX {
        t.to_string()
    } else {
        format!("{}…", prefix_chars(t, LABEL_MAX))
    }
}

fn empty_clue_error(warnings: &[String]) -> MindmapError {
    const HEAD: &str = "没有得到可用的概要节点。";
    const TAIL: &str = "请重试，或换一个模型。";
    if warnings.is_empty() {
        return MindmapError::Msg(format!("{HEAD}{TAIL}"));
    }
    let mut seen = HashSet::new();
    let mut reasons: Vec<&str> = Vec::new();
    for w in warnings {
        let t = w.trim();
        if t.is_empty() || !seen.insert(t) {
            continue;
        }
        reasons.push(t);
        if reasons.len() == 8 {
            break;
        }
    }
    let unique_n = {
        let mut all = HashSet::new();
        warnings.iter().map(|w| w.trim()).filter(|t| !t.is_empty()).filter(|t| all.insert(*t)).count()
    };
    let extra = unique_n.saturating_sub(reasons.len());
    let mut body = reasons.join("\n");
    if extra > 0 {
        body.push_str(&format!("\n…另有 {extra} 条"));
    }
    MindmapError::Msg(format!("{HEAD}\n{body}\n{TAIL}"))
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
    "你是编辑，不是作者。只根据用户提供的划线、想法与批注，归纳这本书的「线索」：一张可扫的概要节点图。读者应能不打开证据就看懂自己划过哪几条主线。工作只有三件：聚类、写概要、挂证据。\n\
\n\
只输出一个 JSON 对象，不要 Markdown、不要前言。形状：\n\
{\"root\":{\"id\":\"root\",\"kind\":\"book\",\"label\":\"书名\",\"children\":[{\"id\":\"t1\",\"kind\":\"theme\",\"label\":\"8到24字归纳句\",\"summary\":\"可选，不超过40字\",\"sourceCardIds\":[12,18],\"children\":[]}]}}\n\
\n\
规则：\n\
1. 节点 kind 只能是 book 或 theme。禁止 kind=card。划线、想法只出现在 sourceCardIds，不进树。\n\
2. 根节点 kind=book。children 是一级概要：默认 5 到 12 个；卡少可以更少，但一条划线不能独占一个一级。\n\
3. 一级是扫读主线（跨章同义要合并）。二级可选，每枝最多 3 个，只放子题或分歧。不要第三层。\n\
4. label 必须是 8 到 24 个字的归纳句，不是摘录截断，也不能把「注:」原文贴上去。不得与任何证据正文或批注的前 24 字相同。\n\
5. 每个 theme 至少 2 个 sourceCardIds（整数，来自输入的 #id）。只能用输入里出现过的 id；不要发明卡号。若一行含「同簇 #id」，这些 id 与代表卡同属一簇，必须全部写入该主题的 sourceCardIds。\n\
6. summary 可选，不超过 40 字，必须能被该节点的证据支持。\n\
7. 没出现的论点不要补。不要写作者生平、全书主旨、豆瓣评价或任何库外知识。资料不够就画浅。\n\
8. 同一观点的多条划线合成一个节点。相互冲突的划线分成两个一级，或在同一一级下加两个二级，不要抹平。\n\
9. [星]、[想法]、有「注:」的卡提高成为主题中心和命名的权重，但节点文案仍是归纳。过短金句并入最近主题当证据，不单独占一个概要。"
}

pub fn user_prompt(title: &str, lines: &[String]) -> String {
    user_prompt_counted(title, lines, lines.len())
}

fn user_prompt_counted(title: &str, lines: &[String], source_n: usize) -> String {
    let compressed = if lines.len() < source_n {
        format!("这是 {source_n} 条里压缩后的 {} 簇；「同簇」id 必须全部挂上。\n", lines.len())
    } else {
        String::new()
    };
    format!(
        "书名：{title}\n\
共 {} 条（不是全书）。\n\
{compressed}行格式：#id [章:章名] [划线|想法|自建] [星] 正文 | 注:批注 | 同簇 #id,#id\n\
只根据这些行归纳线索。请输出 JSON。\n\n{}",
        lines.len(),
        lines.join("\n")
    )
}

fn chapter_system_prompt() -> &'static str {
    "你是编辑。只根据这一章的划线归纳 2 到 5 个概要节点。不要写其他章，不要补没出现的论点。\n\
只输出 JSON：{\"root\":{\"kind\":\"book\",\"children\":[{\"kind\":\"theme\",\"label\":\"8到24字归纳句\",\"sourceCardIds\":[1,2],\"children\":[]}]}}\n\
禁止 kind=card。每个 theme 至少 2 个 sourceCardIds。有「同簇」的 id 全部挂上。"
}

fn chapter_user_prompt(book: &str, chapter: &str, lines: &[String]) -> String {
    format!(
        "书名：{book}\n本章：{chapter}\n共 {} 条。请归纳本章线索。请输出 JSON。\n\n{}",
        lines.len(),
        lines.join("\n")
    )
}

fn merge_system_prompt() -> &'static str {
    "你是编辑。下面是各章已经归纳好的概要，不是划线原文。把它们合并成全书线索：5 到 12 个一级主题。跨章同义合并，分歧保留。\n\
只输出 JSON：{\"root\":{\"kind\":\"book\",\"children\":[{...}]}}\n\
每个 theme 的 sourceCardIds 必须是下列卡号的并集，不要丢 id，不要发明 id，不要把卡号变成树节点。二级每枝最多 3 个。"
}

fn merge_user_prompt(title: &str, chapters: &[(String, Vec<MindmapNode>)]) -> String {
    let mut body = String::new();
    for (ch, themes) in chapters {
        body.push_str(&format!("## 章：{ch}\n"));
        for t in themes {
            let ids: Vec<String> = t.source_card_ids.iter().map(|id| id.to_string()).collect();
            body.push_str(&format!("- {} | 卡: {}\n", t.label, ids.join(",")));
            if let Some(s) = &t.summary {
                if !s.is_empty() {
                    body.push_str(&format!("  概要：{s}\n"));
                }
            }
            for c in &t.children {
                let cids: Vec<String> = c.source_card_ids.iter().map(|id| id.to_string()).collect();
                body.push_str(&format!("  - {} | 卡: {}\n", c.label, cids.join(",")));
            }
        }
    }
    format!("书名：{title}\n请合并成全书线索。请输出 JSON。\n\n{body}")
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
    let mut seq = 0u32;
    if let Some(root) = v.get_mut("root") {
        fill_node_defaults(root, "root", &mut seq, true);
    }
    serde_json::from_value(v).map_err(|e| MindmapError::Msg(format!("脑图结构不对: {e}")))
}

fn fill_node_defaults(v: &mut serde_json::Value, fallback_id: &str, seq: &mut u32, is_root: bool) {
    let Some(obj) = v.as_object_mut() else { return };
    let id_ok = obj.get("id").and_then(|x| x.as_str()).map(|s| !s.trim().is_empty()).unwrap_or(false);
    if !id_ok {
        obj.insert("id".into(), serde_json::json!(fallback_id));
    }
    if obj.get("kind").and_then(|x| x.as_str()).unwrap_or("").is_empty() {
        obj.insert("kind".into(), serde_json::json!(if is_root { "book" } else { "theme" }));
    }
    if obj.get("label").and_then(|x| x.as_str()).unwrap_or("").trim().is_empty() {
        let from_summary = obj.get("summary").and_then(|x| x.as_str()).unwrap_or("").trim();
        let label = if from_summary.is_empty() { "未命名主题" } else { from_summary };
        obj.insert("label".into(), serde_json::json!(label));
    }
    // 模型常写 source_card_ids / cardIds / ids
    if obj.get("sourceCardIds").is_none() {
        let alt = obj
            .get("source_card_ids")
            .or_else(|| obj.get("cardIds"))
            .or_else(|| obj.get("ids"))
            .cloned()
            .unwrap_or(serde_json::json!([]));
        obj.insert("sourceCardIds".into(), alt);
    }
    obj.remove("source_card_ids");
    obj.remove("cardIds");
    obj.remove("ids");
    if obj.get("children").is_none() {
        obj.insert("children".into(), serde_json::json!([]));
    }
    if let Some(children) = obj.get_mut("children").and_then(|c| c.as_array_mut()) {
        for child in children {
            *seq += 1;
            let child_id = format!("t-{seq}");
            fill_node_defaults(child, &child_id, seq, false);
        }
    }
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
    on_progress: Channel<MindmapEvent>,
) -> MindmapResult<Mindmap> {
    if cards.len() < MIN_EVIDENCE {
        return Err(MindmapError::Msg("至少两张卡片才能归纳线索。".into()));
    }
    let (cfg, key) = llm::load_runtime(dir).map_err(|e| MindmapError::Msg(e.to_string()))?;

    let hash = input_hash(cards);
    let plan = plan_clue(cards);
    let (steps, plan_msg) = plan_progress(&plan, cards.len());
    emit(&on_progress, "start", 0, steps, "", &plan_msg);
    let mut parsed = match &plan {
        CluePlan::OneShot { lines, clusters } => {
            emit(&on_progress, "oneshot", 1, steps, "", &format!("{} 张", cards.len()));
            let raw = chat_complete(
                &cfg.base_url,
                &cfg.model,
                &key,
                system_prompt(),
                &user_prompt_counted(&book.title, lines, cards.len()),
                Some((&on_progress, 1, steps)),
            )
            .await?;
            let mut tree = parse_model_json(&raw)?;
            expand_clusters(&mut tree, clusters);
            tree
        }
        CluePlan::ByChapter { chapters } => {
            generate_by_chapter(&cfg.base_url, &cfg.model, &key, &book.title, chapters, &on_progress, steps).await?
        }
    };
    parsed.warnings.retain(|w| !w.is_empty());
    emit(&on_progress, "sanitize", steps, steps, "", "");
    let clean = sanitize(parsed, cards, book.id, &book.title, &hash)?;
    save_cache(dir, &clean)?;
    emit(&on_progress, "done", steps, steps, "", "");
    Ok(clean)
}

struct ChapterPack {
    title: String,
    lines: Vec<String>,
    clusters: Vec<Vec<i64>>,
}

enum CluePlan {
    OneShot { lines: Vec<String>, clusters: Vec<Vec<i64>> },
    ByChapter { chapters: Vec<ChapterPack> },
}

fn pack_for_prompt(cards: &[CardRow]) -> Vec<String> {
    let mut ordered: Vec<&CardRow> = cards.iter().collect();
    ordered.sort_by_key(|c| chapter_sort_key(c));
    ordered.iter().map(|c| pack_card_line(c)).collect()
}

fn chapter_sort_key(c: &CardRow) -> (i64, i64, i64) {
    (c.chapter_uid.unwrap_or(i64::MAX), c.created_at, c.id)
}

fn card_weight(c: &CardRow) -> i32 {
    let mut s = 0;
    if c.starred { s += 4; }
    if c.kind == "thought" { s += 3; }
    if !c.note.trim().is_empty() { s += 2; }
    s
}

fn cluster_key(c: &CardRow) -> String {
    normalize_cmp(&prefix_chars(c.text.trim(), CLUSTER_PREFIX))
}

fn pick_rep<'a>(cluster: &[&'a CardRow]) -> &'a CardRow {
    cluster.iter().copied().max_by(|a, b| {
        card_weight(a).cmp(&card_weight(b))
            .then_with(|| char_len(&a.text).cmp(&char_len(&b.text)))
            .then_with(|| a.id.cmp(&b.id))
    }).unwrap_or(cluster[0])
}

fn pack_cluster_line(cluster: &[&CardRow]) -> String {
    let rep = pick_rep(cluster);
    let mut line = pack_card_line(rep);
    if cluster.len() > 1 {
        let rest: Vec<String> = cluster.iter().filter(|c| c.id != rep.id).map(|c| format!("#{}", c.id)).collect();
        line.push_str(" | 同簇 ");
        line.push_str(&rest.join(","));
    }
    line
}

fn cluster_in_order<'a>(cards: &[&'a CardRow]) -> Vec<Vec<&'a CardRow>> {
    let mut clusters: Vec<Vec<&CardRow>> = Vec::new();
    for c in cards {
        let short = char_len(c.text.trim()) < SHORT_QUOTE;
        let key = cluster_key(c);
        if let Some(last) = clusters.last_mut() {
            let last_key = cluster_key(pick_rep(last));
            if short || (!key.is_empty() && !last_key.is_empty() && key == last_key) {
                last.push(c);
                continue;
            }
        }
        clusters.push(vec![c]);
    }
    clusters
}

fn group_by_chapter(cards: &[CardRow]) -> Vec<(String, Vec<&CardRow>)> {
    let mut ordered: Vec<&CardRow> = cards.iter().collect();
    ordered.sort_by_key(|c| chapter_sort_key(c));
    let mut groups: Vec<(String, Vec<&CardRow>)> = Vec::new();
    for c in ordered {
        let title = c.chapter_title.as_deref().map(str::trim).filter(|s| !s.is_empty()).unwrap_or("未分章");
        if let Some((prev, list)) = groups.last_mut() {
            if prev == title {
                list.push(c);
                continue;
            }
        }
        groups.push((title.to_string(), vec![c]));
    }
    groups
}

fn packs_by_chapter(cards: &[CardRow], cluster: bool) -> Vec<ChapterPack> {
    group_by_chapter(cards)
        .into_iter()
        .map(|(title, group)| {
            let clusters = if cluster {
                cluster_in_order(&group)
            } else {
                group.iter().map(|c| vec![*c]).collect()
            };
            let lines = clusters.iter().map(|cl| pack_cluster_line(cl)).collect();
            let cluster_ids = clusters.iter().map(|cl| cl.iter().map(|c| c.id).collect()).collect();
            ChapterPack { title, lines, clusters: cluster_ids }
        })
        .collect()
}

fn singletons(cards: &[CardRow]) -> Vec<Vec<i64>> {
    cards.iter().map(|c| vec![c.id]).collect()
}

fn plan_clue(cards: &[CardRow]) -> CluePlan {
    let n = cards.len();
    if n <= SMALL_MAX {
        return CluePlan::OneShot {
            lines: pack_for_prompt(cards),
            clusters: singletons(cards),
        };
    }
    if n <= MEDIUM_MAX {
        return CluePlan::ByChapter { chapters: packs_by_chapter(cards, false) };
    }
    let chapters = packs_by_chapter(cards, true);
    let total: usize = chapters.iter().map(|c| c.lines.len()).sum();
    if total <= SMALL_MAX {
        let mut lines = Vec::new();
        let mut clusters = Vec::new();
        for ch in chapters {
            lines.extend(ch.lines);
            clusters.extend(ch.clusters);
        }
        CluePlan::OneShot { lines, clusters }
    } else {
        CluePlan::ByChapter { chapters }
    }
}

fn expand_clusters(tree: &mut Mindmap, clusters: &[Vec<i64>]) {
    let mut by_member: HashMap<i64, &[i64]> = HashMap::new();
    for cl in clusters {
        for id in cl {
            by_member.insert(*id, cl.as_slice());
        }
    }
    fn walk(n: &mut MindmapNode, by_member: &HashMap<i64, &[i64]>) {
        let mut extra = Vec::new();
        for id in &n.source_card_ids {
            if let Some(cl) = by_member.get(id) {
                extra.extend(cl.iter().copied());
            }
        }
        for id in extra {
            if !n.source_card_ids.contains(&id) {
                n.source_card_ids.push(id);
            }
        }
        for c in &mut n.children {
            walk(c, by_member);
        }
    }
    walk(&mut tree.root, &by_member);
}

fn empty_tree(warnings: Vec<String>) -> Mindmap {
    Mindmap {
        book_id: 0,
        title: String::new(),
        mode: "theme".into(),
        input_hash: String::new(),
        prompt_version: PROMPT_VERSION.into(),
        stats: MindmapStats { cards: 0, chapters: 0, themes: 0, unplaced: 0 },
        root: MindmapNode {
            id: "root".into(),
            label: String::new(),
            kind: "book".into(),
            summary: None,
            source_card_ids: vec![],
            children: vec![],
        },
        warnings,
    }
}

fn chapter_card_count(ch: &ChapterPack) -> usize {
    ch.clusters.iter().map(|c| c.len()).sum()
}

fn plan_progress(plan: &CluePlan, card_n: usize) -> (i64, String) {
    match plan {
        CluePlan::OneShot { .. } => (1, format!("{card_n} 张卡片，全书一次归纳")),
        CluePlan::ByChapter { chapters } => {
            let n = chapters.iter().filter(|ch| chapter_card_count(ch) >= MIN_EVIDENCE).count() as i64;
            let steps = if n > 1 { n + 1 } else { n };
            (steps, format!("{card_n} 张卡片，按 {n} 章归纳"))
        }
    }
}

async fn generate_by_chapter(
    base_url: &str,
    model: &str,
    key: &str,
    book_title: &str,
    chapters: &[ChapterPack],
    on_progress: &Channel<MindmapEvent>,
    total_steps: i64,
) -> MindmapResult<Mindmap> {
    let mut outlines: Vec<(String, Vec<MindmapNode>)> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut all_clusters: Vec<Vec<i64>> = Vec::new();
    let mut current = 0i64;
    for ch in chapters {
        all_clusters.extend(ch.clusters.iter().cloned());
        if chapter_card_count(ch) < MIN_EVIDENCE {
            emit(on_progress, "chapter_skip", current, total_steps, &ch.title, "");
            warnings.push(format!("「{}」卡片不足，跳过", ch.title));
            continue;
        }
        current += 1;
        emit(on_progress, "chapter", current, total_steps, &ch.title, "");
        match chat_complete(
            base_url,
            model,
            key,
            chapter_system_prompt(),
            &chapter_user_prompt(book_title, &ch.title, &ch.lines),
            Some((on_progress, current, total_steps)),
        )
        .await
        {
            Ok(raw) => match parse_model_json(&raw) {
                Ok(mut tree) => {
                    expand_clusters(&mut tree, &ch.clusters);
                    if tree.root.children.is_empty() {
                        let msg = "没有可用概要";
                        emit(on_progress, "chapter_failed", current, total_steps, &ch.title, msg);
                        warnings.push(format!("「{}」{msg}", ch.title));
                    } else {
                        emit(on_progress, "chapter_ok", current, total_steps, &ch.title, "");
                        outlines.push((ch.title.clone(), tree.root.children));
                    }
                }
                Err(e) => {
                    let msg = format!("结果无法解析：{e}");
                    emit(on_progress, "chapter_failed", current, total_steps, &ch.title, &msg);
                    warnings.push(format!("「{}」归纳结果无法解析：{e}", ch.title));
                }
            },
            Err(e) => {
                let msg = e.to_string();
                emit(on_progress, "chapter_failed", current, total_steps, &ch.title, &msg);
                warnings.push(format!("「{}」归纳失败：{msg}", ch.title));
            }
        }
    }
    if outlines.is_empty() {
        return Err(empty_clue_error(&warnings));
    }
    let mut tree = if outlines.len() == 1 {
        let mut t = empty_tree(warnings);
        t.root.children = outlines.pop().map(|(_, ch)| ch).unwrap_or_default();
        t
    } else {
        current += 1;
        emit(on_progress, "merge", current, total_steps, "", "");
        match chat_complete(
            base_url,
            model,
            key,
            merge_system_prompt(),
            &merge_user_prompt(book_title, &outlines),
            Some((on_progress, current, total_steps)),
        )
        .await
        {
            Ok(raw) => match parse_model_json(&raw) {
                Ok(mut merged) => {
                    expand_clusters(&mut merged, &all_clusters);
                    merged.warnings.extend(warnings);
                    merged
                }
                Err(_) => {
                    emit(on_progress, "chapter_failed", current, total_steps, "全书合并", "暂按各章概要并列");
                    warnings.push("全书合并失败，暂按各章概要并列。".into());
                    flatten_chapters(outlines, warnings)
                }
            },
            Err(_) => {
                emit(on_progress, "chapter_failed", current, total_steps, "全书合并", "暂按各章概要并列");
                warnings.push("全书合并失败，暂按各章概要并列。".into());
                flatten_chapters(outlines, warnings)
            }
        }
    };
    expand_clusters(&mut tree, &all_clusters);
    Ok(tree)
}

fn flatten_chapters(outlines: Vec<(String, Vec<MindmapNode>)>, warnings: Vec<String>) -> Mindmap {
    let mut tree = empty_tree(warnings);
    for (_, mut themes) in outlines {
        tree.root.children.append(&mut themes);
        if tree.root.children.len() >= MAX_TOP {
            tree.root.children.truncate(MAX_TOP);
            break;
        }
    }
    tree
}

async fn chat_complete(
    base_url: &str,
    model: &str,
    key: &str,
    system: &str,
    user: &str,
    progress: Option<(&Channel<MindmapEvent>, i64, i64)>,
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
                if let Some((chan, cur, tot)) = progress {
                    emit(chan, "retry", cur, tot, "", &format!("连接不稳，第 {} 次尝试…", attempt + 2));
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
    fn empty_after_sanitize_names_drop_reasons() {
        let cards = sample_cards();
        let tree = raw_tree(vec![
            theme("one", "单独一条不能成题啊", &[4], vec![]),
            theme("excerpt", "人不是拥有习惯，而是由习惯塑造。", &[1, 2], vec![]),
            theme("ghost", "库外节点必须丢掉啊", &[999, 998], vec![]),
        ]);
        let err = sanitize(tree, &cards, 1, "原子习惯", "h").unwrap_err().to_string();
        assert!(err.starts_with("没有得到可用的概要节点。"));
        assert!(err.contains("丢掉「单独一条不能成题啊」：只有 1 张证据，至少要 2 张"));
        assert!(err.contains("丢掉原文截断节点「人不是拥有习惯，而是由习惯塑造。」"));
        assert!(err.contains("丢掉「库外节点必须丢掉啊」：没有有效证据卡号"));
        assert!(err.contains("请重试，或换一个模型。"));
    }

    #[test]
    fn empty_model_children_keeps_fallback_line() {
        let err = sanitize(raw_tree(vec![]), &sample_cards(), 1, "原子习惯", "h")
            .unwrap_err()
            .to_string();
        assert_eq!(err, "没有得到可用的概要节点。请重试，或换一个模型。");
    }

    #[test]
    fn system_prompt_states_sanitize_invariants() {
        let p = system_prompt();
        assert!(p.contains("线索"));
        assert!(p.contains("sourceCardIds"));
        assert!(p.contains("禁止 kind=card"));
        assert!(p.contains("8 到 24"));
        assert!(p.contains("至少 2 个"));
        assert!(p.contains("不要第三层"));
        assert!(p.contains("没出现的论点不要补"));
        assert!(!p.contains("AI 思维导图"));
    }

    #[test]
    fn user_prompt_explains_input_lines() {
        let p = user_prompt("原子习惯", &["#1 [章:复利] [划线] [星] 人由习惯塑造。".into()]);
        assert!(p.contains("书名：原子习惯"));
        assert!(p.contains("#id"));
        assert!(p.contains("请输出 JSON"));
        assert!(!p.contains("主题脑图"));
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
    fn pack_for_prompt_keeps_every_card() {
        let cards: Vec<CardRow> = (1..=50)
            .map(|i| card(i, "章", "highlight", "一段用来凑数的划线原文。", "", false))
            .collect();
        let packed = pack_for_prompt(&cards);
        assert_eq!(packed.len(), 50);
        assert!(packed[0].contains("#1"));
        assert!(packed[49].contains("#50"));
    }

    fn n_cards(n: usize) -> Vec<CardRow> {
        (1..=n as i64)
            .map(|i| {
                let ch = ((i - 1) % 5) + 1;
                let mut c = card(
                    i,
                    &format!("第{ch}章"),
                    "highlight",
                    &format!("这是第{i}条足够长的划线原文用来归纳主题。"),
                    "",
                    false,
                );
                c.chapter_uid = Some(ch);
                c
            })
            .collect()
    }

    #[test]
    fn plan_oneshot_includes_all_eighty() {
        match plan_clue(&n_cards(80)) {
            CluePlan::OneShot { lines, .. } => assert_eq!(lines.len(), 80),
            CluePlan::ByChapter { .. } => panic!("80 张应一次归纳"),
        }
    }

    #[test]
    fn plan_eighty_one_goes_by_chapter_and_keeps_every_card() {
        match plan_clue(&n_cards(81)) {
            CluePlan::ByChapter { chapters } => {
                assert_eq!(chapters.len(), 5);
                let n: usize = chapters.iter().map(|c| c.lines.len()).sum();
                assert_eq!(n, 81);
            }
            CluePlan::OneShot { .. } => panic!("81 张应按章归纳"),
        }
    }

    #[test]
    fn plan_progress_oneshot_is_one_step() {
        let (steps, msg) = plan_progress(&plan_clue(&n_cards(80)), 80);
        assert_eq!(steps, 1);
        assert_eq!(msg, "80 张卡片，全书一次归纳");
    }

    #[test]
    fn plan_progress_by_chapter_counts_eligible_plus_merge() {
        let (steps, msg) = plan_progress(&plan_clue(&n_cards(81)), 81);
        assert_eq!(steps, 6);
        assert_eq!(msg, "81 张卡片，按 5 章归纳");
    }

    #[test]
    fn plan_large_near_dupes_compresses_before_oneshot() {
        let cards: Vec<CardRow> = (1..=401)
            .map(|i| {
                let mut c = card(i, "环境", "highlight", "环境是无形的手在替你做决定。", "", i == 1);
                c.chapter_uid = Some(1);
                c
            })
            .collect();
        match plan_clue(&cards) {
            CluePlan::OneShot { lines, clusters } => {
                assert_eq!(lines.len(), 1);
                assert_eq!(clusters[0].len(), 401);
                assert!(lines[0].contains("同簇"));
            }
            CluePlan::ByChapter { .. } => panic!("近重复大书应压成一簇"),
        }
    }

    #[test]
    fn plan_large_unique_keeps_every_card_by_chapter() {
        match plan_clue(&n_cards(401)) {
            CluePlan::ByChapter { chapters } => {
                let n: usize = chapters.iter().map(|c| c.lines.len()).sum();
                assert_eq!(n, 401);
            }
            CluePlan::OneShot { .. } => panic!("互不重复的 401 张应按章送，不得丢掉"),
        }
    }

    #[test]
    fn short_quotes_join_previous_cluster() {
        let a = card(1, "环境", "highlight", "环境是无形的手在替你做决定。", "", false);
        let b = card(2, "环境", "highlight", "金句。", "", false);
        let c = card(3, "环境", "highlight", "把遥控器藏起来才读得进书。", "", false);
        let refs = vec![&a, &b, &c];
        let clusters = cluster_in_order(&refs);
        assert_eq!(clusters.len(), 2);
        assert_eq!(clusters[0].iter().map(|x| x.id).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(clusters[1][0].id, 3);
    }

    #[test]
    fn expand_clusters_fills_in_mates() {
        let mut tree = raw_tree(vec![theme("env", "环境在替你做决定", &[8], vec![])]);
        expand_clusters(&mut tree, &[vec![8, 9, 10]]);
        let mut ids = tree.root.children[0].source_card_ids.clone();
        ids.sort();
        assert_eq!(ids, vec![8, 9, 10]);
    }

    #[test]
    fn merge_prompt_keeps_chapter_titles_and_ids() {
        let p = merge_user_prompt("原子习惯", &[
            ("环境".into(), vec![theme("a", "环境在替你做决定", &[8, 9], vec![])]),
            ("身份".into(), vec![theme("b", "身份由重复塑造", &[1, 2], vec![])]),
        ]);
        assert!(p.contains("章：环境"));
        assert!(p.contains("8,9"));
        assert!(p.contains("身份由重复塑造"));
    }

    #[test]
    fn parse_accepts_fenced_json_and_root_only() {
        let raw = "```json\n{\"root\":{\"id\":\"root\",\"label\":\"书\",\"kind\":\"book\",\"children\":[]}}\n```";
        let m = parse_model_json(raw).unwrap();
        assert_eq!(m.root.kind, "book");
    }

    #[test]
    fn parse_fills_missing_id_kind_and_source_ids() {
        let raw = r#"{
          "root": {
            "label": "原子习惯 · 我的划线",
            "children": [
              { "label": "身份由重复塑造", "source_card_ids": [1, 2, 3] },
              { "label": "环境在替你做决定", "ids": [8, 9, 10] }
            ]
          }
        }"#;
        let m = parse_model_json(raw).unwrap();
        assert_eq!(m.root.id, "root");
        assert_eq!(m.root.kind, "book");
        assert_eq!(m.root.children.len(), 2);
        assert!(!m.root.children[0].id.is_empty());
        assert_eq!(m.root.children[0].kind, "theme");
        assert_eq!(m.root.children[0].source_card_ids, vec![1, 2, 3]);
        assert_eq!(m.root.children[1].source_card_ids, vec![8, 9, 10]);
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
