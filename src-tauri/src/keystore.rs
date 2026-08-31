//! API Key 本机文件存取：应用数据目录下的 `api.key`（权限 0600）。
//!
//! 不再依赖 macOS 钥匙串，改为随 SQLite 同目录的明文小文件，
//! 便于备份/迁移，也保持跨平台（Linux / Windows 同样可用）。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "api.key";

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("读写 Key 文件失败: {0}")]
    Io(#[from] std::io::Error),
    #[error("未保存 API Key")]
    NotFound,
}

fn key_path(dir: &Path) -> PathBuf {
    dir.join(FILE_NAME)
}

/// 原子写入：先写临时文件（0600）再改名，避免并发读到半个文件。
pub fn set_key(dir: &Path, key: &str) -> Result<(), KeyError> {
    let path = key_path(dir);
    let tmp = dir.join(format!("{FILE_NAME}.tmp"));
    {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
        }
        f.write_all(key.trim().as_bytes())?;
        f.sync_all()?;
    }
    #[cfg(windows)]
    let _ = fs::remove_file(&path); // Windows 的 rename 不覆盖已存在文件
    fs::rename(&tmp, &path)?;
    Ok(())
}

pub fn get_key(dir: &Path) -> Result<String, KeyError> {
    match fs::read_to_string(key_path(dir)) {
        Ok(s) => {
            let s = s.trim().to_string();
            if s.is_empty() {
                Err(KeyError::NotFound)
            } else {
                Ok(s)
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(KeyError::NotFound),
        Err(e) => Err(e.into()),
    }
}

pub fn clear_key(dir: &Path) -> Result<(), KeyError> {
    match fs::remove_file(key_path(dir)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

pub fn has_key(dir: &Path) -> bool {
    get_key(dir).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        // 用测试名区分目录，避免并行测试互相 remove_dir_all。
        let d = std::env::temp_dir().join(format!("mudflat-keystore-test-{name}"));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn set_get_clear_roundtrip() {
        let dir = temp_dir("roundtrip");
        assert!(matches!(get_key(&dir), Err(KeyError::NotFound)));
        set_key(&dir, "  wrk-abc123  ").unwrap();
        assert_eq!(get_key(&dir).unwrap(), "wrk-abc123");
        assert!(has_key(&dir));
        clear_key(&dir).unwrap();
        clear_key(&dir).unwrap(); // 重复清除不报错
        assert!(!has_key(&dir));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn overwrite_existing_key() {
        let dir = temp_dir("overwrite");
        set_key(&dir, "wrk-old").unwrap();
        set_key(&dir, "wrk-new").unwrap();
        assert_eq!(get_key(&dir).unwrap(), "wrk-new");
        fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn key_file_is_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("perms");
        set_key(&dir, "wrk-abc").unwrap();
        let mode = fs::metadata(key_path(&dir)).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        fs::remove_dir_all(&dir).ok();
    }
}
