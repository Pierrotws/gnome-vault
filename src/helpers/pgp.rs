use std::{
    fs,
    path::{Path, PathBuf},
};

use std::io::Write;
use std::time::Duration;

use gpgme::{
    Context, CreateKeyFlags, Data, ExportMode, InteractionStatus, Interactor, Key, Protocol,
    Validity,
};
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
    /// The name or email typed for a new key cannot form a user id.
    InvalidUserId,
    KeyCreationError(String),
    ImportError(String),
    /// The imported file held no usable secret key (e.g. only a public
    /// key).
    NoSecretKey,
    ExportError(String),
    /// The user dismissed the passphrase prompt.
    Cancelled,
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
            PgpError::InvalidUserId => "InvalidUserId",
            PgpError::KeyCreationError(_) => "KeyCreationError",
            PgpError::ImportError(_) => "ImportError",
            PgpError::NoSecretKey => "NoSecretKey",
            PgpError::ExportError(_) => "ExportError",
            PgpError::Cancelled => "Cancelled",
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
            PgpError::InvalidUserId => f.write_str("InvalidUserId"),
            PgpError::KeyCreationError(s) => f.debug_tuple("KeyCreationError").field(s).finish(),
            PgpError::ImportError(s) => f.debug_tuple("ImportError").field(s).finish(),
            PgpError::NoSecretKey => f.write_str("NoSecretKey"),
            PgpError::ExportError(s) => f.debug_tuple("ExportError").field(s).finish(),
            PgpError::Cancelled => f.write_str("Cancelled"),
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
            PgpError::ReadError { source, .. } => {
                write!(f, "Error reading file ({})", source.kind())
            }
            PgpError::LoadingError(_) => f.write_str("Error loading encrypted file"),
            PgpError::DecryptError(_) => f.write_str("Error decrypting data"),
            PgpError::EncryptError(_) => f.write_str("Error encrypting data"),
            PgpError::Utf8Error => f.write_str("Decrypted content is not valid UTF-8"),
            PgpError::NoRecipients(_) => f.write_str("No recipients found"),
            PgpError::NoKeys(_) => f.write_str("No usable keys found"),
            PgpError::InvalidUserId => f.write_str(
                "Enter a name, and optionally a valid email, without “<”, “>” or line breaks",
            ),
            PgpError::KeyCreationError(_) => f.write_str("Could not create the key"),
            PgpError::ImportError(_) => f.write_str("Could not import this file"),
            PgpError::NoSecretKey => f.write_str(
                "This file has no secret key. Import the backup of your secret key, not a public key.",
            ),
            PgpError::ExportError(_) => f.write_str("Could not save the key backup"),
            PgpError::Cancelled => f.write_str("Cancelled"),
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
    if let Some(rejected) = keys.iter().find(|key| !key.can_encrypt() || key.is_bad()) {
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

fn openpgp_context() -> Result<Context, PgpError> {
    Context::from_protocol(Protocol::OpenPgp).map_err(|e| PgpError::ContextError(e.to_string()))
}

/// Maps a gpgme failure, keeping a dismissed pinentry apart so the UI
/// does not report it as an error.
fn op_error(make: fn(String) -> PgpError) -> impl Fn(gpgme::Error) -> PgpError {
    move |err| {
        if err.code() == gpgme::Error::CANCELED.code() {
            PgpError::Cancelled
        } else {
            make(err.to_string())
        }
    }
}

/// Builds the user id of a new key: `Name <email>`, or `Name` alone.
pub fn key_user_id(name: &str, email: &str) -> Result<String, PgpError> {
    let name = name.trim();
    let email = email.trim();
    let bad = |text: &str| text.chars().any(|c| c == '<' || c == '>' || c.is_control());
    let email_ok = email.is_empty()
        || (email.contains('@') && !email.contains(char::is_whitespace) && !bad(email));
    if name.is_empty() || bad(name) || !email_ok {
        return Err(PgpError::InvalidUserId);
    }
    Ok(if email.is_empty() {
        name.to_string()
    } else {
        format!("{name} <{email}>")
    })
}

/// Creates a key pair with gpg's default algorithms (a primary key and
/// an encryption subkey) and no expiry, and returns its fingerprint.
///
/// gpg-agent asks for the passphrase through pinentry: Passel never sees
/// it. No expiry because an expired key silently stops a password
/// manager from saving entries, which non-expert users cannot recover
/// from; the backup and the passphrase are what protect the key.
pub fn create_key(name: &str, email: &str) -> Result<String, PgpError> {
    let user_id = key_user_id(name, email)?;
    create_key_in(&mut openpgp_context()?, &user_id, CreateKeyFlags::NOEXPIRE)
}

fn create_key_in(
    ctx: &mut Context,
    user_id: &str,
    flags: CreateKeyFlags,
) -> Result<String, PgpError> {
    let result = ctx
        .create_key_with_flags(user_id, "default", Duration::ZERO, flags)
        .map_err(op_error(PgpError::KeyCreationError))?;
    result
        .fingerprint()
        .map(str::to_string)
        .map_err(|_| PgpError::KeyCreationError("no fingerprint returned".to_string()))
}

/// Imports a key file and returns the fingerprints of the secret keys it
/// held that can encrypt. They are marked as ultimately trusted, like a
/// key created on this computer: otherwise gpg refuses to encrypt to them
/// ("no assurance this key belongs to the named user").
pub fn import_secret_keys(path: &Path) -> Result<Vec<String>, PgpError> {
    let path_str = path
        .to_str()
        .ok_or_else(|| PgpError::InvalidPath(path.to_path_buf()))?;
    let data = Data::load(path_str).map_err(|e| PgpError::LoadingError(e.to_string()))?;
    import_secret_keys_in(&mut openpgp_context()?, data)
}

fn import_secret_keys_in(ctx: &mut Context, data: Data<'_>) -> Result<Vec<String>, PgpError> {
    let result = ctx.import(data).map_err(op_error(PgpError::ImportError))?;
    let mut fingerprints = result
        .imports()
        .filter(|import| import.result().is_ok())
        .filter_map(|import| import.fingerprint().ok().map(str::to_string))
        .collect::<Vec<_>>();
    fingerprints.dedup();

    let mut usable = Vec::new();
    for fingerprint in fingerprints {
        let Ok(key) = ctx.get_secret_key(fingerprint.as_str()) else {
            continue;
        };
        if !key.can_encrypt() || key.is_bad() {
            continue;
        }
        if key.owner_trust() != Validity::Ultimate {
            ctx.interact(&key, UltimateTrust::default(), Vec::new())
                .map_err(op_error(PgpError::ImportError))?;
        }
        usable.push(fingerprint);
    }
    if usable.is_empty() {
        return Err(PgpError::NoSecretKey);
    }
    Ok(usable)
}

/// Answers gpg's `--edit-key` prompts to set a key's owner trust to
/// ultimate (`trust`, `5`, `y`, `quit`).
#[derive(Default)]
struct UltimateTrust {
    asked: bool,
}

impl Interactor for UltimateTrust {
    fn interact(
        &mut self,
        status: InteractionStatus<'_>,
        out: Option<&mut dyn Write>,
    ) -> Result<(), gpgme::Error> {
        let keyword = status.keyword().unwrap_or("");
        let prompt = status.args().unwrap_or("");
        let reply = match (keyword, prompt) {
            ("GET_LINE", "keyedit.prompt") if self.asked => "quit",
            ("GET_LINE", "keyedit.prompt") => {
                self.asked = true;
                "trust"
            }
            ("GET_LINE", "edit_ownertrust.value") => "5",
            ("GET_BOOL", "edit_ownertrust.set_ultimate.okay" | "keyedit.save.okay") => "y",
            _ => return Ok(()),
        };
        if let Some(out) = out {
            writeln!(out, "{reply}")?;
        }
        Ok(())
    }
}

/// Writes an armored backup of a secret key to `dest` (owner-only
/// permissions). gpg-agent asks for the key's passphrase first.
pub fn export_secret_key(fingerprint: &str, dest: &Path) -> Result<(), PgpError> {
    let backup = export_secret_key_in(&mut openpgp_context()?, fingerprint)?;
    write_private_file(dest, &backup).map_err(|e| PgpError::ExportError(e.to_string()))
}

fn export_secret_key_in(
    ctx: &mut Context,
    fingerprint: &str,
) -> Result<Zeroizing<Vec<u8>>, PgpError> {
    ctx.set_armor(true);
    let key = ctx
        .get_secret_key(fingerprint)
        .map_err(|e| PgpError::NoKeys(e.to_string()))?;
    let mut backup = Zeroizing::new(Vec::new());
    ctx.export_keys([&key], ExportMode::SECRET, &mut *backup)
        .map_err(op_error(PgpError::ExportError))?;
    if backup.is_empty() {
        // gpg exports nothing when the passphrase prompt is dismissed.
        return Err(PgpError::Cancelled);
    }
    Ok(backup)
}

fn write_private_file(dest: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(dest)?;
    file.write_all(contents)?;
    file.sync_all()
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
        std::env::temp_dir().join(format!("passel-{name}-{}-{unique}", std::process::id()))
    }

    /// A gpgme context on a throwaway GnuPG home.
    fn context_in(home: &Path) -> Context {
        use std::os::unix::fs::PermissionsExt;
        fs::create_dir_all(home).unwrap();
        fs::set_permissions(home, fs::Permissions::from_mode(0o700)).unwrap();
        let mut ctx = Context::from_protocol(Protocol::OpenPgp).unwrap();
        ctx.set_engine_home_dir(home.to_str().unwrap()).unwrap();
        ctx
    }

    fn stop_agent(home: &Path) {
        let _ = std::process::Command::new("gpgconf")
            .arg("--homedir")
            .arg(home)
            .args(["--kill", "gpg-agent"])
            .status();
    }

    #[test]
    fn builds_key_user_ids() {
        assert_eq!(
            key_user_id(" Ada Lovelace ", " ada@example.org ").unwrap(),
            "Ada Lovelace <ada@example.org>"
        );
        assert_eq!(key_user_id("Ada", "").unwrap(), "Ada");
        for (name, email) in [
            ("", "ada@example.org"),
            ("Ada <x>", ""),
            ("Ada", "not-an-email"),
            ("Ada", "a da@example.org"),
            ("Ada\nEvil", ""),
        ] {
            assert!(matches!(
                key_user_id(name, email),
                Err(PgpError::InvalidUserId)
            ));
        }
    }

    #[test]
    fn created_key_survives_a_backup_and_restore() {
        let root = temp_dir("gpg-keys");
        let (home_a, home_b, home_c) = (root.join("a"), root.join("b"), root.join("c"));

        let mut a = context_in(&home_a);
        let flags = CreateKeyFlags::NOEXPIRE | CreateKeyFlags::NOPASSWD;
        let fingerprint =
            create_key_in(&mut a, "Passel Tests <tests@example.invalid>", flags).unwrap();
        let key = a.get_secret_key(fingerprint.as_str()).unwrap();
        assert!(
            key.can_encrypt(),
            "default algorithms add an encryption subkey"
        );
        assert!(key
            .subkeys()
            .all(|subkey| subkey.expiration_time().is_none()));

        let backup = export_secret_key_in(&mut a, &fingerprint).unwrap();
        assert!(backup.starts_with(b"-----BEGIN PGP PRIVATE KEY BLOCK-----"));

        // Restoring the backup elsewhere yields a key gpg encrypts to.
        let mut b = context_in(&home_b);
        let imported =
            import_secret_keys_in(&mut b, Data::from_bytes(&backup[..]).unwrap()).unwrap();
        assert_eq!(imported, std::slice::from_ref(&fingerprint));
        let restored = b.get_key(fingerprint.as_str()).unwrap();
        assert_eq!(restored.owner_trust(), Validity::Ultimate);
        let mut ciphertext = Vec::new();
        b.encrypt([&restored], "secret", &mut ciphertext).unwrap();
        assert!(!ciphertext.is_empty());

        // A public key alone cannot open a vault.
        let mut public = Vec::new();
        a.export_keys([&key], ExportMode::empty(), &mut public)
            .unwrap();
        let mut c = context_in(&home_c);
        assert!(matches!(
            import_secret_keys_in(&mut c, Data::from_bytes(&public[..]).unwrap()),
            Err(PgpError::NoSecretKey)
        ));

        for home in [&home_a, &home_b, &home_c] {
            stop_agent(home);
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn writes_backups_readable_by_the_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("gpg-backup-file");
        fs::create_dir_all(&dir).unwrap();
        let dest = dir.join("key.asc");
        fs::write(&dest, "old, longer contents").unwrap();
        write_private_file(&dest, b"new").unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"new");
        fs::set_permissions(&dest, fs::Permissions::from_mode(0o644)).unwrap();
        fs::remove_file(&dest).unwrap();
        write_private_file(&dest, b"new").unwrap();
        assert_eq!(
            fs::metadata(&dest).unwrap().permissions().mode() & 0o777,
            0o600
        );
        fs::remove_dir_all(dir).unwrap();
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
