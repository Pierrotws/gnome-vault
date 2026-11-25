//! The mobile API end to end, as the app uses it: a phone (this
//! `Passel`) and a desktop share a vault through a bare remote.
//!
//! `Passel` is process-wide (it installs the OpenPGP backend and git
//! credentials), so everything runs in one test.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use passel_core::{
    helpers::git,
    pass::{
        model::{EntryData, EntryField, PassNode, PassNodeKind},
        store,
    },
};
use passel_ffi::{
    generate_password, otp_code, Entry, EntryRef, Field, FieldKind, KeyInfo, Passel, PasselError,
    PassphrasePrompt, RemoteState, VaultInfo,
};

const PASSPHRASE: &str = "phone passphrase";
const OTP_URI: &str = "otpauth://totp/GitHub:me?secret=JBSWY3DPEHPK3PXP&issuer=GitHub";

#[derive(Default)]
struct Prompt {
    asked: AtomicU32,
}

impl PassphrasePrompt for Prompt {
    fn passphrase(&self, key: KeyInfo, _retry: bool) -> Option<String> {
        assert_eq!(
            key.user_id.as_deref(),
            Some("Phone <phone@example.invalid>")
        );
        self.asked.fetch_add(1, Ordering::SeqCst);
        Some(PASSPHRASE.to_string())
    }

    fn new_passphrase(&self, _user_id: String) -> Option<String> {
        Some(PASSPHRASE.to_string())
    }
}

struct Dirs {
    root: PathBuf,
}

impl Drop for Dirs {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn temp_root() -> Dirs {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("passel-ffi-{}-{unique}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    Dirs { root }
}

fn node(path: &str) -> PassNode {
    PassNode {
        name: Path::new(path)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        path: PathBuf::from(path),
        kind: PassNodeKind::Entry,
    }
}

fn desktop_password(desktop: &Path, path: &str) -> String {
    store::load_entry_from_node(desktop, &node(path))
        .unwrap()
        .password
        .display_value()
        .into_owned()
}

/// The desktop side: a working copy pushing to the bare remote.
fn desktop_vault(root: &Path, recipient: &str) -> (PathBuf, String) {
    let remote = root.join("remote.git");
    git::init_bare(&remote).unwrap();
    let desktop = root.join("desktop");
    git::init(&desktop).unwrap();
    git::set_identity(&desktop, "Desktop", "desktop@example.invalid").unwrap();
    git::set_origin(&desktop, remote.to_str().unwrap()).unwrap();
    fs::write(desktop.join(".gpg-id"), format!("{recipient}\n")).unwrap();
    git::add(&desktop, &desktop.join(".gpg-id")).unwrap();
    git::commit(&desktop, "Initialize vault").unwrap();

    let entry = EntryData {
        password: EntryField::Password("desktop-secret".into()),
        fields: vec![
            ("login".into(), EntryField::Plain("me".into())),
            ("otp".into(), EntryField::Otp(OTP_URI.into())),
        ],
    };
    store::create_entry_data(&desktop, Path::new("web"), "github", &entry, false).unwrap();
    git::push(&desktop, None).unwrap();
    (desktop, remote.to_string_lossy().into_owned())
}

#[test]
fn a_phone_and_a_desktop_share_a_vault() {
    let dirs = temp_root();
    let data_dir = dirs.root.join("phone");
    let prompt = Arc::new(Prompt::default());
    let passel = Passel::new(data_dir.to_string_lossy().into_owned(), prompt.clone()).unwrap();

    // Same folder: the same instance. Another folder: refused.
    let again = Passel::new(data_dir.to_string_lossy().into_owned(), prompt.clone()).unwrap();
    assert!(Arc::ptr_eq(&passel, &again));
    assert!(Passel::new("/elsewhere".into(), prompt.clone()).is_err());

    // A key made on the phone; the desktop vault is encrypted to it.
    assert!(passel.keys().unwrap().is_empty());
    let fingerprint = passel
        .create_key("Phone".into(), "phone@example.invalid".into())
        .unwrap();
    assert_eq!(passel.keys().unwrap()[0].fingerprint, fingerprint);
    let public = passel.export_public_key(fingerprint.clone()).unwrap();
    assert!(public.starts_with(b"-----BEGIN PGP PUBLIC KEY BLOCK-----"));
    passel.lock();
    let (desktop, remote) = desktop_vault(&dirs.root, &fingerprint);

    // Clone and browse.
    assert!(!passel.has_vault());
    assert_eq!(passel.check_remote(remote.clone()), RemoteState::NotEmpty);
    let shared = passel
        .clone_vault(
            "Shared".into(),
            remote.clone(),
            "Phone".into(),
            "phone@example.invalid".into(),
        )
        .unwrap();
    assert_eq!(
        shared,
        VaultInfo {
            id: "vaults/shared".into(),
            name: "Shared".into(),
            remote_url: Some(remote.clone()),
            active: true,
        }
    );
    assert!(passel.has_vault());
    assert_eq!(passel.remote_url(), Some(remote.clone()));
    assert_eq!(
        passel.entries().unwrap(),
        vec![EntryRef {
            path: "web/github.gpg".into(),
            name: "github".into(),
            folder: "web".into(),
        }]
    );

    // Open: the passphrase is asked once, then cached.
    let entry = passel.open_entry("web/github.gpg".into()).unwrap();
    passel.open_entry("web/github.gpg".into()).unwrap();
    assert_eq!(prompt.asked.load(Ordering::SeqCst), 1);
    assert_eq!(entry.password, "desktop-secret");
    assert_eq!(
        entry.fields,
        vec![
            Field {
                name: "login".into(),
                value: "me".into(),
                kind: FieldKind::Text,
            },
            Field {
                name: "otp".into(),
                value: OTP_URI.into(),
                kind: FieldKind::Otp,
            },
        ]
    );
    let code = otp_code(OTP_URI.into()).unwrap();
    assert_eq!(code.code.len(), 6);
    assert!(code.seconds_left <= code.period && code.period == 30);

    // Edit on the phone, sync, read on the desktop.
    let new_password = generate_password(24, true).unwrap();
    assert_eq!(new_password.chars().count(), 24);
    passel
        .save_entry(
            "web/github.gpg".into(),
            Entry {
                password: new_password.clone(),
                ..entry.clone()
            },
        )
        .unwrap();
    assert_eq!(passel.unsynced_count(), 1);
    let synced = passel.sync().unwrap();
    assert_eq!((synced.received, synced.sent), (0, 1));
    git::sync(&desktop, None).unwrap();
    assert_eq!(desktop_password(&desktop, "web/github.gpg"), new_password);

    // Create and delete.
    let path = passel
        .create_entry(
            "mail".into(),
            "fastmail".into(),
            Entry {
                password: "p".into(),
                fields: vec![Field {
                    name: "totp".into(),
                    value: OTP_URI.into(),
                    // The value decides: an otpauth URI is an OTP field.
                    kind: FieldKind::Text,
                }],
            },
        )
        .unwrap();
    assert_eq!(path, "mail/fastmail.gpg");
    assert_eq!(
        passel.open_entry(path.clone()).unwrap().fields[0].kind,
        FieldKind::Otp
    );
    passel.delete_entry(path).unwrap();
    assert_eq!(passel.entries().unwrap().len(), 1);
    passel.sync().unwrap();

    // The same entry edited on both sides: reported, nothing lost.
    git::sync(&desktop, None).unwrap();
    store::save_entry_data(
        &desktop,
        &node("web/github.gpg"),
        &EntryData {
            password: EntryField::Password("desktop-v3".into()),
            fields: vec![],
        },
        true,
    )
    .unwrap();
    passel
        .save_entry(
            "web/github.gpg".into(),
            Entry {
                password: "phone-v3".into(),
                fields: vec![],
            },
        )
        .unwrap();
    match passel.sync() {
        Err(PasselError::Conflict { entries, kept_as }) => {
            assert_eq!(entries, vec!["web/github"]);
            assert!(kept_as.is_some_and(|branch| branch.starts_with("passel-unmerged/")));
        }
        other => panic!("expected a conflict, got {other:?}"),
    }
    assert_eq!(
        passel.open_entry("web/github.gpg".into()).unwrap().password,
        "phone-v3"
    );

    // Paths cannot leave the vault.
    for path in ["../keys/x.gpg", "/etc/passwd.gpg", "web/github"] {
        assert!(
            matches!(
                passel.open_entry(path.into()),
                Err(PasselError::Failed { .. })
            ),
            "{path}"
        );
    }

    // A second vault, created on the phone and sent to an empty remote.
    let empty_remote = dirs.root.join("new.git");
    git::init_bare(&empty_remote).unwrap();
    let empty_url = empty_remote.to_string_lossy().into_owned();
    assert_eq!(passel.check_remote(empty_url.clone()), RemoteState::Empty);
    let created = passel
        .create_vault(
            "Personal".into(),
            fingerprint.clone(),
            Some(empty_url.clone()),
            "Phone".into(),
            "phone@example.invalid".into(),
        )
        .unwrap();
    assert_eq!(created.upload_error, None);
    assert_eq!(created.vault.id, "vaults/personal");
    assert!(created.vault.active);
    assert!(passel.entries().unwrap().is_empty());
    passel
        .create_entry(
            "".into(),
            "bank".into(),
            Entry {
                password: "1234".into(),
                fields: vec![],
            },
        )
        .unwrap();
    assert_eq!(
        (passel.sync().unwrap().sent, passel.unsynced_count()),
        (1, 0)
    );
    // The remote now holds it: a clone elsewhere reads .gpg-id and bank.
    let elsewhere = dirs.root.join("elsewhere");
    git::clone(&empty_url, &elsewhere).unwrap();
    assert_eq!(
        std::fs::read_to_string(elsewhere.join(".gpg-id")).unwrap(),
        format!("{fingerprint}\n")
    );
    assert!(elsewhere.join("bank.gpg").exists());

    // Creating where a vault already is: refused, nothing made.
    assert!(matches!(
        passel.create_vault(
            "Again".into(),
            fingerprint.clone(),
            Some(empty_url),
            "Phone".into(),
            "phone@example.invalid".into(),
        ),
        Err(PasselError::Failed { .. })
    ));
    // An unknown key: refused too.
    assert!(passel
        .create_vault(
            "Nope".into(),
            "DEADBEEF".into(),
            None,
            "Phone".into(),
            "phone@example.invalid".into(),
        )
        .is_err());
    assert!(!data_dir.join("vaults/nope").exists());

    // Two vaults now; switching changes what the entry calls see.
    let names = |p: &Passel| {
        p.vaults()
            .into_iter()
            .map(|v| (v.name, v.active))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(&passel),
        vec![("Shared".into(), false), ("Personal".into(), true)]
    );
    passel.switch_vault("vaults/shared".into()).unwrap();
    assert!(passel
        .entries()
        .unwrap()
        .iter()
        .any(|e| e.path == "web/github.gpg"));
    assert!(passel.switch_vault("../keys".into()).is_err());

    // Removing the open vault opens the other one.
    passel.remove_vault("vaults/shared".into()).unwrap();
    assert_eq!(names(&passel), vec![("Personal".into(), true)]);
    assert!(!data_dir.join("vaults/shared").exists());

    // A wrong remote fails cleanly and leaves no half-cloned vault.
    let missing = dirs.root.join("missing.git").to_string_lossy().into_owned();
    assert!(passel
        .clone_vault(
            "Missing".into(),
            missing,
            "Phone".into(),
            "phone@example.invalid".into()
        )
        .is_err());
    assert!(!data_dir.join("vaults/missing").exists());
    assert_eq!(passel.vaults().len(), 1);
}
