use std::{
    fs,
    path::{Path, PathBuf},
};

use gpgme::{Context, Data, Key, Protocol};

#[derive(Debug)]
pub enum PgpError {
    ContextError(String),
    InvalidPath(PathBuf),
    LoadingError(String),
    DecryptError(String),
    EncryptError(String),
    Utf8Error(String),
    NoRecipients(PathBuf),
    NoKeys(String),
}

impl std::fmt::Display for PgpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PgpError::ContextError(err) => {
                write!(f, "Failed to initialize GPGME OpenPGP context: {err}")
            }
            PgpError::InvalidPath(path) => write!(f, "Invalid path: {}", path.display()),
            PgpError::LoadingError(str) => write!(f, "Error loading path: {str}"),
            PgpError::DecryptError(str) => write!(f, "Error decrypting data: {str}"),
            PgpError::EncryptError(str) => write!(f, "Error encrypting data: {str}"),
            PgpError::Utf8Error(str) => write!(f, "Decrypted content is not valid UTF-8: {str}"),
            PgpError::NoRecipients(path) => write!(f, "No recipients found at: {}", path.display()),
            PgpError::NoKeys(str) => write!(f, "No keys found: {str}"),
        }
    }
}

pub fn decrypt(path: &Path) -> Result<String, PgpError> {
    let mut ctx = Context::from_protocol(Protocol::OpenPgp)
        .map_err(|e| PgpError::ContextError(e.to_string()))?;

    let path_str = path
        .to_str()
        .ok_or_else(|| PgpError::InvalidPath(path.to_path_buf()))?;

    let mut cipher = Data::load(path_str).map_err(|e| PgpError::LoadingError(e.to_string()))?;

    let mut plain = Vec::<u8>::new();

    ctx.decrypt(&mut cipher, &mut plain)
        .map_err(|e| PgpError::DecryptError(e.to_string()))?;

    String::from_utf8(plain).map_err(|e| PgpError::Utf8Error(e.to_string()))
}

pub fn encrypt(plaintext: &str, recipient_ids: &[String]) -> Result<Vec<u8>, PgpError> {
    let mut ctx = Context::from_protocol(Protocol::OpenPgp)
        .map_err(|e| PgpError::ContextError(e.to_string()))?;

    let keys = recipient_ids
        .iter()
        .map(|id| ctx.get_key(id))
        .collect::<std::result::Result<Vec<Key>, gpgme::Error>>()
        .map_err(|e| PgpError::NoKeys(e.to_string()))?;

    let key_refs = keys.iter().collect::<Vec<_>>();

    let mut ciphertext = Vec::new();
    ctx.encrypt(key_refs, plaintext.as_bytes(), &mut ciphertext)
        .map_err(|e| PgpError::EncryptError(e.to_string()))?;

    Ok(ciphertext)
}

pub fn recipient_ids(store_dir: &Path) -> Result<Vec<String>, PgpError> {
    let gpg_id_path = store_dir.join(".gpg-id");
    let content = fs::read_to_string(&gpg_id_path)
        .map_err(|_| PgpError::InvalidPath(gpg_id_path.to_path_buf()))?;

    let ids = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with('#'))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    if ids.is_empty() {
        return Err(PgpError::NoRecipients(gpg_id_path.to_path_buf()));
    }

    Ok(ids)
}
