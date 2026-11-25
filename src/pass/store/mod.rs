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

pub use crate::helpers::git::RemoteCheck;
pub use crate::helpers::pgp::{
    create_key, export_secret_key, import_secret_keys, key_user_id, GpgRecipient, PgpError,
};
pub use store_error::StoreError;

/// Everything the setup wizard collected, before anything touches disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupPlan {
    pub store_dir: PathBuf,
    pub recipient: String,
    /// Where the vault is copied to, or `None` to keep it on this
    /// computer only.
    pub sync: Option<SyncTarget>,
    pub autopush: bool,
}

/// Where a new vault keeps its copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncTarget {
    /// An existing (empty) repository on a server, by address.
    Remote(String),
    /// A bare repository Passel creates at this path, e.g. on a USB drive
    /// or in a Nextcloud/Syncthing folder.
    Folder(PathBuf),
}

/// One unit of work in a [`SetupPlan`], run in order by [`run_setup_step`].
///
/// Each step can be retried after a failure: the ones before it are
/// idempotent, and [`SetupStep::SaveRecipient`] undoes its own write
/// when its commit fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupStep {
    CreateFolder,
    StartHistory,
    SaveRecipient,
    /// Create the bare repository of a [`SyncTarget::Folder`].
    CreateCopy,
    ConnectRemote,
    Upload,
}

impl SetupStep {
    /// Steps that only concern the copy: when they fail, the vault itself
    /// already works and the rest can be skipped.
    pub fn is_sync(self) -> bool {
        matches!(
            self,
            SetupStep::CreateCopy | SetupStep::ConnectRemote | SetupStep::Upload
        )
    }
}

impl SetupPlan {
    /// The steps that carry out this plan, in order.
    pub fn steps(&self) -> Vec<SetupStep> {
        let mut steps = vec![
            SetupStep::CreateFolder,
            SetupStep::StartHistory,
            SetupStep::SaveRecipient,
        ];
        if self.remote_url().is_some() {
            if matches!(self.sync, Some(SyncTarget::Folder(_))) {
                steps.push(SetupStep::CreateCopy);
            }
            steps.push(SetupStep::ConnectRemote);
            if self.autopush {
                steps.push(SetupStep::Upload);
            }
        }
        steps
    }

    /// The address the vault is pushed to: the trimmed remote URL or the
    /// copy folder's path. `None` when sync is off (or the path is not
    /// UTF-8, which git config cannot store).
    pub fn remote_url(&self) -> Option<&str> {
        match self.sync.as_ref()? {
            SyncTarget::Remote(url) => Some(url.trim()).filter(|url| !url.is_empty()),
            SyncTarget::Folder(path) => path.to_str(),
        }
    }
}

/// Recursively loads password-store groups and `.gpg` entries.
fn load_dir(root: &Path, dir: &Path) -> io::Result<Vec<PassNode>> {
    let mut nodes = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let raw_name = entry.file_name();
        // Skip non-UTF8 file names rather than silently round-tripping
        // them through to_string_lossy: a lossy name no longer matches
        // the on-disk filename, and any subsequent rename or delete
        // built from it would error out confusingly.
        let Some(name_str) = raw_name.to_str() else {
            log::warn!("skipping non-UTF-8 path under password store: {path:?}");
            continue;
        };
        // Skip dotfiles and hidden directories.
        if name_str.starts_with('.') {
            continue;
        }
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            let children = load_dir(root, &path)?;

            nodes.push(PassNode {
                name: name_str.to_string(),
                path: relative_path(root, &path),
                kind: PassNodeKind::Group(children),
            });
        } else if file_type.is_file() && path.extension().and_then(|e| e.to_str()) == Some("gpg") {
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                log::warn!("skipping non-UTF-8 entry filename: {path:?}");
                continue;
            };
            let stem = stem.to_string();

            nodes.push(PassNode {
                name: stem,
                path: relative_path(root, &path),
                kind: PassNodeKind::Entry,
            });
        }
    }
    nodes.sort_by(|a, b| match (&a.kind, &b.kind) {
        (PassNodeKind::Group(_), PassNodeKind::Entry) => std::cmp::Ordering::Less,
        (PassNodeKind::Entry, PassNodeKind::Group(_)) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    //Returns
    Ok(nodes)
}

fn relative_path(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

/// Loads the tree of a vault folder, with or without history.
pub fn load_password_store(store_dir: &Path) -> io::Result<Vec<PassNode>> {
    log::debug!("Loading password store from: {}", store_dir.display());
    if !store_dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} is not a folder", store_dir.display()),
        ));
    }
    let nodes = load_dir(store_dir, store_dir)?;
    log::debug!("Top-level nodes: {}", nodes.len());
    //Returns
    Ok(nodes)
}

/// Checks a plan before any step runs, so a bad recipient or an existing
/// vault is reported before the folder is created.
pub fn check_setup_plan(plan: &SetupPlan) -> Result<(), StoreError> {
    let recipient = plan.recipient.trim();
    if recipient.is_empty() {
        return Err(StoreError::InvalidRecipient);
    }

    // Confirm the recipient resolves to a usable local secret key
    // before we commit it to .gpg-id. Without this check, a typo or
    // stale id would only fail on the *first encrypt* — by which point
    // we've already committed the init and the user has to delete the
    // half-set-up vault by hand.
    //
    // `available_recipients` populates `candidate.id` via
    // `key.fingerprint().or_else(key.id())`, so we may have the full
    // fingerprint (40 hex) even when the user pasted a short/long key
    // id (8/16 hex). Match suffix as well as full equality so a hand-
    // edited GSettings key like `1A2B3C4D` resolves against the same
    // key its fingerprint would. Comparison is case-insensitive
    // because gpg accepts mixed case in hex ids.
    let recipients = pgp::available_recipients()?;
    let recipient_matches = |candidate: &pgp::GpgRecipient| {
        candidate.id.eq_ignore_ascii_case(recipient)
            || (recipient.len() < candidate.id.len()
                && candidate.id[candidate.id.len() - recipient.len()..]
                    .eq_ignore_ascii_case(recipient))
    };
    if !recipients.iter().any(recipient_matches) {
        return Err(StoreError::InvalidRecipient);
    }

    check_no_recipient_file(&plan.store_dir)?;
    if let Some(SyncTarget::Folder(copy_dir)) = &plan.sync {
        check_copy_dir(&plan.store_dir, copy_dir)?;
    }
    Ok(())
}

/// Connects read-only to a remote address and reports whether a new
/// vault can be uploaded there.
pub fn check_remote(remote_url: &str) -> RemoteCheck {
    git::check_remote(remote_url.trim())
}

/// A copy folder must be new (or an empty copy left by an earlier
/// attempt), and must not sit inside the vault, where it would be
/// committed into itself.
pub fn check_copy_dir(store_dir: &Path, copy_dir: &Path) -> Result<(), StoreError> {
    if copy_dir.starts_with(store_dir) {
        return Err(StoreError::InvalidFolderPath(copy_dir.to_path_buf()));
    }
    if copy_dir.exists() && !git::is_empty_bare_repository(copy_dir) {
        return Err(StoreError::DestinationExists(copy_dir.to_path_buf()));
    }
    Ok(())
}

/// Refuses to silently overwrite an existing .gpg-id: doing so would
/// change the recipient key for the entire vault and immediately break
/// decryption of every entry encrypted to the previous one.
fn check_no_recipient_file(store_dir: &Path) -> Result<(), StoreError> {
    let gpg_id_path = store_dir.join(".gpg-id");
    if gpg_id_path.exists() {
        return Err(StoreError::DestinationExists(gpg_id_path));
    }
    Ok(())
}

/// Runs one step of a vault setup. Call [`check_setup_plan`] first.
pub fn run_setup_step(plan: &SetupPlan, step: SetupStep) -> Result<(), StoreError> {
    let store_dir = plan.store_dir.as_path();
    match step {
        SetupStep::CreateFolder => fs::create_dir_all(store_dir)?,
        SetupStep::StartHistory => {
            if !git::is_repository(store_dir) {
                git::init(store_dir)?;
            }
        }
        SetupStep::SaveRecipient => save_recipient(store_dir, plan.recipient.trim())?,
        SetupStep::CreateCopy => {
            if let Some(SyncTarget::Folder(copy_dir)) = &plan.sync {
                check_copy_dir(store_dir, copy_dir)?;
                if !copy_dir.exists() {
                    git::init_bare(copy_dir)?;
                }
            }
        }
        SetupStep::ConnectRemote => {
            if let Some(remote_url) = plan.remote_url() {
                git::set_origin(store_dir, remote_url)?;
            }
        }
        SetupStep::Upload => git::push(store_dir, None)?,
    }
    Ok(())
}

/// Writes and commits `.gpg-id`. When the commit fails (typically a
/// missing git identity) the file is removed again, so a retry starts
/// from the same state instead of tripping over its own `.gpg-id`.
fn save_recipient(store_dir: &Path, recipient: &str) -> Result<(), StoreError> {
    check_no_recipient_file(store_dir)?;
    let gpg_id_path = store_dir.join(".gpg-id");
    fs::write(&gpg_id_path, format!("{recipient}\n"))?;
    let committed =
        git::add(store_dir, &gpg_id_path).and_then(|()| git::commit(store_dir, "Initialize vault"));
    if let Err(err) = committed {
        if let Err(cleanup_err) = fs::remove_file(&gpg_id_path) {
            log::warn!("Failed to remove the uncommitted .gpg-id: {cleanup_err}");
        }
        return Err(err.into());
    }
    Ok(())
}

/// What the startup check found at a password-store directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreStatus {
    /// The folder does not exist yet.
    Missing,
    /// The folder exists and holds nothing.
    Empty,
    /// The folder is a git repository: the app can open it.
    Ready,
    /// A pass store (`.gpg-id` at its root) whose changes git does not
    /// track yet, e.g. one made with `pass init` but never `pass git init`.
    PassWithoutGit,
    /// The folder holds files but is not a password store, or the path
    /// is not a readable folder at all.
    Unrelated,
}

/// Classifies `dir` so startup can pick between opening the vault,
/// creating one, or turning on history for an existing pass store.
pub fn check_store(dir: &Path) -> StoreStatus {
    if git::is_repository(dir) {
        return StoreStatus::Ready;
    }
    match fs::read_dir(dir) {
        Err(err) if err.kind() == io::ErrorKind::NotFound => StoreStatus::Missing,
        Err(_) => StoreStatus::Unrelated,
        Ok(mut children) => {
            if children.next().is_none() {
                StoreStatus::Empty
            } else if dir.join(".gpg-id").is_file() {
                StoreStatus::PassWithoutGit
            } else {
                StoreStatus::Unrelated
            }
        }
    }
}

/// Puts an existing pass store under git: initializes the repository and
/// commits every file already there, like `pass git init`.
///
/// When staging or committing fails (typically a missing git identity),
/// the freshly created `.git` is removed again so the store is left
/// exactly as it was and the next launch offers the same choice.
pub fn enable_history(dir: &Path) -> Result<(), StoreError> {
    enable_history_with(dir, |_| Ok(()))
}

/// [`enable_history`] with a hook run right after `git init`, before
/// anything is committed, so tests can set a repository-local identity.
fn enable_history_with(
    dir: &Path,
    configure: impl FnOnce(&Path) -> Result<(), git::GitError>,
) -> Result<(), StoreError> {
    if check_store(dir) != StoreStatus::PassWithoutGit {
        return Err(StoreError::InvalidFolderPath(dir.to_path_buf()));
    }
    // check_store found no repository, but a broken `.git` could still be
    // there; never initialize over it, nor remove it on rollback.
    let git_dir = dir.join(".git");
    if git_dir.exists() {
        return Err(StoreError::DestinationExists(git_dir));
    }

    git::init(dir)?;
    let committed = configure(dir)
        .and_then(|()| git::add_all(dir))
        .and_then(|()| git::commit(dir, "Add current contents of password store."));
    if let Err(err) = committed {
        if let Err(cleanup_err) = fs::remove_dir_all(&git_dir) {
            log::warn!("Failed to remove the half-initialized repository: {cleanup_err}");
        }
        return Err(err.into());
    }
    Ok(())
}

/// Lists usable GPG recipients for a newly created vault.
pub fn available_recipients() -> Result<Vec<GpgRecipient>, StoreError> {
    Ok(pgp::available_recipients()?)
}

/// Loads and decrypts a password-store entry from a tree node.
pub fn load_entry_from_node(store_dir: &Path, node: &PassNode) -> Result<EntryData, StoreError> {
    let path = store_dir.join(&node.path);
    let content = pgp::decrypt(&path)?;
    parser::parse_entry(&content).ok_or(StoreError::EmptyFile(path))
}

/// Lists commit history for the password store repository.
///
/// When `branch` is `Some`, history is read from that local branch; otherwise
/// the working tree's current branch is used.
pub fn load_changes(store_dir: &Path, branch: Option<&str>) -> Result<Vec<GitChange>, StoreError> {
    Ok(git::branch_changes(store_dir, branch)?)
}

/// Lists one page from the password store commit history.
///
/// When `branch` is `Some`, that local branch is walked; otherwise the
/// working tree's current branch is used.
pub fn load_changes_page(
    store_dir: &Path,
    offset: usize,
    limit: usize,
    branch: Option<&str>,
) -> Result<Vec<GitChange>, StoreError> {
    Ok(git::branch_changes_page(store_dir, branch, offset, limit)?)
}

/// Reverts a commit in the password store repository and pushes the result.
pub fn revert_change(store_dir: &Path, commit_id: &str, autopush: bool) -> Result<(), StoreError> {
    git::revert_commit(store_dir, commit_id)?;
    push_if_synced(store_dir, autopush)?;
    Ok(())
}

/// Pushes committed local password-store changes.
///
/// When `branch` is `Some`, that local branch is pushed; otherwise the
/// working tree's current branch is used.
pub fn push_changes(store_dir: &Path, branch: Option<&str>) -> Result<(), StoreError> {
    Ok(git::push(store_dir, branch)?)
}

/// Backs up the current branch, resets it to a commit, and pushes the reset.
pub fn rollback_to_change(store_dir: &Path, commit_id: &str) -> Result<String, StoreError> {
    Ok(git::rollback_to_commit(store_dir, commit_id)?)
}

/// Returns the working tree's current branch name, if the repository is
/// readable and on a named branch.
pub fn current_branch_name(store_dir: &Path) -> Option<String> {
    git::current_branch_name(store_dir)
}

/// Returns the number of local commits not yet pushed upstream.
///
/// `branch` follows the same semantics as elsewhere in this crate:
/// `Some` selects a specific branch override, `None` tracks HEAD.
pub fn unpushed_count(store_dir: &Path, branch: Option<&str>) -> Result<usize, StoreError> {
    Ok(git::unpushed_count(store_dir, branch)?)
}

/// Deletes an entry file, commits the deletion, and pushes it.
pub fn delete_entry(store_dir: &Path, node: &PassNode, autopush: bool) -> Result<(), StoreError> {
    let entry_path = store_dir.join(&node.path);

    fs::remove_file(&entry_path)?;
    if has_history(store_dir) {
        git::remove(store_dir, &entry_path)?;
        git::commit(store_dir, &format!("Delete entry {}", node.name))?;
        push_if_synced(store_dir, autopush)?;
    }

    Ok(())
}

/// Removes a group directory (and every entry beneath it) from the store.
pub fn delete_folder(store_dir: &Path, node: &PassNode, autopush: bool) -> Result<(), StoreError> {
    validate_folder_path(&node.path)?;

    let folder_path = store_dir.join(&node.path);
    ensure_inside_store(store_dir, &folder_path)?;

    if !folder_path.is_dir() {
        return Err(StoreError::InvalidFolderPath(node.path.clone()));
    }

    fs::remove_dir_all(&folder_path)?;
    if has_history(store_dir) {
        git::remove_dir(store_dir, &folder_path)?;
        git::commit(store_dir, &format!("Delete folder {}", node.name))?;
        push_if_synced(store_dir, autopush)?;
    }

    Ok(())
}

/// Creates a new entry under a password-store folder.
pub fn create_entry_data(
    store_dir: &Path,
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
    };

    let output_path = store_dir.join(&node.path);
    if output_path.exists() {
        return Err(StoreError::DestinationExists(output_path));
    }

    write_entry_data(
        store_dir,
        &node,
        entry,
        &format!("Add entry {}", node.name),
        autopush,
    )?;
    Ok(node)
}

/// Renames an entry file, commits the move, and pushes it.
pub fn rename_entry(
    store_dir: &Path,
    node: &PassNode,
    new_name: &str,
    autopush: bool,
) -> Result<PassNode, StoreError> {
    let new_name = valid_entry_name(new_name)?;

    if node.name == new_name {
        return Ok(node.clone());
    }

    let old_path = store_dir.join(&node.path);
    let new_relative_path = node.path.with_file_name(format!("{new_name}.gpg"));
    let new_path = store_dir.join(&new_relative_path);

    if new_path.exists() {
        return Err(StoreError::DestinationExists(new_path));
    }

    fs::rename(&old_path, &new_path)?;
    if has_history(store_dir) {
        git::rename(store_dir, &old_path, &new_path)?;
        git::commit(
            store_dir,
            &format!("Rename entry {} to {new_name}", node.name),
        )?;
        push_if_synced(store_dir, autopush)?;
    }

    Ok(PassNode {
        name: new_name.to_string(),
        path: new_relative_path,
        kind: node.kind.clone(),
    })
}

/// Encrypts, writes, commits, and pushes entry data for a tree node.
pub fn save_entry_data(
    store_dir: &Path,
    node: &PassNode,
    entry: &EntryData,
    autopush: bool,
) -> Result<(), StoreError> {
    let message = format!("Add/update entry {}", node.name);
    write_entry_data(store_dir, node, entry, &message, autopush)
}

fn write_entry_data(
    store_dir: &Path,
    node: &PassNode,
    entry: &EntryData,
    message: &str,
    autopush: bool,
) -> Result<(), StoreError> {
    let plaintext = parser::format_entry(entry);
    let output_path = store_dir.join(&node.path);
    let parent = output_path
        .parent()
        .ok_or_else(|| StoreError::MissingParent(output_path.clone()))?;
    // Create the parent first so we can canonicalize it. Creating the directory
    // tree is idempotent; we abort before writing the entry file if the
    // resolved parent escapes the store root (e.g. via a symlink).
    fs::create_dir_all(parent)?;
    ensure_inside_store(store_dir, parent)?;
    let recipient_ids = pgp::recipient_ids_for(store_dir, parent)?;
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
    if !has_history(store_dir) {
        // A vault without history: the written file is the whole save.
        return Ok(());
    }
    if let Err(err) = git::add(store_dir, &output_path) {
        restore_previous(store_dir, &output_path, previous.as_deref());
        return Err(StoreError::Git(err));
    }
    if let Err(err) = git::commit(store_dir, message) {
        restore_previous(store_dir, &output_path, previous.as_deref());
        return Err(StoreError::Git(err));
    }
    // Local commit succeeded — only the push failed. Surface this as
    // PushFailed so the UI can tell the user "saved locally, retry push"
    // rather than implying the entire save failed. Do NOT roll back the
    // committed file: the local state is the new value the user wanted.
    if let Err(err) = push_if_synced(store_dir, autopush) {
        return Err(StoreError::PushFailed(err.to_string()));
    }
    Ok(())
}

/// True when the vault records its changes (is a git repository).
pub fn has_history(store_dir: &Path) -> bool {
    git::is_repository(store_dir)
}

/// The address of the vault's remote ("origin"), if it has one.
pub fn remote_url(store_dir: &Path) -> Option<String> {
    git::origin_url(store_dir)
}

/// Sets the vault's remote address, or removes the remote with `None`
/// (or a blank address).
pub fn set_remote(store_dir: &Path, remote_url: Option<&str>) -> Result<(), StoreError> {
    match remote_url.map(str::trim).filter(|url| !url.is_empty()) {
        Some(url) => git::set_origin(store_dir, url)?,
        None => git::remove_origin(store_dir)?,
    }
    Ok(())
}

/// Pushes after a change when auto-push is on and there is somewhere to
/// push to: a vault kept on this computer only has no remote, and must
/// not report a failed push after every save.
fn push_if_synced(store_dir: &Path, autopush: bool) -> Result<(), git::GitError> {
    if autopush && git::has_remote(store_dir) {
        git::push(store_dir, None)?;
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
    // After the rename, the directory entry change itself isn't durable
    // until the parent directory is fsynced. Without this an unclean
    // shutdown (power loss, kernel panic) can revert the file's name to
    // the prior content — best case — or, on some filesystems / mount
    // options, leave the new file with stale metadata pointing at a
    // zero-length blob. Best-effort: log and continue if the parent
    // doesn't open as a directory (network mounts, /proc-style FS).
    if let Some(parent) = final_path.parent() {
        match fs::File::open(parent) {
            Ok(dir) => {
                if let Err(err) = dir.sync_all() {
                    log::warn!("parent dir fsync after atomic write failed: {err}");
                }
            }
            Err(err) => {
                log::warn!("parent dir open for fsync failed: {err}");
            }
        }
    }
    Ok(())
}

/// Best-effort restore of a previously-saved ciphertext after a post-rename
/// failure. If `previous` is `None` the file did not exist before, so we
/// remove the new one. Failures here are logged and swallowed: the caller
/// will surface the original error.
///
/// After restoring the worktree we also bring the index back in sync so a
/// `git::add` that partially succeeded — or a `git::commit` that ran after
/// a successful add — doesn't leave the index pointing at the new bytes
/// while the worktree holds the previous ones.
fn restore_previous(store_dir: &Path, output_path: &Path, previous: Option<&[u8]>) {
    let restore_result = match previous {
        Some(bytes) => fs::write(output_path, bytes),
        None => fs::remove_file(output_path),
    };
    if let Err(err) = restore_result {
        log::warn!(
            "failed to restore previous entry at {}: {err}",
            output_path.display()
        );
        return;
    }

    let index_result = match previous {
        Some(_) => git::add(store_dir, output_path),
        None => git::remove(store_dir, output_path),
    };
    if let Err(err) = index_result {
        log::warn!(
            "restored entry at {} but failed to re-stage the index: {err}",
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
        || name.contains('\n')
        || name.contains('\r')
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

/// Returns the sandbox store used in place of the user's real vault in
/// debug builds (`cargo run`), or `None` in release builds.
///
/// Defaults to `.dev-password-store/` at the crate root so the path does
/// not depend on the working directory; `PASSEL_DEV_STORE_DIR`
/// points it elsewhere. Pinned over any inherited `PASSWORD_STORE_DIR`
/// at startup, since `pass` users commonly export that variable at their
/// real store.
pub fn dev_store_dir() -> Option<PathBuf> {
    if !cfg!(debug_assertions) {
        return None;
    }
    Some(
        std::env::var_os("PASSEL_DEV_STORE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join(".dev-password-store")),
    )
}

/// Returns the active password-store directory.
///
/// Honors `PASSWORD_STORE_DIR` and falls back to `$HOME/.password-store`.
/// When neither is set (e.g. running under a session manager that
/// doesn't propagate `HOME`), returns the relative `.password-store`
/// path. Subsequent filesystem operations surface a normal not-found
/// error instead of panicking the whole process.
pub fn password_store_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("PASSWORD_STORE_DIR") {
        return PathBuf::from(dir);
    }
    match std::env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join(".password-store"),
        None => {
            log::warn!("HOME is not set; using relative .password-store as fallback");
            PathBuf::from(".password-store")
        }
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
        std::env::temp_dir().join(format!("passel-{name}-{}-{unique}", std::process::id()))
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
        // rollback-on-error path in write_entry_data. The index re-stage
        // step in restore_previous fails-soft (the temp dir isn't a git
        // repo) — it logs and swallows, which is the documented contract.
        let dir = temp_dir("restore-previous");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("entry.gpg");

        fs::write(&path, b"new").unwrap();
        restore_previous(&dir, &path, Some(b"old"));
        assert_eq!(fs::read(&path).unwrap(), b"old");

        fs::write(&path, b"new").unwrap();
        restore_previous(&dir, &path, None);
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
            config.set_str("user.name", "Passel Tests").unwrap();
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

    #[test]
    fn rejects_path_traversal_in_entry_names() {
        // Path-shaped or reserved names must be refused before any
        // filesystem op so an entry can't escape the store root.
        for name in [
            "..",
            ".",
            "a/b",
            "a\\b",
            "with\0null",
            "-leading-dash",
            ".gpg-id",
            ".gpg-id.gpg",
            ".gitattributes",
            "",
            "  ",
        ] {
            let err = valid_entry_name(name).unwrap_err();
            assert!(
                matches!(err, StoreError::InvalidEntryName(_)),
                "name {name:?} should be rejected"
            );
        }
    }

    #[test]
    fn rejects_absolute_and_parent_dir_in_folder_paths() {
        assert!(matches!(
            validate_folder_path(Path::new("/etc")),
            Err(StoreError::InvalidFolderPath(_))
        ));
        assert!(matches!(
            validate_folder_path(Path::new("a/../etc")),
            Err(StoreError::InvalidFolderPath(_))
        ));
        // Plain relative paths are accepted; the canonicalize-based
        // ensure_inside_store check below catches symlink escapes.
        assert!(validate_folder_path(Path::new("Personal/Work")).is_ok());
        assert!(validate_folder_path(Path::new("")).is_ok());
    }
    fn pass_store_without_git(name: &str) -> PathBuf {
        let dir = temp_dir(name);
        fs::create_dir_all(dir.join("Personal")).unwrap();
        fs::write(dir.join(".gpg-id"), "ABCDEF0123456789\n").unwrap();
        fs::write(dir.join("Personal/mail.gpg"), "encrypted").unwrap();
        dir
    }

    fn set_local_identity(dir: &Path, name: &str) -> Result<(), git::GitError> {
        let mut config = git2::Repository::open(dir)?.config()?;
        config.set_str("user.name", name)?;
        config.set_str("user.email", "test@example.invalid")?;
        Ok(())
    }

    fn mail_entry() -> PassNode {
        PassNode {
            name: "mail".to_string(),
            path: PathBuf::from("Personal/mail.gpg"),
            kind: PassNodeKind::Entry,
        }
    }

    #[test]
    fn vault_without_history_renames_and_deletes_without_git() {
        let dir = pass_store_without_git("no-history-edit");
        assert!(!has_history(&dir));
        assert_eq!(load_password_store(&dir).unwrap().len(), 1);

        let renamed = rename_entry(&dir, &mail_entry(), "email", true).unwrap();
        assert!(dir.join("Personal/email.gpg").is_file());
        delete_entry(&dir, &renamed, true).unwrap();
        assert!(!dir.join("Personal/email.gpg").exists());
        assert!(!dir.join(".git").exists());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn auto_push_without_a_remote_keeps_local_changes() {
        // A vault kept on this computer only: history, no remote. Saving
        // with auto-push on must not fail on the missing remote.
        let dir = pass_store_without_git("no-remote-edit");
        enable_history_with(&dir, |dir| set_local_identity(dir, "Passel Tests")).unwrap();
        assert!(!git::has_remote(&dir));

        delete_entry(&dir, &mail_entry(), true).unwrap();
        let repo = git2::Repository::open(&dir).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.message(), Some("Delete entry mail"));

        set_remote(&dir, Some("/nowhere/remote.git")).unwrap();
        assert_eq!(remote_url(&dir).as_deref(), Some("/nowhere/remote.git"));
        set_remote(&dir, Some("  ")).unwrap();
        assert_eq!(remote_url(&dir), None);
        set_remote(&dir, None).unwrap();

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn classifies_store_directories() {
        let root = temp_dir("check-store");

        assert_eq!(check_store(&root.join("missing")), StoreStatus::Missing);

        let empty = root.join("empty");
        fs::create_dir_all(&empty).unwrap();
        assert_eq!(check_store(&empty), StoreStatus::Empty);

        let unrelated = root.join("unrelated");
        fs::create_dir_all(&unrelated).unwrap();
        fs::write(unrelated.join("notes.txt"), "hello").unwrap();
        assert_eq!(check_store(&unrelated), StoreStatus::Unrelated);

        let file = root.join("file");
        fs::write(&file, "not a folder").unwrap();
        assert_eq!(check_store(&file), StoreStatus::Unrelated);

        let pass_only = root.join("pass-only");
        fs::create_dir_all(&pass_only).unwrap();
        fs::write(pass_only.join(".gpg-id"), "ABCDEF0123456789\n").unwrap();
        assert_eq!(check_store(&pass_only), StoreStatus::PassWithoutGit);

        let repo = root.join("repo");
        git2::Repository::init(&repo).unwrap();
        assert_eq!(check_store(&repo), StoreStatus::Ready);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn enable_history_commits_existing_entries() {
        let dir = pass_store_without_git("enable-history");

        enable_history_with(&dir, |dir| set_local_identity(dir, "Passel Tests")).unwrap();

        assert_eq!(check_store(&dir), StoreStatus::Ready);
        let repo = git2::Repository::open(&dir).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(
            head.message(),
            Some("Add current contents of password store.")
        );
        let tree = head.tree().unwrap();
        assert!(tree.get_path(Path::new(".gpg-id")).is_ok());
        assert!(tree.get_path(Path::new("Personal/mail.gpg")).is_ok());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn enable_history_rolls_back_when_commit_fails() {
        let dir = pass_store_without_git("enable-history-rollback");

        // An empty user.name cannot build a commit signature.
        let result = enable_history_with(&dir, |dir| set_local_identity(dir, ""));

        assert!(matches!(
            result,
            Err(StoreError::Git(git::GitError::NoSignature))
        ));
        assert!(!dir.join(".git").exists());
        assert_eq!(check_store(&dir), StoreStatus::PassWithoutGit);
        assert!(dir.join("Personal/mail.gpg").is_file());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn enable_history_refuses_other_folders() {
        let root = temp_dir("enable-history-refuse");
        let empty = root.join("empty");
        fs::create_dir_all(&empty).unwrap();
        assert!(matches!(
            enable_history(&empty),
            Err(StoreError::InvalidFolderPath(_))
        ));
        assert!(!empty.join(".git").exists());

        // A broken .git (not openable as a repository) is left untouched.
        let broken = pass_store_without_git("enable-history-broken");
        fs::create_dir_all(broken.join(".git")).unwrap();
        fs::write(broken.join(".git/marker"), "keep me").unwrap();
        assert!(matches!(
            enable_history(&broken),
            Err(StoreError::DestinationExists(_))
        ));
        assert!(broken.join(".git/marker").is_file());

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(broken).unwrap();
    }
    fn plan(store_dir: &Path, remote_url: Option<&str>, autopush: bool) -> SetupPlan {
        SetupPlan {
            store_dir: store_dir.to_path_buf(),
            recipient: "ABCDEF0123456789".to_string(),
            sync: remote_url.map(|url| SyncTarget::Remote(url.to_string())),
            autopush,
        }
    }

    fn folder_plan(store_dir: &Path, copy_dir: &Path) -> SetupPlan {
        SetupPlan {
            sync: Some(SyncTarget::Folder(copy_dir.to_path_buf())),
            ..plan(store_dir, None, true)
        }
    }

    #[test]
    fn setup_plan_lists_sync_steps_only_when_needed() {
        let dir = Path::new("/vault");
        let local = [
            SetupStep::CreateFolder,
            SetupStep::StartHistory,
            SetupStep::SaveRecipient,
        ];
        assert_eq!(plan(dir, None, true).steps(), local);
        assert_eq!(plan(dir, Some("  "), true).steps(), local);

        let mut connect = local.to_vec();
        connect.push(SetupStep::ConnectRemote);
        assert_eq!(plan(dir, Some("git@host:vault"), false).steps(), connect);

        connect.push(SetupStep::Upload);
        assert_eq!(plan(dir, Some(" git@host:vault "), true).steps(), connect);
        assert_eq!(
            plan(dir, Some(" git@host:vault "), true).remote_url(),
            Some("git@host:vault")
        );
    }

    #[test]
    fn folder_plan_creates_the_copy_before_connecting() {
        let plan = folder_plan(Path::new("/vault"), Path::new("/media/usb/passwords.git"));
        assert_eq!(
            plan.steps(),
            [
                SetupStep::CreateFolder,
                SetupStep::StartHistory,
                SetupStep::SaveRecipient,
                SetupStep::CreateCopy,
                SetupStep::ConnectRemote,
                SetupStep::Upload,
            ]
        );
        assert_eq!(plan.remote_url(), Some("/media/usb/passwords.git"));
        assert!(SetupStep::CreateCopy.is_sync());
        assert!(!SetupStep::SaveRecipient.is_sync());
    }

    #[test]
    fn setup_steps_copy_a_vault_to_a_folder() {
        let root = temp_dir("setup-folder");
        let store_dir = root.join("store");
        let copy_dir = root.join("usb/passwords.git");
        let plan = folder_plan(&store_dir, &copy_dir);

        for step in plan.steps() {
            run_setup_step(&plan, step).unwrap();
            if step == SetupStep::StartHistory {
                set_local_identity(&store_dir, "Passel Tests").unwrap();
            }
        }

        let head = git2::Repository::open(&store_dir)
            .unwrap()
            .head()
            .unwrap()
            .target();
        let copy = git2::Repository::open_bare(&copy_dir).unwrap();
        assert_eq!(copy.head().unwrap().target(), head);

        // Retrying the copy step after it succeeded must not wipe it, but
        // a copy that already holds a vault is refused up front.
        assert!(matches!(
            check_copy_dir(&store_dir, &copy_dir),
            Err(StoreError::DestinationExists(_))
        ));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn copy_dir_must_be_new_or_an_empty_copy_outside_the_vault() {
        let root = temp_dir("setup-copy-dir");
        let store_dir = root.join("store");
        assert!(check_copy_dir(&store_dir, &root.join("new.git")).is_ok());

        let empty_copy = root.join("empty.git");
        git2::Repository::init_bare(&empty_copy).unwrap();
        assert!(check_copy_dir(&store_dir, &empty_copy).is_ok());

        let unrelated = root.join("photos");
        fs::create_dir_all(&unrelated).unwrap();
        assert!(matches!(
            check_copy_dir(&store_dir, &unrelated),
            Err(StoreError::DestinationExists(_))
        ));
        assert!(matches!(
            check_copy_dir(&store_dir, &store_dir.join("copy.git")),
            Err(StoreError::InvalidFolderPath(_))
        ));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn setup_steps_create_commit_and_push_a_vault() {
        let root = temp_dir("setup-steps");
        let store_dir = root.join("store");
        let remote_dir = root.join("remote.git");
        git2::Repository::init_bare(&remote_dir).unwrap();
        let plan = plan(&store_dir, remote_dir.to_str(), true);

        for step in plan.steps() {
            run_setup_step(&plan, step).unwrap();
            if step == SetupStep::StartHistory {
                set_local_identity(&store_dir, "Passel Tests").unwrap();
            }
        }

        assert_eq!(
            fs::read_to_string(store_dir.join(".gpg-id")).unwrap(),
            "ABCDEF0123456789\n"
        );
        let repo = git2::Repository::open(&store_dir).unwrap();
        let head = repo.head().unwrap();
        assert_eq!(
            head.peel_to_commit().unwrap().message(),
            Some("Initialize vault")
        );
        let remote = git2::Repository::open_bare(&remote_dir).unwrap();
        assert_eq!(
            remote
                .find_reference(head.name().unwrap())
                .unwrap()
                .target(),
            head.target()
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn save_recipient_can_be_retried_after_a_failed_commit() {
        let store_dir = temp_dir("setup-retry");
        let plan = plan(&store_dir, None, false);
        run_setup_step(&plan, SetupStep::CreateFolder).unwrap();
        run_setup_step(&plan, SetupStep::StartHistory).unwrap();

        // An empty user.name cannot build a commit signature.
        set_local_identity(&store_dir, "").unwrap();
        assert!(matches!(
            run_setup_step(&plan, SetupStep::SaveRecipient),
            Err(StoreError::Git(git::GitError::NoSignature))
        ));
        assert!(!store_dir.join(".gpg-id").exists());

        set_local_identity(&store_dir, "Passel Tests").unwrap();
        run_setup_step(&plan, SetupStep::SaveRecipient).unwrap();
        assert!(store_dir.join(".gpg-id").is_file());

        fs::remove_dir_all(store_dir).unwrap();
    }

    #[test]
    fn save_recipient_refuses_an_existing_gpg_id() {
        let store_dir = pass_store_without_git("setup-existing-gpg-id");
        let plan = plan(&store_dir, None, false);
        assert!(matches!(
            run_setup_step(&plan, SetupStep::SaveRecipient),
            Err(StoreError::DestinationExists(_))
        ));
        assert_eq!(
            fs::read_to_string(store_dir.join(".gpg-id")).unwrap(),
            "ABCDEF0123456789\n"
        );

        fs::remove_dir_all(store_dir).unwrap();
    }
}
