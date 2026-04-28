use std::{
    fs,
    path::{Path, PathBuf},
};

use gpgme::{Context, Data, Key, Protocol};
use zeroize::{Zeroize, Zeroizing};

/// Secret key identity that can decrypt password-store entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpgRecipient {
    pub id: String,
    pub label: String,
}

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
///
/// The returned plaintext is wrapped in [`Zeroizing`] so its memory is wiped
/// when dropped.
pub fn decrypt(path: &Path) -> Result<Zeroizing<String>, PgpError> {
    let mut ctx = Context::from_protocol(Protocol::OpenPgp)
        .map_err(|e| PgpError::ContextError(e.to_string()))?;

    let path_str = path
        .to_str()
        .ok_or_else(|| PgpError::InvalidPath(path.to_path_buf()))?;

    let mut cipher = Data::load(path_str).map_err(|e| PgpError::LoadingError(e.to_string()))?;

    let mut plain = Zeroizing::new(Vec::<u8>::new());

    ctx.decrypt(&mut cipher, &mut *plain)
        .map_err(|e| PgpError::DecryptError(e.to_string()))?;

    let plain_vec: Vec<u8> = std::mem::take(&mut *plain);
    String::from_utf8(plain_vec)
        .map(Zeroizing::new)
        .map_err(|e| {
            let mut bytes = e.into_bytes();
            bytes.zeroize();
            PgpError::Utf8Error("decrypted content is not valid UTF-8".to_string())
        })
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

/// Lists usable local secret keys that can decrypt password-store entries.
pub fn available_recipients() -> Result<Vec<GpgRecipient>, PgpError> {
    let mut ctx = Context::from_protocol(Protocol::OpenPgp)
        .map_err(|e| PgpError::ContextError(e.to_string()))?;

    let recipients = ctx
        .secret_keys()
        .map_err(|e| PgpError::NoKeys(e.to_string()))?
        .filter_map(Result::ok)
        .filter(|key| key.can_encrypt() && !key.is_bad())
        .filter_map(|key| {
            let id = key
                .fingerprint()
                .ok()
                .or_else(|| key.id().ok())?
                .to_string();
            let user_id = key
                .user_ids()
                .next()
                .and_then(|user_id| user_id.id().ok())
                .map(str::to_string)
                .unwrap_or_else(|| id.clone());
            let label = if user_id == id {
                id.clone()
            } else {
                format!("{user_id} ({id})")
            };

            Some(GpgRecipient { id, label })
        })
        .collect::<Vec<_>>();

    if recipients.is_empty() {
        return Err(PgpError::NoKeys(
            "No usable local secret encryption keys found".to_string(),
        ));
    }

    Ok(recipients)
}

/// Reads recipient key IDs from the password-store root `.gpg-id` file.
///
/// Empty lines and comment lines beginning with `#` are ignored. Entry-write
/// code paths should prefer [`recipient_ids_for`], which honors per-folder
/// `.gpg-id` overrides like `pass(1)`.
#[allow(dead_code)]
pub fn recipient_ids(store_dir: &Path) -> Result<Vec<String>, PgpError> {
    read_gpg_id_file(&store_dir.join(".gpg-id"))
}

/// Reads recipient key IDs by walking from `entry_parent` upward, taking the
/// first `.gpg-id` found and stopping at (and including) `store_root`.
///
/// This mirrors `pass(1)` semantics, where each subdirectory may override its
/// recipients by providing its own `.gpg-id` file.
pub fn recipient_ids_for(store_root: &Path, entry_parent: &Path) -> Result<Vec<String>, PgpError> {
    // Use canonicalized variants when both sides exist so that ancestor checks
    // are robust against path quirks (trailing components, `.` segments, etc.).
    let canonical_root = store_root
        .canonicalize()
        .unwrap_or_else(|_| store_root.to_path_buf());

    let start = if entry_parent.exists() {
        entry_parent
            .canonicalize()
            .unwrap_or_else(|_| entry_parent.to_path_buf())
    } else {
        entry_parent.to_path_buf()
    };

    let mut current: Option<&Path> = Some(start.as_path());
    while let Some(dir) = current {
        let candidate = dir.join(".gpg-id");
        if candidate.is_file() {
            return read_gpg_id_file(&candidate);
        }
        if dir == canonical_root || dir == store_root {
            break;
        }
        current = dir.parent();
    }

    // Fallback to the root `.gpg-id` (will produce a useful ReadError if
    // missing).
    read_gpg_id_file(&store_root.join(".gpg-id"))
}

fn read_gpg_id_file(gpg_id_path: &Path) -> Result<Vec<String>, PgpError> {
    let content = fs::read_to_string(gpg_id_path).map_err(|source| PgpError::ReadError {
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

    #[test]
    fn per_folder_gpg_id_overrides_root() {
        let dir = temp_dir("pgp-per-folder-recipients");
        let sub = dir.join("sub");
        fs::create_dir_all(&sub).unwrap();
        fs::write(dir.join(".gpg-id"), "root@example.invalid\n").unwrap();
        fs::write(sub.join(".gpg-id"), "sub@example.invalid\n").unwrap();

        let from_sub = recipient_ids_for(&dir, &sub).unwrap();
        assert_eq!(from_sub, vec!["sub@example.invalid"]);

        let from_root = recipient_ids_for(&dir, &dir).unwrap();
        assert_eq!(from_root, vec!["root@example.invalid"]);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn per_folder_walks_up_to_root_when_subfolder_has_none() {
        let dir = temp_dir("pgp-walk-up-recipients");
        let sub = dir.join("a").join("b");
        fs::create_dir_all(&sub).unwrap();
        fs::write(dir.join(".gpg-id"), "root@example.invalid\n").unwrap();

        let ids = recipient_ids_for(&dir, &sub).unwrap();
        assert_eq!(ids, vec!["root@example.invalid"]);

        fs::remove_dir_all(dir).unwrap();
    }
}
