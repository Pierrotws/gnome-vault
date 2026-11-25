//! Passel's mobile API, exported to Kotlin (and later Swift) by UniFFI.
//!
//! One [`Passel`] per process owns everything a phone needs and a
//! desktop gets from the system: an rPGP keyring in the app's private
//! storage, credentials for the vault's remote, and the vault itself (a
//! clone of the user's `pass` repository). Every method blocks: call them
//! off the UI thread. Passphrases are asked through [`PassphrasePrompt`],
//! which the app implements, from the thread running the call.

use std::{
    fmt,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
};

use passel_core::{
    helpers::{
        git::{self, Credential, CredentialProvider, CredentialRequest, StoredCredentials},
        otp,
        password::{self, PasswordMode},
        pgp::{
            self,
            rpgp::{PassphraseProvider, RpgpBackend},
            PgpBackend, PgpError, SecretKeyInfo,
        },
    },
    pass::{
        model::{EntryData, EntryField, PassNode, PassNodeKind},
        store::{self, StoreError},
    },
};
use zeroize::Zeroizing;

uniffi::setup_scaffolding!();

/// Why a call failed. Messages are written for the user.
#[derive(Debug, uniffi::Error)]
pub enum PasselError {
    /// The user dismissed a passphrase prompt.
    Cancelled,
    /// Syncing found these entries changed both here and on the remote;
    /// nothing local was changed. This device's changes were sent as
    /// branch `kept_as` (if that worked): merging it on a computer, then
    /// syncing here, settles it.
    Conflict {
        entries: Vec<String>,
        kept_as: Option<String>,
    },
    /// The remote refused the credentials (or there were none).
    AccessDenied,
    /// Anything else.
    Failed { reason: String },
}

impl fmt::Display for PasselError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PasselError::Cancelled => f.write_str("Cancelled"),
            PasselError::Conflict { entries, .. } => write!(
                f,
                "{} changed both here and on the remote",
                entries.join(", ")
            ),
            PasselError::AccessDenied => f.write_str("The remote refused the credentials"),
            PasselError::Failed { reason } => f.write_str(reason),
        }
    }
}

impl std::error::Error for PasselError {}

impl PasselError {
    fn failed(message: impl Into<String>) -> Self {
        PasselError::Failed {
            reason: message.into(),
        }
    }
}

impl From<StoreError> for PasselError {
    fn from(err: StoreError) -> Self {
        match err {
            StoreError::Pgp(err) => err.into(),
            StoreError::Git(err) => err.into(),
            other => PasselError::failed(other.to_string()),
        }
    }
}

impl From<PgpError> for PasselError {
    fn from(err: PgpError) -> Self {
        match err {
            PgpError::Cancelled => PasselError::Cancelled,
            other => PasselError::failed(other.to_string()),
        }
    }
}

impl From<git::GitError> for PasselError {
    fn from(err: git::GitError) -> Self {
        match err {
            git::GitError::MergeConflicts { entries, kept_as } => PasselError::Conflict {
                entries: entries.iter().map(|e| entry_label(e)).collect(),
                kept_as,
            },
            git::GitError::Git(err) if err.code() == git::ErrorCode::Auth => {
                PasselError::AccessDenied
            }
            git::GitError::Git(err) => {
                // The class and code are safe to log; the message can
                // contain the address, so debug only.
                log::info!("git failed: class {:?}, code {:?}", err.class(), err.code());
                log::debug!("git: {err:?}");
                PasselError::failed(git_failure_message(&err))
            }
            other => PasselError::failed(other.to_string()),
        }
    }
}

/// A short, address-free explanation of a libgit2 failure.
fn git_failure_message(err: &git::Git2Error) -> &'static str {
    use git::ErrorClass;
    match err.class() {
        ErrorClass::Net | ErrorClass::Http | ErrorClass::Os => "Could not reach the remote",
        ErrorClass::Ssl => "The remote's certificate is not trusted",
        ErrorClass::Ssh => "The SSH connection failed",
        _ => "Git operation failed",
    }
}

/// Asks the user for key passphrases. Called on the thread running the
/// Passel call (never the UI thread): block until the user answers.
#[uniffi::export(with_foreign)]
pub trait PassphrasePrompt: Send + Sync {
    /// The passphrase of `key`, or `None` if the user cancelled. `retry`
    /// is true after a wrong one.
    fn passphrase(&self, key: KeyInfo, retry: bool) -> Option<String>;

    /// A passphrase for a key being created (`user_id`), or `None` if the
    /// user cancelled. Empty leaves the key unprotected.
    fn new_passphrase(&self, user_id: String) -> Option<String>;
}

/// A secret key in the keyring.
#[derive(Debug, Clone, uniffi::Record)]
pub struct KeyInfo {
    /// Uppercase hex fingerprint.
    pub fingerprint: String,
    pub user_id: Option<String>,
}

/// An entry of the vault.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EntryRef {
    /// Path of the `.gpg` file in the vault, e.g. `web/github.gpg`: the
    /// id to pass back to [`Passel::open_entry`] and friends.
    pub path: String,
    /// Entry name, e.g. `github`.
    pub name: String,
    /// Folder of the entry, e.g. `web` (empty at the top).
    pub folder: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FieldKind {
    /// Hidden until shown.
    Password,
    Text,
    /// An `otpauth://` URI: show codes from [`otp_code`].
    Otp,
    Multiline,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Field {
    pub name: String,
    pub value: String,
    pub kind: FieldKind,
}

/// A decrypted entry: the password (first line of the file) and the
/// other fields, in order.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Entry {
    pub password: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct OtpCode {
    pub code: String,
    /// Seconds the code stays valid.
    pub seconds_left: u32,
    pub period: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct SyncResult {
    /// Commits made elsewhere, now here.
    pub received: u32,
    /// Commits made here, now on the remote.
    pub sent: u32,
}

/// The app's Passel: keyring, credentials and vault, under `data_dir`.
#[derive(uniffi::Object)]
pub struct Passel {
    data_dir: PathBuf,
    keyring: Arc<RpgpBackend>,
    credentials: Arc<StoredCredentials>,
    prompt: Arc<PromptSlot>,
    /// Serializes operations that write the vault (git is not safe to run
    /// twice at once on one repository).
    vault_lock: Mutex<()>,
}

/// The process-wide instance: the OpenPGP backend and git credentials are
/// global, so a second `Passel` would fight the first over them.
static INSTANCE: Mutex<Option<Arc<Passel>>> = Mutex::new(None);

#[uniffi::export]
impl Passel {
    /// Opens (creating it on first use) the app's data under `data_dir`:
    /// `keys/` (the keyring) and `vault/` (the cloned vault). Calling it
    /// again with the same folder returns the same instance with the new
    /// prompt (e.g. after an activity was recreated).
    #[uniffi::constructor]
    pub fn new(
        data_dir: String,
        prompt: Arc<dyn PassphrasePrompt>,
    ) -> Result<Arc<Self>, PasselError> {
        let mut instance = INSTANCE.lock().unwrap_or_else(|p| p.into_inner());
        let data_dir = PathBuf::from(data_dir);
        if let Some(existing) = instance.as_ref() {
            if existing.data_dir != data_dir {
                return Err(PasselError::failed(
                    "Passel is already open with another folder",
                ));
            }
            existing.prompt.set(prompt);
            return Ok(Arc::clone(existing));
        }

        init_logging();
        std::fs::create_dir_all(&data_dir).map_err(|e| PasselError::failed(e.to_string()))?;
        #[cfg(target_os = "android")]
        trust_android_certificates();

        let prompt_slot = Arc::new(PromptSlot::default());
        prompt_slot.set(prompt);
        let keyring = Arc::new(RpgpBackend::open(
            data_dir.join("keys"),
            Box::new(SharedPrompt(Arc::clone(&prompt_slot))),
        )?);
        let credentials = Arc::new(StoredCredentials::new());
        pgp::install_backend(Box::new(SharedKeyring(Arc::clone(&keyring))))
            .map_err(|_| PasselError::failed("An OpenPGP backend is already installed"))?;
        git::install_credential_provider(Box::new(SharedCredentials(Arc::clone(&credentials))))
            .map_err(|_| PasselError::failed("A git credential provider is already installed"))?;

        let passel = Arc::new(Self {
            data_dir,
            keyring,
            credentials,
            prompt: prompt_slot,
            vault_lock: Mutex::new(()),
        });
        *instance = Some(Arc::clone(&passel));
        Ok(passel)
    }

    // ---- Keys -------------------------------------------------------

    /// The secret keys that can open entries.
    pub fn keys(&self) -> Result<Vec<KeyInfo>, PasselError> {
        Ok(self
            .keyring
            .secret_keys()?
            .into_iter()
            .map(key_info)
            .collect())
    }

    /// Imports a secret key backup (armored or binary, e.g. from
    /// `gpg --export-secret-keys --armor`); returns the fingerprints.
    pub fn import_key(&self, data: Vec<u8>) -> Result<Vec<String>, PasselError> {
        let data = Zeroizing::new(data);
        Ok(self.keyring.import_secret_keys(&data)?)
    }

    /// Imports public keys of the other people a vault is shared with.
    pub fn import_public_keys(&self, data: Vec<u8>) -> Result<Vec<String>, PasselError> {
        Ok(self.keyring.import_public_keys(&data)?)
    }

    /// Creates a key (asking for its passphrase); returns its fingerprint.
    pub fn create_key(&self, name: String, email: String) -> Result<String, PasselError> {
        let user_id = pgp::key_user_id(&name, &email)?;
        Ok(self.keyring.create_key(&user_id)?)
    }

    /// An armored backup of a secret key, still passphrase-protected.
    pub fn export_key(&self, fingerprint: String) -> Result<Vec<u8>, PasselError> {
        Ok(self.keyring.export_secret_key(&fingerprint)?.to_vec())
    }

    /// The armored public key of one of this device's keys, to add to a
    /// vault's recipients on another device.
    pub fn export_public_key(&self, fingerprint: String) -> Result<Vec<u8>, PasselError> {
        Ok(self.keyring.export_public_key(&fingerprint)?)
    }

    /// Forgets the cached passphrases: the next entry asks again.
    pub fn lock(&self) {
        self.keyring.lock();
    }

    // ---- Remote credentials -----------------------------------------

    /// Uses `username` and `token` (a personal access token or password)
    /// for HTTPS remotes on `host`. Kept in memory only: the app stores
    /// them (in the Keystore) and sets them again at each start.
    pub fn set_https_credentials(&self, host: String, username: String, token: String) {
        self.credentials.set(
            &host,
            Credential::UserPassword {
                username,
                password: Zeroizing::new(token),
            },
        );
    }

    pub fn forget_credentials(&self, host: String) {
        self.credentials.remove(&host);
    }

    // ---- Vault ------------------------------------------------------

    pub fn has_vault(&self) -> bool {
        store::has_history(&self.vault_dir())
    }

    /// The vault's remote address, if any.
    pub fn remote_url(&self) -> Option<String> {
        store::remote_url(&self.vault_dir())
    }

    /// Clones the vault from `remote_url`; this device's commits are
    /// signed `author_name <author_email>`.
    pub fn clone_vault(
        &self,
        remote_url: String,
        author_name: String,
        author_email: String,
    ) -> Result<(), PasselError> {
        let _guard = self.lock_vault();
        let dir = self.vault_dir();
        if dir.exists() {
            return Err(PasselError::failed("A vault is already set up"));
        }
        if let Err(err) = store::clone_vault(&remote_url, &dir, &author_name, &author_email) {
            // A failed clone can leave a partial folder behind.
            let _ = std::fs::remove_dir_all(&dir);
            return Err(err.into());
        }
        Ok(())
    }

    /// Deletes this device's copy of the vault (the remote keeps its own).
    pub fn remove_vault(&self) -> Result<(), PasselError> {
        let _guard = self.lock_vault();
        match std::fs::remove_dir_all(self.vault_dir()) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(PasselError::failed(err.to_string())),
        }
    }

    /// Exchanges changes with the remote, both ways.
    pub fn sync(&self) -> Result<SyncResult, PasselError> {
        let _guard = self.lock_vault();
        let report = store::sync_vault(&self.vault_dir(), None)?;
        Ok(SyncResult {
            received: saturating_u32(report.received),
            sent: saturating_u32(report.sent),
        })
    }

    /// Local changes not sent to the remote yet.
    pub fn unsynced_count(&self) -> u32 {
        store::unpushed_count(&self.vault_dir(), None)
            .map(saturating_u32)
            .unwrap_or(0)
    }

    /// Every entry of the vault, sorted by path.
    pub fn entries(&self) -> Result<Vec<EntryRef>, PasselError> {
        let nodes = store::load_password_store(&self.vault_dir())
            .map_err(|e| PasselError::failed(e.to_string()))?;
        let mut entries = Vec::new();
        collect_entries(&nodes, &mut entries);
        entries.sort_by_key(|entry| entry.path.to_lowercase());
        Ok(entries)
    }

    /// Decrypts an entry (asking for the passphrase if needed).
    pub fn open_entry(&self, path: String) -> Result<Entry, PasselError> {
        let node = entry_node(&path)?;
        let data = store::load_entry_from_node(&self.vault_dir(), &node)?;
        Ok(entry_from(&data))
    }

    /// Encrypts and commits an edited entry. Call [`Passel::sync`] to
    /// send it.
    pub fn save_entry(&self, path: String, entry: Entry) -> Result<(), PasselError> {
        let _guard = self.lock_vault();
        let node = entry_node(&path)?;
        store::save_entry_data(&self.vault_dir(), &node, &entry_data(entry), false)?;
        Ok(())
    }

    /// Creates an entry `name` in `folder` (empty for the top); returns
    /// its path. Call [`Passel::sync`] to send it.
    pub fn create_entry(
        &self,
        folder: String,
        name: String,
        entry: Entry,
    ) -> Result<String, PasselError> {
        let _guard = self.lock_vault();
        let folder = relative_path(&folder, false)?;
        let node =
            store::create_entry_data(&self.vault_dir(), &folder, &name, &entry_data(entry), false)?;
        Ok(path_string(&node.path))
    }

    /// Deletes an entry and commits it. Call [`Passel::sync`] to send it.
    pub fn delete_entry(&self, path: String) -> Result<(), PasselError> {
        let _guard = self.lock_vault();
        let node = entry_node(&path)?;
        store::delete_entry(&self.vault_dir(), &node, false)?;
        Ok(())
    }
}

impl Passel {
    fn vault_dir(&self) -> PathBuf {
        self.data_dir.join("vault")
    }

    fn lock_vault(&self) -> std::sync::MutexGuard<'_, ()> {
        self.vault_lock.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// The current code of an `otpauth://` URI.
#[uniffi::export]
pub fn otp_code(uri: String) -> Result<OtpCode, PasselError> {
    let code = otp::generate_totp_from_url(&uri)
        .map_err(|_| PasselError::failed("This one-time password setting is not valid"))?;
    Ok(OtpCode {
        code: code.code,
        seconds_left: code.remaining.try_into().unwrap_or(u32::MAX),
        period: code.period.try_into().unwrap_or(u32::MAX),
    })
}

/// A random password of `length` characters: letters and digits, plus
/// easy-to-type symbols when `symbols` is set.
#[uniffi::export]
pub fn generate_password(length: u32, symbols: bool) -> Result<String, PasselError> {
    let mode = if symbols {
        PasswordMode::LimitedSpecial
    } else {
        PasswordMode::Alphanumeric
    };
    password::generate_password(length as usize, mode)
        .map(|password| password.to_string())
        .map_err(|_| PasselError::failed("Could not generate a password of this length"))
}

/// Forwards passphrase requests to the app's current prompt, which can
/// change when the app's UI is recreated.
#[derive(Default)]
struct PromptSlot(RwLock<Option<Arc<dyn PassphrasePrompt>>>);

impl PromptSlot {
    fn set(&self, prompt: Arc<dyn PassphrasePrompt>) {
        *self.0.write().unwrap_or_else(|p| p.into_inner()) = Some(prompt);
    }

    fn current(&self) -> Option<Arc<dyn PassphrasePrompt>> {
        self.0.read().unwrap_or_else(|p| p.into_inner()).clone()
    }
}

/// The slot as the keyring sees it.
struct SharedPrompt(Arc<PromptSlot>);

impl PassphraseProvider for SharedPrompt {
    fn passphrase(&self, key: &SecretKeyInfo, retry: bool) -> Option<Zeroizing<String>> {
        self.0
            .current()?
            .passphrase(key_info(key.clone()), retry)
            .map(Zeroizing::new)
    }

    fn new_passphrase(&self, user_id: &str) -> Option<Zeroizing<String>> {
        self.0
            .current()?
            .new_passphrase(user_id.to_string())
            .map(Zeroizing::new)
    }
}

/// The keyring, shared between the process-wide backend and `Passel`
/// (which also needs `lock` and `import_public_keys`).
struct SharedKeyring(Arc<RpgpBackend>);

impl PgpBackend for SharedKeyring {
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>, PgpError> {
        self.0.decrypt(ciphertext)
    }
    fn encrypt(&self, plaintext: &[u8], recipient_ids: &[String]) -> Result<Vec<u8>, PgpError> {
        self.0.encrypt(plaintext, recipient_ids)
    }
    fn secret_keys(&self) -> Result<Vec<SecretKeyInfo>, PgpError> {
        self.0.secret_keys()
    }
    fn create_key(&self, user_id: &str) -> Result<String, PgpError> {
        self.0.create_key(user_id)
    }
    fn import_secret_keys(&self, data: &[u8]) -> Result<Vec<String>, PgpError> {
        self.0.import_secret_keys(data)
    }
    fn export_secret_key(&self, fingerprint: &str) -> Result<Zeroizing<Vec<u8>>, PgpError> {
        self.0.export_secret_key(fingerprint)
    }
}

struct SharedCredentials(Arc<StoredCredentials>);

impl CredentialProvider for SharedCredentials {
    fn credential(&self, request: &CredentialRequest<'_>) -> Option<Credential> {
        self.0.credential(request)
    }
}

fn key_info(key: SecretKeyInfo) -> KeyInfo {
    KeyInfo {
        fingerprint: key.fingerprint,
        user_id: key.user_id,
    }
}

fn collect_entries(nodes: &[PassNode], out: &mut Vec<EntryRef>) {
    for node in nodes {
        match &node.kind {
            PassNodeKind::Group(children) => collect_entries(children, out),
            PassNodeKind::Entry => out.push(EntryRef {
                path: path_string(&node.path),
                name: node.name.clone(),
                folder: node.path.parent().map(path_string).unwrap_or_default(),
            }),
        }
    }
}

fn path_string(path: &Path) -> String {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(part) => Some(part.to_string_lossy()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// `pass`-style label of an entry path: `web/github.gpg` → `web/github`.
fn entry_label(path: &str) -> String {
    path.strip_suffix(".gpg").unwrap_or(path).to_string()
}

/// A path relative to the vault, refusing anything that could leave it.
fn relative_path(path: &str, is_entry: bool) -> Result<PathBuf, PasselError> {
    let relative = PathBuf::from(path);
    let escapes = relative
        .components()
        .any(|c| !matches!(c, Component::Normal(_)));
    let not_entry = is_entry && relative.extension().is_none_or(|ext| ext != "gpg");
    if escapes || not_entry {
        return Err(PasselError::failed("Not an entry of this vault"));
    }
    Ok(relative)
}

fn entry_node(path: &str) -> Result<PassNode, PasselError> {
    let path = relative_path(path, true)?;
    let name = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(PassNode {
        name,
        path,
        kind: PassNodeKind::Entry,
    })
}

fn entry_from(data: &EntryData) -> Entry {
    Entry {
        password: data.password.display_value().into_owned(),
        fields: data
            .fields
            .iter()
            .map(|(name, field)| Field {
                name: name.clone(),
                value: field.display_value().into_owned(),
                kind: match field {
                    EntryField::Password(_) => FieldKind::Password,
                    EntryField::Plain(_) => FieldKind::Text,
                    EntryField::Otp(_) => FieldKind::Otp,
                    EntryField::Multiline(_) => FieldKind::Multiline,
                },
            })
            .collect(),
    }
}

/// Builds the entry to write. The value decides over the kind the UI
/// sent where they disagree: an `otpauth://` URI is an OTP field, text
/// with line breaks is multiline.
fn entry_data(entry: Entry) -> EntryData {
    EntryData {
        password: EntryField::Password(entry.password),
        fields: entry
            .fields
            .into_iter()
            .map(|field| {
                let value = field.value;
                let kind = if otp::is_otp_url(&value) {
                    FieldKind::Otp
                } else if value.contains('\n') {
                    FieldKind::Multiline
                } else if field.kind == FieldKind::Password {
                    FieldKind::Password
                } else {
                    FieldKind::Text
                };
                let value = match kind {
                    FieldKind::Password => EntryField::Password(value),
                    FieldKind::Text => EntryField::Plain(value),
                    FieldKind::Otp => EntryField::Otp(value),
                    FieldKind::Multiline => EntryField::Multiline(value),
                };
                (field.name, value)
            })
            .collect(),
    }
}

fn saturating_u32(n: usize) -> u32 {
    n.try_into().unwrap_or(u32::MAX)
}

fn init_logging() {
    #[cfg(target_os = "android")]
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("passel"),
    );
}

/// libgit2's OpenSSL is built from source on Android: it knows no
/// authorities and cannot read certificate files. Hand it the system's
/// (one PEM file per certificate) in memory.
#[cfg(target_os = "android")]
fn trust_android_certificates() {
    let source = [
        "/apex/com.android.conscrypt/cacerts",
        "/system/etc/security/cacerts",
    ]
    .into_iter()
    .map(Path::new)
    .find(|dir| dir.is_dir());
    let Some(source) = source else {
        log::warn!("no system certificate folder found");
        return;
    };
    let mut bundle = Vec::new();
    if let Ok(files) = std::fs::read_dir(source) {
        for file in files.flatten() {
            if let Ok(pem) = std::fs::read(file.path()) {
                bundle.extend_from_slice(&pem);
                bundle.push(b'\n');
            }
        }
    }
    match git::trust_certificates(&bundle) {
        Ok(count) => log::info!("trusting {count} system certificate authorities"),
        Err(err) => log::warn!("could not load the system certificates: {err:?}"),
    }
}
