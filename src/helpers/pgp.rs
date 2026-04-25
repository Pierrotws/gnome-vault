use std::{
    fs,
    path::{Path, PathBuf},
};

use gpgme::{Context, Data, Key, Protocol};

/// Errors raised by GPGME operations and `.gpg-id` handling.
#[derive(Debug)]
pub enum PgpError {
    ContextError(String),
    InvalidPath(PathBuf),
    ReadError {
        path: PathBuf,
        source: std::io::Error,
    },
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
            PgpError::ReadError { path, source } => {
                write!(f, "Error reading {}: {source}", path.display())
            }
            PgpError::LoadingError(str) => write!(f, "Error loading path: {str}"),
            PgpError::DecryptError(str) => write!(f, "Error decrypting data: {str}"),
            PgpError::EncryptError(str) => write!(f, "Error encrypting data: {str}"),
            PgpError::Utf8Error(str) => write!(f, "Decrypted content is not valid UTF-8: {str}"),
            PgpError::NoRecipients(path) => write!(f, "No recipients found at: {}", path.display()),
            PgpError::NoKeys(str) => write!(f, "No keys found: {str}"),
        }
    }
}

/// Decrypts an encrypted password-store file into UTF-8 plaintext.
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

/// Encrypts plaintext for the configured recipient key IDs.
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

/// Reads recipient key IDs from the password-store `.gpg-id` file.
///
/// Empty lines and comment lines beginning with `#` are ignored.
pub fn recipient_ids(store_dir: &Path) -> Result<Vec<String>, PgpError> {
    let gpg_id_path = store_dir.join(".gpg-id");
    let content = fs::read_to_string(&gpg_id_path).map_err(|source| PgpError::ReadError {
        path: gpg_id_path.to_path_buf(),
        source,
    })?;

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

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "gnome-vault-{name}-{}-{unique}",
            std::process::id()
        ))
    }

    #[test]
    fn reads_recipient_ids_from_gpg_id() {
        let dir = temp_dir("pgp-recipients");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(".gpg-id"),
            "\n# comment\nalice@example.invalid\n\nbob@example.invalid\n",
        )
        .unwrap();

        let ids = recipient_ids(&dir).unwrap();

        assert_eq!(ids, vec!["alice@example.invalid", "bob@example.invalid"]);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_empty_gpg_id() {
        let dir = temp_dir("pgp-empty-recipients");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(".gpg-id"), "\n# comment\n\n").unwrap();

        let err = recipient_ids(&dir).unwrap_err();

        assert!(matches!(err, PgpError::NoRecipients(_)));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reports_missing_gpg_id_read_error() {
        let dir = temp_dir("pgp-missing-recipients");
        fs::create_dir_all(&dir).unwrap();

        let err = recipient_ids(&dir).unwrap_err();

        assert!(matches!(err, PgpError::ReadError { .. }));

        fs::remove_dir_all(dir).unwrap();
    }
}
