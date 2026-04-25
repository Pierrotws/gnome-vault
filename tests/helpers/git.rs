use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use gnome_vault::helpers::git;

fn temp_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("gnome-vault-{name}-{}-{unique}", std::process::id()))
}

fn run_git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(dir)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

#[test]
fn adds_and_commits_file() {
    let dir = temp_dir("git-commit");
    fs::create_dir_all(&dir).unwrap();

    run_git(&dir, &["init"]);
    run_git(&dir, &["config", "user.email", "test@example.invalid"]);
    run_git(&dir, &["config", "user.name", "Gnome Vault Tests"]);

    let file_path = dir.join("entry.gpg");
    fs::write(&file_path, "encrypted").unwrap();

    git::add(&dir, &file_path).unwrap();
    git::commit(&dir, "Add entry").unwrap();

    let output = Command::new("git")
        .current_dir(&dir)
        .args(["log", "--oneline", "-1"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout).unwrap().contains("Add entry"));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn reports_command_failure_stderr() {
    let dir = temp_dir("git-failure");
    fs::create_dir_all(&dir).unwrap();

    let err = git::commit(&dir, "No repository").unwrap_err();
    let message = err.to_string();

    assert!(message.contains("git commit failed with status"));
    assert!(!message.trim().is_empty());

    fs::remove_dir_all(dir).unwrap();
}
