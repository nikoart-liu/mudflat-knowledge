//! 从 GitHub Releases 判断是否有新版本，并下载本机对应安装包。
//!
//! 数据源是公开仓库的 `/releases/latest`（不含草稿与预发布）。
//! 版本比较只看 major.minor.patch；安装是打开下载好的包，不替换正在运行的进程。

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;

const REPO: &str = "nikoart-liu/mudflat-knowledge";
const USER_AGENT: &str = "mudflat-knowledge";

fn api_latest_url() -> String {
    format!("https://api.github.com/repos/{REPO}/releases/latest")
}

fn releases_page_url() -> String {
    format!("https://github.com/{REPO}/releases/latest")
}

fn download_prefix() -> String {
    format!("https://github.com/{REPO}/releases/download/")
}

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("网络错误: {0}")]
    Network(String),
    #[error("GitHub 请求过于频繁，请稍后再试")]
    RateLimited,
    #[error("{0}")]
    Failed(String),
}

pub type UpdateResult<T> = Result<T, UpdateError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Macos,
    Windows,
    Linux,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    Aarch64,
    X86_64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Platform {
    pub os: Os,
    pub arch: Arch,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub html_url: String,
    pub body: Option<String>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub assets: Vec<GitHubAsset>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub available: bool,
    pub notes: String,
    pub html_url: String,
    pub asset_name: Option<String>,
    pub asset_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEvent {
    pub stage: String,
    pub current: i64,
    pub total: i64,
}

impl Platform {
    pub fn current() -> Option<Self> {
        let os = if cfg!(target_os = "macos") {
            Os::Macos
        } else if cfg!(target_os = "windows") {
            Os::Windows
        } else if cfg!(target_os = "linux") {
            Os::Linux
        } else {
            return None;
        };
        let arch = if cfg!(target_arch = "aarch64") {
            Arch::Aarch64
        } else if cfg!(target_arch = "x86_64") {
            Arch::X86_64
        } else {
            return None;
        };
        Some(Self { os, arch })
    }
}

/// 解析 `v1.2.3` / `1.2` / `1.2.3-beta` 的数字三段。去前缀 v，预发布后缀忽略。
pub fn parse_version(raw: &str) -> Option<Version> {
    let s = raw.trim().trim_start_matches(['v', 'V']);
    let core = s.split(['-', '+']).next()?.trim();
    if core.is_empty() {
        return None;
    }
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(Version { major, minor, patch })
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

/// 为本机挑选安装包。macOS 优先 dmg，Linux 优先 AppImage，Windows 要 NSIS exe。
pub fn pick_asset(assets: &[GitHubAsset], platform: Platform) -> Option<&GitHubAsset> {
    assets
        .iter()
        .filter_map(|a| rank_asset(&a.name, platform).map(|rank| (rank, a)))
        .min_by_key(|(rank, _)| *rank)
        .map(|(_, a)| a)
}

fn rank_asset(name: &str, platform: Platform) -> Option<u8> {
    let n = name.to_ascii_lowercase();
    match (platform.os, platform.arch) {
        (Os::Macos, Arch::Aarch64) => {
            if !(n.contains("darwin") && n.contains("aarch64")) {
                return None;
            }
            if n.ends_with(".dmg") {
                Some(0)
            } else if n.ends_with(".app.tar.gz") {
                Some(1)
            } else {
                None
            }
        }
        (Os::Macos, Arch::X86_64) => {
            if !n.contains("darwin") || n.contains("aarch64") || n.contains("arm64") {
                return None;
            }
            if !(n.contains("_x64") || n.contains("x86_64")) {
                return None;
            }
            if n.ends_with(".dmg") {
                Some(0)
            } else if n.ends_with(".app.tar.gz") {
                Some(1)
            } else {
                None
            }
        }
        (Os::Windows, Arch::X86_64) => {
            if !n.contains("windows") || n.contains("arm") {
                return None;
            }
            if n.ends_with("-setup.exe") {
                Some(0)
            } else if n.ends_with(".exe") {
                Some(1)
            } else {
                None
            }
        }
        (Os::Linux, Arch::X86_64) => {
            if n.contains("aarch64") || n.contains("arm64") {
                return None;
            }
            let linuxish = n.contains("linux") || n.contains("amd64") || n.contains("x86_64");
            if !linuxish {
                return None;
            }
            if n.ends_with(".appimage") {
                Some(0)
            } else if n.ends_with(".deb") {
                Some(1)
            } else if n.ends_with(".rpm") {
                Some(2)
            } else {
                None
            }
        }
        (Os::Windows | Os::Linux, Arch::Aarch64) => None,
    }
}

pub fn evaluate(current: &str, release: &GitHubRelease, platform: Option<Platform>) -> UpdateInfo {
    let html_url = if release.html_url.is_empty() {
        releases_page_url()
    } else {
        release.html_url.clone()
    };
    if release.draft || release.prerelease {
        return UpdateInfo {
            current_version: current.to_string(),
            latest_version: current.to_string(),
            available: false,
            notes: String::new(),
            html_url,
            asset_name: None,
            asset_url: None,
        };
    }
    let latest = release.tag_name.trim();
    let available = is_newer(latest, current);
    let asset = platform.and_then(|p| pick_asset(&release.assets, p));
    UpdateInfo {
        current_version: current.trim_start_matches(['v', 'V']).to_string(),
        latest_version: latest.trim_start_matches(['v', 'V']).to_string(),
        available,
        notes: release.body.clone().unwrap_or_default(),
        html_url,
        asset_name: asset.map(|a| a.name.clone()),
        asset_url: asset.map(|a| a.browser_download_url.clone()),
    }
}

fn http_client(timeout: Duration) -> UpdateResult<reqwest::Client> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(timeout)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| UpdateError::Network(e.to_string()))
}

async fn fetch_latest_release() -> UpdateResult<Option<GitHubRelease>> {
    let http = http_client(Duration::from_secs(15))?;
    let resp = http
        .get(api_latest_url())
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    let status = resp.status().as_u16();
    if status == 404 {
        return Ok(None);
    }
    if status == 403 || status == 429 {
        return Err(UpdateError::RateLimited);
    }
    if !(200..300).contains(&status) {
        let body = resp.text().await.unwrap_or_default();
        let snippet: String = body.chars().take(120).collect();
        return Err(UpdateError::Failed(format!("GitHub 返回 {status}: {snippet}")));
    }
    let release = resp
        .json::<GitHubRelease>()
        .await
        .map_err(|e| UpdateError::Failed(format!("无法解析 GitHub 发布信息: {e}")))?;
    Ok(Some(release))
}

pub async fn check_latest(current: &str) -> UpdateResult<UpdateInfo> {
    match fetch_latest_release().await? {
        Some(release) => Ok(evaluate(current, &release, Platform::current())),
        None => Ok(UpdateInfo {
            current_version: current.trim_start_matches(['v', 'V']).to_string(),
            latest_version: current.trim_start_matches(['v', 'V']).to_string(),
            available: false,
            notes: String::new(),
            html_url: releases_page_url(),
            asset_name: None,
            asset_url: None,
        }),
    }
}

fn emit(chan: &Channel<UpdateEvent>, stage: &str, current: i64, total: i64) {
    let _ = chan.send(UpdateEvent {
        stage: stage.into(),
        current,
        total,
    });
}

fn allowed_download_url(url: &str) -> bool {
    url.starts_with(&download_prefix())
}

fn dest_path(name: &str) -> PathBuf {
    let safe: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '_' })
        .collect();
    std::env::temp_dir().join(if safe.is_empty() { "mudflat-update.bin".into() } else { safe })
}

pub async fn download_and_open(current: &str, on_progress: Channel<UpdateEvent>) -> UpdateResult<String> {
    let info = check_latest(current).await?;
    if !info.available {
        return Err(UpdateError::Failed("已经是最新版本".into()));
    }
    let (Some(name), Some(url)) = (info.asset_name.as_deref(), info.asset_url.as_deref()) else {
        return Err(UpdateError::Failed(format!(
            "没有本机对应的安装包，请到发布页下载：{}",
            info.html_url
        )));
    };
    if !allowed_download_url(url) {
        return Err(UpdateError::Failed("安装包地址不是本仓库的 GitHub Release".into()));
    }

    emit(&on_progress, "downloading", 0, 0);
    let http = http_client(Duration::from_secs(120))?;
    let resp = http
        .get(url)
        .send()
        .await
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    let status = resp.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(UpdateError::Failed(format!("下载失败：HTTP {status}")));
    }
    let total = resp.content_length().unwrap_or(0) as i64;
    emit(&on_progress, "downloading", 0, total);
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    let path = dest_path(name);
    std::fs::write(&path, &bytes).map_err(|e| UpdateError::Failed(format!("写入安装包失败: {e}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if name.to_ascii_lowercase().ends_with(".appimage") {
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
        }
    }
    emit(&on_progress, "downloading", bytes.len() as i64, total.max(bytes.len() as i64));
    emit(&on_progress, "opening", 0, 0);
    open_downloaded(&path)?;
    emit(&on_progress, "done", 0, 0);
    Ok("已打开安装包。装好后重新打开应用即可；本地卡片仍在本机，不会丢掉。".into())
}

fn open_downloaded(path: &Path) -> UpdateResult<()> {
    let mut cmd = open_command(path);
    match cmd.spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            Ok(())
        }
        Err(e) => Err(UpdateError::Failed(format!("打开安装包失败: {e}"))),
    }
}

#[cfg(target_os = "macos")]
fn open_command(path: &Path) -> std::process::Command {
    let mut cmd = std::process::Command::new("open");
    cmd.arg(path);
    cmd
}

#[cfg(target_os = "linux")]
fn open_command(path: &Path) -> std::process::Command {
    let mut cmd = std::process::Command::new("xdg-open");
    cmd.arg(path);
    cmd
}

#[cfg(target_os = "windows")]
fn open_command(path: &Path) -> std::process::Command {
    // NSIS 安装包直接拉起；explorer 对 .exe 往往只打开所在文件夹。
    std::process::Command::new(path)
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn open_command(_path: &Path) -> std::process::Command {
    panic!("当前平台不支持打开安装包");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> GitHubAsset {
        GitHubAsset {
            name: name.into(),
            browser_download_url: format!("{}v0.2.0/{name}", download_prefix()),
        }
    }

    fn release(tag: &str, names: &[&str]) -> GitHubRelease {
        GitHubRelease {
            tag_name: tag.into(),
            html_url: format!("https://github.com/{REPO}/releases/tag/{tag}"),
            body: Some("修复同步。".into()),
            draft: false,
            prerelease: false,
            assets: names.iter().copied().map(asset).collect(),
        }
    }

    const NAMES: &[&str] = &[
        "mudflat-knowledge_0.2.0_darwin_aarch64.app.tar.gz",
        "mudflat-knowledge_0.2.0_darwin_aarch64.dmg",
        "mudflat-knowledge_0.2.0_darwin_x64.app.tar.gz",
        "mudflat-knowledge_0.2.0_darwin_x64.dmg",
        "mudflat-knowledge_0.2.0_linux_amd64.AppImage",
        "mudflat-knowledge_0.2.0_linux_amd64.deb",
        "mudflat-knowledge_0.2.0_linux_x86_64.rpm",
        "mudflat-knowledge_0.2.0_windows_x64-setup.exe",
    ];

    #[test]
    fn parse_strips_v_and_prerelease_suffix() {
        assert_eq!(parse_version("v0.2.0"), Some(Version { major: 0, minor: 2, patch: 0 }));
        assert_eq!(parse_version("0.2.0-beta.1"), Some(Version { major: 0, minor: 2, patch: 0 }));
        assert_eq!(parse_version("1.2"), Some(Version { major: 1, minor: 2, patch: 0 }));
        assert_eq!(parse_version(""), None);
        assert_eq!(parse_version("latest"), None);
    }

    #[test]
    fn newer_only_when_latest_outranks_current() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("v0.1.0", "0.2.0"));
        assert!(!is_newer("not-a-version", "0.1.0"));
    }

    #[test]
    fn macos_arm_prefers_dmg_over_tarball() {
        let rel = release("v0.2.0", NAMES);
        let a = pick_asset(&rel.assets, Platform { os: Os::Macos, arch: Arch::Aarch64 }).unwrap();
        assert_eq!(a.name, "mudflat-knowledge_0.2.0_darwin_aarch64.dmg");
    }

    #[test]
    fn macos_intel_does_not_take_arm_build() {
        let rel = release("v0.2.0", NAMES);
        let a = pick_asset(&rel.assets, Platform { os: Os::Macos, arch: Arch::X86_64 }).unwrap();
        assert_eq!(a.name, "mudflat-knowledge_0.2.0_darwin_x64.dmg");
    }

    #[test]
    fn windows_picks_nsis_setup() {
        let rel = release("v0.2.0", NAMES);
        let a = pick_asset(&rel.assets, Platform { os: Os::Windows, arch: Arch::X86_64 }).unwrap();
        assert_eq!(a.name, "mudflat-knowledge_0.2.0_windows_x64-setup.exe");
    }

    #[test]
    fn linux_prefers_appimage_over_deb() {
        let rel = release("v0.2.0", NAMES);
        let a = pick_asset(&rel.assets, Platform { os: Os::Linux, arch: Arch::X86_64 }).unwrap();
        assert_eq!(a.name, "mudflat-knowledge_0.2.0_linux_amd64.AppImage");
    }

    #[test]
    fn evaluate_marks_update_when_tag_is_newer() {
        let rel = release("v0.2.0", NAMES);
        let info = evaluate("0.1.0", &rel, Some(Platform { os: Os::Macos, arch: Arch::Aarch64 }));
        assert!(info.available);
        assert_eq!(info.latest_version, "0.2.0");
        assert_eq!(info.current_version, "0.1.0");
        assert_eq!(info.asset_name.as_deref(), Some("mudflat-knowledge_0.2.0_darwin_aarch64.dmg"));
        assert!(info.asset_url.unwrap().starts_with(&download_prefix()));
        assert_eq!(info.notes, "修复同步。");
    }

    #[test]
    fn evaluate_ignores_same_version_and_prerelease() {
        let same = evaluate("0.2.0", &release("v0.2.0", NAMES), Platform::current());
        assert!(!same.available);

        let mut pre = release("v0.3.0", NAMES);
        pre.prerelease = true;
        let info = evaluate("0.2.0", &pre, Platform::current());
        assert!(!info.available);
        assert_eq!(info.latest_version, "0.2.0");
    }

    #[test]
    fn evaluate_keeps_update_even_without_this_platform_asset() {
        let rel = release("v0.2.0", &["notes.txt"]);
        let info = evaluate("0.1.0", &rel, Some(Platform { os: Os::Macos, arch: Arch::Aarch64 }));
        assert!(info.available);
        assert!(info.asset_url.is_none());
    }

    #[test]
    fn deserializes_github_release_json_ignoring_unknown_fields() {
        let raw = r#"{
            "url": "https://api.github.com/repos/nikoart-liu/mudflat-knowledge/releases/1",
            "tag_name": "v0.1.0",
            "html_url": "https://github.com/nikoart-liu/mudflat-knowledge/releases/tag/v0.1.0",
            "body": "自动构建的安装包。",
            "draft": false,
            "prerelease": false,
            "assets": [{
                "name": "mudflat-knowledge_0.1.0_darwin_aarch64.dmg",
                "browser_download_url": "https://github.com/nikoart-liu/mudflat-knowledge/releases/download/v0.1.0/mudflat-knowledge_0.1.0_darwin_aarch64.dmg",
                "size": 12
            }]
        }"#;
        let rel: GitHubRelease = serde_json::from_str(raw).unwrap();
        assert_eq!(rel.tag_name, "v0.1.0");
        assert_eq!(rel.assets[0].name, "mudflat-knowledge_0.1.0_darwin_aarch64.dmg");
        let info = evaluate("0.1.0", &rel, Some(Platform { os: Os::Macos, arch: Arch::Aarch64 }));
        assert!(!info.available);
    }

    #[test]
    fn download_url_must_be_this_repo_release() {
        assert!(allowed_download_url(
            "https://github.com/nikoart-liu/mudflat-knowledge/releases/download/v0.2.0/a.dmg"
        ));
        assert!(!allowed_download_url("https://evil.example/a.dmg"));
        assert!(!allowed_download_url("https://github.com/other/repo/releases/download/v1/a.dmg"));
    }
}
