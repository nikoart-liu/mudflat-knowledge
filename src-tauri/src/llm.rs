//! 语言模型与向量模型配置：与微信读书 Key 分开存放。
//!
//! - 非密钥：`llm.json`（chat 与 embedding 可走不同供应商）
//! - 密钥：`llm.key`、`llm.embedding.key`（0600，与 api.key 同一套原子写入）
//!
//! 默认关闭。http 只允许回环地址；其余必须 https。
//! 向量可单独启用：主模型供应商（如 xAI）不必提供 embeddings。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const CONFIG_FILE: &str = "llm.json";
const KEY_FILE: &str = "llm.key";
const EMBEDDING_KEY_FILE: &str = "llm.embedding.key";

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("读写语言模型配置失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Invalid(String),
}

pub type LlmResult<T> = Result<T, LlmError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Off,
    Openai,
    Xai,
    Ollama,
    Custom,
}

impl Provider {
    pub fn default_base_url(self) -> &'static str {
        match self {
            Provider::Off => "",
            Provider::Openai => "https://api.openai.com/v1",
            Provider::Xai => "https://api.x.ai/v1",
            Provider::Ollama => "http://127.0.0.1:11434/v1",
            Provider::Custom => "",
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Provider::Off => "",
            Provider::Openai => "gpt-4o-mini",
            Provider::Xai => "grok-4.5",
            Provider::Ollama => "qwen2.5",
            Provider::Custom => "",
        }
    }

    pub fn default_embedding_model(self) -> &'static str {
        match self {
            Provider::Off | Provider::Xai | Provider::Custom => "",
            Provider::Openai => "text-embedding-3-small",
            Provider::Ollama => "nomic-embed-text",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingConfig {
    pub provider: Provider,
    pub base_url: String,
    pub model: String,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: Provider::Off,
            base_url: String::new(),
            model: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfig {
    pub provider: Provider,
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub embedding: EmbeddingConfig,
    /// 旧文件只存了 embeddingModel 字符串；读入时迁到 embedding，不再写出。
    #[serde(default, skip_serializing)]
    pub embedding_model: String,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: Provider::Off,
            base_url: String::new(),
            model: String::new(),
            embedding: EmbeddingConfig::default(),
            embedding_model: String::new(),
        }
    }
}

impl LlmConfig {
    fn after_load(mut self) -> Self {
        if self.embedding.provider == Provider::Off
            && !self.embedding_model.trim().is_empty()
            && matches!(self.provider, Provider::Openai | Provider::Ollama | Provider::Custom)
        {
            self.embedding = EmbeddingConfig {
                provider: self.provider,
                base_url: self.base_url.clone(),
                model: self.embedding_model.trim().to_string(),
            };
        }
        self.embedding_model.clear();
        self
    }

    fn both_off(&self) -> bool {
        self.provider == Provider::Off && self.embedding.provider == Provider::Off
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LlmSettings {
    pub provider: Provider,
    pub base_url: String,
    pub model: String,
    pub has_key: bool,
    pub embedding_provider: Provider,
    pub embedding_base_url: String,
    pub embedding_model: String,
    pub has_embedding_key: bool,
}

/// 用户保存/测试语言模型时提交的草稿。key 为空表示「沿用已存密钥」。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmDraft {
    pub provider: Provider,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub key: String,
}

/// 用户保存/测试向量模型时提交的草稿。与语言模型独立。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingDraft {
    pub provider: Provider,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Normalized {
    pub config: LlmConfig,
    /// None = 沿用已存密钥；Some("") = 明确无密钥（仅 Ollama 回环允许）
    pub key: Option<String>,
}

pub fn config_path(dir: &Path) -> PathBuf {
    dir.join(CONFIG_FILE)
}

pub fn key_path(dir: &Path) -> PathBuf {
    dir.join(KEY_FILE)
}

pub fn embedding_key_path(dir: &Path) -> PathBuf {
    dir.join(EMBEDDING_KEY_FILE)
}

fn to_settings(dir: &Path, config: &LlmConfig) -> LlmSettings {
    LlmSettings {
        provider: config.provider,
        base_url: config.base_url.clone(),
        model: config.model.clone(),
        has_key: has_key(dir),
        embedding_provider: config.embedding.provider,
        embedding_base_url: config.embedding.base_url.clone(),
        embedding_model: config.embedding.model.clone(),
        has_embedding_key: has_embedding_key(dir, config),
    }
}

fn empty_settings() -> LlmSettings {
    LlmSettings {
        provider: Provider::Off,
        base_url: String::new(),
        model: String::new(),
        has_key: false,
        embedding_provider: Provider::Off,
        embedding_base_url: String::new(),
        embedding_model: String::new(),
        has_embedding_key: false,
    }
}

pub fn embedding_ready(cfg: &LlmConfig) -> bool {
    cfg.embedding.provider != Provider::Off && !cfg.embedding.model.trim().is_empty()
}

pub fn load_settings(dir: &Path) -> LlmResult<LlmSettings> {
    let config = load_config(dir)?;
    Ok(to_settings(dir, &config))
}

pub fn load_config(dir: &Path) -> LlmResult<LlmConfig> {
    match fs::read_to_string(config_path(dir)) {
        Ok(s) => {
            let cfg: LlmConfig = serde_json::from_str(&s)
                .map_err(|e| LlmError::Invalid(format!("语言模型配置损坏: {e}")))?;
            Ok(cfg.after_load())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(LlmConfig::default()),
        Err(e) => Err(e.into()),
    }
}

pub fn has_key(dir: &Path) -> bool {
    get_key(dir).is_ok()
}

fn read_secret(path: PathBuf, missing: &str) -> LlmResult<String> {
    match fs::read_to_string(&path) {
        Ok(s) => {
            let s = s.trim().to_string();
            if s.is_empty() {
                Err(LlmError::Invalid(missing.into()))
            } else {
                Ok(s)
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(LlmError::Invalid(missing.into())),
        Err(e) => Err(e.into()),
    }
}

pub fn get_key(dir: &Path) -> LlmResult<String> {
    read_secret(key_path(dir), "未保存语言模型 API Key")
}

pub fn get_embedding_key(dir: &Path) -> LlmResult<String> {
    read_secret(embedding_key_path(dir), "未保存向量模型 API Key")
}

fn same_endpoint(chat: &LlmConfig, embedding: &EmbeddingConfig) -> bool {
    embedding.provider != Provider::Off
        && embedding.provider == chat.provider
        && embedding.base_url == chat.base_url
}

fn has_embedding_key(dir: &Path, cfg: &LlmConfig) -> bool {
    if get_embedding_key(dir).is_ok() {
        return true;
    }
    same_endpoint(cfg, &cfg.embedding) && has_key(dir)
}

fn embedding_key_for(dir: &Path, cfg: &LlmConfig) -> String {
    match get_embedding_key(dir) {
        Ok(k) => k,
        Err(_) if same_endpoint(cfg, &cfg.embedding) => get_key(dir).unwrap_or_default(),
        Err(_) => String::new(),
    }
}

pub fn save(dir: &Path, draft: &LlmDraft, existing_key: Option<&str>) -> LlmResult<LlmSettings> {
    let normalized = normalize_draft(draft, existing_key)?;
    let mut cfg = load_config(dir)?;
    cfg.provider = normalized.config.provider;
    cfg.base_url = normalized.config.base_url;
    cfg.model = normalized.config.model;

    if cfg.both_off() {
        clear(dir)?;
        return Ok(empty_settings());
    }

    fs::create_dir_all(dir)?;
    write_json(dir, &cfg)?;
    if cfg.provider == Provider::Off {
        let _ = fs::remove_file(key_path(dir));
    } else if let Some(key) = normalized.key.as_deref() {
        if key.is_empty() {
            let _ = fs::remove_file(key_path(dir));
        } else {
            write_secret(dir, key)?;
        }
    }
    Ok(to_settings(dir, &cfg))
}

pub fn save_embedding(
    dir: &Path,
    draft: &EmbeddingDraft,
    existing_key: Option<&str>,
) -> LlmResult<LlmSettings> {
    let mut cfg = load_config(dir)?;
    let chat_key = get_key(dir).ok();
    let normalized = normalize_embedding_draft(draft, existing_key, &cfg, chat_key.as_deref())?;
    cfg.embedding = normalized.config;

    if cfg.both_off() {
        clear(dir)?;
        return Ok(empty_settings());
    }

    fs::create_dir_all(dir)?;
    write_json(dir, &cfg)?;
    if cfg.embedding.provider == Provider::Off {
        let _ = fs::remove_file(embedding_key_path(dir));
    } else if let Some(key) = normalized.key.as_deref() {
        if key.is_empty() {
            let _ = fs::remove_file(embedding_key_path(dir));
        } else {
            write_embedding_secret(dir, key)?;
        }
    }
    Ok(to_settings(dir, &cfg))
}

pub fn clear(dir: &Path) -> LlmResult<()> {
    for path in [config_path(dir), key_path(dir), embedding_key_path(dir)] {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

pub fn clear_chat(dir: &Path) -> LlmResult<LlmSettings> {
    let mut cfg = load_config(dir)?;
    cfg.provider = Provider::Off;
    cfg.base_url.clear();
    cfg.model.clear();
    if cfg.both_off() {
        clear(dir)?;
        return Ok(empty_settings());
    }
    fs::create_dir_all(dir)?;
    write_json(dir, &cfg)?;
    let _ = fs::remove_file(key_path(dir));
    Ok(to_settings(dir, &cfg))
}

pub fn clear_embedding(dir: &Path) -> LlmResult<LlmSettings> {
    let mut cfg = load_config(dir)?;
    cfg.embedding = EmbeddingConfig::default();
    if cfg.both_off() {
        clear(dir)?;
        return Ok(empty_settings());
    }
    fs::create_dir_all(dir)?;
    write_json(dir, &cfg)?;
    let _ = fs::remove_file(embedding_key_path(dir));
    Ok(to_settings(dir, &cfg))
}

/// 测试连接用的 OpenAI 兼容 models 端点。
pub fn models_url(base_url: &str) -> String {
    format!("{}/models", base_url.trim_end_matches('/'))
}

const USER_AGENT: &str = "mudflat-knowledge/0.1 (https://github.com/mudflat-knowledge)";

pub fn http_client(timeout: std::time::Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(timeout)
        .user_agent(USER_AGENT)
        // OpenCode / Cloudflare 对 HTTP/2 POST 偶发 RST（os error 54）；chat 走 1.1 更稳。
        .http1_only()
        .pool_max_idle_per_host(0)
        .tcp_nodelay(true)
        .build()
        .map_err(|e| format!("无法创建连接: {e}"))
}

pub fn format_reqwest(err: reqwest::Error) -> String {
    let mut s = err.to_string();
    let mut src = std::error::Error::source(&err);
    while let Some(e) = src {
        s.push_str(" → ");
        s.push_str(&e.to_string());
        src = e.source();
    }
    s
}

pub fn describe_http_failure(action: &str, err: reqwest::Error) -> String {
    let detail = format_reqwest(err);
    if detail.contains("timed out") || detail.contains("timeout") {
        format!(
            "{action}超时。测试连接只打 /models，生成线索要等模型写完整张图，请再试一次；若反复超时，换更快的模型或检查网络。"
        )
    } else if detail.contains("reset by peer") || detail.contains("os error 54") || detail.contains("connection error") {
        format!(
            "{action}时连接被对端断开。多半是网关/CDN 掐了 HTTP 连接，不是模型名错。请再试一次；若反复出现，换直连供应商或检查代理。"
        )
    } else if detail.contains("dns") || detail.contains("failed to lookup") {
        format!("{action}失败：解析不到主机。{detail}")
    } else if detail.contains("certificate") || detail.contains("tls") || detail.contains("SSL") {
        format!("{action}失败：证书校验没通过。{detail}")
    } else {
        format!("{action}失败。{detail}")
    }
}

pub fn normalize_draft(draft: &LlmDraft, existing_key: Option<&str>) -> LlmResult<Normalized> {
    if draft.provider == Provider::Off {
        return Ok(Normalized {
            config: LlmConfig::default(),
            key: Some(String::new()),
        });
    }

    let base_url = resolve_base_url(draft.provider, &draft.base_url)?;
    let model = resolve_model(draft.provider, &draft.model)?;
    let incoming = draft.key.trim();
    let key = if !incoming.is_empty() {
        Some(incoming.to_string())
    } else {
        None
    };
    let effective = key.as_deref().or(existing_key).unwrap_or("");
    require_key(draft.provider, &base_url, effective)?;

    Ok(Normalized {
        config: LlmConfig {
            provider: draft.provider,
            base_url,
            model,
            embedding: EmbeddingConfig::default(),
            embedding_model: String::new(),
        },
        key,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedEmbedding {
    pub config: EmbeddingConfig,
    /// None = 沿用已存密钥；Some("") = 明确无密钥（仅 Ollama 回环允许）
    pub key: Option<String>,
}

pub fn normalize_embedding_draft(
    draft: &EmbeddingDraft,
    existing_key: Option<&str>,
    chat: &LlmConfig,
    chat_key: Option<&str>,
) -> LlmResult<NormalizedEmbedding> {
    if draft.provider == Provider::Off {
        return Ok(NormalizedEmbedding {
            config: EmbeddingConfig::default(),
            key: Some(String::new()),
        });
    }
    if draft.provider == Provider::Xai {
        return Err(LlmError::Invalid(
            "xAI 不提供向量模型。请改用 OpenAI、Ollama 或自定义接口。".into(),
        ));
    }

    let base_url = resolve_base_url(draft.provider, &draft.base_url)?;
    let model = resolve_embedding_model(draft.provider, &draft.model)?;
    let incoming = draft.key.trim();
    let key = if !incoming.is_empty() {
        Some(incoming.to_string())
    } else {
        None
    };
    let reuse_chat = draft.provider == chat.provider && base_url == chat.base_url;
    let effective = key
        .as_deref()
        .or(existing_key)
        .or(if reuse_chat { chat_key } else { None })
        .unwrap_or("");
    require_key(draft.provider, &base_url, effective)?;

    Ok(NormalizedEmbedding {
        config: EmbeddingConfig {
            provider: draft.provider,
            base_url,
            model,
        },
        key,
    })
}

/// 生成用的已保存配置。关闭或缺 Key 时给出和设置页一致的说明。
pub fn load_runtime(dir: &Path) -> LlmResult<(LlmConfig, String)> {
    let cfg = load_config(dir)?;
    if cfg.provider == Provider::Off {
        return Err(LlmError::Invalid(
            "还没有启用语言模型。到设置「四、语言模型」选择供应商并保存。".into(),
        ));
    }
    let key = get_key(dir).unwrap_or_default();
    require_key(cfg.provider, &cfg.base_url, &key)?;
    Ok((cfg, key))
}

/// 向量检索用的已保存配置。与主模型独立，缺配置时由调用方降级到规则检索。
pub fn load_embedding_runtime(dir: &Path) -> LlmResult<(EmbeddingConfig, String)> {
    let cfg = load_config(dir)?;
    if !embedding_ready(&cfg) {
        return Err(LlmError::Invalid(
            "还没有启用向量模型。到设置「五、向量检索」选择供应商并保存。".into(),
        ));
    }
    if cfg.embedding.provider == Provider::Xai {
        return Err(LlmError::Invalid(
            "xAI 不提供向量模型。到设置「五、向量检索」改选 OpenAI、Ollama 或自定义。".into(),
        ));
    }
    let key = embedding_key_for(dir, &cfg);
    require_key(cfg.embedding.provider, &cfg.embedding.base_url, &key)?;
    Ok((cfg.embedding, key))
}

pub fn chat_url(base_url: &str) -> String {
    format!("{}/chat/completions", base_url.trim_end_matches('/'))
}

pub fn embeddings_url(base_url: &str) -> String {
    format!("{}/embeddings", base_url.trim_end_matches('/'))
}

fn resolve_base_url(provider: Provider, raw: &str) -> LlmResult<String> {
    let trimmed = raw.trim();
    let url = if trimmed.is_empty() {
        let def = provider.default_base_url();
        if def.is_empty() {
            return Err(LlmError::Invalid("请填写接口地址，例如 https://api.deepseek.com/v1".into()));
        }
        def.to_string()
    } else {
        trimmed.trim_end_matches('/').to_string()
    };
    validate_endpoint(&url)?;
    Ok(url)
}

fn resolve_embedding_model(provider: Provider, raw: &str) -> LlmResult<String> {
    let trimmed = raw.trim();
    if !trimmed.is_empty() {
        if trimmed.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(LlmError::Invalid("向量模型名不能含空白或控制字符".into()));
        }
        return Ok(trimmed.to_string());
    }
    let def = provider.default_embedding_model();
    if def.is_empty() {
        return Err(LlmError::Invalid("请填写向量模型名".into()));
    }
    Ok(def.to_string())
}

fn resolve_model(provider: Provider, raw: &str) -> LlmResult<String> {
    let trimmed = raw.trim();
    if !trimmed.is_empty() {
        if trimmed.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(LlmError::Invalid("模型名不能含空白或控制字符".into()));
        }
        return Ok(trimmed.to_string());
    }
    let def = provider.default_model();
    if def.is_empty() {
        return Err(LlmError::Invalid("请填写模型名".into()));
    }
    Ok(def.to_string())
}

fn require_key(provider: Provider, base_url: &str, key: &str) -> LlmResult<()> {
    if !key.is_empty() {
        if key.chars().any(char::is_control) {
            return Err(LlmError::Invalid("API Key 含非法字符".into()));
        }
        return Ok(());
    }
    if provider == Provider::Ollama && is_loopback(base_url) {
        return Ok(());
    }
    Err(LlmError::Invalid("请填写语言模型 API Key".into()))
}

pub fn validate_endpoint(url: &str) -> LlmResult<()> {
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(LlmError::Invalid("接口地址含非法字符".into()));
    }
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("https://") {
        if url.len() <= "https://".len() {
            return Err(LlmError::Invalid("接口地址不完整".into()));
        }
        return Ok(());
    }
    if lower.starts_with("http://") {
        if is_loopback(url) {
            return Ok(());
        }
        return Err(LlmError::Invalid(
            "非本机地址必须使用 https。本机可用 http://127.0.0.1 或 http://localhost".into(),
        ));
    }
    Err(LlmError::Invalid("接口地址必须以 https:// 开头（本机可用 http://）".into()))
}

fn is_loopback(url: &str) -> bool {
    let rest = url
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(url);
    let host = rest.split('/').next().unwrap_or(rest);
    let host = host.rsplit_once('@').map(|(_, h)| h).unwrap_or(host);
    let host = if host.starts_with('[') {
        host.trim_start_matches('[')
            .split(']')
            .next()
            .unwrap_or(host)
    } else {
        host.split(':').next().unwrap_or(host)
    };
    let host = host.trim().to_ascii_lowercase();
    matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1" | "0.0.0.0")
}

fn write_json(dir: &Path, cfg: &LlmConfig) -> LlmResult<()> {
    let path = config_path(dir);
    let tmp = dir.join(format!("{CONFIG_FILE}.tmp"));
    let body = serde_json::to_vec_pretty(cfg)
        .map_err(|e| LlmError::Invalid(format!("序列化失败: {e}")))?;
    atomic_write(&tmp, &path, &body)?;
    Ok(())
}

fn write_secret(dir: &Path, key: &str) -> LlmResult<()> {
    let path = key_path(dir);
    let tmp = dir.join(format!("{KEY_FILE}.tmp"));
    atomic_write(&tmp, &path, key.as_bytes())?;
    Ok(())
}

fn write_embedding_secret(dir: &Path, key: &str) -> LlmResult<()> {
    let path = embedding_key_path(dir);
    let tmp = dir.join(format!("{EMBEDDING_KEY_FILE}.tmp"));
    atomic_write(&tmp, &path, key.as_bytes())?;
    Ok(())
}

fn atomic_write(tmp: &Path, final_path: &Path, bytes: &[u8]) -> LlmResult<()> {
    {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(tmp)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(tmp, fs::Permissions::from_mode(0o600))?;
        }
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    #[cfg(windows)]
    let _ = fs::remove_file(final_path);
    fs::rename(tmp, final_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mudflat-llm-test-{name}"));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn draft(provider: Provider, base: &str, model: &str, key: &str) -> LlmDraft {
        LlmDraft {
            provider,
            base_url: base.into(),
            model: model.into(),
            key: key.into(),
        }
    }

    fn embed_draft(provider: Provider, base: &str, model: &str, key: &str) -> EmbeddingDraft {
        EmbeddingDraft {
            provider,
            base_url: base.into(),
            model: model.into(),
            key: key.into(),
        }
    }

    #[test]
    fn off_is_default_and_clears_files() {
        let dir = temp_dir("off");
        save(
            &dir,
            &draft(Provider::Openai, "", "gpt-4o-mini", "sk-test"),
            None,
        )
        .unwrap();
        assert!(config_path(&dir).exists());
        assert!(key_path(&dir).exists());

        let out = save(&dir, &draft(Provider::Off, "", "", ""), None).unwrap();
        assert_eq!(out.provider, Provider::Off);
        assert!(!out.has_key);
        assert!(!config_path(&dir).exists());
        assert!(!key_path(&dir).exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn openai_fills_default_endpoint_and_requires_key() {
        let n = normalize_draft(&draft(Provider::Openai, "", "", "sk-abc"), None).unwrap();
        assert_eq!(n.config.base_url, "https://api.openai.com/v1");
        assert_eq!(n.config.model, "gpt-4o-mini");
        assert_eq!(n.config.embedding.provider, Provider::Off);
        assert_eq!(n.key.as_deref(), Some("sk-abc"));

        let err = normalize_draft(&draft(Provider::Openai, "", "", ""), None).unwrap_err();
        assert!(err.to_string().contains("API Key"));
    }

    #[test]
    fn openai_keeps_existing_key_when_input_blank() {
        let n = normalize_draft(&draft(Provider::Openai, "", "gpt-4o-mini", ""), Some("sk-old")).unwrap();
        assert!(n.key.is_none());
        assert_eq!(n.config.model, "gpt-4o-mini");
    }

    #[test]
    fn ollama_allows_missing_key_on_loopback() {
        let n = normalize_draft(&draft(Provider::Ollama, "", "", ""), None).unwrap();
        assert_eq!(n.config.base_url, "http://127.0.0.1:11434/v1");
        assert_eq!(n.config.model, "qwen2.5");
    }

    #[test]
    fn custom_https_requires_key_and_model() {
        let err = normalize_draft(
            &draft(Provider::Custom, "https://api.deepseek.com/v1", "deepseek-chat", ""),
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("API Key"));

        let n = normalize_draft(
            &draft(Provider::Custom, "https://api.deepseek.com/v1/", "deepseek-chat", "sk-1"),
            None,
        )
        .unwrap();
        assert_eq!(n.config.base_url, "https://api.deepseek.com/v1");
    }

    #[test]
    fn rejects_plain_http_on_public_hosts() {
        let err = validate_endpoint("http://api.openai.com/v1").unwrap_err();
        assert!(err.to_string().contains("https"));
    }

    #[test]
    fn allows_http_on_localhost_and_loopback() {
        validate_endpoint("http://127.0.0.1:11434/v1").unwrap();
        validate_endpoint("http://localhost:1234/v1").unwrap();
        validate_endpoint("https://api.openai.com/v1").unwrap();
    }

    #[test]
    fn rejects_non_http_schemes_and_control_chars() {
        assert!(validate_endpoint("javascript:alert(1)").is_err());
        assert!(validate_endpoint("https://api.openai.com/v1\n").is_err());
        assert!(validate_endpoint("").is_err());
    }

    #[test]
    fn models_url_joins_without_double_slash() {
        assert_eq!(
            models_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/models"
        );
        assert_eq!(
            models_url("http://127.0.0.1:11434/v1"),
            "http://127.0.0.1:11434/v1/models"
        );
        assert_eq!(
            chat_url("https://api.deepseek.com/v1/"),
            "https://api.deepseek.com/v1/chat/completions"
        );
        assert_eq!(
            embeddings_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/embeddings"
        );
    }

    #[test]
    fn xai_fills_chat_default_and_leaves_embedding_blank() {
        let n = normalize_draft(&draft(Provider::Xai, "", "", "xai-key"), None).unwrap();
        assert_eq!(n.config.base_url, "https://api.x.ai/v1");
        assert_eq!(n.config.model, "grok-4.5");
        assert_eq!(n.config.embedding.provider, Provider::Off);
    }

    #[test]
    fn xai_chat_can_pair_with_openai_embedding() {
        let dir = temp_dir("xai-embed");
        save(&dir, &draft(Provider::Xai, "", "", "xai-key"), None).unwrap();
        let out = save_embedding(&dir, &embed_draft(Provider::Openai, "", "", "sk-embed"), None).unwrap();
        assert_eq!(out.provider, Provider::Xai);
        assert_eq!(out.model, "grok-4.5");
        assert_eq!(out.embedding_provider, Provider::Openai);
        assert_eq!(out.embedding_model, "text-embedding-3-small");
        assert_eq!(out.embedding_base_url, "https://api.openai.com/v1");
        assert!(out.has_key);
        assert!(out.has_embedding_key);

        let (chat, chat_key) = load_runtime(&dir).unwrap();
        assert_eq!(chat.provider, Provider::Xai);
        assert_eq!(chat_key, "xai-key");
        let (emb, emb_key) = load_embedding_runtime(&dir).unwrap();
        assert_eq!(emb.model, "text-embedding-3-small");
        assert_eq!(emb_key, "sk-embed");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn turning_chat_off_keeps_embedding() {
        let dir = temp_dir("chat-off-keeps-embed");
        save(&dir, &draft(Provider::Xai, "", "", "xai-key"), None).unwrap();
        save_embedding(&dir, &embed_draft(Provider::Openai, "", "", "sk-embed"), None).unwrap();
        let out = save(&dir, &draft(Provider::Off, "", "", ""), None).unwrap();
        assert_eq!(out.provider, Provider::Off);
        assert!(!out.has_key);
        assert_eq!(out.embedding_provider, Provider::Openai);
        assert_eq!(out.embedding_model, "text-embedding-3-small");
        assert!(out.has_embedding_key);
        assert!(config_path(&dir).exists());
        assert!(!key_path(&dir).exists());
        assert!(embedding_key_path(&dir).exists());
        assert!(load_runtime(&dir).is_err());
        let (emb, key) = load_embedding_runtime(&dir).unwrap();
        assert_eq!(emb.provider, Provider::Openai);
        assert_eq!(key, "sk-embed");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn embedding_reuses_chat_key_on_same_endpoint() {
        let dir = temp_dir("reuse-key");
        save(&dir, &draft(Provider::Openai, "", "gpt-4o-mini", "sk-shared"), None).unwrap();
        let out = save_embedding(&dir, &embed_draft(Provider::Openai, "", "", ""), None).unwrap();
        assert_eq!(out.embedding_provider, Provider::Openai);
        assert_eq!(out.embedding_model, "text-embedding-3-small");
        assert!(out.has_embedding_key);
        assert!(!embedding_key_path(&dir).exists());
        let (_, key) = load_embedding_runtime(&dir).unwrap();
        assert_eq!(key, "sk-shared");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn embedding_rejects_xai() {
        let err = normalize_embedding_draft(
            &embed_draft(Provider::Xai, "", "", "xai-key"),
            None,
            &LlmConfig::default(),
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("不提供向量模型"));
    }

    #[test]
    fn legacy_openai_embedding_model_migrates() {
        let dir = temp_dir("legacy-embed");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            config_path(&dir),
            r#"{
              "provider": "openai",
              "baseUrl": "https://api.openai.com/v1",
              "model": "gpt-4o-mini",
              "embeddingModel": "text-embedding-3-small"
            }"#,
        )
        .unwrap();
        let cfg = load_config(&dir).unwrap();
        assert_eq!(cfg.embedding.provider, Provider::Openai);
        assert_eq!(cfg.embedding.model, "text-embedding-3-small");
        assert_eq!(cfg.embedding.base_url, "https://api.openai.com/v1");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn embedding_only_does_not_enable_chat() {
        let dir = temp_dir("embed-only");
        let out = save_embedding(&dir, &embed_draft(Provider::Ollama, "", "", ""), None).unwrap();
        assert_eq!(out.provider, Provider::Off);
        assert_eq!(out.embedding_provider, Provider::Ollama);
        assert_eq!(out.embedding_model, "nomic-embed-text");
        assert!(load_runtime(&dir).is_err());
        let (emb, key) = load_embedding_runtime(&dir).unwrap();
        assert_eq!(emb.provider, Provider::Ollama);
        assert!(key.is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn clear_chat_leaves_embedding() {
        let dir = temp_dir("clear-chat");
        save(&dir, &draft(Provider::Xai, "", "", "xai-key"), None).unwrap();
        save_embedding(&dir, &embed_draft(Provider::Openai, "", "", "sk-embed"), None).unwrap();
        let out = clear_chat(&dir).unwrap();
        assert_eq!(out.provider, Provider::Off);
        assert_eq!(out.embedding_provider, Provider::Openai);
        assert!(out.has_embedding_key);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn runtime_rejects_off_and_missing_key() {
        let dir = temp_dir("runtime-off");
        let err = load_runtime(&dir).unwrap_err();
        assert!(err.to_string().contains("还没有启用"));
        save(&dir, &draft(Provider::Openai, "", "gpt-4o-mini", "sk-abc"), None).unwrap();
        let (cfg, key) = load_runtime(&dir).unwrap();
        assert_eq!(cfg.provider, Provider::Openai);
        assert_eq!(key, "sk-abc");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_roundtrip_does_not_echo_key() {
        let dir = temp_dir("roundtrip");
        let out = save(
            &dir,
            &draft(Provider::Custom, "https://api.deepseek.com/v1", "deepseek-chat", " sk-secret "),
            None,
        )
        .unwrap();
        assert_eq!(out.provider, Provider::Custom);
        assert!(out.has_key);
        assert_eq!(out.base_url, "https://api.deepseek.com/v1");
        let loaded = load_settings(&dir).unwrap();
        assert_eq!(loaded, out);
        assert_eq!(get_key(&dir).unwrap(), "sk-secret");
        let raw = fs::read_to_string(config_path(&dir)).unwrap();
        assert!(!raw.contains("sk-secret"));
        fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn secret_file_is_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("perms");
        save(
            &dir,
            &draft(Provider::Openai, "", "gpt-4o-mini", "sk-abc"),
            None,
        )
        .unwrap();
        let mode = fs::metadata(key_path(&dir)).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    #[ignore = "hits the live OpenCode Go network"]
    async fn opencode_go_chat_is_reachable_without_key() {
        let http = http_client(std::time::Duration::from_secs(20)).unwrap();
        let resp = http
            .post("https://opencode.ai/zen/go/v1/chat/completions")
            .json(&serde_json::json!({
                "model": "deepseek-v4-flash",
                "messages": [{ "role": "user", "content": "hi" }]
            }))
            .send()
            .await
            .expect("reqwest should reach OpenCode Go");
        assert_eq!(resp.status().as_u16(), 401, "no key must be AuthError, not a transport failure");
    }
}
