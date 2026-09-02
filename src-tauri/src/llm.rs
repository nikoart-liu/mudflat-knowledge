//! 语言模型供应商配置：与微信读书 Key 分开存放。
//!
//! - 非密钥：`llm.json`（provider / baseUrl / model）
//! - 密钥：`llm.key`（0600，与 api.key 同一套原子写入）
//! 默认关闭。http 只允许回环地址；其余必须 https。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const CONFIG_FILE: &str = "llm.json";
const KEY_FILE: &str = "llm.key";

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
    Ollama,
    Custom,
}

impl Provider {
    pub fn default_base_url(self) -> &'static str {
        match self {
            Provider::Off => "",
            Provider::Openai => "https://api.openai.com/v1",
            Provider::Ollama => "http://127.0.0.1:11434/v1",
            Provider::Custom => "",
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Provider::Off => "",
            Provider::Openai => "gpt-4o-mini",
            Provider::Ollama => "qwen2.5",
            Provider::Custom => "",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfig {
    pub provider: Provider,
    pub base_url: String,
    pub model: String,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: Provider::Off,
            base_url: String::new(),
            model: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LlmSettings {
    pub provider: Provider,
    pub base_url: String,
    pub model: String,
    pub has_key: bool,
}

/// 用户保存/测试时提交的草稿。key 为空表示「沿用已存密钥」。
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

pub fn load_settings(dir: &Path) -> LlmResult<LlmSettings> {
    let config = load_config(dir)?;
    Ok(LlmSettings {
        provider: config.provider,
        base_url: config.base_url,
        model: config.model,
        has_key: has_key(dir),
    })
}

pub fn load_config(dir: &Path) -> LlmResult<LlmConfig> {
    match fs::read_to_string(config_path(dir)) {
        Ok(s) => {
            let cfg: LlmConfig = serde_json::from_str(&s)
                .map_err(|e| LlmError::Invalid(format!("语言模型配置损坏: {e}")))?;
            Ok(cfg)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(LlmConfig::default()),
        Err(e) => Err(e.into()),
    }
}

pub fn has_key(dir: &Path) -> bool {
    get_key(dir).is_ok()
}

pub fn get_key(dir: &Path) -> LlmResult<String> {
    match fs::read_to_string(key_path(dir)) {
        Ok(s) => {
            let s = s.trim().to_string();
            if s.is_empty() {
                Err(LlmError::Invalid("未保存语言模型 API Key".into()))
            } else {
                Ok(s)
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Err(LlmError::Invalid("未保存语言模型 API Key".into()))
        }
        Err(e) => Err(e.into()),
    }
}

pub fn save(dir: &Path, draft: &LlmDraft, existing_key: Option<&str>) -> LlmResult<LlmSettings> {
    let normalized = normalize_draft(draft, existing_key)?;
    fs::create_dir_all(dir)?;
    if normalized.config.provider == Provider::Off {
        clear(dir)?;
        return Ok(LlmSettings {
            provider: Provider::Off,
            base_url: String::new(),
            model: String::new(),
            has_key: false,
        });
    }
    write_json(dir, &normalized.config)?;
    if let Some(key) = normalized.key.as_deref() {
        if key.is_empty() {
            let _ = fs::remove_file(key_path(dir));
        } else {
            write_secret(dir, key)?;
        }
    }
    Ok(LlmSettings {
        provider: normalized.config.provider,
        base_url: normalized.config.base_url,
        model: normalized.config.model,
        has_key: has_key(dir),
    })
}

pub fn clear(dir: &Path) -> LlmResult<()> {
    match fs::remove_file(config_path(dir)) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    match fs::remove_file(key_path(dir)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// 测试连接用的 OpenAI 兼容 models 端点。
pub fn models_url(base_url: &str) -> String {
    format!("{}/models", base_url.trim_end_matches('/'))
}

const USER_AGENT: &str = "mudflat-knowledge/0.1 (https://github.com/mudflat-knowledge)";

pub fn http_client(timeout: std::time::Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(timeout)
        .user_agent(USER_AGENT)
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
    let key = match get_key(dir) {
        Ok(k) => k,
        Err(_) => String::new(),
    };
    require_key(cfg.provider, &cfg.base_url, &key)?;
    Ok((cfg, key))
}

pub fn chat_url(base_url: &str) -> String {
    format!("{}/chat/completions", base_url.trim_end_matches('/'))
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
