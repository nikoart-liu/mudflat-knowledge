//! AI 派生能力：问题面、相似卡、混合检索、回忆支架。
//!
//! 原文永不写入这里。生成物先入 proposed，人确认后才 accepted。
//! 无供应商或调用失败时，规则版相似卡/支架仍可用。

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::db::{self, CardFilter, CardRow};
use crate::llm;

pub const QUESTION_PROMPT_VERSION: &str = "question-face-v1";
pub const SCAFFOLD_PROMPT_VERSION: &str = "review-scaffold-v1";
const QUESTION_MIN: usize = 8;
const QUESTION_MAX: usize = 72;
const RRF_K: f32 = 60.0;
const EMBED_BATCH: usize = 128;
const SIMILAR_LIMIT: usize = 5;
const SHINGLE_N: usize = 2;
const JACCARD_MIN: f32 = 0.12;
/// 搜索「意思相关」：0.28 大约只是「同一语言」，会把书名相近、主题稀薄的卡灌进来。
const SEMANTIC_MIN: f32 = 0.42;
/// 相对最高分的地板。最高 0.70 时低于 0.62 的不要。
const SEMANTIC_RELATIVE: f32 = 0.88;
const SEMANTIC_LIMIT: usize = 8;

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("{0}")]
    Msg(String),
    #[error("{0}")]
    Db(#[from] rusqlite::Error),
}

pub type AiResult<T> = Result<T, AiError>;

fn msg(s: impl Into<String>) -> AiError {
    AiError::Msg(s.into())
}

// ---------- hash ----------

pub fn fnv1a64(s: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    format!("{h:016x}")
}

pub fn card_content_hash(card: &CardRow) -> String {
    // e2：向量只吃正文和想法。前缀一变，旧的带书名向量会在下次搜索时重算。
    fnv1a64(&format!(
        "e2|{}|{}|{}",
        card.id,
        card.text.trim(),
        card.note.trim()
    ))
}

pub fn question_input_hash(card: &CardRow) -> String {
    fnv1a64(&format!(
        "q|{}|{}|{}|{}",
        QUESTION_PROMPT_VERSION,
        card.id,
        card.text.trim(),
        card.note.trim()
    ))
}

pub fn scaffold_input_hash(card: &CardRow, neighbor_ids: &[i64]) -> String {
    let mut ids = neighbor_ids.to_vec();
    ids.sort();
    fnv1a64(&format!(
        "s|{}|{}|{}|{}|{}",
        SCAFFOLD_PROMPT_VERSION,
        card.id,
        card.text.trim(),
        card.note.trim(),
        ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",")
    ))
}

// ---------- artifacts ----------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuestionCandidate {
    pub kind: String,
    pub question: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuestionContent {
    #[serde(default)]
    pub unsuitable: bool,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub candidates: Vec<QuestionCandidate>,
    #[serde(default)]
    pub accepted_question: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuestionFace {
    pub artifact_id: i64,
    pub card_id: i64,
    pub status: String,
    pub content: QuestionContent,
    pub user_edited: bool,
    pub stale: bool,
    pub provider: String,
    pub model: String,
    pub prompt_version: String,
}

#[derive(Debug, Clone)]
struct ArtifactRow {
    id: i64,
    artifact_type: String,
    primary_card_id: i64,
    source_card_ids: String,
    input_hash: String,
    provider: String,
    model: String,
    prompt_version: String,
    content_json: String,
    status: String,
    user_edited: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn insert_artifact(
    conn: &Connection,
    artifact_type: &str,
    primary_card_id: i64,
    source_card_ids: &[i64],
    input_hash: &str,
    provider: &str,
    model: &str,
    prompt_version: &str,
    content_json: &str,
    status: &str,
    now: i64,
) -> AiResult<i64> {
    let ids = serde_json::to_string(source_card_ids).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "INSERT INTO ai_artifacts (
            artifact_type, primary_card_id, source_card_ids, input_hash,
            provider, model, prompt_version, content_json, status, user_edited, created_at, updated_at
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,0,?10,?10)",
        rusqlite::params![
            artifact_type,
            primary_card_id,
            ids,
            input_hash,
            provider,
            model,
            prompt_version,
            content_json,
            status,
            now
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn load_artifact(conn: &Connection, id: i64) -> AiResult<Option<ArtifactRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, artifact_type, primary_card_id, source_card_ids, input_hash,
                provider, model, prompt_version, content_json, status, user_edited
         FROM ai_artifacts WHERE id=?1",
    )?;
    let mut rows = stmt.query_map([id], parse_artifact)?;
    match rows.next() {
        Some(v) => Ok(Some(v?)),
        None => Ok(None),
    }
}

fn parse_artifact(r: &rusqlite::Row) -> rusqlite::Result<ArtifactRow> {
    Ok(ArtifactRow {
        id: r.get(0)?,
        artifact_type: r.get(1)?,
        primary_card_id: r.get(2)?,
        source_card_ids: r.get(3)?,
        input_hash: r.get(4)?,
        provider: r.get(5)?,
        model: r.get(6)?,
        prompt_version: r.get(7)?,
        content_json: r.get(8)?,
        status: r.get(9)?,
        user_edited: r.get::<_, i64>(10)? != 0,
    })
}

fn latest_question_row(conn: &Connection, card_id: i64) -> AiResult<Option<ArtifactRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, artifact_type, primary_card_id, source_card_ids, input_hash,
                provider, model, prompt_version, content_json, status, user_edited
         FROM ai_artifacts
         WHERE primary_card_id=?1 AND artifact_type='question_face'
         ORDER BY CASE status WHEN 'accepted' THEN 0 WHEN 'proposed' THEN 1 WHEN 'stale' THEN 2 ELSE 3 END,
                  updated_at DESC, id DESC
         LIMIT 1",
    )?;
    let mut rows = stmt.query_map([card_id], parse_artifact)?;
    match rows.next() {
        Some(v) => Ok(Some(v?)),
        None => Ok(None),
    }
}

fn to_question_face(row: ArtifactRow, expected_hash: &str) -> AiResult<QuestionFace> {
    let mut content: QuestionContent =
        serde_json::from_str(&row.content_json).map_err(|e| msg(format!("问题面损坏: {e}")))?;
    let stale = row.status == "stale" || row.input_hash != expected_hash;
    if stale && row.status == "accepted" {
        // 卡片变了：已采用的问题仍可展示，但标过期，不自动覆盖。
        content.accepted_question = content
            .accepted_question
            .or_else(|| content.candidates.first().map(|c| c.question.clone()));
    }
    Ok(QuestionFace {
        artifact_id: row.id,
        card_id: row.primary_card_id,
        status: if stale && row.status == "accepted" {
            "stale".into()
        } else {
            row.status
        },
        content,
        user_edited: row.user_edited,
        stale,
        provider: row.provider,
        model: row.model,
        prompt_version: row.prompt_version,
    })
}

pub fn get_question_face(conn: &Connection, card: &CardRow) -> AiResult<Option<QuestionFace>> {
    let Some(row) = latest_question_row(conn, card.id)? else {
        return Ok(None);
    };
    if (row.status == "stale" || row.input_hash != question_input_hash(card))
        && (row.status == "accepted" || row.status == "proposed")
    {
        conn.execute(
            "UPDATE ai_artifacts SET status='stale', updated_at=updated_at WHERE id=?1 AND status<>'stale'",
            [row.id],
        )?;
    }
    let row = load_artifact(conn, row.id)?.ok_or_else(|| msg("问题面读取失败"))?;
    Ok(Some(to_question_face(row, &question_input_hash(card))?))
}

pub fn accepted_questions(conn: &Connection, card_ids: &[i64]) -> AiResult<Vec<QuestionFace>> {
    if card_ids.is_empty() {
        return Ok(vec![]);
    }
    let cards = db::cards_by_ids(conn, card_ids)?;
    let mut out = Vec::new();
    for c in cards {
        if let Some(face) = get_question_face(conn, &c)? {
            if face.content.accepted_question.is_some()
                && (face.status == "accepted" || face.status == "stale")
            {
                out.push(face);
            }
        }
    }
    Ok(out)
}

pub fn accept_question(
    conn: &Connection,
    artifact_id: i64,
    edited: Option<&str>,
    now: i64,
) -> AiResult<QuestionFace> {
    let row = load_artifact(conn, artifact_id)?.ok_or_else(|| msg("找不到这条建议问题"))?;
    if row.artifact_type != "question_face" {
        return Err(msg("不是问题面"));
    }
    let mut content: QuestionContent =
        serde_json::from_str(&row.content_json).map_err(|e| msg(format!("问题面损坏: {e}")))?;
    if content.unsuitable || content.candidates.is_empty() {
        return Err(msg("这条不适合作为问题"));
    }
    let question = match edited.map(str::trim).filter(|s| !s.is_empty()) {
        Some(q) => q.to_string(),
        None => content
            .accepted_question
            .clone()
            .or_else(|| content.candidates.first().map(|c| c.question.clone()))
            .ok_or_else(|| msg("没有可采用的问题"))?,
    };
    let q_len = question.chars().count();
    if !(QUESTION_MIN..=QUESTION_MAX + 24).contains(&q_len) {
        return Err(msg("问题长度不合适"));
    }
    let user_edited = edited.map(str::trim).filter(|s| !s.is_empty()).is_some();
    content.accepted_question = Some(question);
    let json = serde_json::to_string(&content).map_err(|e| msg(format!("序列化失败: {e}")))?;
    conn.execute(
        "UPDATE ai_artifacts SET status='rejected', updated_at=?2
         WHERE primary_card_id=?1 AND artifact_type='question_face' AND status='accepted' AND id<>?3",
        rusqlite::params![row.primary_card_id, now, artifact_id],
    )?;
    conn.execute(
        "UPDATE ai_artifacts SET content_json=?2, status='accepted', user_edited=?3, updated_at=?4 WHERE id=?1",
        rusqlite::params![artifact_id, json, user_edited as i64, now],
    )?;
    let card = db::get_card(conn, row.primary_card_id)?.ok_or_else(|| msg("卡片已隐藏"))?;
    get_question_face(conn, &card)?.ok_or_else(|| msg("采用后读取失败"))
}

pub fn reject_question(conn: &Connection, artifact_id: i64, now: i64) -> AiResult<()> {
    let n = conn.execute(
        "UPDATE ai_artifacts SET status='rejected', updated_at=?2 WHERE id=?1 AND artifact_type='question_face'",
        rusqlite::params![artifact_id, now],
    )?;
    if n == 0 {
        return Err(msg("找不到这条建议问题"));
    }
    Ok(())
}

pub fn clear_derived(conn: &Connection) -> AiResult<(i64, i64)> {
    let embeddings = conn.execute("DELETE FROM card_embeddings", [])? as i64;
    let artifacts = conn.execute("DELETE FROM ai_artifacts", [])? as i64;
    Ok((embeddings, artifacts))
}

pub fn derived_counts(conn: &Connection) -> AiResult<(i64, i64)> {
    let embeddings: i64 = conn.query_row("SELECT COUNT(*) FROM card_embeddings", [], |r| r.get(0))?;
    let artifacts: i64 = conn.query_row("SELECT COUNT(*) FROM ai_artifacts", [], |r| r.get(0))?;
    Ok((embeddings, artifacts))
}

// ---------- question parse ----------

const ALLOWED_KINDS: &[&str] = &["concept", "cloze", "why", "contrast"];

pub fn extract_json_object(raw: &str) -> AiResult<&str> {
    let trimmed = raw.trim();
    let start = trimmed.find('{').ok_or_else(|| msg("模型没有返回 JSON 对象"))?;
    let end = trimmed.rfind('}').ok_or_else(|| msg("模型没有返回 JSON 对象"))?;
    if end < start {
        return Err(msg("模型没有返回 JSON 对象"));
    }
    Ok(&trimmed[start..=end])
}

fn char_len(s: &str) -> usize {
    s.chars().count()
}

fn normalize_cmp(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && *c != '…' && *c != '.' && *c != '。' && *c != '?' && *c != '？')
        .collect()
}

pub fn parse_question_response(raw: &str, source: &CardRow) -> AiResult<QuestionContent> {
    let json = extract_json_object(raw)?;
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| msg(format!("无法解析问题 JSON: {e}")))?;
    let unsuitable = v.get("unsuitable").and_then(|x| x.as_bool()).unwrap_or(false);
    let reason = v
        .get("reason")
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.chars().take(80).collect::<String>());
    if unsuitable {
        return Ok(QuestionContent {
            unsuitable: true,
            reason: reason.or(Some("这段不适合做成问题".into())),
            candidates: vec![],
            accepted_question: None,
        });
    }
    let arr = v
        .get("candidates")
        .and_then(|x| x.as_array())
        .cloned()
        .unwrap_or_default();
    let needle = normalize_cmp(&source.text);
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for item in arr {
        if out.len() >= 3 {
            break;
        }
        let kind = item
            .get("kind")
            .and_then(|x| x.as_str())
            .unwrap_or("concept")
            .trim()
            .to_ascii_lowercase();
        if !ALLOWED_KINDS.contains(&kind.as_str()) {
            continue;
        }
        let q = item
            .get("question")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let n = char_len(&q);
        if !(QUESTION_MIN..=QUESTION_MAX).contains(&n) {
            continue;
        }
        let cmp = normalize_cmp(&q);
        if cmp.is_empty() || cmp == needle || needle.starts_with(&cmp) || cmp.starts_with(&needle) {
            continue;
        }
        if !seen.insert(cmp) {
            continue;
        }
        out.push(QuestionCandidate { kind, question: q });
    }
    if out.is_empty() {
        return Ok(QuestionContent {
            unsuitable: true,
            reason: Some("没有得到可用的问题".into()),
            candidates: vec![],
            accepted_question: None,
        });
    }
    Ok(QuestionContent {
        unsuitable: false,
        reason: None,
        candidates: out,
        accepted_question: None,
    })
}

pub fn question_system_prompt() -> &'static str {
    "你是编者，不是作者。只根据用户提供的这一张划线或想法，写出帮助主动回忆的问题。\
不要补充库外知识，不要改写原文当问题。\
只输出一个 JSON 对象，不要 Markdown。形状：\
{\"unsuitable\":false,\"reason\":null,\"candidates\":[{\"kind\":\"concept\",\"question\":\"……\"}]}\n\
规则：\n\
1. candidates 1 到 3 条。kind 只能是 concept（概念问答）、cloze（填空）、why（为什么/如何）、contrast（反例或辨析）。\n\
2. question 必须是完整、可答、忠于原文的中文问句，8 到 72 个字。不要把原文整句复制成问题。\n\
3. 原文只是抒情、过短、没有可问概念时，输出 {\"unsuitable\":true,\"reason\":\"……\",\"candidates\":[]}。\n\
4. 不要发明原文没有的术语。资料不够就说不适合。"
}

pub fn question_user_prompt(card: &CardRow) -> String {
    let kind = match card.kind.as_str() {
        "thought" => "想法",
        "self" => "自建",
        _ => "划线",
    };
    let mut s = format!(
        "书名：{}\n章节：{}\n类型：{}\n正文：{}",
        if card.book_title.is_empty() { "（无）" } else { &card.book_title },
        card.chapter_title.as_deref().filter(|t| !t.trim().is_empty()).unwrap_or("未分章"),
        kind,
        card.text.trim()
    );
    if !card.note.trim().is_empty() {
        s.push_str("\n批注：");
        s.push_str(card.note.trim());
    }
    s
}

// ---------- vectors ----------

pub fn l2_normalize(v: &mut [f32]) {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        for x in v {
            *x /= n;
        }
    }
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

pub fn vec_to_blob(v: &[f32]) -> Vec<u8> {
    let mut b = Vec::with_capacity(v.len() * 4);
    for x in v {
        b.extend_from_slice(&x.to_le_bytes());
    }
    b
}

pub fn blob_to_vec(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

pub fn upsert_embedding(
    conn: &Connection,
    card_id: i64,
    provider: &str,
    model: &str,
    content_hash: &str,
    mut vector: Vec<f32>,
    now: i64,
) -> AiResult<()> {
    l2_normalize(&mut vector);
    let dim = vector.len() as i64;
    let blob = vec_to_blob(&vector);
    conn.execute(
        "INSERT INTO card_embeddings (card_id, provider, model, dimensions, content_hash, vector, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(card_id) DO UPDATE SET
           provider=excluded.provider, model=excluded.model, dimensions=excluded.dimensions,
           content_hash=excluded.content_hash, vector=excluded.vector, updated_at=excluded.updated_at",
        rusqlite::params![card_id, provider, model, dim, content_hash, blob, now],
    )?;
    Ok(())
}

struct StoredVec {
    card_id: i64,
    hash: String,
    vector: Vec<f32>,
}

fn load_vectors(conn: &Connection, provider: &str, model: &str) -> AiResult<Vec<StoredVec>> {
    let mut stmt = conn.prepare(
        "SELECT e.card_id, e.content_hash, e.vector FROM card_embeddings e
         JOIN cards c ON c.id=e.card_id
         WHERE e.provider=?1 AND e.model=?2 AND c.deleted=0",
    )?;
    let rows = stmt.query_map(rusqlite::params![provider, model], |r| {
        let blob: Vec<u8> = r.get(2)?;
        Ok(StoredVec {
            card_id: r.get(0)?,
            hash: r.get(1)?,
            vector: blob_to_vec(&blob),
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

// ---------- similar (lexical fallback) ----------

fn shingles(s: &str) -> HashSet<String> {
    let chars: Vec<char> = s.chars().filter(|c| !c.is_whitespace()).collect();
    if chars.len() < SHINGLE_N {
        return chars.into_iter().map(|c| c.to_string()).collect();
    }
    chars
        .windows(SHINGLE_N)
        .map(|w| w.iter().collect())
        .collect()
}

pub fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let inter = a.intersection(b).count() as f32;
    let union = a.union(b).count() as f32;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

fn same_chapter(a: &CardRow, b: &CardRow) -> bool {
    if a.book_id.is_none() || a.book_id != b.book_id {
        return false;
    }
    match (a.chapter_uid, b.chapter_uid) {
        (Some(x), Some(y)) if x == y => true,
        _ => {
            let ta = a.chapter_title.as_deref().map(str::trim).unwrap_or("");
            let tb = b.chapter_title.as_deref().map(str::trim).unwrap_or("");
            !ta.is_empty() && ta == tb
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RelatedCard {
    pub card: CardRow,
    pub score: f32,
    pub reason: String,
}

pub fn related_lexical(source: &CardRow, pool: &[CardRow], limit: usize) -> Vec<RelatedCard> {
    let src = shingles(&format!("{} {}", source.text, source.note));
    let mut scored: Vec<RelatedCard> = pool
        .iter()
        .filter(|c| c.id != source.id && !c.deleted)
        .map(|c| {
            let sim = jaccard(&src, &shingles(&format!("{} {}", c.text, c.note)));
            let chapter_boost = if same_chapter(source, c) { 0.18 } else { 0.0 };
            let score = sim + chapter_boost;
            let reason = if same_chapter(source, c) && sim < JACCARD_MIN {
                "same_chapter"
            } else if sim >= JACCARD_MIN {
                "similar"
            } else {
                "weak"
            };
            RelatedCard {
                card: c.clone(),
                score,
                reason: reason.into(),
            }
        })
        .filter(|r| r.reason != "weak")
        .collect();
    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    scored
}

pub fn related_from_vectors(
    source_id: i64,
    source_vec: &[f32],
    others: &[(i64, Vec<f32>)],
    by_id: &HashMap<i64, CardRow>,
    limit: usize,
) -> Vec<RelatedCard> {
    let mut scored: Vec<(i64, f32)> = others
        .iter()
        .filter(|(id, _)| *id != source_id)
        .map(|(id, v)| (*id, cosine(source_vec, v)))
        .filter(|(_, s)| *s >= 0.35)
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    scored
        .into_iter()
        .filter_map(|(id, score)| {
            by_id.get(&id).map(|c| RelatedCard {
                card: c.clone(),
                score,
                reason: "semantic".into(),
            })
        })
        .collect()
}

// ---------- hybrid search ----------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub card: CardRow,
    pub match_kind: String,
}

/// Reciprocal Rank Fusion：词法排名 + 向量排名。k=60 是标准常数。
pub fn rrf_merge(lexical: &[CardRow], semantic: &[CardRow], limit: usize) -> Vec<SearchHit> {
    let mut scores: HashMap<i64, f32> = HashMap::new();
    let mut kinds: HashMap<i64, &str> = HashMap::new();
    let mut by_id: HashMap<i64, CardRow> = HashMap::new();
    for (i, c) in lexical.iter().enumerate() {
        *scores.entry(c.id).or_insert(0.0) += 1.0 / (RRF_K + (i as f32) + 1.0);
        kinds.insert(c.id, "lexical");
        by_id.insert(c.id, c.clone());
    }
    for (i, c) in semantic.iter().enumerate() {
        *scores.entry(c.id).or_insert(0.0) += 1.0 / (RRF_K + (i as f32) + 1.0);
        match kinds.get(&c.id).copied() {
            Some("lexical") => {
                kinds.insert(c.id, "both");
            }
            None => {
                kinds.insert(c.id, "semantic");
            }
            _ => {}
        }
        by_id.entry(c.id).or_insert_with(|| c.clone());
    }
    let mut ids: Vec<i64> = scores.keys().copied().collect();
    ids.sort_by(|a, b| {
        scores
            .get(b)
            .unwrap()
            .partial_cmp(scores.get(a).unwrap())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.cmp(b))
    });
    ids.truncate(limit);
    ids.into_iter()
        .filter_map(|id| {
            by_id.get(&id).map(|c| SearchHit {
                card: c.clone(),
                match_kind: kinds.get(&id).unwrap_or(&"lexical").to_string(),
            })
        })
        .collect()
}

pub fn lexical_hits(rows: Vec<CardRow>) -> Vec<SearchHit> {
    rows.into_iter()
        .map(|card| SearchHit {
            card,
            match_kind: "lexical".into(),
        })
        .collect()
}

pub fn rank_semantic(query: &[f32], stored: &[(i64, Vec<f32>)], by_id: &HashMap<i64, CardRow>, limit: usize) -> Vec<CardRow> {
    rank_semantic_ids(query, stored, limit)
        .into_iter()
        .filter_map(|id| by_id.get(&id).cloned())
        .collect()
}

// ---------- scaffold ----------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Scaffold {
    pub paraphrase: Option<String>,
    pub example: Option<String>,
    pub neighbors: Vec<CardRow>,
    pub source_card_ids: Vec<i64>,
    pub from_ai: bool,
}

pub fn rule_scaffold(card: &CardRow, pool: &[CardRow], limit: usize) -> Scaffold {
    let mut neighbors: Vec<CardRow> = pool
        .iter()
        .filter(|c| c.id != card.id && same_chapter(card, c))
        .cloned()
        .collect();
    neighbors.sort_by_key(|c| (c.id - card.id).abs());
    neighbors.truncate(limit);
    if neighbors.len() < limit {
        let mut rest: Vec<CardRow> = pool
            .iter()
            .filter(|c| c.id != card.id && c.book_id == card.book_id && !neighbors.iter().any(|n| n.id == c.id))
            .cloned()
            .collect();
        rest.sort_by_key(|c| (c.id - card.id).abs());
        for c in rest {
            if neighbors.len() >= limit {
                break;
            }
            neighbors.push(c);
        }
    }
    let paraphrase = {
        let n = card.note.trim();
        if n.is_empty() {
            None
        } else {
            Some(n.chars().take(120).collect())
        }
    };
    let mut source_card_ids = vec![card.id];
    source_card_ids.extend(neighbors.iter().map(|c| c.id));
    Scaffold {
        paraphrase,
        example: None,
        neighbors,
        source_card_ids,
        from_ai: false,
    }
}

pub fn scaffold_system_prompt() -> &'static str {
    "你是编者。用户刚刚忘记或觉得困难一张卡片。根据本卡、用户批注和给出的相邻卡，写一句白话解释，再给一个具体例子或易混概念。\
每句话都必须能回溯到提供的卡片。不要引入库外知识。\
只输出 JSON：{\"paraphrase\":\"一句白话\",\"example\":\"一个例子或相邻概念\",\"sourceCardIds\":[1,2]}"
}

pub fn scaffold_user_prompt(card: &CardRow, neighbors: &[CardRow]) -> String {
    let mut s = format!("本卡 #{}：{}", card.id, card.text.trim());
    if !card.note.trim().is_empty() {
        s.push_str("\n批注：");
        s.push_str(card.note.trim());
    }
    for n in neighbors {
        s.push_str(&format!("\n相邻 #{}：{}", n.id, n.text.trim()));
    }
    s
}

pub fn parse_scaffold_response(raw: &str, allowed: &HashSet<i64>) -> AiResult<(String, String, Vec<i64>)> {
    let json = extract_json_object(raw)?;
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| msg(format!("无法解析支架 JSON: {e}")))?;
    let paraphrase = v
        .get("paraphrase")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let example = v
        .get("example")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if char_len(&paraphrase) < 6 || char_len(&paraphrase) > 80 {
        return Err(msg("支架解释不可用"));
    }
    let ids = v
        .get("sourceCardIds")
        .or_else(|| v.get("source_card_ids"))
        .and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_i64())
                .filter(|id| allowed.contains(id))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok((paraphrase, example, ids))
}

// ---------- HTTP ----------

pub async fn chat_complete(
    base_url: &str,
    model: &str,
    key: &str,
    system: &str,
    user: &str,
    action: &str,
) -> AiResult<String> {
    let url = llm::chat_url(base_url);
    let http = llm::http_client(Duration::from_secs(60)).map_err(msg)?;
    let body = serde_json::json!({
        "model": model,
        "temperature": 0.3,
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
            return Err(msg(format!("{action}失败。{detail}")));
        }
    };
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| msg(format!("读取响应失败: {e}")))?;
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(msg("语言模型 API Key 无效或没有权限"));
    }
    if !status.is_success() {
        let snippet: String = text.chars().take(160).collect();
        return Err(msg(format!("语言模型返回 {status}: {snippet}")));
    }
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| msg("语言模型响应不是 JSON"))?;
    let content = v
        .pointer("/choices/0/message/content")
        .and_then(|c| c.as_str())
        .ok_or_else(|| msg("语言模型没有返回内容"))?;
    Ok(content.to_string())
}

async fn embed_texts(
    base_url: &str,
    model: &str,
    key: &str,
    inputs: &[String],
) -> AiResult<Vec<Vec<f32>>> {
    if inputs.is_empty() {
        return Ok(vec![]);
    }
    let url = llm::embeddings_url(base_url);
    let http = llm::http_client(Duration::from_secs(60)).map_err(msg)?;
    let body = serde_json::json!({ "model": model, "input": inputs });
    let mut req = http.post(&url).json(&body);
    if !key.is_empty() {
        req = req.bearer_auth(key);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| msg(llm::describe_http_failure("生成向量", e)))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| msg(format!("读取向量响应失败: {e}")))?;
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(msg("向量接口 API Key 无效或没有权限"));
    }
    if !status.is_success() {
        let snippet: String = text.chars().take(160).collect();
        return Err(msg(format!("向量接口返回 {status}: {snippet}")));
    }
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| msg("向量响应不是 JSON"))?;
    let mut pairs: Vec<(usize, Vec<f32>)> = v
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| msg("向量响应缺少 data"))?
        .iter()
        .filter_map(|item| {
            let idx = item.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
            let emb = item.get("embedding")?.as_array()?;
            let vec: Vec<f32> = emb.iter().filter_map(|x| x.as_f64().map(|f| f as f32)).collect();
            if vec.is_empty() {
                None
            } else {
                Some((idx, vec))
            }
        })
        .collect();
    if pairs.len() != inputs.len() {
        return Err(msg(format!(
            "向量条数不匹配：请求 {}，返回 {}",
            inputs.len(),
            pairs.len()
        )));
    }
    pairs.sort_by_key(|(i, _)| *i);
    Ok(pairs.into_iter().map(|(_, v)| v).collect())
}

fn embed_text_for(card: &CardRow) -> String {
    let mut s = card.text.trim().to_string();
    if !card.note.trim().is_empty() {
        s.push(' ');
        s.push_str(card.note.trim());
    }
    s.chars().take(1200).collect()
}

pub struct EmbedJob {
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub key: String,
    pub items: Vec<(i64, String, String)>, // id, hash, text
}

pub fn plan_embeddings(conn: &Connection, dir: &Path, cards: &[CardRow]) -> AiResult<Option<EmbedJob>> {
    let (emb, key) = match llm::load_embedding_runtime(dir) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let provider = provider_tag(emb.provider).to_string();
    let stored = load_vectors(conn, &provider, &emb.model)?;
    let have: HashMap<i64, String> = stored.into_iter().map(|s| (s.card_id, s.hash)).collect();
    let items: Vec<(i64, String, String)> = cards
        .iter()
        .filter_map(|c| {
            let h = card_content_hash(c);
            if have.get(&c.id).map(|old| old != &h).unwrap_or(true) {
                Some((c.id, h, embed_text_for(c)))
            } else {
                None
            }
        })
        .take(EMBED_BATCH)
        .collect();
    if items.is_empty() {
        return Ok(None);
    }
    Ok(Some(EmbedJob {
        provider,
        model: emb.model,
        base_url: emb.base_url,
        key,
        items,
    }))
}

pub async fn run_embed_job(job: &EmbedJob) -> AiResult<Vec<(i64, String, Vec<f32>)>> {
    let inputs: Vec<String> = job.items.iter().map(|(_, _, t)| t.clone()).collect();
    let vectors = embed_texts(&job.base_url, &job.model, &job.key, &inputs).await?;
    Ok(job
        .items
        .iter()
        .zip(vectors)
        .map(|((id, hash, _), vec)| (*id, hash.clone(), vec))
        .collect())
}

pub fn commit_embeddings(
    conn: &Connection,
    job: &EmbedJob,
    rows: Vec<(i64, String, Vec<f32>)>,
    now: i64,
) -> AiResult<usize> {
    let n = rows.len();
    for (id, hash, vec) in rows {
        upsert_embedding(conn, id, &job.provider, &job.model, &hash, vec, now)?;
    }
    Ok(n)
}

pub fn semantic_hits_from_store(
    conn: &Connection,
    provider: &str,
    model: &str,
    query: &[f32],
    allowed: &HashSet<i64>,
    limit: usize,
) -> AiResult<Vec<CardRow>> {
    let stored = load_vectors(conn, provider, model)?;
    let pairs: Vec<(i64, Vec<f32>)> = stored
        .into_iter()
        .filter(|s| allowed.contains(&s.card_id))
        .map(|s| (s.card_id, s.vector))
        .collect();
    let ids = rank_semantic_ids(query, &pairs, limit);
    Ok(db::cards_by_ids(conn, &ids)?)
}

pub fn rank_semantic_ids(query: &[f32], stored: &[(i64, Vec<f32>)], limit: usize) -> Vec<i64> {
    let mut scored: Vec<(i64, f32)> = stored
        .iter()
        .map(|(id, v)| (*id, cosine(query, v)))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let Some(&(_, best)) = scored.first() else {
        return vec![];
    };
    if best < SEMANTIC_MIN {
        return vec![];
    }
    let floor = (best * SEMANTIC_RELATIVE).max(SEMANTIC_MIN);
    scored.retain(|(_, s)| *s >= floor);
    scored.truncate(limit.min(SEMANTIC_LIMIT));
    scored.into_iter().map(|(id, _)| id).collect()
}

pub fn store_question(
    conn: &Connection,
    card: &CardRow,
    content: &QuestionContent,
    provider: &str,
    model: &str,
    now: i64,
) -> AiResult<QuestionFace> {
    let json = serde_json::to_string(content).map_err(|e| msg(format!("序列化失败: {e}")))?;
    let hash = question_input_hash(card);
    let id = insert_artifact(
        conn,
        "question_face",
        card.id,
        &[card.id],
        &hash,
        provider,
        model,
        QUESTION_PROMPT_VERSION,
        &json,
        "proposed",
        now,
    )?;
    let row = load_artifact(conn, id)?.ok_or_else(|| msg("写入后读取失败"))?;
    to_question_face(row, &hash)
}

pub fn cached_question(conn: &Connection, card: &CardRow) -> AiResult<Option<QuestionFace>> {
    let hash = question_input_hash(card);
    let Some(existing) = latest_question_row(conn, card.id)? else {
        return Ok(None);
    };
    if existing.input_hash == hash && (existing.status == "proposed" || existing.status == "accepted") {
        return Ok(Some(to_question_face(existing, &hash)?));
    }
    Ok(None)
}

fn provider_tag(p: llm::Provider) -> &'static str {
    match p {
        llm::Provider::Off => "off",
        llm::Provider::Openai => "openai",
        llm::Provider::Xai => "xai",
        llm::Provider::Ollama => "ollama",
        llm::Provider::Custom => "custom",
    }
}

pub fn cached_scaffold(conn: &Connection, card: &CardRow, pool: &[CardRow], rule: &Scaffold) -> AiResult<Option<Scaffold>> {
    let hash = scaffold_input_hash(card, &rule.neighbors.iter().map(|c| c.id).collect::<Vec<_>>());
    let Some(row) = latest_scaffold_row(conn, card.id)? else {
        return Ok(None);
    };
    if row.input_hash == hash && (row.status == "proposed" || row.status == "accepted") {
        return Ok(scaffold_from_row(&row, pool).ok());
    }
    Ok(None)
}

#[allow(clippy::too_many_arguments)]
pub fn store_scaffold(
    conn: &Connection,
    card: &CardRow,
    rule: &Scaffold,
    paraphrase: &str,
    example: &str,
    ids: &[i64],
    provider: &str,
    model: &str,
    now: i64,
) -> AiResult<Scaffold> {
    let source_ids = if ids.is_empty() {
        rule.source_card_ids.clone()
    } else {
        ids.to_vec()
    };
    let content = serde_json::json!({
        "paraphrase": paraphrase,
        "example": example,
        "sourceCardIds": source_ids,
    });
    let hash = scaffold_input_hash(card, &rule.neighbors.iter().map(|c| c.id).collect::<Vec<_>>());
    let _ = insert_artifact(
        conn,
        "review_scaffold",
        card.id,
        &rule.source_card_ids,
        &hash,
        provider,
        model,
        SCAFFOLD_PROMPT_VERSION,
        &content.to_string(),
        "proposed",
        now,
    );
    let mut out = rule.clone();
    out.paraphrase = Some(paraphrase.to_string());
    out.example = if example.is_empty() { None } else { Some(example.to_string()) };
    out.from_ai = true;
    out.source_card_ids = source_ids;
    Ok(out)
}

fn latest_scaffold_row(conn: &Connection, card_id: i64) -> AiResult<Option<ArtifactRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, artifact_type, primary_card_id, source_card_ids, input_hash,
                provider, model, prompt_version, content_json, status, user_edited
         FROM ai_artifacts
         WHERE primary_card_id=?1 AND artifact_type='review_scaffold'
         ORDER BY updated_at DESC, id DESC LIMIT 1",
    )?;
    let mut rows = stmt.query_map([card_id], parse_artifact)?;
    match rows.next() {
        Some(v) => Ok(Some(v?)),
        None => Ok(None),
    }
}

fn scaffold_from_row(row: &ArtifactRow, pool: &[CardRow]) -> AiResult<Scaffold> {
    let v: serde_json::Value =
        serde_json::from_str(&row.content_json).map_err(|e| msg(format!("支架损坏: {e}")))?;
    let paraphrase = v
        .get("paraphrase")
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let example = v
        .get("example")
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let ids: Vec<i64> = serde_json::from_str(&row.source_card_ids).unwrap_or_default();
    let neighbors: Vec<CardRow> = pool
        .iter()
        .filter(|c| ids.contains(&c.id) && c.id != row.primary_card_id)
        .cloned()
        .collect();
    Ok(Scaffold {
        paraphrase,
        example,
        neighbors,
        source_card_ids: ids,
        from_ai: true,
    })
}

fn load_pool(conn: &Connection, card: &CardRow) -> AiResult<Vec<CardRow>> {
    let filter = CardFilter {
        book_id: card.book_id,
        ..CardFilter::default()
    };
    Ok(db::query_cards(conn, &filter, 2_000, 0)?)
}

pub fn get_rule_scaffold(conn: &Connection, card: &CardRow) -> AiResult<Scaffold> {
    let pool = load_pool(conn, card)?;
    Ok(rule_scaffold(card, &pool, 3))
}

pub fn related_from_store(
    conn: &Connection,
    provider: &str,
    model: &str,
    card: &CardRow,
    pool: &[CardRow],
) -> AiResult<Vec<RelatedCard>> {
    let stored = load_vectors(conn, provider, model)?;
    let Some(src_vec) = stored.iter().find(|s| s.card_id == card.id).map(|s| s.vector.clone()) else {
        return Ok(related_lexical(card, pool, SIMILAR_LIMIT));
    };
    let by_id: HashMap<i64, CardRow> = pool.iter().cloned().map(|c| (c.id, c)).collect();
    let others: Vec<(i64, Vec<f32>)> = stored.into_iter().map(|s| (s.card_id, s.vector)).collect();
    let semantic = related_from_vectors(card.id, &src_vec, &others, &by_id, SIMILAR_LIMIT);
    if semantic.is_empty() {
        Ok(related_lexical(card, pool, SIMILAR_LIMIT))
    } else {
        Ok(semantic)
    }
}

pub async fn embed_query(dir: &Path, q: &str) -> AiResult<Option<(String, String, Vec<f32>)>> {
    let (emb, key) = match llm::load_embedding_runtime(dir) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let text: String = q.trim().chars().take(400).collect();
    let mut vecs = embed_texts(&emb.base_url, &emb.model, &key, &[text]).await?;
    let Some(mut v) = vecs.pop() else {
        return Ok(None);
    };
    l2_normalize(&mut v);
    Ok(Some((provider_tag(emb.provider).to_string(), emb.model, v)))
}

pub fn load_related_pool(conn: &Connection, card: &CardRow) -> AiResult<Vec<CardRow>> {
    if card.book_id.is_some() {
        load_pool(conn, card)
    } else {
        Ok(db::query_cards(conn, &CardFilter::default(), 2_000, 0)?)
    }
}

pub fn provider_name(p: llm::Provider) -> &'static str {
    provider_tag(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn mem() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        db::apply_schema(&conn, db::SchemaPlan::Fresh).unwrap();
        conn
    }

    fn sample(id: i64, text: &str, note: &str, chapter: &str) -> CardRow {
        CardRow {
            id,
            kind: "highlight".into(),
            book_id: Some(1),
            remote_id: Some(format!("r{id}")),
            chapter_uid: Some(chapter.chars().map(|c| c as i64).sum::<i64>().max(1)),
            chapter_title: Some(chapter.into()),
            text: text.into(),
            abstract_text: None,
            range_str: None,
            color_style: 0,
            note: note.into(),
            starred: false,
            excluded_from_review: false,
            created_at: 1,
            updated_at: 1,
            deleted: false,
            book_title: "原子习惯".into(),
            tags: vec![],
        }
    }

    #[test]
    fn parse_question_keeps_three_valid_and_drops_echo() {
        let card = sample(1, "人不是拥有习惯，而是由习惯塑造。", "", "习惯的复利");
        let raw = r#"```json
{"unsuitable":false,"candidates":[
  {"kind":"concept","question":"这段话如何解释身份与重复行为的关系？"},
  {"kind":"why","question":"为什么说人是被习惯塑造的，而不是拥有习惯？"},
  {"kind":"cloze","question":"人不是拥有习惯，而是由习惯塑造。"},
  {"kind":"nope","question":"无效类型"},
  {"kind":"contrast","question":"若把习惯理解成身外之物，会漏掉什么？"}
]}
```"#;
        let out = parse_question_response(raw, &card).unwrap();
        assert!(!out.unsuitable);
        assert_eq!(out.candidates.len(), 3);
        assert!(out.candidates.iter().all(|c| c.question != card.text));
        assert_eq!(out.candidates[2].kind, "contrast");
    }

    #[test]
    fn parse_question_marks_unsuitable() {
        let card = sample(1, "啊。", "", "");
        let out = parse_question_response(
            r#"{"unsuitable":true,"reason":"过短，没有可问概念","candidates":[]}"#,
            &card,
        )
        .unwrap();
        assert!(out.unsuitable);
        assert!(out.candidates.is_empty());
    }

    #[test]
    fn cosine_identical_normalized_is_one() {
        let mut a = vec![3.0, 4.0];
        l2_normalize(&mut a);
        assert!((cosine(&a, &a) - 1.0).abs() < 1e-5);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
    }

    fn unit(x: f32, y: f32) -> Vec<f32> {
        let mut v = vec![x, y];
        l2_normalize(&mut v);
        v
    }

    #[test]
    fn rank_semantic_drops_scores_far_below_the_best() {
        let query = unit(1.0, 0.0);
        let stored = vec![
            (1, unit(1.0, 0.0)),
            (2, unit(0.92, 0.39)),
            (3, unit(0.50, 0.866)),
        ];
        let ids = rank_semantic_ids(&query, &stored, 10);
        assert_eq!(ids, vec![1, 2]);
        assert!(!ids.contains(&3), "过了绝对门槛但远低于最高分的不应进意思相关");
    }

    #[test]
    fn rank_semantic_empty_when_nothing_clears_the_floor() {
        let query = unit(1.0, 0.0);
        let stored = vec![(1, unit(0.20, 0.98)), (2, unit(0.25, 0.97))];
        assert!(rank_semantic_ids(&query, &stored, 10).is_empty());
    }

    #[test]
    fn embed_text_is_highlight_and_note_without_book_title() {
        let mut card = sample(1, "沉没成本让人加码。", "已经付出的不该再决定下一步。", "决策");
        let t = embed_text_for(&card);
        assert!(t.contains("沉没成本"));
        assert!(t.contains("已经付出"));
        assert!(!t.contains("原子习惯"), "书名会污染短划线的向量");
        let hashed = card_content_hash(&card);
        card.book_title = "另一本书".into();
        assert_eq!(hashed, card_content_hash(&card), "书名变化不应迫使重算向量");
        card.note = "改了想法".into();
        assert_ne!(hashed, card_content_hash(&card));
    }

    #[test]
    fn rrf_marks_overlap_as_both_and_keeps_semantic_only() {
        let a = sample(1, "沉没成本让人继续投入。", "", "决策");
        let b = sample(2, "人会为已经付出的东西加码。", "", "决策");
        let c = sample(3, "工作记忆容量有限。", "", "认知");
        let hits = rrf_merge(&[a.clone(), c.clone()], &[b.clone(), a.clone()], 10);
        assert_eq!(hits.len(), 3);
        let by: HashMap<i64, String> = hits.into_iter().map(|h| (h.card.id, h.match_kind)).collect();
        assert_eq!(by.get(&1).unwrap(), "both");
        assert_eq!(by.get(&2).unwrap(), "semantic");
        assert_eq!(by.get(&3).unwrap(), "lexical");
    }

    #[test]
    fn related_lexical_prefers_same_chapter_and_similar_wording() {
        let src = sample(1, "沉没成本让人继续投入已经付出的东西。", "", "决策");
        let pool = vec![
            sample(2, "已经投入的成本会让人加码。", "", "决策"),
            sample(3, "早饭后立刻写二十分钟。", "", "习惯"),
            sample(4, "人会为沉没的投入继续付出。", "", "别的章"),
        ];
        let rel = related_lexical(&src, &pool, 5);
        assert!(rel.iter().any(|r| r.card.id == 2 && r.reason == "similar"));
        assert!(rel.iter().any(|r| r.card.id == 4));
        assert!(rel.iter().all(|r| r.card.id != 3), "无关卡不该进来");
    }

    #[test]
    fn rule_scaffold_uses_note_and_neighbors() {
        let src = sample(1, "人不是拥有习惯，而是由习惯塑造。", "身份是重复出来的", "习惯的复利");
        let pool = vec![
            sample(2, "每做一个 1% 的改进，你都在变成那种人。", "", "习惯的复利"),
            sample(3, "环境是无形的手。", "", "环境"),
        ];
        let sc = rule_scaffold(&src, &pool, 3);
        assert_eq!(sc.paraphrase.as_deref(), Some("身份是重复出来的"));
        assert_eq!(sc.neighbors[0].id, 2);
        assert!(sc.source_card_ids.contains(&1));
        assert!(!sc.from_ai);
    }

    #[test]
    fn artifact_accept_does_not_touch_card_text() {
        let conn = mem();
        conn.execute(
            "INSERT INTO books (weread_book_id, title) VALUES ('w','原子习惯')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cards (kind, book_id, text, note, created_at, updated_at) VALUES ('highlight',1,'人不是拥有习惯，而是由习惯塑造。','',1,1)",
            [],
        )
        .unwrap();
        let card = db::get_card(&conn, 1).unwrap().unwrap();
        let content = QuestionContent {
            unsuitable: false,
            reason: None,
            candidates: vec![QuestionCandidate {
                kind: "concept".into(),
                question: "这段话如何解释身份与重复行为的关系？".into(),
            }],
            accepted_question: None,
        };
        let id = insert_artifact(
            &conn,
            "question_face",
            1,
            &[1],
            &question_input_hash(&card),
            "openai",
            "gpt-4o-mini",
            QUESTION_PROMPT_VERSION,
            &serde_json::to_string(&content).unwrap(),
            "proposed",
            10,
        )
        .unwrap();
        let accepted = accept_question(&conn, id, None, 11).unwrap();
        assert_eq!(accepted.status, "accepted");
        assert_eq!(
            accepted.content.accepted_question.as_deref(),
            Some("这段话如何解释身份与重复行为的关系？")
        );
        let again = db::get_card(&conn, 1).unwrap().unwrap();
        assert_eq!(again.text, "人不是拥有习惯，而是由习惯塑造。");
        assert_eq!(again.note, "");
        reject_question(&conn, id, 12).unwrap();
        let after = get_question_face(&conn, &card).unwrap().unwrap();
        assert_eq!(after.status, "rejected");
    }

    #[test]
    fn stale_when_card_text_changes() {
        let conn = mem();
        conn.execute(
            "INSERT INTO books (weread_book_id, title) VALUES ('w','原子习惯')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cards (kind, book_id, text, note, created_at, updated_at) VALUES ('self',1,'原文','',1,1)",
            [],
        )
        .unwrap();
        let card = db::get_card(&conn, 1).unwrap().unwrap();
        let content = QuestionContent {
            unsuitable: false,
            reason: None,
            candidates: vec![QuestionCandidate {
                kind: "why".into(),
                question: "为什么原文值得记住？".into(),
            }],
            accepted_question: Some("为什么原文值得记住？".into()),
        };
        insert_artifact(
            &conn,
            "question_face",
            1,
            &[1],
            &question_input_hash(&card),
            "ollama",
            "qwen2.5",
            QUESTION_PROMPT_VERSION,
            &serde_json::to_string(&content).unwrap(),
            "accepted",
            10,
        )
        .unwrap();
        db::update_card_text(&conn, 1, "改过的正文", 20).unwrap();
        let changed = db::get_card(&conn, 1).unwrap().unwrap();
        let face = get_question_face(&conn, &changed).unwrap().unwrap();
        assert!(face.stale);
        assert_eq!(face.content.accepted_question.as_deref(), Some("为什么原文值得记住？"));
    }

    #[test]
    fn semantic_hits_include_older_cards_outside_a_recency_pool() {
        let conn = mem();
        conn.execute(
            "INSERT INTO cards (id, kind, text, created_at, updated_at) VALUES (1,'self','沉没成本',1,1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cards (id, kind, text, created_at, updated_at) VALUES (2,'self','早饭',100,100)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cards (id, kind, text, created_at, updated_at) VALUES (3,'self','天气',200,200)",
            [],
        )
        .unwrap();
        upsert_embedding(&conn, 1, "openai", "m", "h", vec![1.0, 0.0], 10).unwrap();
        let query = vec![1.0, 0.0];
        let newest_two: HashSet<i64> = [2, 3].into_iter().collect();
        let miss = semantic_hits_from_store(&conn, "openai", "m", &query, &newest_two, 10).unwrap();
        assert!(miss.is_empty(), "墙的最近 N 张若不包含已索引卡，语义会空");
        let all = db::query_card_ids(&conn, &CardFilter::default()).unwrap();
        assert!(all.contains(&1) && all.len() == 3);
        let hits = semantic_hits_from_store(&conn, "openai", "m", &query, &all, 10).unwrap();
        assert_eq!(hits.iter().map(|c| c.id).collect::<Vec<_>>(), vec![1]);
    }

    #[test]
    fn embedding_roundtrip_and_clear() {
        let conn = mem();
        conn.execute(
            "INSERT INTO cards (kind, text, created_at, updated_at) VALUES ('self','hello',1,1)",
            [],
        )
        .unwrap();
        upsert_embedding(&conn, 1, "openai", "text-embedding-3-small", "abc", vec![1.0, 0.0], 10).unwrap();
        let stored = load_vectors(&conn, "openai", "text-embedding-3-small").unwrap();
        assert_eq!(stored.len(), 1);
        assert!((stored[0].vector[0] - 1.0).abs() < 1e-5);
        let (e, a) = derived_counts(&conn).unwrap();
        assert_eq!((e, a), (1, 0));
        clear_derived(&conn).unwrap();
        assert_eq!(derived_counts(&conn).unwrap(), (0, 0));
    }

    #[test]
    fn parse_scaffold_filters_unknown_ids() {
        let allowed: HashSet<i64> = [1, 2].into_iter().collect();
        let (p, e, ids) = parse_scaffold_response(
            r#"{"paraphrase":"习惯在塑造身份，而不是一份清单。","example":"每天写二十分钟会把自己写成写作者。","sourceCardIds":[1,99,2]}"#,
            &allowed,
        )
        .unwrap();
        assert!(p.contains("习惯"));
        assert!(e.contains("写作者"));
        assert_eq!(ids, vec![1, 2]);
    }
}
