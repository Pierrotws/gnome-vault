use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use gnome_vault::helpers::pgp::{self, PgpError};

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

    let ids = pgp::recipient_ids(&dir).unwrap();

    assert_eq!(ids, vec!["alice@example.invalid", "bob@example.invalid"]);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn rejects_empty_gpg_id() {
    let dir = temp_dir("pgp-empty-recipients");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(".gpg-id"), "\n# comment\n\n").unwrap();

    let err = pgp::recipient_ids(&dir).unwrap_err();

    assert!(matches!(err, PgpError::NoRecipients(_)));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn reports_missing_gpg_id_as_invalid_path() {
    let dir = temp_dir("pgp-missing-recipients");
    fs::create_dir_all(&dir).unwrap();

    let err = pgp::recipient_ids(&dir).unwrap_err();

    assert!(matches!(err, PgpError::InvalidPath(_)));

    fs::remove_dir_all(dir).unwrap();
}
