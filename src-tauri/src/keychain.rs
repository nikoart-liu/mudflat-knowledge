//! macOS 钥匙串存取微信读书 API Key。

const SERVICE: &str = "mudflat-knowledge";
const ACCOUNT: &str = "weread-api-key";

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("钥匙串错误: {0}")]
    Keyring(#[from] keyring::Error),
    #[error("未保存 API Key")]
    NotFound,
}

pub fn set_key(key: &str) -> Result<(), KeyError> {
    let entry = keyring::Entry::new(SERVICE, ACCOUNT)?;
    entry.set_password(key)?;
    Ok(())
}

pub fn get_key() -> Result<String, KeyError> {
    let entry = keyring::Entry::new(SERVICE, ACCOUNT)?;
    match entry.get_password() {
        Ok(k) => Ok(k),
        Err(keyring::Error::NoEntry) => Err(KeyError::NotFound),
        Err(e) => Err(e.into()),
    }
}

pub fn clear_key() -> Result<(), KeyError> {
    let entry = keyring::Entry::new(SERVICE, ACCOUNT)?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

pub fn has_key() -> bool {
    matches!(get_key(), Ok(_))
}
