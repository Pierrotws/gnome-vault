//! OpenPGP through GPGME: the user's GnuPG keyring, gpg-agent and
//! pinentry. The desktop backend: it keeps smartcards, agent caching and
//! the keys `pass(1)` already uses.

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use gpgme::{
    Context, CreateKeyFlags, Data, ExportMode, InteractionStatus, Interactor, Key, Protocol,
    Validity,
};
use zeroize::Zeroizing;

use super::{PgpBackend, PgpError, SecretKeyInfo};

/// The GnuPG keyring of the current user (or of `home`, for tests).
#[derive(Debug, Default)]
pub struct GpgmeBackend {
    /// GnuPG home directory; `None` uses gpg's default (`GNUPGHOME` or
    /// `~/.gnupg`).
    home: Option<PathBuf>,
}

impl GpgmeBackend {
    /// A backend on another GnuPG home directory.
    pub fn with_home(home: PathBuf) -> Self {
        Self { home: Some(home) }
    }

    fn context(&self) -> Result<Context, PgpError> {
        let mut ctx = Context::from_protocol(Protocol::OpenPgp)
            .map_err(|e| PgpError::ContextError(e.to_string()))?;
        if let Some(home) = &self.home {
            let home = home
                .to_str()
                .ok_or_else(|| PgpError::InvalidPath(home.clone()))?;
            ctx.set_engine_home_dir(home)
                .map_err(|e| PgpError::ContextError(e.to_string()))?;
        }
        Ok(ctx)
    }
}

impl PgpBackend for GpgmeBackend {
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>, PgpError> {
        let mut ctx = self.context()?;
        let mut cipher =
            Data::from_bytes(ciphertext).map_err(|e| PgpError::LoadingError(e.to_string()))?;
        let mut plain = Zeroizing::new(Vec::<u8>::new());
        ctx.decrypt(&mut cipher, &mut *plain)
            .map_err(|e| PgpError::DecryptError(e.to_string()))?;
        Ok(plain)
    }

    fn encrypt(&self, plaintext: &[u8], recipient_ids: &[String]) -> Result<Vec<u8>, PgpError> {
        let mut ctx = self.context()?;

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

        let mut ciphertext = Vec::new();
        ctx.encrypt(keys.iter(), plaintext, &mut ciphertext)
            .map_err(|e| PgpError::EncryptError(e.to_string()))?;
        Ok(ciphertext)
    }

    fn secret_keys(&self) -> Result<Vec<SecretKeyInfo>, PgpError> {
        let mut ctx = self.context()?;
        let keys = ctx
            .secret_keys()
            .map_err(|e| PgpError::NoKeys(e.to_string()))?
            .filter_map(Result::ok)
            .filter(|key| key.can_encrypt() && !key.is_bad())
            .filter_map(|key| {
                let fingerprint = key.fingerprint().ok().or_else(|| key.id().ok())?;
                let user_id = key
                    .user_ids()
                    .next()
                    .and_then(|user_id| user_id.id().ok())
                    .map(str::to_string);
                Some(SecretKeyInfo {
                    fingerprint: fingerprint.to_string(),
                    user_id,
                })
            })
            .collect();
        Ok(keys)
    }

    /// Creates a key pair with gpg's default algorithms (a primary key and
    /// an encryption subkey) and no expiry. gpg-agent asks for the
    /// passphrase through pinentry: Passel never sees it.
    fn create_key(&self, user_id: &str) -> Result<String, PgpError> {
        create_key_in(&mut self.context()?, user_id, CreateKeyFlags::NOEXPIRE)
    }

    /// Imported keys are marked as ultimately trusted, like a key created
    /// on this computer: otherwise gpg refuses to encrypt to them ("no
    /// assurance this key belongs to the named user").
    fn import_secret_keys(&self, data: &[u8]) -> Result<Vec<String>, PgpError> {
        let data = Data::from_bytes(data).map_err(|e| PgpError::LoadingError(e.to_string()))?;
        import_secret_keys_in(&mut self.context()?, data)
    }

    /// gpg-agent asks for the key's passphrase first.
    fn export_secret_key(&self, fingerprint: &str) -> Result<Zeroizing<Vec<u8>>, PgpError> {
        export_secret_key_in(&mut self.context()?, fingerprint)
    }
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

#[cfg(test)]
pub(super) mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;

    /// A throwaway GnuPG home; the agent it starts is stopped on drop.
    pub struct TempGnupgHome {
        pub path: PathBuf,
    }

    impl TempGnupgHome {
        pub fn new(name: &str) -> Self {
            let path = super::super::tests::temp_dir(name);
            fs::create_dir_all(&path).unwrap();
            // gpg warns about a home others can read.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            }
            Self { path }
        }

        pub fn backend(&self) -> GpgmeBackend {
            GpgmeBackend::with_home(self.path.clone())
        }

        pub fn context(&self) -> Context {
            self.backend().context().unwrap()
        }

        /// Creates a key with no passphrase (no pinentry in tests) and
        /// returns its fingerprint.
        pub fn create_unprotected_key(&self, user_id: &str) -> String {
            let flags = CreateKeyFlags::NOEXPIRE | CreateKeyFlags::NOPASSWD;
            create_key_in(&mut self.context(), user_id, flags).unwrap()
        }

        /// Exports a secret key without the agent's passphrase prompt.
        pub fn export_secret(&self, fingerprint: &str) -> Zeroizing<Vec<u8>> {
            export_secret_key_in(&mut self.context(), fingerprint).unwrap()
        }
    }

    impl Drop for TempGnupgHome {
        fn drop(&mut self) {
            stop_agent(&self.path);
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn stop_agent(home: &Path) {
        let _ = std::process::Command::new("gpgconf")
            .arg("--homedir")
            .arg(home)
            .args(["--kill", "gpg-agent"])
            .status();
    }

    #[test]
    fn created_key_survives_a_backup_and_restore() {
        let (a, b, c) = (
            TempGnupgHome::new("gpg-keys-a"),
            TempGnupgHome::new("gpg-keys-b"),
            TempGnupgHome::new("gpg-keys-c"),
        );

        let fingerprint = a.create_unprotected_key("Passel Tests <tests@example.invalid>");
        let mut a_ctx = a.context();
        let key = a_ctx.get_secret_key(fingerprint.as_str()).unwrap();
        assert!(
            key.can_encrypt(),
            "default algorithms add an encryption subkey"
        );
        assert!(key
            .subkeys()
            .all(|subkey| subkey.expiration_time().is_none()));

        let backup = a.export_secret(&fingerprint);
        assert!(backup.starts_with(b"-----BEGIN PGP PRIVATE KEY BLOCK-----"));

        // Restoring the backup elsewhere yields a key gpg encrypts to.
        let imported = b.backend().import_secret_keys(&backup).unwrap();
        assert_eq!(imported, std::slice::from_ref(&fingerprint));
        let restored = b.context().get_key(fingerprint.as_str()).unwrap();
        assert_eq!(restored.owner_trust(), Validity::Ultimate);
        let ciphertext = b
            .backend()
            .encrypt(b"secret", std::slice::from_ref(&fingerprint))
            .unwrap();
        assert_eq!(&*b.backend().decrypt(&ciphertext).unwrap(), b"secret");

        // A public key alone cannot open a vault.
        let mut public = Vec::new();
        a_ctx
            .export_keys([&key], ExportMode::empty(), &mut public)
            .unwrap();
        assert!(matches!(
            c.backend().import_secret_keys(&public),
            Err(PgpError::NoSecretKey)
        ));
    }
}
