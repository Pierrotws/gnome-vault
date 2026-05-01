use std::{
    fs,
    path::{Path, PathBuf},
};

use gpgme::{Context, Data, Key, Protocol};
use zeroize::Zeroizing;

/// Secret key identity that can decrypt password-store entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpgRecipient {
    pub id: String,
    pub label: String,
}

/// Errors raised by GPGME operations and `.gpg-id` handling.
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
    Utf8Error,
    NoRecipients(PathBuf),
    NoKeys(String),
}

// Hand-rolled Debug matches Display's redaction posture so a stray
// `log::warn!("{err:?}")` or any panic via `.unwrap()` / `.expect()`
// (both render their cause via `Debug`) doesn't put gpgme stderr,
// key fingerprints, recipient ids, or absolute store paths into
// journalctl. Tests get verbose Debug under `cfg(test)` so
// `assert!(matches!(...))` panic messages stay useful.
#[cfg(not(test))]
impl std::fmt::Debug for PgpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            PgpError::ContextError(_) => "ContextError",
            PgpError::InvalidPath(_) => "InvalidPath",
            PgpError::ReadError { .. } => "ReadError",
            PgpError::LoadingError(_) => "LoadingError",
            PgpError::DecryptError(_) => "DecryptError",
            PgpError::EncryptError(_) => "EncryptError",
            PgpError::Utf8Error => "Utf8Error",
            PgpError::NoRecipients(_) => "NoRecipients",
            PgpError::NoKeys(_) => "NoKeys",
        };
        write!(f, "PgpError::{name}(<redacted>)")
    }
}

#[cfg(test)]
impl std::fmt::Debug for PgpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PgpError::ContextError(s) => f.debug_tuple("ContextError").field(s).finish(),
            PgpError::InvalidPath(p) => f.debug_tuple("InvalidPath").field(p).finish(),
            PgpError::ReadError { path, source } => f
                .debug_struct("ReadError")
                .field("path", path)
                .field("source", source)
                .finish(),
            PgpError::LoadingError(s) => f.debug_tuple("LoadingError").field(s).finish(),
            PgpError::DecryptError(s) => f.debug_tuple("DecryptError").field(s).finish(),
            PgpError::EncryptError(s) => f.debug_tuple("EncryptError").field(s).finish(),
            PgpError::Utf8Error => f.debug_struct("Utf8Error").finish(),
            PgpError::NoRecipients(p) => f.debug_tuple("NoRecipients").field(p).finish(),
            PgpError::NoKeys(s) => f.debug_tuple("NoKeys").field(s).finish(),
        }
    }
}

// Display intentionally drops the upstream gpgme error string from the
// variants that wrap one (ContextError, LoadingError, DecryptError,
// EncryptError, NoKeys). gpgme stderr can echo key fingerprints,
// recipient identifiers, and absolute store paths — none of which
// belong in a UI error toast. With Debug now redacted too (see above),
// neither `{err}` nor `{err:?}` puts the upstream string anywhere a
// log scrubber would have to handle.
impl std::fmt::Display for PgpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PgpError::ContextError(_) => f.write_str("Failed to initialize GPGME OpenPGP context"),
            PgpError::InvalidPath(_) => f.write_str("Invalid path"),
            PgpError::ReadError { source, .. } => write!(f, "Error reading file ({})", source.kind()),
            PgpError::LoadingError(_) => f.write_str("Error loading encrypted file"),
            PgpError::DecryptError(_) => f.write_str("Error decrypting data"),
            PgpError::EncryptError(_) => f.write_str("Error encrypting data"),
            PgpError::Utf8Error => f.write_str("Decrypted content is not valid UTF-8"),
            PgpError::NoRecipients(_) => f.write_str("No recipients found"),
            PgpError::NoKeys(_) => f.write_str("No usable keys found"),
        }
    }
}

impl std::error::Error for PgpError {}

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

    // Validate UTF-8 *in place* on the Zeroizing<Vec<u8>> so the
    // failure path drops `plain` (which wipes its bytes) rather than
    // moving them into a FromUtf8Error that we have to manually zero
    // after the fact.
    if std::str::from_utf8(&plain).is_err() {
        return Err(PgpError::Utf8Error);
    }
    let plain_vec: Vec<u8> = std::mem::take(&mut *plain);
    Ok(Zeroizing::new(
        String::from_utf8(plain_vec).expect("UTF-8 validated above"),
    ))
}

/// Encrypts plaintext for the configured recipient key IDs.
pub fn encrypt(plaintext: &str, recipient_ids: &[String]) -> Result<Vec<u8>, PgpError> {
    // Refuse empty recipient lists explicitly: depending on GPGME flags
    // an empty key set can produce symmetric-only or even unencrypted
    // output. We never want to silently weaken encryption.
    if recipient_ids.is_empty() {
        return Err(PgpError::NoKeys("no recipients configured".to_string()));
    }

    let mut ctx = Context::from_protocol(Protocol::OpenPgp)
        .map_err(|e| PgpError::ContextError(e.to_string()))?;

    let keys = recipient_ids
        .iter()
        .map(|id| ctx.get_key(id))
        .collect::<std::result::Result<Vec<Key>, gpgme::Error>>()
        .map_err(|e| PgpError::NoKeys(e.to_string()))?;

    // Re-validate every fetched key. setup_vault checks at init time but
    // a key can be revoked, expired, or otherwise marked bad after the
    // vault was created. Without this, gpgme's behavior on a bad-but-
    // resolvable key depends on the user's gpg.conf — and silently
    // producing ciphertext encrypted to an unusable key is the worst
    // failure mode (the entry only fails on next decrypt, when there
    // is no path back).
    if let Some(rejected) = keys
        .iter()
        .find(|key| !key.can_encrypt() || key.is_bad())
    {
        let id = rejected
            .fingerprint()
            .ok()
            .or_else(|| rejected.id().ok())
            .unwrap_or("<unknown>")
            .to_string();
        return Err(PgpError::NoKeys(format!(
            "recipient key {id} is unusable (revoked, expired, or disabled)"
        )));
    }

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
                .map(sanitize_user_id)
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
///
/// `entry_parent` MUST be inside `store_root` — both are canonicalized
/// before the containment check. If `entry_parent` resolves outside the
/// store root, the function returns `PgpError::InvalidPath` rather than
/// walking up to `/` and picking up a stray `.gpg-id` (e.g. `$HOME/.gpg-id`).
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

    // Reject anything that doesn't sit under the store root. Without
    // this, the upward walk below would eventually reach `/` and read
    // any stray `.gpg-id` it finds (the user's $HOME, /etc, ...). The
    // only current caller — write_entry_data — pre-validates via
    // ensure_inside_store, but this helper is `pub` and the contract
    // would be implicit.
    if !start.starts_with(&canonical_root) && !start.starts_with(store_root) {
        return Err(PgpError::InvalidPath(entry_parent.to_path_buf()));
    }

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

/// Strips control and bidirectional-override characters from a GPG user
/// id before it is shown in the UI. A malicious / pathological key UID
/// could otherwise inject e.g. `U+202E` (right-to-left override) and
/// visually disguise its real fingerprint in the recipient picker.
fn sanitize_user_id(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_control()
                || matches!(
                    c,
                    '\u{202A}'..='\u{202E}'
                        | '\u{2066}'..='\u{2069}'
                        | '\u{200E}'
                        | '\u{200F}'
                )
            {
                '?'
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod sanitize_tests {
    use super::sanitize_user_id;

    #[test]
    fn replaces_bidi_overrides_and_controls() {
        let cleaned = sanitize_user_id("Alice\u{202E}gnola.example");
        assert_eq!(cleaned, "Alice?gnola.example");
    }

    #[test]
    fn preserves_normal_unicode_letters() {
        let raw = "Élise <e@example>";
        let cleaned = sanitize_user_id(raw);
        assert_eq!(cleaned, raw);
    }
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
