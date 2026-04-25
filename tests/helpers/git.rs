use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use git2::Repository;
use gnome_vault::helpers::git::{self, GitError};

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

fn init_repo(dir: &PathBuf) -> Repository {
    let repo = Repository::init(dir).unwrap();
    let mut config = repo.config().unwrap();
    config
        .set_str("user.email", "test@example.invalid")
        .unwrap();
    config.set_str("user.name", "Gnome Vault Tests").unwrap();
    drop(config);
    repo
}

#[test]
fn adds_and_commits_file() {
    let dir = temp_dir("git-commit");
    fs::create_dir_all(&dir).unwrap();
    let repo = init_repo(&dir);

    let file_path = dir.join("entry.gpg");
    fs::write(&file_path, "encrypted").unwrap();

    git::add(&dir, &file_path).unwrap();
    git::commit(&dir, "Add entry").unwrap();

    let commit = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(commit.message(), Some("Add entry"));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn reports_open_repository_failure() {
    let dir = temp_dir("git-failure");
    fs::create_dir_all(&dir).unwrap();

    let err = git::commit(&dir, "No repository").unwrap_err();
    let message = err.to_string();

    assert!(matches!(err, GitError::Git(_)));
    assert!(message.contains("Git error:"));
    assert!(!message.trim().is_empty());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn rejects_add_paths_outside_workdir() {
    let dir = temp_dir("git-invalid-path");
    let outside_dir = temp_dir("git-outside");
    fs::create_dir_all(&dir).unwrap();
    fs::create_dir_all(&outside_dir).unwrap();
    let _repo = init_repo(&dir);

    let outside_file = outside_dir.join("entry.gpg");
    fs::write(&outside_file, "encrypted").unwrap();

    let err = git::add(&dir, &outside_file).unwrap_err();

    assert!(matches!(err, GitError::InvalidPath { .. }));

    fs::remove_dir_all(dir).unwrap();
    fs::remove_dir_all(outside_dir).unwrap();
}
