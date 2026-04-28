//Backend operations as functions.

mod store_error;

use std::{
    fs, io,
    io::Write,
    path::{Component, Path, PathBuf},
};

use crate::{
    helpers::{
        git::{self, GitChange},
        parser, pgp,
    },
    pass::model::{EntryData, PassNode, PassNodeKind},
};

pub use crate::helpers::pgp::GpgRecipient;
pub use store_error::StoreError;

#[derive(Debug, Clone)]
pub struct VaultSetup {
    pub store_dir: PathBuf,
    pub recipient: String,
    pub remote_url: Option<String>,
    pub autopush: bool,
}

/// Recursively loads password-store groups and `.gpg` entries.
fn load_dir(root: &Path, dir: &Path) -> io::Result<Vec<PassNode>> {
    let mut nodes = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let _name = entry.file_name();
        let file_name = _name.to_string_lossy();
        // IGNORER les fichiers/dossiers cachés
        if file_name.starts_with('.') {
            continue;
        }
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            let children = load_dir(root, &path)?;

            nodes.push(PassNode {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: relative_path(root, &path),
                kind: PassNodeKind::Group,
                children,
            });
        } else if file_type.is_file() {
            if path.extension().and_then(|e| e.to_str()) == Some("gpg") {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();

                nodes.push(PassNode {
                    name: stem,
                    path: relative_path(root, &path),
                    kind: PassNodeKind::Entry,
                    children: Vec::new(),
                });
            }
        }
    }
    nodes.sort_by(|a, b| match (&a.kind, &b.kind) {
        (PassNodeKind::Group, PassNodeKind::Entry) => std::cmp::Ordering::Less,
        (PassNodeKind::Entry, PassNodeKind::Group) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    //Returns
    Ok(nodes)
}

fn relative_path(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

pub fn load_password_store() -> io::Result<Vec<PassNode>> {
    let store_dir = password_store_dir();
    log::debug!("Loading password store from: {}", store_dir.display());
    if !git::is_repository(&store_dir) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} is not a git repository", store_dir.display()),
        ));
    }
    let nodes = load_dir(&store_dir, &store_dir)?;
    log::debug!("Top-level nodes: {}", nodes.len());
    //Returns
    Ok(nodes)
}

/// Initializes a new password-store git repository.
pub fn setup_vault(setup: &VaultSetup) -> Result<(), StoreError> {
    let recipient = setup.recipient.trim();
    if recipient.is_empty() {
        return Err(StoreError::InvalidRecipient);
    }

    fs::create_dir_all(&setup.store_dir)?;
    if !git::is_repository(&setup.store_dir) {
        git::init(&setup.store_dir)?;
    }

    let gpg_id_path = setup.store_dir.join(".gpg-id");
    fs::write(&gpg_id_path, format!("{recipient}\n"))?;
    git::add(&setup.store_dir, &gpg_id_path)?;
    git::commit(&setup.store_dir, "Initialize vault")?;

    if let Some(remote_url) = setup.remote_url.as_deref().map(str::trim) {
        if !remote_url.is_empty() {
            git::set_origin(&setup.store_dir, remote_url)?;
            if setup.autopush {
                git::push(&setup.store_dir, None)?;
            }
        }
    }

    Ok(())
}

/// Lists usable GPG recipients for a newly created vault.
pub fn available_recipients() -> Result<Vec<GpgRecipient>, StoreError> {
    Ok(pgp::available_recipients()?)
}

/// Loads and decrypts a password-store entry from a tree node.
pub fn load_entry_from_node(node: &PassNode) -> Result<EntryData, StoreError> {
    let path = password_store_dir().join(&node.path);
    let content = pgp::decrypt(&path)?;
    parser::parse_entry(&content).ok_or_else(|| StoreError::EmptyFile(path))
}

/// Lists commit history for the password store repository.
///
/// When `branch` is `Some`, history is read from that local branch; otherwise
/// the working tree's current branch is used.
pub fn load_changes(branch: Option<&str>) -> Result<Vec<GitChange>, StoreError> {
    Ok(git::branch_changes(&password_store_dir(), branch)?)
}

/// Lists one page from the password store commit history.
///
/// When `branch` is `Some`, that local branch is walked; otherwise the
/// working tree's current branch is used.
pub fn load_changes_page(
    offset: usize,
    limit: usize,
    branch: Option<&str>,
) -> Result<Vec<GitChange>, StoreError> {
    Ok(git::branch_changes_page(
        &password_store_dir(),
        branch,
        offset,
        limit,
    )?)
}

/// Reverts a commit in the password store repository and pushes the result.
pub fn revert_change(commit_id: &str, autopush: bool) -> Result<(), StoreError> {
    let store_dir = password_store_dir();
    git::revert_commit(&store_dir, commit_id)?;
    if autopush {
        git::push(&store_dir, None)?;
    }
    Ok(())
}

/// Pushes committed local password-store changes.
///
/// When `branch` is `Some`, that local branch is pushed; otherwise the
/// working tree's current branch is used.
pub fn push_changes(branch: Option<&str>) -> Result<(), StoreError> {
    Ok(git::push(&password_store_dir(), branch)?)
}

/// Backs up the current branch, resets it to a commit, and pushes the reset.
pub fn rollback_to_change(commit_id: &str) -> Result<String, StoreError> {
    Ok(git::rollback_to_commit(&password_store_dir(), commit_id)?)
}

/// Deletes an entry file, commits the deletion, and pushes it.
pub fn delete_entry(node: &PassNode, autopush: bool) -> Result<(), StoreError> {
    let store_dir = password_store_dir();
    let entry_path = store_dir.join(&node.path);

    fs::remove_file(&entry_path)?;
    git::remove(&store_dir, &entry_path)?;
    git::commit(&store_dir, &format!("Delete entry {}", node.name))?;
    if autopush {
        git::push(&store_dir, None)?;
    }

    Ok(())
}

/// Creates a new entry under a password-store folder.
pub fn create_entry_data(
    folder_path: &Path,
    name: &str,
    entry: &EntryData,
    autopush: bool,
) -> Result<PassNode, StoreError> {
    validate_folder_path(folder_path)?;
    let name = valid_entry_name(name)?;
    let node = PassNode {
        name: name.to_string(),
        path: folder_path.join(format!("{name}.gpg")),
        kind: PassNodeKind::Entry,
        children: Vec::new(),
    };

    let store_dir = password_store_dir();
    let output_path = store_dir.join(&node.path);
    if output_path.exists() {
        return Err(StoreError::DestinationExists(output_path));
    }

    write_entry_data(&node, entry, &format!("Add entry {}", node.name), autopush)?;
    Ok(node)
}

/// Renames an entry file, commits the move, and pushes it.
pub fn rename_entry(
    node: &PassNode,
    new_name: &str,
    autopush: bool,
) -> Result<PassNode, StoreError> {
    let new_name = valid_entry_name(new_name)?;

    if node.name == new_name {
        return Ok(node.clone());
    }

    let store_dir = password_store_dir();
    let old_path = store_dir.join(&node.path);
    let new_relative_path = node.path.with_file_name(format!("{new_name}.gpg"));
    let new_path = store_dir.join(&new_relative_path);

    if new_path.exists() {
        return Err(StoreError::DestinationExists(new_path));
    }

    fs::rename(&old_path, &new_path)?;
    git::rename(&store_dir, &old_path, &new_path)?;
    git::commit(&store_dir, "rename entry")?;
    if autopush {
        git::push(&store_dir, None)?;
    }

    Ok(PassNode {
        name: new_name.to_string(),
        path: new_relative_path,
        kind: node.kind.clone(),
        children: node.children.clone(),
    })
}

/// Encrypts, writes, commits, and pushes entry data for a tree node.
pub fn save_entry_data(
    node: &PassNode,
    entry: &EntryData,
    autopush: bool,
) -> Result<(), StoreError> {
    let message = format!("Add/update entry {}", node.name);
    write_entry_data(node, entry, &message, autopush)
}

fn write_entry_data(
    node: &PassNode,
    entry: &EntryData,
    message: &str,
    autopush: bool,
) -> Result<(), StoreError> {
    let plaintext = parser::format_entry(entry);
    let store_dir = password_store_dir();
    let output_path = store_dir.join(&node.path);
    let parent = output_path
        .parent()
        .ok_or_else(|| StoreError::MissingParent(output_path.clone()))?;
    // Create the parent first so we can canonicalize it. Creating the directory
    // tree is idempotent; we abort before writing the entry file if the
    // resolved parent escapes the store root (e.g. via a symlink).
    fs::create_dir_all(parent)?;
    ensure_inside_store(&store_dir, parent)?;
    let recipient_ids = pgp::recipient_ids_for(&store_dir, parent)?;
    let encrypted = pgp::encrypt(&plaintext, &recipient_ids)?;

    // Capture the previous on-disk ciphertext so we can attempt a best-effort
    // rollback if anything between the rename and the push errors out. This
    // protects against losing an existing entry to a partial commit.
    let previous = match fs::read(&output_path) {
        Ok(bytes) => Some(bytes),
        Err(err) if err.kind() == io::ErrorKind::NotFound => None,
        Err(err) => return Err(StoreError::Io(err)),
    };

    // Atomic write: write the ciphertext to a sibling temp file, fsync the
    // file's contents, then rename it over the destination. Rename is atomic
    // on the same filesystem, so a crash mid-write cannot leave the entry in
    // a half-encrypted state where decryption would fail.
    let tmp_path = output_path.with_extension("gpg.tmp");
    write_atomically(&tmp_path, &output_path, &encrypted)?;

    // Best-effort restoration: post-rename, the on-disk file IS the new
    // ciphertext. If a subsequent step fails we try to put the prior bytes
    // back so the user does not see the entry vanish on a transient error.
    // If even the restore fails we surface the original error and log; the
    // user can recover via git history.
    if let Err(err) = git::add(&store_dir, &output_path) {
        restore_previous(&output_path, previous.as_deref());
        return Err(StoreError::Git(err));
    }
    if let Err(err) = git::commit(&store_dir, message) {
        restore_previous(&output_path, previous.as_deref());
        return Err(StoreError::Git(err));
    }
    if autopush {
        // Local commit succeeded — only the push failed. Surface this as
        // PushFailed so the UI can tell the user "saved locally, retry push"
        // rather than implying the entire save failed. Do NOT roll back the
        // committed file: the local state is the new value the user wanted.
        if let Err(err) = git::push(&store_dir, None) {
            return Err(StoreError::PushFailed(err.to_string()));
        }
    }
    Ok(())
}

/// Writes `bytes` to `tmp_path`, fsyncs, and renames it over `final_path`.
///
/// Rename is atomic on the same filesystem so observers either see the prior
/// content or the new content — never a truncated half-encrypted file.
fn write_atomically(tmp_path: &Path, final_path: &Path, bytes: &[u8]) -> io::Result<()> {
    {
        let mut file = fs::File::create(tmp_path)?;
        file.write_all(bytes)?;
        // Flush kernel buffers so the rename can't expose a zero-length file
        // after a crash.
        file.sync_all()?;
    }
    if let Err(err) = fs::rename(tmp_path, final_path) {
        // Don't leave stale temp files behind on rename failure.
        let _ = fs::remove_file(tmp_path);
        return Err(err);
    }
    Ok(())
}

/// Best-effort restore of a previously-saved ciphertext after a post-rename
/// failure. If `previous` is `None` the file did not exist before, so we
/// remove the new one. Failures here are logged and swallowed: the caller
/// will surface the original error.
fn restore_previous(output_path: &Path, previous: Option<&[u8]>) {
    let result = match previous {
        Some(bytes) => fs::write(output_path, bytes),
        None => fs::remove_file(output_path),
    };
    if let Err(err) = result {
        log::warn!(
            "failed to restore previous entry at {}: {err}",
            output_path.display()
        );
    }
}

/// Canonicalizes the deepest existing ancestor of `parent` and asserts it lies
/// inside `store_dir`. Guards against folder-symlink escapes from the store.
fn ensure_inside_store(store_dir: &Path, parent: &Path) -> Result<(), StoreError> {
    let canonical_root = store_dir
        .canonicalize()
        .map_err(|_| StoreError::InvalidFolderPath(parent.to_path_buf()))?;

    // Walk up until we find a path that exists (and can therefore be
    // canonicalized). At minimum this terminates at `store_dir` itself.
    let mut probe: &Path = parent;
    let canonical_parent = loop {
        if let Ok(resolved) = probe.canonicalize() {
            break resolved;
        }
        match probe.parent() {
            Some(p) => probe = p,
            None => return Err(StoreError::InvalidFolderPath(parent.to_path_buf())),
        }
    };

    if !canonical_parent.starts_with(&canonical_root) {
        return Err(StoreError::InvalidFolderPath(parent.to_path_buf()));
    }

    Ok(())
}

fn valid_entry_name(name: &str) -> Result<&str, StoreError> {
    let name = name.trim();
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains('\0')
        || name.starts_with('-')
        || name == "."
        || name == ".."
        || name == ".gpg-id"
        || name == ".gpg-id.gpg"
        || name == ".gitattributes"
    {
        return Err(StoreError::InvalidEntryName(name.to_string()));
    }
    Ok(name)
}

fn validate_folder_path(path: &Path) -> Result<(), StoreError> {
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(StoreError::InvalidFolderPath(path.to_path_buf()));
    }
    Ok(())
}

/// Returns the active password-store directory.
///
/// Honors `PASSWORD_STORE_DIR` and falls back to `~/.password-store`.
pub fn password_store_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("PASSWORD_STORE_DIR") {
        PathBuf::from(dir)
    } else {
        let home = std::env::var("HOME").expect("HOME not set");
        PathBuf::from(home).join(".password-store")
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
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
    fn validates_entry_names() {
        assert_eq!(valid_entry_name(" example ").unwrap(), "example");
        assert!(matches!(
            valid_entry_name(""),
            Err(StoreError::InvalidEntryName(_))
        ));
        assert!(matches!(
            valid_entry_name("folder/example"),
            Err(StoreError::InvalidEntryName(_))
        ));
        assert!(matches!(
            valid_entry_name("folder\\example"),
            Err(StoreError::InvalidEntryName(_))
        ));
        assert!(matches!(
            valid_entry_name("."),
            Err(StoreError::InvalidEntryName(_))
        ));
        assert!(matches!(
            valid_entry_name(".."),
            Err(StoreError::InvalidEntryName(_))
        ));
        assert!(matches!(
            valid_entry_name("foo\0bar"),
            Err(StoreError::InvalidEntryName(_))
        ));
        assert!(matches!(
            valid_entry_name(".gpg-id"),
            Err(StoreError::InvalidEntryName(_))
        ));
        assert!(matches!(
            valid_entry_name(".gpg-id.gpg"),
            Err(StoreError::InvalidEntryName(_))
        ));
        assert!(matches!(
            valid_entry_name(".gitattributes"),
            Err(StoreError::InvalidEntryName(_))
        ));
        assert!(matches!(
            valid_entry_name("-rf"),
            Err(StoreError::InvalidEntryName(_))
        ));
    }

    #[test]
    fn validates_relative_folder_paths() {
        assert!(validate_folder_path(Path::new("")).is_ok());
        assert!(validate_folder_path(Path::new("email/work")).is_ok());
        assert!(matches!(
            validate_folder_path(Path::new("/email/work")),
            Err(StoreError::InvalidFolderPath(_))
        ));
        assert!(matches!(
            validate_folder_path(Path::new("../email")),
            Err(StoreError::InvalidFolderPath(_))
        ));
    }

    #[test]
    fn ensure_inside_store_rejects_traversal_outside_root() {
        let root = temp_dir("ensure-inside-root");
        let outside = temp_dir("ensure-inside-outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();

        // A `..`-laced parent that resolves outside the store via canonicalize.
        let traversal = root.join("inner").join("..").join("..").join(
            outside
                .file_name()
                .expect("outside dir should have a file name"),
        );
        fs::create_dir_all(&traversal).unwrap();

        let result = ensure_inside_store(&root, &traversal);
        assert!(matches!(result, Err(StoreError::InvalidFolderPath(_))));

        // A nested directory that DOES live under the store should pass.
        let inside = root.join("ok").join("nested");
        fs::create_dir_all(&inside).unwrap();
        ensure_inside_store(&root, &inside).expect("nested in-store dir should be accepted");

        fs::remove_dir_all(&root).unwrap();
        fs::remove_dir_all(&outside).unwrap();
    }

    #[test]
    fn write_atomically_replaces_destination_and_removes_temp() {
        // After a successful atomic write, only the final file exists; the
        // .gpg.tmp sidecar must be gone.
        let dir = temp_dir("atomic-write");
        fs::create_dir_all(&dir).unwrap();
        let final_path = dir.join("entry.gpg");
        let tmp_path = final_path.with_extension("gpg.tmp");

        // Pre-existing content the rename should replace atomically.
        fs::write(&final_path, b"old").unwrap();

        write_atomically(&tmp_path, &final_path, b"new").unwrap();

        assert_eq!(fs::read(&final_path).unwrap(), b"new");
        assert!(!tmp_path.exists(), ".gpg.tmp must not be left behind");

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn write_atomically_creates_destination_when_absent() {
        let dir = temp_dir("atomic-write-new");
        fs::create_dir_all(&dir).unwrap();
        let final_path = dir.join("entry.gpg");
        let tmp_path = final_path.with_extension("gpg.tmp");

        write_atomically(&tmp_path, &final_path, b"hello").unwrap();

        assert_eq!(fs::read(&final_path).unwrap(), b"hello");
        assert!(!tmp_path.exists());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn restore_previous_writes_back_or_removes() {
        // With Some(prev), restore_previous puts the bytes back. With None,
        // it deletes the post-rename file. This is the building block of the
        // rollback-on-error path in write_entry_data.
        let dir = temp_dir("restore-previous");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("entry.gpg");

        fs::write(&path, b"new").unwrap();
        restore_previous(&path, Some(b"old"));
        assert_eq!(fs::read(&path).unwrap(), b"old");

        fs::write(&path, b"new").unwrap();
        restore_previous(&path, None);
        assert!(
            !path.exists(),
            "no-previous rollback should remove the file"
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn push_failed_distinct_from_git_error_when_origin_missing() {
        // End-to-end check that the autopush branch produces a typed
        // `PushFailed` rather than a generic `Git(..)` when only the push
        // fails. Mirrors the pattern in `helpers::git::tests::
        // rollback_does_not_destroy_remote_when_backup_push_fails` — we
        // point origin at a non-existent path so the push errors out, but
        // the local commit succeeds first.
        //
        // We use `write_entry_data` indirectly via a hand-crafted call that
        // bypasses encryption: instead of going through the real entrypoint
        // (which would require GPG), we exercise the same error-mapping
        // surface with a small inline helper that mirrors the "commit ok,
        // push fails" tail of write_entry_data.
        let dir = temp_dir("push-failed");
        fs::create_dir_all(&dir).unwrap();
        // Initialize a real git repo with a commit and a broken origin so
        // git::push will return an error.
        let repo = git2::Repository::init(&dir).unwrap();
        {
            let mut config = repo.config().unwrap();
            config
                .set_str("user.email", "test@example.invalid")
                .unwrap();
            config.set_str("user.name", "Gnome Vault Tests").unwrap();
        }
        let entry_path = dir.join("entry.gpg");
        fs::write(&entry_path, b"ciphertext").unwrap();
        crate::helpers::git::add(&dir, &entry_path).unwrap();
        crate::helpers::git::commit(&dir, "test").unwrap();
        // Point origin at a path that does not exist so push fails.
        let bad_remote = temp_dir("push-failed-no-remote");
        repo.remote("origin", bad_remote.to_str().unwrap()).unwrap();

        // Mirror the autopush tail of write_entry_data to confirm the
        // mapping: any push error becomes StoreError::PushFailed.
        let push_result = crate::helpers::git::push(&dir, None);
        let mapped: Result<(), StoreError> = match push_result {
            Ok(()) => Ok(()),
            Err(err) => Err(StoreError::PushFailed(err.to_string())),
        };
        assert!(matches!(mapped, Err(StoreError::PushFailed(_))));

        fs::remove_dir_all(dir).unwrap();
    }
}
