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

use passel_core::i18n::{gettext, ngettext_f};
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
        store::{self, RemoteCheck, SetupPlan, SetupStep, StoreError, SyncTarget},
    },
};
use zeroize::Zeroizing;

mod i18n;
mod vaults;
use vaults::Vaults;

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
            PasselError::Cancelled => f.write_str(&gettext("Cancelled")),
            // Translators: {entries} is one entry name, or several
            // separated by commas.
            PasselError::Conflict { entries, .. } => f.write_str(&ngettext_f(
                "{entries} changed both here and on the remote",
                "{entries} changed both here and on the remote",
                entries.len() as u64,
                &[("entries", &entries.join(", "))],
            )),
            PasselError::AccessDenied => {
                f.write_str(&gettext("The remote refused the credentials"))
            }
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
fn git_failure_message(err: &git::Git2Error) -> String {
    use git::ErrorClass;
    match err.class() {
        ErrorClass::Net | ErrorClass::Http | ErrorClass::Os => {
            gettext("Could not reach the remote")
        }
        ErrorClass::Ssl => gettext("The remote's certificate is not trusted"),
        ErrorClass::Ssh => gettext("The SSH connection failed"),
        _ => gettext("Git operation failed"),
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

/// A vault on this device.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct VaultInfo {
    /// Stable id (its folder in the app's storage, e.g. `vaults/work`).
    pub id: String,
    pub name: String,
    pub remote_url: Option<String>,
    /// The vault the entry calls work on.
    pub active: bool,
}

/// A vault made by [`Passel::create_vault`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct CreatedVault {
    pub vault: VaultInfo,
    /// Set when the vault works on this device but could not be sent to
    /// its remote yet; the next [`Passel::sync`] retries.
    pub upload_error: Option<String>,
}

/// What [`Passel::check_remote`] found at an address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum RemoteState {
    /// Reachable and empty: a new vault can be created there.
    Empty,
    /// Reachable and holding commits: an existing vault to download.
    NotEmpty,
    /// The server refused the credentials (or there were none).
    AccessDenied,
    /// An SSH server whose key is not known yet.
    UnknownHost,
    /// Nothing usable at this address.
    Unreachable,
    /// Not an `https://` address: refused without connecting, so the
    /// vault never travels unencrypted (see [`remote_allowed`]).
    NotHttps,
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
    /// Serializes operations that write a vault (git is not safe to run
    /// twice at once on one repository).
    vault_lock: Mutex<()>,
    vaults: Mutex<Vaults>,
}

/// The process-wide instance: the OpenPGP backend and git credentials are
/// global, so a second `Passel` would fight the first over them.
static INSTANCE: Mutex<Option<Arc<Passel>>> = Mutex::new(None);

#[uniffi::export]
impl Passel {
    /// Opens (creating it on first use) the app's data under `data_dir`:
    /// `keys/` (the keyring), `vaults/` and `vaults.json`. Calling it
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
                return Err(PasselError::failed(gettext(
                    "Passel is already open with another folder",
                )));
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
            .map_err(|_| PasselError::failed(gettext("An OpenPGP backend is already installed")))?;
        git::install_credential_provider(Box::new(SharedCredentials(Arc::clone(&credentials))))
            .map_err(|_| {
                PasselError::failed(gettext("A git credential provider is already installed"))
            })?;

        let passel = Arc::new(Self {
            keyring,
            credentials,
            prompt: prompt_slot,
            vault_lock: Mutex::new(()),
            vaults: Mutex::new(Vaults::load(&data_dir)),
            data_dir,
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

    /// The vaults on this device, the open one marked `active`.
    pub fn vaults(&self) -> Vec<VaultInfo> {
        self.vault_list()
            .listed()
            .into_iter()
            .map(|vault| VaultInfo {
                remote_url: store::remote_url(&vault.dir),
                id: vault.id,
                name: vault.name,
                active: vault.active,
            })
            .collect()
    }

    /// Whether a vault is open (else, run the setup).
    pub fn has_vault(&self) -> bool {
        self.vault_list().active_dir().is_some()
    }

    /// Opens another vault of this device.
    pub fn switch_vault(&self, id: String) -> Result<(), PasselError> {
        let mut vaults = self.vault_list();
        if !vaults.activate(&id) {
            return Err(PasselError::failed(gettext("No such vault on this device")));
        }
        save_list(&vaults);
        Ok(())
    }

    /// The open vault's remote address, if any.
    pub fn remote_url(&self) -> Option<String> {
        store::remote_url(&self.vault_dir().ok()?)
    }

    /// Whether `remote_url` holds a vault, is empty, or cannot be used
    /// (with the credentials set for its host).
    pub fn check_remote(&self, remote_url: String) -> RemoteState {
        let remote_url = remote_url.trim();
        if !remote_allowed(remote_url) {
            return RemoteState::NotHttps;
        }
        match store::check_remote(remote_url) {
            RemoteCheck::Empty => RemoteState::Empty,
            RemoteCheck::NotEmpty => RemoteState::NotEmpty,
            RemoteCheck::AccessDenied => RemoteState::AccessDenied,
            RemoteCheck::UnknownHost => RemoteState::UnknownHost,
            RemoteCheck::Unreachable => RemoteState::Unreachable,
        }
    }

    /// Downloads an existing vault from `remote_url` as `name`, and opens
    /// it. This device's commits are signed `author_name <author_email>`.
    pub fn clone_vault(
        &self,
        name: String,
        remote_url: String,
        author_name: String,
        author_email: String,
    ) -> Result<VaultInfo, PasselError> {
        let remote_url = remote_url.trim();
        if !remote_allowed(remote_url) {
            return Err(not_https_error());
        }
        let _guard = self.lock_vault();
        let dir = self.vault_list().new_dir(&name);
        if let Err(err) = store::clone_vault(remote_url, &dir, &author_name, &author_email) {
            // A failed clone can leave a partial folder behind.
            let _ = std::fs::remove_dir_all(&dir);
            return Err(err.into());
        }
        Ok(self.register(&dir, &name))
    }

    /// Creates a new, empty vault `name` encrypted for the key
    /// `recipient` (a fingerprint), and opens it. With `remote_url` (an
    /// empty repository), the vault is also sent there; if only that part
    /// fails, the vault is still created and `upload_error` says why.
    pub fn create_vault(
        &self,
        name: String,
        recipient: String,
        remote_url: Option<String>,
        author_name: String,
        author_email: String,
    ) -> Result<CreatedVault, PasselError> {
        let _guard = self.lock_vault();
        let remote_url = remote_url
            .map(|url| url.trim().to_string())
            .filter(|url| !url.is_empty());
        if let Some(url) = &remote_url {
            match self.check_remote(url.clone()) {
                RemoteState::Empty => {}
                RemoteState::NotEmpty => {
                    return Err(PasselError::failed(gettext(
                        "This repository already holds a vault: download it instead",
                    )))
                }
                RemoteState::AccessDenied => return Err(PasselError::AccessDenied),
                RemoteState::UnknownHost | RemoteState::Unreachable => {
                    return Err(PasselError::failed(gettext("Could not reach this address")))
                }
                RemoteState::NotHttps => return Err(not_https_error()),
            }
        }
        let dir = self.vault_list().new_dir(&name);
        let plan = SetupPlan {
            store_dir: dir.clone(),
            recipient,
            sync: remote_url.map(SyncTarget::Remote),
            autopush: true,
        };
        store::check_setup_plan(&plan).map_err(|err| match err {
            StoreError::InvalidRecipient => {
                PasselError::failed(gettext("This key is not on this device"))
            }
            other => other.into(),
        })?;

        let mut upload_error = None;
        for step in plan.steps() {
            let mut result = store::run_setup_step(&plan, step);
            if step == SetupStep::StartHistory && result.is_ok() {
                // A phone has no global git identity: set the vault's own
                // before the first commit.
                result = git::set_identity(&dir, &author_name, &author_email).map_err(Into::into);
            }
            match result {
                Ok(()) => {}
                Err(err) if step.is_sync() => {
                    upload_error = Some(PasselError::from(err).to_string());
                    break;
                }
                Err(err) => {
                    let _ = std::fs::remove_dir_all(&dir);
                    return Err(err.into());
                }
            }
        }
        Ok(CreatedVault {
            vault: self.register(&dir, &name),
            upload_error,
        })
    }

    /// Deletes this device's copy of a vault (its remote keeps its own)
    /// and opens the next one, if any.
    pub fn remove_vault(&self, id: String) -> Result<(), PasselError> {
        let _guard = self.lock_vault();
        let mut vaults = self.vault_list();
        let dir = vaults
            .remove(&id)
            .ok_or_else(|| PasselError::failed(gettext("No such vault on this device")))?;
        save_list(&vaults);
        match std::fs::remove_dir_all(dir) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(PasselError::failed(err.to_string())),
        }
    }

    /// Exchanges changes with the remote, both ways.
    pub fn sync(&self) -> Result<SyncResult, PasselError> {
        let _guard = self.lock_vault();
        let dir = self.vault_dir()?;
        // A vault set up before https was required may still point at
        // another kind of address: it stays usable here, but unsynced.
        if store::remote_url(&dir).is_some_and(|url| !remote_allowed(&url)) {
            return Err(not_https_error());
        }
        let report = store::sync_vault(&dir, None)?;
        Ok(SyncResult {
            received: saturating_u32(report.received),
            sent: saturating_u32(report.sent),
        })
    }

    /// Local changes not sent to the remote yet.
    pub fn unsynced_count(&self) -> u32 {
        self.vault_dir()
            .ok()
            .and_then(|dir| store::unpushed_count(&dir, None).ok())
            .map(saturating_u32)
            .unwrap_or(0)
    }

    /// Every entry of the vault, sorted by path.
    pub fn entries(&self) -> Result<Vec<EntryRef>, PasselError> {
        let nodes = store::load_password_store(&self.vault_dir()?)
            .map_err(|e| PasselError::failed(e.to_string()))?;
        let mut entries = Vec::new();
        collect_entries(&nodes, &mut entries);
        entries.sort_by_key(|entry| entry.path.to_lowercase());
        Ok(entries)
    }

    /// Decrypts an entry (asking for the passphrase if needed).
    pub fn open_entry(&self, path: String) -> Result<Entry, PasselError> {
        let node = entry_node(&path)?;
        let data = store::load_entry_from_node(&self.vault_dir()?, &node)?;
        Ok(entry_from(&data))
    }

    /// Encrypts and commits an edited entry. Call [`Passel::sync`] to
    /// send it.
    pub fn save_entry(&self, path: String, entry: Entry) -> Result<(), PasselError> {
        let _guard = self.lock_vault();
        let node = entry_node(&path)?;
        store::save_entry_data(&self.vault_dir()?, &node, &entry_data(entry), false)?;
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
        let node = store::create_entry_data(
            &self.vault_dir()?,
            &folder,
            &name,
            &entry_data(entry),
            false,
        )?;
        Ok(path_string(&node.path))
    }

    /// Deletes an entry and commits it. Call [`Passel::sync`] to send it.
    pub fn delete_entry(&self, path: String) -> Result<(), PasselError> {
        let _guard = self.lock_vault();
        let node = entry_node(&path)?;
        store::delete_entry(&self.vault_dir()?, &node, false)?;
        Ok(())
    }
}

impl Passel {
    /// The open vault's folder.
    fn vault_dir(&self) -> Result<PathBuf, PasselError> {
        self.vault_list()
            .active_dir()
            .ok_or_else(|| PasselError::failed(gettext("No vault is open")))
    }

    fn vault_list(&self) -> std::sync::MutexGuard<'_, Vaults> {
        self.vaults.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Lists a vault that now exists at `dir`, opens it, and describes it.
    fn register(&self, dir: &Path, name: &str) -> VaultInfo {
        let mut vaults = self.vault_list();
        vaults.add(dir, name);
        save_list(&vaults);
        let listed = vaults
            .listed()
            .into_iter()
            .find(|vault| vault.dir == dir)
            .expect("just added");
        VaultInfo {
            remote_url: store::remote_url(dir),
            id: listed.id,
            name: listed.name,
            active: listed.active,
        }
    }

    fn lock_vault(&self) -> std::sync::MutexGuard<'_, ()> {
        self.vault_lock.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// Whether the mobile app may sync with `remote_url`: only over HTTPS,
/// so a vault (whose entry names are plain file names) never crosses
/// the network unencrypted. Plain `http://`, `git://` and SSH addresses
/// are refused. A folder on this device (an absolute path or `file://`)
/// is allowed too: nothing leaves the device. The desktop app does not
/// go through this API and keeps every kind of remote.
fn remote_allowed(remote_url: &str) -> bool {
    let url = remote_url.trim();
    let scheme_is = |scheme: &str| {
        url.get(..scheme.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(scheme))
    };
    if scheme_is("https://") {
        // Something must follow the scheme: a host.
        return url.len() > "https://".len();
    }
    scheme_is("file://") || (!url.contains("://") && Path::new(url).is_absolute())
}

fn not_https_error() -> PasselError {
    PasselError::failed(gettext(
        "Only https:// addresses are supported, so the vault is always sent encrypted",
    ))
}

fn save_list(vaults: &Vaults) {
    if let Err(err) = vaults.save() {
        log::warn!("could not save the vault list: {err}");
    }
}

/// The current code of an `otpauth://` URI.
#[uniffi::export]
pub fn otp_code(uri: String) -> Result<OtpCode, PasselError> {
    let code = otp::generate_totp_from_url(&uri)
        .map_err(|_| PasselError::failed(gettext("This one-time password setting is not valid")))?;
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
        .map_err(|_| PasselError::failed(gettext("Could not generate a password of this length")))
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
        return Err(PasselError::failed(gettext("Not an entry of this vault")));
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

#[cfg(test)]
mod tests {
    use super::remote_allowed;

    #[test]
    fn only_https_and_local_remotes_are_allowed() {
        for url in [
            "https://github.com/me/vault.git",
            "HTTPS://gitlab.com/me/vault",
            "  https://example.org/vault.git  ",
            "/data/user/0/io.pierrotws.passel/files/remote.git",
            "file:///sdcard/vault.git",
        ] {
            assert!(remote_allowed(url), "{url}");
        }
        for url in [
            "http://github.com/me/vault.git",
            "HTTP://example.org/vault",
            "git://example.org/vault.git",
            "ssh://git@github.com/me/vault.git",
            "git@github.com:me/vault.git",
            "github.com/me/vault.git",
            "relative/vault.git",
            "https://",
            "",
        ] {
            assert!(!remote_allowed(url), "{url}");
        }
    }
}
