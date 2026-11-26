//! OpenPGP in pure Rust (rPGP), for platforms without GnuPG.
//!
//! There is no gpg-agent here, so this backend owns what the agent would:
//!
//! - **The keyring**: armored key files in a folder the app picks (its
//!   private storage on Android/iOS), `secret/<FINGERPRINT>.asc` and
//!   `public/<FINGERPRINT>.asc`. Secret keys stay encrypted with their
//!   passphrase on disk, exactly as gpg exports them.
//! - **Passphrases**: asked through [`PassphraseProvider`] (the app shows
//!   a prompt, or unlocks a passphrase it keeps behind biometrics), then
//!   cached in memory until [`RpgpBackend::lock`], like the agent's cache.
//!
//! Everything written stays readable by `pass` and GnuPG (2.2 and later):
//! v4 keys (Ed25519 + Curve25519), SEIPDv1 messages with AES-256, and
//! iterated-and-salted S2K for key passphrases. Reading also covers what
//! gpg writes, including the OCB packet GnuPG 2.3+ uses for keys that
//! advertise it. The tests check all of this against the real gpg.

use std::{
    collections::HashMap,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

use pgp::{
    composed::{
        ArmorOptions, DecryptionOptions, Deserializable, EncryptionCaps, Esk, KeyType, Message,
        MessageBuilder, SecretKeyParamsBuilder, SignedPublicKey, SignedPublicSubKey,
        SignedSecretKey, SubkeyParamsBuilder, TheRing,
    },
    crypto::{ecc_curve::ECCCurve, hash::HashAlgorithm, sym::SymmetricKeyAlgorithm},
    packet::{PublicKeyEncryptedSessionKey, SignatureType},
    types::{CompressionAlgorithm, KeyDetails, Password, Tag, Timestamp},
};
use rand::thread_rng;
use zeroize::Zeroizing;

use super::{PgpBackend, PgpError, SecretKeyInfo};

/// How many times a wrong passphrase is asked again before giving up.
const PASSPHRASE_ATTEMPTS: u32 = 3;

/// Asks the user for key passphrases. Called while the backend is busy:
/// an implementation must not call back into the backend.
pub trait PassphraseProvider: Send + Sync {
    /// The passphrase of an existing secret key, or `None` if the user
    /// cancelled. `retry` is true after a wrong passphrase.
    fn passphrase(&self, key: &SecretKeyInfo, retry: bool) -> Option<Zeroizing<String>>;

    /// A passphrase for a key being created, or `None` if the user
    /// cancelled. An empty passphrase leaves the key unprotected.
    fn new_passphrase(&self, user_id: &str) -> Option<Zeroizing<String>>;
}

/// An rPGP keyring stored in a folder; see the module docs.
pub struct RpgpBackend {
    dir: PathBuf,
    passphrases: Box<dyn PassphraseProvider>,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    secret: Vec<SignedSecretKey>,
    public: Vec<SignedPublicKey>,
    /// Passphrases that unlocked a key, by primary fingerprint.
    unlocked: HashMap<String, Zeroizing<String>>,
}

impl RpgpBackend {
    /// Opens (creating it if needed) the keyring stored in `dir`.
    pub fn open(
        dir: impl Into<PathBuf>,
        passphrases: Box<dyn PassphraseProvider>,
    ) -> Result<Self, PgpError> {
        let dir = dir.into();
        let mut state = State::default();
        for (sub, armored_secret) in [("secret", true), ("public", false)] {
            let folder = dir.join(sub);
            fs::create_dir_all(&folder).map_err(|source| PgpError::ReadError {
                path: folder.clone(),
                source,
            })?;
            restrict_to_owner(&folder);
            let entries = fs::read_dir(&folder).map_err(|source| PgpError::ReadError {
                path: folder.clone(),
                source,
            })?;
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_none_or(|ext| ext != "asc") {
                    continue;
                }
                let data =
                    Zeroizing::new(fs::read(&path).map_err(|source| PgpError::ReadError {
                        path: path.clone(),
                        source,
                    })?);
                // A damaged file must not lock the user out of the others.
                if armored_secret {
                    match SignedSecretKey::from_reader_single(&data[..]) {
                        Ok((key, _)) => state.secret.push(key),
                        Err(err) => log::warn!("skipping unreadable secret key file: {err}"),
                    }
                } else {
                    match SignedPublicKey::from_reader_single(&data[..]) {
                        Ok((key, _)) => state.public.push(key),
                        Err(err) => log::warn!("skipping unreadable public key file: {err}"),
                    }
                }
            }
        }
        Ok(Self {
            dir,
            passphrases,
            state: Mutex::new(state),
        })
    }

    /// Forgets every cached passphrase: the next decryption asks again.
    pub fn lock(&self) {
        self.state().unlocked.clear();
    }

    /// Imports public keys (other people a vault is shared with) and
    /// returns their fingerprints. Secret keys go through
    /// [`PgpBackend::import_secret_keys`].
    /// The armored public key of one of the secret keys: what another
    /// device needs to encrypt entries this one can read.
    pub fn export_public_key(&self, fingerprint: &str) -> Result<Vec<u8>, PgpError> {
        let wanted = normalize_hex(fingerprint);
        let state = self.state();
        let key = state
            .secret
            .iter()
            .find(|key| wanted.as_deref() == Some(secret_fingerprint(key).as_str()))
            .ok_or_else(|| PgpError::NoKeys("no such secret key".to_string()))?;
        key.to_public_key()
            .to_armored_bytes(ArmorOptions::default())
            .map_err(|e| PgpError::ExportError(e.to_string()))
    }

    pub fn import_public_keys(&self, data: &[u8]) -> Result<Vec<String>, PgpError> {
        let (keys, _) = SignedPublicKey::from_reader_many(data)
            .map_err(|e| PgpError::ImportError(e.to_string()))?;
        let mut state = self.state();
        let mut imported = Vec::new();
        for key in keys {
            let key = key.map_err(|e| PgpError::ImportError(e.to_string()))?;
            let Some(view) = CertView::evaluate(&key, now()) else {
                continue;
            };
            if view.encryption_subkey.is_none() {
                continue;
            }
            let fingerprint = fingerprint_hex(&key);
            let armored = key
                .to_armored_bytes(ArmorOptions::default())
                .map_err(|e| PgpError::ImportError(e.to_string()))?;
            self.write_key_file("public", &fingerprint, &armored)
                .map_err(|e| PgpError::ImportError(e.to_string()))?;
            state.public.retain(|k| fingerprint_hex(k) != fingerprint);
            state.public.push(key);
            imported.push(fingerprint);
        }
        if imported.is_empty() {
            return Err(PgpError::NoKeys("no usable public key".to_string()));
        }
        Ok(imported)
    }

    fn state(&self) -> MutexGuard<'_, State> {
        // A panic while holding the lock leaves the keyring readable:
        // every mutation replaces whole values.
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write_key_file(&self, sub: &str, fingerprint: &str, contents: &[u8]) -> std::io::Result<()> {
        let folder = self.dir.join(sub);
        fs::create_dir_all(&folder)?;
        let dest = folder.join(format!("{fingerprint}.asc"));
        let tmp = folder.join(format!(".{fingerprint}.asc.tmp"));
        super::write_private_file(&tmp, contents)?;
        fs::rename(&tmp, &dest)
    }

    fn store_secret_key(&self, state: &mut State, key: SignedSecretKey) -> Result<String, String> {
        let fingerprint = secret_fingerprint(&key);
        let armored = Zeroizing::new(
            key.to_armored_bytes(ArmorOptions::default())
                .map_err(|e| e.to_string())?,
        );
        self.write_key_file("secret", &fingerprint, &armored)
            .map_err(|e| e.to_string())?;
        state
            .secret
            .retain(|k| secret_fingerprint(k) != fingerprint);
        state.unlocked.remove(&fingerprint);
        state.secret.push(key);
        Ok(fingerprint)
    }

    /// The passphrase that unlocks `component` of `key`: empty for an
    /// unprotected key, else the cached one, else asked (and checked).
    fn passphrase_for(
        &self,
        state: &mut State,
        key: &SignedSecretKey,
        component: Component<'_>,
    ) -> Result<Password, PgpError> {
        if !component.is_encrypted() {
            return Ok(Password::empty());
        }
        let fingerprint = secret_fingerprint(key);
        if let Some(cached) = state.unlocked.get(&fingerprint) {
            let password = Password::from(cached.as_str());
            if component.unlocks_with(&password) {
                return Ok(password);
            }
            state.unlocked.remove(&fingerprint);
        }
        let info = SecretKeyInfo {
            fingerprint: fingerprint.clone(),
            user_id: CertView::evaluate(&key.to_public_key(), now()).and_then(|v| v.user_id),
        };
        for attempt in 0..PASSPHRASE_ATTEMPTS {
            let Some(passphrase) = self.passphrases.passphrase(&info, attempt > 0) else {
                return Err(PgpError::Cancelled);
            };
            let password = Password::from(passphrase.as_str());
            if component.unlocks_with(&password) {
                state.unlocked.insert(fingerprint, passphrase);
                return Ok(password);
            }
        }
        Err(PgpError::DecryptError("wrong passphrase".to_string()))
    }
}

impl PgpBackend for RpgpBackend {
    fn decrypt(&self, ciphertext: &[u8]) -> Result<Zeroizing<Vec<u8>>, PgpError> {
        let (message, _) = Message::from_reader(Cursor::new(ciphertext))
            .map_err(|e| PgpError::LoadingError(e.to_string()))?;
        let pkesks: Vec<&PublicKeyEncryptedSessionKey> = match &message {
            Message::Encrypted { esk, .. } => esk
                .iter()
                .filter_map(|esk| match esk {
                    Esk::PublicKeyEncryptedSessionKey(pkesk) => Some(pkesk),
                    Esk::SymKeyEncryptedSessionKey(_) => None,
                })
                .collect(),
            _ => {
                return Err(PgpError::DecryptError(
                    "not an encrypted message".to_string(),
                ))
            }
        };

        let mut state = self.state();
        let (key, component) = state
            .secret
            .iter()
            .find_map(|key| {
                Component::all(key)
                    .find(|component| pkesks.iter().any(|esk| component.matches(esk)))
                    .map(|component| (key.clone(), component.index()))
            })
            .ok_or_else(|| PgpError::DecryptError("no secret key for this message".to_string()))?;
        let component = Component::at(&key, component);
        let password = self.passphrase_for(&mut state, &key, component)?;
        drop(state);

        let ring = TheRing {
            secret_keys: vec![&key],
            key_passwords: vec![&password],
            // GnuPG 2.3+ writes its own OCB packet (not in RFC 9580) for
            // keys that advertise it; `pass` vaults can hold such entries.
            decrypt_options: DecryptionOptions::new().enable_gnupg_aead(),
            ..TheRing::default()
        };
        let (mut message, _) = message
            .decrypt_the_ring(ring, true)
            .map_err(|e| PgpError::DecryptError(e.to_string()))?;
        if message.is_compressed() {
            message = message
                .decompress()
                .map_err(|e| PgpError::DecryptError(e.to_string()))?;
        }
        // Reading to the end also checks the MDC / AEAD tag.
        let plain = message
            .as_data_vec()
            .map_err(|e| PgpError::DecryptError(e.to_string()))?;
        Ok(Zeroizing::new(plain))
    }

    fn encrypt(&self, plaintext: &[u8], recipient_ids: &[String]) -> Result<Vec<u8>, PgpError> {
        let state = self.state();
        let now = now();
        let certs = state
            .secret
            .iter()
            .map(SignedSecretKey::to_public_key)
            .chain(state.public.iter().cloned())
            .collect::<Vec<_>>();
        drop(state);

        let mut subkeys = Vec::with_capacity(recipient_ids.len());
        for id in recipient_ids {
            // Like gpg, the first matching key that can encrypt wins.
            let subkey = certs
                .iter()
                .filter(|cert| matches_recipient(cert, id))
                .find_map(|cert| CertView::evaluate(cert, now)?.encryption_subkey)
                .ok_or_else(|| {
                    PgpError::NoKeys(format!(
                        "recipient {id} has no usable key (unknown, revoked, or expired)"
                    ))
                })?;
            subkeys.push(subkey);
        }

        let mut rng = thread_rng();
        let mut builder = MessageBuilder::from_bytes("", plaintext.to_vec())
            .seipd_v1(&mut rng, SymmetricKeyAlgorithm::AES256);
        for subkey in &subkeys {
            builder
                .encrypt_to_key(&mut rng, subkey)
                .map_err(|e| PgpError::EncryptError(e.to_string()))?;
        }
        builder
            .to_vec(&mut rng)
            .map_err(|e| PgpError::EncryptError(e.to_string()))
    }

    fn secret_keys(&self) -> Result<Vec<SecretKeyInfo>, PgpError> {
        let state = self.state();
        let now = now();
        Ok(state
            .secret
            .iter()
            .filter_map(|key| {
                let view = CertView::evaluate(&key.to_public_key(), now)?;
                view.encryption_subkey.as_ref()?;
                Some(SecretKeyInfo {
                    fingerprint: secret_fingerprint(key),
                    user_id: view.user_id,
                })
            })
            .collect())
    }

    /// Ed25519 primary key (certify, sign) with a Curve25519 encryption
    /// subkey, like `gpg --quick-generate-key` makes by default.
    fn create_key(&self, user_id: &str) -> Result<String, PgpError> {
        let passphrase = self
            .passphrases
            .new_passphrase(user_id)
            .ok_or(PgpError::Cancelled)?;
        // rPGP takes the passphrase as a plain String; it is dropped
        // (not wiped) once the key is generated.
        let passphrase = (!passphrase.is_empty()).then(|| passphrase.to_string());

        let mut subkey = SubkeyParamsBuilder::default();
        subkey
            .key_type(KeyType::ECDH(ECCCurve::Curve25519Legacy))
            .can_encrypt(EncryptionCaps::All)
            .passphrase(passphrase.clone());
        let subkey = subkey
            .build()
            .map_err(|e| PgpError::KeyCreationError(e.to_string()))?;

        let mut params = SecretKeyParamsBuilder::default();
        params
            .key_type(KeyType::Ed25519Legacy)
            .can_certify(true)
            .can_sign(true)
            .primary_user_id(user_id.to_string())
            .preferred_symmetric_algorithms(
                [
                    SymmetricKeyAlgorithm::AES256,
                    SymmetricKeyAlgorithm::AES192,
                    SymmetricKeyAlgorithm::AES128,
                ]
                .into_iter()
                .collect(),
            )
            .preferred_hash_algorithms(
                [
                    HashAlgorithm::Sha512,
                    HashAlgorithm::Sha384,
                    HashAlgorithm::Sha256,
                ]
                .into_iter()
                .collect(),
            )
            .preferred_compression_algorithms(
                [CompressionAlgorithm::ZLIB, CompressionAlgorithm::ZIP]
                    .into_iter()
                    .collect(),
            )
            .passphrase(passphrase)
            .subkeys(vec![subkey]);
        let key = params
            .build()
            .map_err(|e| PgpError::KeyCreationError(e.to_string()))?
            .generate(thread_rng())
            .map_err(|e| PgpError::KeyCreationError(e.to_string()))?;

        let mut state = self.state();
        self.store_secret_key(&mut state, key)
            .map_err(PgpError::KeyCreationError)
    }

    fn import_secret_keys(&self, data: &[u8]) -> Result<Vec<String>, PgpError> {
        // A public-key file parses as zero secret keys (or fails on its
        // first packet): both mean "not a secret key backup".
        let Ok((keys, _)) = SignedSecretKey::from_reader_many(data) else {
            return Err(PgpError::NoSecretKey);
        };
        let now = now();
        let mut state = self.state();
        let mut usable = Vec::new();
        for key in keys {
            let Ok(key) = key else { continue };
            let can_decrypt = CertView::evaluate(&key.to_public_key(), now)
                .is_some_and(|view| view.encryption_subkey.is_some())
                && key
                    .secret_subkeys
                    .iter()
                    .any(|sub| sub.key.public_key().algorithm().can_encrypt());
            if !can_decrypt {
                continue;
            }
            let fingerprint = self
                .store_secret_key(&mut state, key)
                .map_err(PgpError::ImportError)?;
            usable.push(fingerprint);
        }
        if usable.is_empty() {
            return Err(PgpError::NoSecretKey);
        }
        Ok(usable)
    }

    fn export_secret_key(&self, fingerprint: &str) -> Result<Zeroizing<Vec<u8>>, PgpError> {
        let wanted = normalize_hex(fingerprint);
        let state = self.state();
        let key = state
            .secret
            .iter()
            .find(|key| wanted.as_deref() == Some(secret_fingerprint(key).as_str()))
            .ok_or_else(|| PgpError::NoKeys("no such secret key".to_string()))?;
        key.to_armored_bytes(ArmorOptions::default())
            .map(Zeroizing::new)
            .map_err(|e| PgpError::ExportError(e.to_string()))
    }
}

/// The primary key or one secret subkey of a secret key.
#[derive(Clone, Copy)]
enum Component<'a> {
    Primary(&'a SignedSecretKey),
    Subkey(&'a SignedSecretKey, usize),
}

impl<'a> Component<'a> {
    fn all(key: &'a SignedSecretKey) -> impl Iterator<Item = Component<'a>> {
        std::iter::once(Component::Primary(key))
            .chain((0..key.secret_subkeys.len()).map(move |i| Component::Subkey(key, i)))
    }

    fn at(key: &'a SignedSecretKey, index: Option<usize>) -> Self {
        match index {
            None => Component::Primary(key),
            Some(i) => Component::Subkey(key, i),
        }
    }

    fn index(self) -> Option<usize> {
        match self {
            Component::Primary(_) => None,
            Component::Subkey(_, i) => Some(i),
        }
    }

    fn matches(self, esk: &PublicKeyEncryptedSessionKey) -> bool {
        match self {
            Component::Primary(key) => esk.match_identity(key.primary_key.public_key()),
            Component::Subkey(key, i) => esk.match_identity(key.secret_subkeys[i].key.public_key()),
        }
    }

    fn is_encrypted(self) -> bool {
        match self {
            Component::Primary(key) => key.primary_key.secret_params().is_encrypted(),
            Component::Subkey(key, i) => key.secret_subkeys[i].key.secret_params().is_encrypted(),
        }
    }

    fn unlocks_with(self, password: &Password) -> bool {
        let unlocked = match self {
            Component::Primary(key) => key.primary_key.unlock(password, |_, _| Ok(())),
            Component::Subkey(key, i) => key.secret_subkeys[i].key.unlock(password, |_, _| Ok(())),
        };
        matches!(unlocked, Ok(Ok(())))
    }
}

/// What a certificate is usable for right now, from its self-signatures
/// only (third-party certifications are ignored, not rejected).
struct CertView {
    user_id: Option<String>,
    /// The newest valid subkey that can encrypt.
    encryption_subkey: Option<SignedPublicSubKey>,
}

impl CertView {
    /// `None` when the certificate as a whole is unusable: revoked,
    /// expired, or without any valid self-signed user id.
    fn evaluate(cert: &SignedPublicKey, now: u32) -> Option<Self> {
        let primary = &cert.primary_key;
        if cert
            .details
            .revocation_signatures
            .iter()
            .any(|sig| sig.verify_key(primary).is_ok())
        {
            return None;
        }

        // The newest valid self-signature of each user id; the newest of
        // those carries the primary key's current expiry.
        let mut newest: Option<(u32, Option<u32>)> = None;
        let mut user_id = None;
        let mut primary_user_id = None;
        for user in &cert.details.users {
            let Some(sig) = user
                .signatures
                .iter()
                .filter(|sig| {
                    sig.verify_certification(primary, Tag::UserId, &user.id)
                        .is_ok()
                })
                .max_by_key(|sig| created(sig))
            else {
                continue;
            };
            if sig.typ() == Some(SignatureType::CertRevocation) {
                continue;
            }
            let created = created(sig);
            if newest.is_none_or(|(at, _)| created > at) {
                newest = Some((created, sig.key_expiration_time().map(|d| d.as_secs())));
            }
            let text = user.id.as_str().map(str::to_string);
            if sig.is_primary() && primary_user_id.is_none() {
                primary_user_id = text.clone();
            }
            if user_id.is_none() {
                user_id = text;
            }
        }
        let (_, expiry) = newest?;
        if is_expired(primary.created_at(), expiry, now) {
            return None;
        }

        let encryption_subkey = cert
            .public_subkeys
            .iter()
            .filter(|sub| subkey_can_encrypt(primary, sub, now))
            .max_by_key(|sub| sub.key.created_at().as_secs())
            .cloned();
        Some(Self {
            user_id: primary_user_id.or(user_id),
            encryption_subkey,
        })
    }
}

fn subkey_can_encrypt(
    primary: &pgp::packet::PublicKey,
    sub: &SignedPublicSubKey,
    now: u32,
) -> bool {
    if !sub.key.algorithm().can_encrypt() {
        return false;
    }
    let valid =
        |sig: &&pgp::packet::Signature| sig.verify_subkey_binding(primary, &sub.key).is_ok();
    if sub
        .signatures
        .iter()
        .filter(|sig| sig.typ() == Some(SignatureType::SubkeyRevocation))
        .any(|sig| sig.verify_subkey_binding(primary, &sub.key).is_ok())
    {
        return false;
    }
    let Some(binding) = sub
        .signatures
        .iter()
        .filter(|sig| sig.typ() == Some(SignatureType::SubkeyBinding))
        .filter(valid)
        .max_by_key(|sig| created(sig))
    else {
        return false;
    };
    let flags = binding.key_flags();
    (flags.encrypt_comms() || flags.encrypt_storage())
        && !is_expired(
            sub.key.created_at(),
            binding.key_expiration_time().map(|d| d.as_secs()),
            now,
        )
}

fn created(sig: &pgp::packet::Signature) -> u32 {
    sig.created().map(Timestamp::as_secs).unwrap_or(0)
}

/// A key expiration time is relative to the key's creation; 0 or absent
/// means it never expires.
fn is_expired(created_at: Timestamp, expiry: Option<u32>, now: u32) -> bool {
    match expiry {
        Some(secs) if secs > 0 => {
            u64::from(now) >= u64::from(created_at.as_secs()) + u64::from(secs)
        }
        _ => false,
    }
}

fn now() -> u32 {
    Timestamp::now().as_secs()
}

fn fingerprint_hex(key: &impl KeyDetails) -> String {
    format!("{:X}", key.fingerprint())
}

fn secret_fingerprint(key: &SignedSecretKey) -> String {
    fingerprint_hex(&key.primary_key)
}

/// Uppercase hex without spaces or a `0x` prefix, or `None` if `id` is
/// not hex.
fn normalize_hex(id: &str) -> Option<String> {
    let id = id.trim().trim_end_matches('!');
    let id = id
        .strip_prefix("0x")
        .or_else(|| id.strip_prefix("0X"))
        .unwrap_or(id);
    let hex: String = id.chars().filter(|c| !c.is_whitespace()).collect();
    (!hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| hex.to_ascii_uppercase())
}

/// Whether a `.gpg-id` line names this certificate, with gpg's rules:
/// a fingerprint or (long/short) key id of the primary key or a subkey,
/// `=exact user id`, `<email>`, an email, or a user id substring
/// (case-insensitive).
fn matches_recipient(cert: &SignedPublicKey, id: &str) -> bool {
    let id = id.trim();
    if let Some(hex) = normalize_hex(id).filter(|hex| matches!(hex.len(), 8 | 16 | 40 | 64)) {
        let fingerprints = std::iter::once(fingerprint_hex(&cert.primary_key)).chain(
            cert.public_subkeys
                .iter()
                .map(|sub| fingerprint_hex(&sub.key)),
        );
        for fingerprint in fingerprints {
            // v4 key ids are the low 64 (or 32) bits of the fingerprint.
            let found = if hex.len() >= 40 {
                fingerprint == hex
            } else {
                fingerprint.ends_with(&hex)
            };
            if found {
                return true;
            }
        }
        return false;
    }

    let user_ids = cert
        .details
        .users
        .iter()
        .filter_map(|user| user.id.as_str());
    if let Some(exact) = id.strip_prefix('=') {
        return user_ids.into_iter().any(|uid| uid == exact);
    }
    let email = id
        .strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'))
        .or_else(|| id.contains('@').then_some(id));
    let needle = id.to_lowercase();
    user_ids.into_iter().any(|uid| match email {
        Some(email) => email_of(uid).is_some_and(|e| e.eq_ignore_ascii_case(email)),
        None => uid.to_lowercase().contains(&needle),
    })
}

/// The address in `Name <address>`, or the whole user id if it is one.
fn email_of(user_id: &str) -> Option<&str> {
    match (user_id.rfind('<'), user_id.rfind('>')) {
        (Some(start), Some(end)) if start < end => Some(&user_id[start + 1..end]),
        _ => user_id.contains('@').then_some(user_id.trim()),
    }
}

#[cfg(unix)]
fn restrict_to_owner(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Err(err) = fs::set_permissions(dir, fs::Permissions::from_mode(0o700)) {
        log::warn!("could not restrict the keyring folder: {err}");
    }
}

/// No mode bits elsewhere: on Windows the folder keeps the ACL of the
/// user's profile it is created in.
#[cfg(not(unix))]
fn restrict_to_owner(_dir: &Path) {}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    use super::*;
    use crate::helpers::pgp::tests::temp_dir;

    /// Answers every prompt with the same passphrase and counts prompts.
    #[derive(Clone)]
    struct Prompts {
        passphrase: Option<&'static str>,
        asked: Arc<AtomicU32>,
    }

    impl Prompts {
        fn answering(passphrase: &'static str) -> Self {
            Self {
                passphrase: Some(passphrase),
                asked: Arc::default(),
            }
        }

        fn cancelling() -> Self {
            Self {
                passphrase: None,
                asked: Arc::default(),
            }
        }

        fn count(&self) -> u32 {
            self.asked.load(Ordering::SeqCst)
        }
    }

    impl PassphraseProvider for Prompts {
        fn passphrase(&self, _key: &SecretKeyInfo, _retry: bool) -> Option<Zeroizing<String>> {
            self.asked.fetch_add(1, Ordering::SeqCst);
            self.passphrase.map(|p| Zeroizing::new(p.to_string()))
        }

        fn new_passphrase(&self, _user_id: &str) -> Option<Zeroizing<String>> {
            self.passphrase.map(|p| Zeroizing::new(p.to_string()))
        }
    }

    struct Keyring {
        dir: PathBuf,
        backend: RpgpBackend,
    }

    impl Keyring {
        fn new(name: &str, prompts: &Prompts) -> Self {
            let dir = temp_dir(name);
            let backend = RpgpBackend::open(&dir, Box::new(prompts.clone())).unwrap();
            Self { dir, backend }
        }
    }

    impl Drop for Keyring {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    const ALICE: &str = "Alice Example <alice@example.invalid>";

    #[test]
    fn created_key_encrypts_and_decrypts_with_a_cached_passphrase() {
        let prompts = Prompts::answering("correct horse");
        let ring = Keyring::new("rpgp-roundtrip", &prompts);
        let fingerprint = ring.backend.create_key(ALICE).unwrap();
        assert_eq!(fingerprint.len(), 40);

        let keys = ring.backend.secret_keys().unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].fingerprint, fingerprint);
        assert_eq!(keys[0].user_id.as_deref(), Some(ALICE));

        let ciphertext = ring
            .backend
            .encrypt(b"hunter2\nuser: alice", std::slice::from_ref(&fingerprint))
            .unwrap();
        assert_eq!(prompts.count(), 0, "encrypting needs no passphrase");

        for _ in 0..3 {
            let plain = ring.backend.decrypt(&ciphertext).unwrap();
            assert_eq!(&*plain, b"hunter2\nuser: alice");
        }
        assert_eq!(prompts.count(), 1, "the passphrase is asked once");

        ring.backend.lock();
        ring.backend.decrypt(&ciphertext).unwrap();
        assert_eq!(prompts.count(), 2, "locking forgets the passphrase");
    }

    #[test]
    fn keyring_persists_across_opens() {
        let prompts = Prompts::answering("pw");
        let ring = Keyring::new("rpgp-persist", &prompts);
        let fingerprint = ring.backend.create_key(ALICE).unwrap();
        let ciphertext = ring
            .backend
            .encrypt(b"secret", std::slice::from_ref(&fingerprint))
            .unwrap();

        let reopened = RpgpBackend::open(&ring.dir, Box::new(prompts.clone())).unwrap();
        assert_eq!(&*reopened.decrypt(&ciphertext).unwrap(), b"secret");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let file = ring.dir.join("secret").join(format!("{fingerprint}.asc"));
            let mode = fs::metadata(file).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn wrong_or_cancelled_passphrases_do_not_decrypt() {
        let owner = Prompts::answering("right");
        let ring = Keyring::new("rpgp-wrong-pw", &owner);
        let fingerprint = ring.backend.create_key(ALICE).unwrap();
        let ciphertext = ring
            .backend
            .encrypt(b"secret", std::slice::from_ref(&fingerprint))
            .unwrap();

        let wrong = Prompts::answering("wrong");
        let other = RpgpBackend::open(&ring.dir, Box::new(wrong.clone())).unwrap();
        assert!(matches!(
            other.decrypt(&ciphertext),
            Err(PgpError::DecryptError(_))
        ));
        assert_eq!(wrong.count(), PASSPHRASE_ATTEMPTS);

        let cancel = Prompts::cancelling();
        let other = RpgpBackend::open(&ring.dir, Box::new(cancel.clone())).unwrap();
        assert!(matches!(
            other.decrypt(&ciphertext),
            Err(PgpError::Cancelled)
        ));
        assert_eq!(cancel.count(), 1);
    }

    #[test]
    fn backup_restores_on_another_keyring() {
        let prompts = Prompts::answering("pw");
        let a = Keyring::new("rpgp-backup-a", &prompts);
        let b = Keyring::new("rpgp-backup-b", &prompts);
        let fingerprint = a.backend.create_key(ALICE).unwrap();
        let ciphertext = a
            .backend
            .encrypt(b"secret", std::slice::from_ref(&fingerprint))
            .unwrap();

        let backup = a.backend.export_secret_key(&fingerprint).unwrap();
        assert!(backup.starts_with(b"-----BEGIN PGP PRIVATE KEY BLOCK-----"));
        let imported = b.backend.import_secret_keys(&backup).unwrap();
        assert_eq!(imported, vec![fingerprint]);
        assert_eq!(&*b.backend.decrypt(&ciphertext).unwrap(), b"secret");
    }

    #[test]
    fn a_public_key_is_not_a_secret_key_backup() {
        let prompts = Prompts::answering("pw");
        let a = Keyring::new("rpgp-public-a", &prompts);
        let b = Keyring::new("rpgp-public-b", &prompts);
        let fingerprint = a.backend.create_key(ALICE).unwrap();
        let public = a.backend.state().secret[0]
            .to_public_key()
            .to_armored_bytes(ArmorOptions::default())
            .unwrap();

        assert!(matches!(
            b.backend.import_secret_keys(&public),
            Err(PgpError::NoSecretKey)
        ));
        // ...but it makes `b` able to encrypt to `a` (a shared vault).
        assert_eq!(
            b.backend.import_public_keys(&public).unwrap(),
            vec![fingerprint.clone()]
        );
        let ciphertext = b
            .backend
            .encrypt(b"for alice", std::slice::from_ref(&fingerprint))
            .unwrap();
        assert_eq!(&*a.backend.decrypt(&ciphertext).unwrap(), b"for alice");
        assert!(matches!(
            b.backend.decrypt(&ciphertext),
            Err(PgpError::DecryptError(_))
        ));
    }

    #[test]
    fn gpg_id_lines_resolve_like_gpg() {
        let prompts = Prompts::answering("");
        let ring = Keyring::new("rpgp-recipients", &prompts);
        let fingerprint = ring.backend.create_key(ALICE).unwrap();
        let encrypts_to = |id: &str| ring.backend.encrypt(b"x", &[id.to_string()]).is_ok();

        for id in [
            fingerprint.clone(),
            fingerprint.to_lowercase(),
            format!("0x{}", &fingerprint[24..]),
            fingerprint[32..].to_string(),
            "alice@example.invalid".to_string(),
            "<ALICE@example.invalid>".to_string(),
            "alice example".to_string(),
            format!("={ALICE}"),
        ] {
            assert!(encrypts_to(&id), "{id} should name the key");
        }
        for id in ["bob@example.invalid", "=Alice", "DEADBEEFDEADBEEF", "Bob"] {
            assert!(
                matches!(
                    ring.backend.encrypt(b"x", &[id.to_string()]),
                    Err(PgpError::NoKeys(_))
                ),
                "{id} should not name the key"
            );
        }
        // One unknown recipient fails the whole write.
        assert!(ring
            .backend
            .encrypt(b"x", &[fingerprint, "bob@example.invalid".to_string()])
            .is_err());
    }

    #[test]
    fn an_unprotected_key_never_prompts() {
        let prompts = Prompts::answering("");
        let ring = Keyring::new("rpgp-unprotected", &prompts);
        let fingerprint = ring.backend.create_key(ALICE).unwrap();
        let ciphertext = ring.backend.encrypt(b"x", &[fingerprint]).unwrap();
        ring.backend.decrypt(&ciphertext).unwrap();
        assert_eq!(prompts.count(), 0);
    }

    /// Interop with the real GnuPG: what `pass` and the desktop app write
    /// must open on mobile, and the other way round.
    #[cfg(feature = "gpgme")]
    mod gnupg {
        use std::process::{Command, Stdio};

        use super::*;
        use crate::helpers::pgp::gpgme::tests::TempGnupgHome;

        const PASSPHRASE: &str = "interop passphrase";

        /// Runs gpg on `home` with the passphrase given inline (no
        /// pinentry), feeding `stdin`, and returns stdout.
        fn gpg(home: &TempGnupgHome, args: &[&str], stdin: &[u8]) -> Vec<u8> {
            let mut child = Command::new("gpg")
                .arg("--homedir")
                .arg(&home.path)
                .args(["--batch", "--yes", "--pinentry-mode", "loopback"])
                .args(["--passphrase", PASSPHRASE])
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("gpg is installed");
            child.stdin.take().unwrap().write_all(stdin).unwrap();
            let out = child.wait_with_output().unwrap();
            assert!(
                out.status.success(),
                "gpg {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            out.stdout
        }

        fn gpg_fingerprint(home: &TempGnupgHome) -> String {
            let listing = gpg(home, &["--with-colons", "--list-secret-keys"], b"");
            String::from_utf8(listing)
                .unwrap()
                .lines()
                .find_map(|line| line.strip_prefix("fpr:"))
                .map(|rest| rest.trim_matches(':').to_string())
                .expect("a fingerprint")
        }

        use std::io::Write as _;

        #[test]
        fn a_protected_gpg_key_works_on_rpgp() {
            let home = TempGnupgHome::new("interop-gpg-key");
            // gpg's defaults: ed25519 primary, cv25519 subkey.
            gpg(
                &home,
                &["--quick-generate-key", ALICE, "default", "default", "never"],
                b"",
            );
            let fingerprint = gpg_fingerprint(&home);
            let backup = gpg(
                &home,
                &["--armor", "--export-secret-keys", &fingerprint],
                b"",
            );

            let prompts = Prompts::answering(PASSPHRASE);
            let ring = Keyring::new("interop-gpg-key-rpgp", &prompts);
            assert_eq!(
                ring.backend.import_secret_keys(&backup).unwrap(),
                vec![fingerprint.clone()]
            );

            // gpg -> rPGP, the way `pass insert` writes entries.
            let ciphertext = gpg(
                &home,
                &["--encrypt", "--recipient", &fingerprint],
                b"from gpg",
            );
            assert_eq!(&*ring.backend.decrypt(&ciphertext).unwrap(), b"from gpg");
            // GnuPG's own OCB packet (LibrePGP), which gpg 2.3+ writes
            // when every recipient key advertises it.
            let ciphertext = gpg(
                &home,
                &["--force-ocb", "--encrypt", "--recipient", &fingerprint],
                b"from gpg, ocb",
            );
            assert_eq!(
                &*ring.backend.decrypt(&ciphertext).unwrap(),
                b"from gpg, ocb"
            );
            // Same through GPGME, the way the desktop app writes them.
            let ciphertext = home
                .backend()
                .encrypt(b"from gpgme", std::slice::from_ref(&fingerprint))
                .unwrap();
            assert_eq!(&*ring.backend.decrypt(&ciphertext).unwrap(), b"from gpgme");

            // rPGP -> gpg.
            let ciphertext = ring
                .backend
                .encrypt(b"from rpgp", std::slice::from_ref(&fingerprint))
                .unwrap();
            assert_eq!(gpg(&home, &["--decrypt"], &ciphertext), b"from rpgp");
        }

        #[test]
        fn a_protected_rpgp_key_works_on_gpg() {
            let prompts = Prompts::answering(PASSPHRASE);
            let ring = Keyring::new("interop-rpgp-key", &prompts);
            let fingerprint = ring.backend.create_key(ALICE).unwrap();
            let backup = ring.backend.export_secret_key(&fingerprint).unwrap();

            let home = TempGnupgHome::new("interop-rpgp-key-gpg");
            gpg(&home, &["--import"], &backup);
            assert_eq!(gpg_fingerprint(&home), fingerprint);
            // The desktop app's import also marks it trusted, so GPGME
            // encrypts to it without prompting.
            home.backend().import_secret_keys(&backup).unwrap();

            let ciphertext = ring
                .backend
                .encrypt(b"from rpgp", std::slice::from_ref(&fingerprint))
                .unwrap();
            assert_eq!(gpg(&home, &["--decrypt"], &ciphertext), b"from rpgp");

            let ciphertext = home
                .backend()
                .encrypt(b"from gpgme", std::slice::from_ref(&fingerprint))
                .unwrap();
            assert_eq!(&*ring.backend.decrypt(&ciphertext).unwrap(), b"from gpgme");
        }
    }
}
