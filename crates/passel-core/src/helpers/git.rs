use std::{
    cell::{Cell, RefCell},
    path::{Path, PathBuf},
    rc::Rc,
    time::{SystemTime, UNIX_EPOCH},
};

use git2::{
    build::CheckoutBuilder, Cred, CredentialType, Oid, PushOptions, RemoteCallbacks, Repository,
    ResetType, Sort,
};

use super::bundle;
use super::git_credentials::credential_provider;
pub use super::git_credentials::{
    install_credential_provider, remote_host, Credential, CredentialProvider, CredentialRequest,
    StoredCredentials, SystemCredentials,
};
use crate::i18n::{gettext, gettext_f, ngettext_f};
/// libgit2 error details, for front ends that explain failures.
pub use git2::{Error as Git2Error, ErrorClass, ErrorCode};

/// Maximum number of times the credential callback is allowed to fire for a
/// single push before we give up. libgit2 invokes the callback in a loop until
/// authentication succeeds or the callback returns an error, so without a cap
/// a permanently-rejected SSH key from `ssh-agent` would hang the app forever.
const MAX_CREDENTIAL_ATTEMPTS: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitChange {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    pub author: String,
    pub author_time_seconds: i64,
    pub author_time_offset_minutes: i32,
    pub parent_count: usize,
    pub is_pushed: bool,
}

#[derive(Debug)]
pub enum GitError {
    Git(git2::Error),
    InvalidPath {
        path: PathBuf,
        base: PathBuf,
    },
    MissingWorkdir(PathBuf),
    NoHeadName,
    NoHeadTarget,
    NoChanges,
    Conflicts(String),
    /// The remote rejected a push for a ref that we required to succeed before
    /// taking a destructive action (e.g. the rollback safety backup ref).
    BackupRejected(String),
    /// `git config user.name` / `user.email` are not set, so we cannot
    /// build a commit signature.
    NoSignature,
    /// Syncing found the same entries changed here and on the remote
    /// (paths relative to the vault). Nothing local was changed; the
    /// local commits were sent to the remote as branch `kept_as` (when
    /// that push worked), for a computer to merge.
    MergeConflicts {
        entries: Vec<String>,
        kept_as: Option<String>,
    },
    /// A vault's copy (a bundle file) could not be read or written.
    Io(std::io::Error),
    /// A file given as a vault's copy is not one.
    NotAVault,
}

// Display intentionally drops the inner `git2::Error` message (and the
// libgit2 push-status reason in `BackupRejected`) so we don't echo
// branch / ref / path strings into a UI toast. Crafted variants —
// `InvalidPath`, `MissingWorkdir`, `Conflicts(commit_id)` — keep their
// pass-store-relative paths and short commit IDs because those are
// already known to the user. The auto-derived `Debug` retains the full
// upstream message for `{err:?}` log output.
impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            GitError::Git(_) => gettext("Git operation failed"),
            GitError::InvalidPath { path, base } => gettext_f(
                "Path {path} is not inside git workdir {base}",
                &[
                    ("path", &path.display().to_string()),
                    ("base", &base.display().to_string()),
                ],
            ),
            GitError::MissingWorkdir(path) => gettext_f(
                "Repository has no workdir: {path}",
                &[("path", &path.display().to_string())],
            ),
            GitError::NoHeadName => gettext("Current git HEAD has no branch name"),
            GitError::NoHeadTarget => gettext("Current git HEAD has no target commit"),
            GitError::NoChanges => gettext("No changes to commit"),
            GitError::Conflicts(commit_id) => gettext_f(
                "Reverting commit {commit} produced conflicts",
                &[("commit", commit_id)],
            ),
            GitError::BackupRejected(_) => gettext("Remote rejected rollback backup ref"),
            GitError::MergeConflicts { entries, .. } => ngettext_f(
                "An entry was changed both here and on the remote; nothing was changed here",
                "{n} entries were changed both here and on the remote; nothing was changed here",
                entries.len() as u64,
                &[("n", &entries.len().to_string())],
            ),
            GitError::Io(_) => gettext("Could not read or write the vault’s copy"),
            GitError::NotAVault => gettext("This file is not a Passel vault"),
            GitError::NoSignature => gettext(
                "git user.name and user.email are not set; configure them with `git config --global user.name '...'` and `git config --global user.email '...'`",
            ),
        };
        f.write_str(&message)
    }
}

impl std::error::Error for GitError {}

impl From<git2::Error> for GitError {
    fn from(value: git2::Error) -> Self {
        GitError::Git(value)
    }
}

/// Opens the password store as a Git repository.
fn open_repository(project_dir: &Path) -> Result<Repository, GitError> {
    Repository::open(project_dir).map_err(GitError::from)
}

/// Returns true when the directory can be opened as a Git repository.
pub fn is_repository(project_dir: &Path) -> bool {
    open_repository(project_dir).is_ok()
}

/// Initializes a Git repository in the directory.
///
/// The first branch is `main`, like new repositories on GitHub, GitLab
/// and Codeberg: an empty remote's HEAD then points at the branch the
/// vault uploads, so other devices clone a checked-out vault.
pub fn init(project_dir: &Path) -> Result<(), GitError> {
    let mut options = git2::RepositoryInitOptions::new();
    options.initial_head("main");
    Repository::init_opts(project_dir, &options)?;
    Ok(())
}

/// Creates an empty bare repository, used as a vault copy on a folder or
/// drive.
///
/// Its HEAD names `branch`, the branch the vault will push there, so a
/// clone of the copy checks it out.
pub fn init_bare(dir: &Path, branch: &str) -> Result<(), GitError> {
    let mut options = git2::RepositoryInitOptions::new();
    options.bare(true).initial_head(branch);
    Repository::init_opts(dir, &options)?;
    Ok(())
}

/// Returns true for a bare repository with no commits yet: a copy that
/// was created but never received the vault.
pub fn is_empty_bare_repository(dir: &Path) -> bool {
    // No reference at all. libgit2's `is_empty` would also require HEAD to
    // name the configured default branch, which a copy made for a vault
    // on another branch does not.
    Repository::open_bare(dir)
        .map(|repo| {
            repo.is_bare()
                && repo
                    .references()
                    .map(|mut refs| refs.next().is_none())
                    .unwrap_or(false)
        })
        .unwrap_or(false)
}

/// What [`check_remote`] found at a remote address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteCheck {
    /// Reachable, with no branches yet: ready to receive a new vault.
    Empty,
    /// Reachable, but it already holds commits.
    NotEmpty,
    /// The server answered but refused our credentials.
    AccessDenied,
    /// The SSH server's host key is not in `known_hosts` yet.
    UnknownHost,
    /// Nothing usable at this address: typo, missing repository, no
    /// network.
    Unreachable,
}

/// Connects to a remote address read-only, with the same credentials as
/// a push, and reports whether a new vault can be uploaded there.
pub fn check_remote(remote_url: &str) -> RemoteCheck {
    match remote_has_history(remote_url, credential_provider()) {
        Ok(false) => RemoteCheck::Empty,
        Ok(true) => RemoteCheck::NotEmpty,
        Err(err) => {
            // Debug only: libgit2 messages can contain the address.
            log::debug!("Remote check failed: {err:?}");
            classify_remote_error(&err)
        }
    }
}

/// Returns whether the remote advertises a default branch, i.e. already
/// holds a vault.
///
/// `RemoteConnection::list` would be the natural call, but git2 0.20
/// builds a slice from libgit2's null pointer when the advertisement is
/// empty (exactly the empty-repository case), which is undefined
/// behaviour. `default_branch` stays in C: it fails with `NotFound` when
/// HEAD is unborn or not advertised.
fn remote_has_history(
    remote_url: &str,
    provider: &dyn CredentialProvider,
) -> Result<bool, git2::Error> {
    let config = git2::Config::open_default()?;
    let mut callbacks = RemoteCallbacks::new();
    install_remote_callbacks(&mut callbacks, &config, provider);
    let mut remote = git2::Remote::create_detached(remote_url)?;
    let connection = remote.connect_auth(git2::Direction::Fetch, Some(callbacks), None)?;
    match connection.default_branch() {
        Ok(_) => Ok(true),
        Err(err) if err.code() == ErrorCode::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

fn classify_remote_error(err: &git2::Error) -> RemoteCheck {
    let message = err.message().to_ascii_lowercase();
    if err.code() == ErrorCode::Certificate || message.contains("hostkey") {
        RemoteCheck::UnknownHost
    } else if err.code() == ErrorCode::Auth
        || message.contains("authenticat")
        || message.contains("permission denied")
    {
        RemoteCheck::AccessDenied
    } else {
        RemoteCheck::Unreachable
    }
}

/// True when the repository has at least one remote configured.
pub fn has_remote(project_dir: &Path) -> bool {
    open_repository(project_dir)
        .and_then(|repo| Ok(!repo.remotes()?.is_empty()))
        .unwrap_or(false)
}

/// The origin remote's URL, if any.
pub fn origin_url(project_dir: &Path) -> Option<String> {
    let repo = open_repository(project_dir).ok()?;
    let remote = repo.find_remote("origin").ok()?;
    remote.url().map(str::to_string)
}

/// Removes the origin remote; a missing one is not an error.
pub fn remove_origin(project_dir: &Path) -> Result<(), GitError> {
    let repo = open_repository(project_dir)?;
    match repo.remote_delete("origin") {
        Err(err) if err.code() == ErrorCode::NotFound => Ok(()),
        other => Ok(other?),
    }
}

/// Adds or updates the origin remote URL.
pub fn set_origin(project_dir: &Path, remote_url: &str) -> Result<(), GitError> {
    let repo = open_repository(project_dir)?;
    match repo.find_remote("origin") {
        Ok(_) => repo.remote_set_url("origin", remote_url)?,
        Err(_) => {
            repo.remote("origin", remote_url)?;
        }
    }
    Ok(())
}

/// Returns the repository worktree path.
fn workdir_path(repo: &Repository) -> Result<&Path, GitError> {
    repo.workdir()
        .ok_or_else(|| GitError::MissingWorkdir(repo.path().to_path_buf()))
}

/// Converts absolute paths to paths relative to the repository worktree.
fn relative_workdir_path(repo: &Repository, file_path: &Path) -> Result<PathBuf, GitError> {
    let workdir = workdir_path(repo)?;

    if !file_path.is_absolute() {
        return Ok(file_path.to_path_buf());
    }
    if let Ok(relative) = file_path.strip_prefix(workdir) {
        return Ok(relative.to_path_buf());
    }
    // The same folder can be spelled differently: libgit2 resolves
    // symlinks in the workdir (Android's /data/user/0 is /data/data), so
    // compare resolved paths. The file itself may be gone (a deletion is
    // being staged): resolve its folder instead.
    let resolved_workdir = workdir.canonicalize().ok();
    let resolved_file = match (file_path.parent(), file_path.file_name()) {
        (Some(parent), Some(name)) => parent.canonicalize().ok().map(|p| p.join(name)),
        _ => None,
    };
    if let (Some(workdir), Some(file)) = (resolved_workdir, resolved_file) {
        if let Ok(relative) = file.strip_prefix(&workdir) {
            return Ok(relative.to_path_buf());
        }
    }
    Err(GitError::InvalidPath {
        path: file_path.to_path_buf(),
        base: workdir.to_path_buf(),
    })
}

/// Stages a single path in the repository index.
pub fn add(project_dir: &Path, file_path: &Path) -> Result<(), GitError> {
    let repo = open_repository(project_dir)?;
    let file_path = relative_workdir_path(&repo, file_path)?;
    let mut index = repo.index()?;

    index.add_path(&file_path)?;
    index.write()?;

    Ok(())
}

/// Stages every file in the worktree, honoring `.gitignore`.
pub fn add_all(project_dir: &Path) -> Result<(), GitError> {
    let repo = open_repository(project_dir)?;
    let mut index = repo.index()?;

    index.add_all(["*"], git2::IndexAddOption::DEFAULT, None)?;
    index.write()?;

    Ok(())
}

/// Stages a file deletion in the repository index.
pub fn remove(project_dir: &Path, file_path: &Path) -> Result<(), GitError> {
    let repo = open_repository(project_dir)?;
    let file_path = relative_workdir_path(&repo, file_path)?;
    let mut index = repo.index()?;

    index.remove_path(&file_path)?;
    index.write()?;

    Ok(())
}

/// Stages the deletion of every index entry under `dir_path`.
///
/// `index.remove_dir` walks the in-memory index for entries whose paths
/// start with `dir_path` and drops them. The on-disk files still need to
/// be removed by the caller; the next commit reflects the index state.
pub fn remove_dir(project_dir: &Path, dir_path: &Path) -> Result<(), GitError> {
    let repo = open_repository(project_dir)?;
    let dir_path = relative_workdir_path(&repo, dir_path)?;
    let mut index = repo.index()?;

    index.remove_dir(&dir_path, 0)?;
    index.write()?;

    Ok(())
}

/// Stages a file rename in the repository index.
pub fn rename(project_dir: &Path, old_path: &Path, new_path: &Path) -> Result<(), GitError> {
    let repo = open_repository(project_dir)?;
    let old_path = relative_workdir_path(&repo, old_path)?;
    let new_path = relative_workdir_path(&repo, new_path)?;
    let mut index = repo.index()?;

    index.remove_path(&old_path)?;
    index.add_path(&new_path)?;
    index.write()?;

    Ok(())
}

/// Creates a commit from the current index.
///
/// The author and committer are read from Git configuration, matching the
/// behavior users expect from `git commit`.
pub fn commit(project_dir: &Path, message: &str) -> Result<(), GitError> {
    let repo = open_repository(project_dir)?;
    let signature = repo.signature().map_err(|err| {
        // libgit2 returns ErrorClass::Config / "config value 'user.name'
        // was not found" here, which is opaque to a normal user. Translate
        // it to a typed variant so the UI can guide them to git config.
        if matches!(
            err.class(),
            git2::ErrorClass::Config | git2::ErrorClass::Invalid
        ) {
            GitError::NoSignature
        } else {
            GitError::Git(err)
        }
    })?;
    let mut index = repo.index()?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let parent = head_commit(&repo)?;

    if parent
        .as_ref()
        .is_some_and(|parent| parent.tree_id() == tree_id)
    {
        return Err(GitError::NoChanges);
    }

    let parents = parent.iter().collect::<Vec<_>>();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &parents,
    )?;

    Ok(())
}

/// Lists commits reachable from `branch`'s head, or the working tree's
/// current branch when `branch` is `None`.
pub fn branch_changes(
    project_dir: &Path,
    branch: Option<&str>,
) -> Result<Vec<GitChange>, GitError> {
    branch_changes_page(project_dir, branch, 0, usize::MAX)
}

/// Lists a page of commits from `branch`'s head, or the working tree's
/// current branch when `branch` is `None`.
pub fn branch_changes_page(
    project_dir: &Path,
    branch: Option<&str>,
    offset: usize,
    limit: usize,
) -> Result<Vec<GitChange>, GitError> {
    let repo = open_repository(project_dir)?;
    let pushed_head = pushed_head(&repo).ok();
    let mut revwalk = repo.revwalk()?;

    let push_result = match branch {
        Some(branch) => revwalk.push_ref(&format!("refs/heads/{branch}")),
        None => revwalk.push_head(),
    };
    if let Err(err) = push_result {
        if err.code() == ErrorCode::UnbornBranch || err.code() == ErrorCode::NotFound {
            return Ok(Vec::new());
        }
        return Err(GitError::Git(err));
    }

    if limit == 0 {
        return Ok(Vec::new());
    }

    revwalk.set_sorting(Sort::TIME)?;
    revwalk
        .skip(offset)
        .take(limit)
        .map(|oid| {
            let oid = oid?;
            let commit = repo.find_commit(oid)?;
            let id = oid.to_string();
            let summary = commit
                .summary()
                .map(str::to_string)
                .unwrap_or_else(|| gettext("(no message)"));
            let author = commit
                .author()
                .name()
                .map(str::to_string)
                .unwrap_or_else(|| gettext("Unknown author"));
            let author_time = commit.author().when();
            let is_pushed = pushed_head
                .map(|pushed_head| {
                    pushed_head == oid
                        || repo.graph_descendant_of(pushed_head, oid).unwrap_or(false)
                })
                .unwrap_or(false);
            Ok(GitChange {
                short_id: id.chars().take(8).collect(),
                id,
                summary,
                author,
                author_time_seconds: author_time.seconds(),
                author_time_offset_minutes: author_time.offset_minutes(),
                parent_count: commit.parent_count(),
                is_pushed,
            })
        })
        .collect::<Result<Vec<_>, git2::Error>>()
        .map_err(GitError::from)
}

/// Creates a revert commit for an existing commit.
pub fn revert_commit(project_dir: &Path, commit_id: &str) -> Result<(), GitError> {
    let repo = open_repository(project_dir)?;
    let oid = Oid::from_str(commit_id)?;
    let commit = repo.find_commit(oid)?;

    repo.revert(&commit, None)?;

    let mut index = repo.index()?;
    if index.has_conflicts() {
        // Clean up REVERT_HEAD before bailing. Without this the next
        // git operation in the same repository sees "you have a revert
        // in progress" and refuses to commit until the user runs
        // `git revert --abort` by hand. The other early-returns below
        // already cleanup_state; this one was the missing case.
        repo.cleanup_state()?;
        return Err(GitError::Conflicts(commit_id.to_string()));
    }

    index.write()?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let parent = head_commit(&repo)?;

    if parent
        .as_ref()
        .is_some_and(|parent| parent.tree_id() == tree_id)
    {
        repo.cleanup_state()?;
        return Err(GitError::NoChanges);
    }

    let signature = repo.signature()?;
    let summary = commit.summary().unwrap_or("commit");
    let message = format!("Revert \"{summary}\"\n\nThis reverts commit {commit_id}.");
    let parents = parent.iter().collect::<Vec<_>>();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        &message,
        &tree,
        &parents,
    )?;
    repo.checkout_head(None)?;
    repo.cleanup_state()?;

    Ok(())
}

/// Creates a backup branch, resets the current branch to a commit, and pushes both.
pub fn rollback_to_commit(project_dir: &Path, commit_id: &str) -> Result<String, GitError> {
    let repo = open_repository(project_dir)?;
    let branch = current_branch(&repo)?;
    let head_oid = repo.head()?.target().ok_or(GitError::NoHeadTarget)?;
    let target_oid = Oid::from_str(commit_id)?;
    let target = repo.find_object(target_oid, None)?;
    let backup_branch = backup_branch_name(&branch, &head_oid.to_string());
    let backup_ref = format!("refs/heads/{backup_branch}");
    let remote_name = remote_name_for_branch(&repo, &branch)?;

    repo.reference(&backup_ref, head_oid, false, "Create reset backup branch")?;
    // libgit2's `Remote::push` returns `Ok(())` even when the server rejects
    // an individual ref. Capture per-ref status via `push_update_reference` and
    // refuse to perform the destructive reset/force-push if the backup was not
    // accepted, otherwise we would destroy remote history with no recovery.
    let backup_refspec = format!("{backup_ref}:{backup_ref}");
    let backup_status = push_refspec_verified(&repo, &remote_name, &backup_refspec, false)?;
    if let Some(reason) = backup_status {
        // Roll back the local backup ref so we don't leak it on retry, then
        // surface a typed error so callers can present a recovery message.
        // If the local delete itself fails, log it: the next rollback's
        // random-suffix backup name will most likely not collide, but a
        // stale `refs/heads/reset/backup/...` accumulating silently is
        // exactly the kind of state we want a hint about in the journal.
        if let Err(err) = repo
            .find_reference(&backup_ref)
            .and_then(|mut reference| reference.delete())
        {
            log::warn!(
                "rollback aborted ({reason}); failed to delete local backup ref {backup_ref}: {err}"
            );
        }
        return Err(GitError::BackupRejected(reason));
    }

    repo.reset(&target, ResetType::Hard, None)?;
    repo.checkout_head(Some(CheckoutBuilder::new().force()))?;

    let remote_ref = remote_ref_for_branch(&repo, &branch)?;
    push_refspec(
        &repo,
        &remote_name,
        &format!("refs/heads/{branch}:{remote_ref}"),
        true,
    )?;
    if let Some(head_oid) = repo.head()?.target() {
        update_tracking_ref(&repo, &remote_name, &remote_ref, head_oid)?;
    }

    Ok(backup_branch)
}

/// Pushes the current branch to its configured remote.
///
/// If the branch has no upstream configuration, this falls back to pushing to
/// `origin` using the same local branch name.
/// Pushes the given local branch to its tracked remote.
///
/// When `branch` is `None`, the working tree's current branch is pushed
/// (the original behavior). When `Some`, that branch is pushed instead;
/// the local branch must exist or the call errors.
pub fn push(project_dir: &Path, branch: Option<&str>) -> Result<(), GitError> {
    let repo = open_repository(project_dir)?;
    let branch = match branch {
        Some(name) => name.to_string(),
        None => current_branch(&repo)?,
    };
    let remote_name = remote_name_for_branch(&repo, &branch)?;
    let remote_ref = remote_ref_for_branch(&repo, &branch)?;
    let refspec = format!("refs/heads/{branch}:{remote_ref}");
    let local_ref = repo.find_reference(&format!("refs/heads/{branch}"))?;
    let local_oid = local_ref.target().ok_or(GitError::NoHeadTarget)?;

    push_refspec(&repo, &remote_name, &refspec, false)?;
    update_tracking_ref(&repo, &remote_name, &remote_ref, local_oid)?;

    Ok(())
}

/// Clones a vault into `dir` (missing or empty), authenticating like a
/// push.
pub fn clone(remote_url: &str, dir: &Path) -> Result<(), GitError> {
    let config = global_config();
    let mut callbacks = RemoteCallbacks::new();
    install_remote_callbacks(&mut callbacks, &config, credential_provider());
    let mut fetch_options = git2::FetchOptions::new();
    fetch_options.remote_callbacks(callbacks);
    git2::build::RepoBuilder::new()
        .fetch_options(fetch_options)
        .clone(remote_url, dir)?;
    Ok(())
}

/// Makes HTTPS remotes trust the certificate authorities in `pem` (PEM
/// certificates, text between them is skipped), on top of OpenSSL's
/// defaults. Returns how many were added.
///
/// Android needs it: libgit2's OpenSSL is built from source there, finds
/// no system authorities, and cannot even read certificate files (it is
/// built without stdio), so the system's certificates are handed over
/// in memory. Call it once at startup, before any network operation.
#[cfg(all(unix, not(any(target_os = "macos", target_os = "ios"))))]
pub fn trust_certificates(pem: &[u8]) -> Result<usize, GitError> {
    use std::{os::raw::c_int, ptr};
    /// `GIT_OPT_ADD_SSL_X509_CERT` (libgit2 1.8+), not in libgit2-sys yet.
    const GIT_OPT_ADD_SSL_X509_CERT: c_int = 45;

    let len = c_int::try_from(pem.len())
        .map_err(|_| GitError::Git(git2::Error::from_str("certificate bundle too large")))?;
    libgit2_sys::init();
    let mut added = 0;
    // SAFETY: the BIO reads `pem`, which outlives it; each X509 is freed
    // after libgit2 took its own reference (X509_STORE_add_cert). libgit2
    // global options are not synchronized with running operations, hence
    // "call once at startup".
    unsafe {
        let bio = openssl_sys::BIO_new_mem_buf(pem.as_ptr().cast(), len);
        if bio.is_null() {
            return Err(GitError::Git(git2::Error::from_str("out of memory")));
        }
        let result = loop {
            let cert = openssl_sys::PEM_read_bio_X509(bio, ptr::null_mut(), None, ptr::null_mut());
            if cert.is_null() {
                break Ok(added);
            }
            let rc = libgit2_sys::git_libgit2_opts(GIT_OPT_ADD_SSL_X509_CERT, cert);
            openssl_sys::X509_free(cert);
            if rc < 0 {
                break Err(GitError::Git(git2::Error::last_error(rc)));
            }
            added += 1;
        };
        // Reaching the end of the PEM data leaves an error on OpenSSL's
        // queue; do not let it surface in an unrelated later call.
        openssl_sys::ERR_clear_error();
        openssl_sys::BIO_free_all(bio);
        result
    }
}

/// The user's global git configuration, or an empty one where there is
/// none (a phone has no `~/.gitconfig`).
fn global_config() -> git2::Config {
    git2::Config::open_default()
        .or_else(|_| git2::Config::new())
        .expect("libgit2 can always create an empty configuration")
}

/// Sets the author of this vault's commits, for devices without a global
/// git identity.
pub fn set_identity(project_dir: &Path, name: &str, email: &str) -> Result<(), GitError> {
    let repo = open_repository(project_dir)?;
    let mut config = repo.config()?.open_level(git2::ConfigLevel::Local)?;
    config.set_str("user.name", name)?;
    config.set_str("user.email", email)?;
    Ok(())
}

/// What [`sync`] exchanged with the remote.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// Commits made elsewhere, now in the local branch.
    pub received: usize,
    /// Local commits the remote did not have, now pushed.
    pub sent: usize,
}

/// Brings `branch` (or the current branch) and its remote in line, both
/// ways: fetches, takes the remote's commits (fast-forward, or a merge
/// commit when both sides changed), then pushes what the remote lacks.
///
/// Edits of different entries merge cleanly. If the same entry changed on
/// both sides, nothing local is touched and [`GitError::MergeConflicts`]
/// is returned.
pub fn sync(project_dir: &Path, branch: Option<&str>) -> Result<SyncReport, GitError> {
    let repo = open_repository(project_dir)?;
    // Read HEAD's name rather than its commit: a clone of an empty remote
    // has an unborn branch that is still the checked-out one.
    let checked_out = repo
        .find_reference("HEAD")?
        .symbolic_target()
        .and_then(|target| target.strip_prefix("refs/heads/"))
        .map(str::to_string);
    let branch = match branch {
        Some(name) => name.to_string(),
        None => checked_out.clone().ok_or(GitError::NoHeadName)?,
    };
    let is_checked_out = checked_out.as_deref() == Some(branch.as_str());
    let remote_name = remote_name_for_branch(&repo, &branch)?;
    let remote_ref = remote_ref_for_branch(&repo, &branch)?;
    let local_ref = format!("refs/heads/{branch}");

    fetch_ref(&repo, &remote_name, &remote_ref)?;
    let local = repo
        .find_reference(&local_ref)
        .ok()
        .and_then(|r| r.target());
    let upstream = upstream_oid_for_branch(&repo, &branch).ok();

    let (merge, new_local) = match take_upstream(
        &repo,
        &local_ref,
        (local, upstream),
        is_checked_out,
        &remote_name,
        OnConflict::Fail,
    ) {
        Err(GitError::MergeConflicts { entries, .. }) => {
            let kept_as =
                local.and_then(|local| keep_unmerged(&repo, &remote_name, &local_ref, local));
            return Err(GitError::MergeConflicts { entries, kept_as });
        }
        other => other?,
    };
    let report = merge.report;
    let Some(new_local) = new_local else {
        return Ok(report);
    };

    if report.sent > 0 {
        push_refspec(
            &repo,
            &remote_name,
            &format!("{local_ref}:{remote_ref}"),
            false,
        )?;
        update_tracking_ref(&repo, &remote_name, &remote_ref, new_local)?;
    }
    Ok(report)
}

/// What a merge does with an entry changed on both sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OnConflict {
    /// Change nothing and fail with [`GitError::MergeConflicts`].
    Fail,
    /// Take the other side's version, and keep this side's as a copy
    /// beside it: see [`CopyMerge::kept_copies`].
    KeepBoth,
}

/// What [`merge_from_copy`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CopyMerge {
    pub report: SyncReport,
    /// Entries changed both here and in the copy: the copy's version took
    /// their place, and this device's was kept under these new paths
    /// (`web/site (conflict).gpg`), so nothing is lost.
    pub kept_copies: Vec<String>,
}

/// Moves the local branch to include `upstream`'s commits: a fast-forward,
/// or a merge commit (named after `source`) when both sides changed.
/// Returns what was exchanged and the branch's new tip (`None` when both
/// sides are empty). `sent` counts the local commits `upstream` lacks,
/// the merge commit included. Conflicting edits are handled as
/// `on_conflict` says; when they fail, nothing is touched and
/// [`GitError::MergeConflicts`] (without `kept_as`) is returned.
fn take_upstream(
    repo: &Repository,
    local_ref: &str,
    (local, upstream): (Option<Oid>, Option<Oid>),
    is_checked_out: bool,
    source: &str,
    on_conflict: OnConflict,
) -> Result<(CopyMerge, Option<Oid>), GitError> {
    let mut report = SyncReport::default();
    let mut kept_copies = Vec::new();
    let new_local = match (local, upstream) {
        // Nothing on the remote yet: everything local is new to it.
        (Some(local), None) => {
            report.sent = count_commits(repo, local)?;
            local
        }
        // A fresh clone of an empty remote that then received commits.
        (None, Some(upstream)) => {
            report.received = count_commits(repo, upstream)?;
            move_branch(repo, local_ref, upstream, is_checked_out)?;
            upstream
        }
        (None, None) => return Ok((CopyMerge::default(), None)),
        (Some(local), Some(upstream)) => {
            let (ahead, behind) = repo.graph_ahead_behind(local, upstream)?;
            report.received = behind;
            report.sent = ahead;
            match (ahead, behind) {
                (_, 0) => local,
                (0, _) => {
                    move_branch(repo, local_ref, upstream, is_checked_out)?;
                    upstream
                }
                _ => {
                    let (merged, kept) = merge_commit(repo, local, upstream, source, on_conflict)?;
                    kept_copies = kept;
                    move_branch(repo, local_ref, merged, is_checked_out)?;
                    report.sent += 1;
                    merged
                }
            }
        }
    };
    Ok((
        CopyMerge {
            report,
            kept_copies,
        },
        Some(new_local),
    ))
}

/// Takes the commits of another copy of the vault, the repository
/// `source_dir` (e.g. a folder copy, bare or not), into the branch
/// checked out in `project_dir`, like [`sync`] does from a remote, but
/// sends nothing back: the caller writes the copy afterwards when `sent`
/// is not zero ([`push_to_copy`]). An entry changed on both sides never
/// stops it: see [`CopyMerge::kept_copies`].
///
/// The source's HEAD branch is fetched into `refs/remotes/<name>/<branch>`
/// (named after the local branch, whatever the source calls its own). A
/// source without commits only counts the local ones as `sent`.
pub fn merge_from_copy(
    project_dir: &Path,
    source_dir: &Path,
    name: &str,
) -> Result<CopyMerge, GitError> {
    let repo = open_repository(project_dir)?;
    let branch = checked_out_branch(&repo)?;
    let tracking = format!("refs/remotes/{name}/{branch}");

    let source = open_repository(source_dir)?;
    let source_ref = source
        .find_reference("HEAD")?
        .symbolic_target()
        .map(str::to_string)
        .ok_or(GitError::NoHeadName)?;
    if source.refname_to_id(&source_ref).is_ok() {
        let url = source_dir
            .to_str()
            .ok_or_else(|| GitError::MissingWorkdir(source_dir.to_path_buf()))?;
        let mut remote = repo.remote_anonymous(url)?;
        remote.fetch(&[format!("+{source_ref}:{tracking}")], None, None)?;
    }
    merge_tracking(&repo, &branch, name)
}

/// Like [`merge_from_copy`], from the bundle file at `bundle_path`
/// (written by [`write_bundle`] on another device).
pub fn merge_from_bundle(
    project_dir: &Path,
    bundle_path: &Path,
    name: &str,
) -> Result<CopyMerge, GitError> {
    let repo = open_repository(project_dir)?;
    let branch = checked_out_branch(&repo)?;
    let header = bundle::read_header(bundle_path).map_err(bundle_error)?;
    if let Some((_, tip)) = header.branch(Some(&branch)) {
        if repo.find_commit(tip).is_err() {
            bundle::import_pack(&repo, bundle_path, &header).map_err(|_| GitError::NotAVault)?;
            repo.find_commit(tip).map_err(|_| GitError::NotAVault)?;
        }
        repo.reference(
            &format!("refs/remotes/{name}/{branch}"),
            tip,
            true,
            "copy read",
        )?;
    }
    merge_tracking(&repo, &branch, name)
}

/// Merges `refs/remotes/<name>/<branch>` into the checked-out `branch`.
fn merge_tracking(repo: &Repository, branch: &str, name: &str) -> Result<CopyMerge, GitError> {
    let local_ref = format!("refs/heads/{branch}");
    let local = repo.refname_to_id(&local_ref).ok();
    let upstream = repo
        .refname_to_id(&format!("refs/remotes/{name}/{branch}"))
        .ok();
    let (merge, _) = take_upstream(
        repo,
        &local_ref,
        (local, upstream),
        true,
        name,
        OnConflict::KeepBoth,
    )?;
    Ok(merge)
}

/// Writes the history of the checked-out branch to `out`, a bundle file
/// (see [`bundle`]). The caller then records it with
/// [`mark_copy_synced`].
pub fn write_bundle(project_dir: &Path, out: &Path) -> Result<(), GitError> {
    let repo = open_repository(project_dir)?;
    let branch = checked_out_branch(&repo)?;
    let tip = repo
        .refname_to_id(&format!("refs/heads/{branch}"))
        .map_err(|_| GitError::NoHeadTarget)?;
    bundle::write(&repo, &branch, tip, out)?;
    Ok(())
}

/// Makes a new vault at `dir` (missing or empty) from the bundle file at
/// `bundle_path`, checked out on the bundle's HEAD branch.
pub fn clone_bundle(bundle_path: &Path, dir: &Path) -> Result<(), GitError> {
    let header = bundle::read_header(bundle_path).map_err(bundle_error)?;
    let (branch, tip) = header.branch(None).ok_or(GitError::NotAVault)?;
    let mut options = git2::RepositoryInitOptions::new();
    options.initial_head(&branch);
    let repo = Repository::init_opts(dir, &options)?;
    // A link in a vault from elsewhere could point anywhere on this
    // device: check links out as plain files.
    repo.config()?.set_bool("core.symlinks", false)?;
    bundle::import_pack(&repo, bundle_path, &header).map_err(|_| GitError::NotAVault)?;
    repo.find_commit(tip).map_err(|_| GitError::NotAVault)?;
    repo.reference(&format!("refs/heads/{branch}"), tip, false, "clone")?;
    repo.checkout_head(Some(CheckoutBuilder::new().force()))?;
    Ok(())
}

/// Sends the checked-out branch to the folder copy `copy_dir` (a bare
/// repository, made with [`init_bare`]). Refused when the copy has
/// commits this vault lacks: [`merge_from_copy`] it first.
pub fn push_to_copy(project_dir: &Path, copy_dir: &Path) -> Result<(), GitError> {
    let repo = open_repository(project_dir)?;
    let branch = checked_out_branch(&repo)?;
    let url = copy_dir
        .to_str()
        .ok_or_else(|| GitError::MissingWorkdir(copy_dir.to_path_buf()))?;
    let mut remote = repo.remote_anonymous(url)?;
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
    remote.push(&[refspec], None)?;
    Ok(())
}

fn bundle_error(err: std::io::Error) -> GitError {
    if err.kind() == std::io::ErrorKind::InvalidData {
        GitError::NotAVault
    } else {
        GitError::Io(err)
    }
}

/// Records that the copy `name` now holds the checked-out branch as it is
/// (it was just uploaded): see [`merge_from_copy`] and [`ahead_of_copy`].
pub fn mark_copy_synced(project_dir: &Path, name: &str) -> Result<(), GitError> {
    let repo = open_repository(project_dir)?;
    let branch = checked_out_branch(&repo)?;
    let Ok(head) = repo.refname_to_id(&format!("refs/heads/{branch}")) else {
        // Nothing committed yet: nothing to record.
        return Ok(());
    };
    repo.reference(
        &format!("refs/remotes/{name}/{branch}"),
        head,
        true,
        "copy synced",
    )?;
    Ok(())
}

/// Local commits the copy `name` does not have yet: all of them when it
/// never received any.
pub fn ahead_of_copy(project_dir: &Path, name: &str) -> Result<usize, GitError> {
    let repo = open_repository(project_dir)?;
    let branch = checked_out_branch(&repo)?;
    let Ok(local) = repo.refname_to_id(&format!("refs/heads/{branch}")) else {
        return Ok(0);
    };
    match repo.refname_to_id(&format!("refs/remotes/{name}/{branch}")) {
        Ok(copy) => Ok(repo.graph_ahead_behind(local, copy)?.0),
        Err(_) => count_commits(&repo, local),
    }
}

/// The checked-out branch's name, even while it is unborn (no commit yet).
fn checked_out_branch(repo: &Repository) -> Result<String, GitError> {
    repo.find_reference("HEAD")?
        .symbolic_target()
        .and_then(|target| target.strip_prefix("refs/heads/"))
        .map(str::to_string)
        .ok_or(GitError::NoHeadName)
}

/// A value of the vault's own git configuration (`.git/config`).
pub fn local_config(project_dir: &Path, key: &str) -> Option<String> {
    let repo = open_repository(project_dir).ok()?;
    let config = repo
        .config()
        .ok()?
        .open_level(git2::ConfigLevel::Local)
        .ok()?;
    config.get_string(key).ok()
}

/// Sets a value of the vault's own git configuration.
pub fn set_local_config(project_dir: &Path, key: &str, value: &str) -> Result<(), GitError> {
    let repo = open_repository(project_dir)?;
    let mut config = repo.config()?.open_level(git2::ConfigLevel::Local)?;
    config.set_str(key, value)?;
    Ok(())
}

/// Sends a branch that could not be merged to the remote under a side
/// name (`passel-unmerged/<commit>`), so another computer can merge it
/// with plain git. Once merged there, the next sync here fast-forwards.
/// Returns the side branch's name, or `None` if the push failed too.
fn keep_unmerged(
    repo: &Repository,
    remote_name: &str,
    local_ref: &str,
    local: Oid,
) -> Option<String> {
    let side = format!("passel-unmerged/{}", &local.to_string()[..12]);
    let refspec = format!("{local_ref}:refs/heads/{side}");
    match push_refspec(repo, remote_name, &refspec, true) {
        Ok(()) => Some(side),
        Err(err) => {
            log::warn!("could not send the unmerged changes: {err}");
            None
        }
    }
}

/// Fetches one remote branch into its remote-tracking ref. A branch the
/// remote does not have (yet) is not an error.
fn fetch_ref(repo: &Repository, remote_name: &str, remote_ref: &str) -> Result<(), GitError> {
    let config = repo.config()?;
    let mut callbacks = RemoteCallbacks::new();
    install_remote_callbacks(&mut callbacks, &config, credential_provider());
    let mut options = git2::FetchOptions::new();
    options.remote_callbacks(callbacks);
    let tracking = remote_ref
        .strip_prefix("refs/heads/")
        .map(|branch| format!("refs/remotes/{remote_name}/{branch}"))
        .unwrap_or_else(|| remote_ref.to_string());
    let mut remote = repo.find_remote(remote_name)?;
    remote.fetch(
        &[format!("+{remote_ref}:{tracking}")],
        Some(&mut options),
        None,
    )?;
    Ok(())
}

fn count_commits(repo: &Repository, tip: Oid) -> Result<usize, GitError> {
    let mut revwalk = repo.revwalk()?;
    revwalk.push(tip)?;
    Ok(revwalk.count())
}

/// Points a branch at `target`, updating the working tree first when the
/// branch is checked out. The checkout is "safe": it refuses to overwrite
/// uncommitted edits instead of losing them.
fn move_branch(
    repo: &Repository,
    local_ref: &str,
    target: Oid,
    is_checked_out: bool,
) -> Result<(), GitError> {
    if is_checked_out {
        let tree = repo.find_commit(target)?.tree()?;
        repo.checkout_tree(tree.as_object(), Some(CheckoutBuilder::new().safe()))?;
    }
    repo.reference(local_ref, target, true, "sync: update from remote")?;
    if is_checked_out {
        repo.set_head(local_ref)?;
    }
    Ok(())
}

/// Creates the merge commit of `local` and `upstream`. Entries changed on
/// both sides fail with [`GitError::MergeConflicts`] without touching
/// anything, or are kept twice (see [`OnConflict::KeepBoth`]): then the
/// paths of this side's copies are returned.
fn merge_commit(
    repo: &Repository,
    local: Oid,
    upstream: Oid,
    remote_name: &str,
    on_conflict: OnConflict,
) -> Result<(Oid, Vec<String>), GitError> {
    let ours = repo.find_commit(local)?;
    let theirs = repo.find_commit(upstream)?;
    let mut index = repo.merge_commits(&ours, &theirs, None)?;
    let mut kept_copies = Vec::new();
    if index.has_conflicts() && on_conflict == OnConflict::KeepBoth {
        kept_copies = keep_both_sides(&mut index)?;
    }
    if index.has_conflicts() {
        let entries = index
            .conflicts()?
            .filter_map(Result::ok)
            .filter_map(|conflict| conflict.our.or(conflict.their))
            .map(|entry| String::from_utf8_lossy(&entry.path).into_owned())
            .collect::<Vec<_>>();
        return Err(GitError::MergeConflicts {
            entries,
            kept_as: None,
        });
    }
    let tree = repo.find_tree(index.write_tree_to(repo)?)?;
    let signature = repo.signature().map_err(|_| GitError::NoSignature)?;
    let oid = repo.commit(
        None,
        &signature,
        &signature,
        &format!("Merge changes from {remote_name}"),
        &tree,
        &[&ours, &theirs],
    )?;
    Ok((oid, kept_copies))
}

/// Settles every conflict of a merge `index` without losing a version:
/// the other side's entry takes the path, and this side's is added as
/// `<name> (conflict).gpg` (numbered when taken) next to it. An entry
/// deleted on one side and changed on the other stays, changed. Returns
/// the paths of the copies.
fn keep_both_sides(index: &mut git2::Index) -> Result<Vec<String>, GitError> {
    /// Stage bits of an index entry's flags (0 = merged).
    const STAGE_MASK: u16 = 0x3000;
    /// Path length bits of an index entry's flags.
    const NAME_MASK: u16 = 0x0fff;
    fn merged(mut entry: git2::IndexEntry, path: Vec<u8>) -> git2::IndexEntry {
        entry.flags = (entry.flags & !(STAGE_MASK | NAME_MASK))
            | u16::try_from(path.len())
                .unwrap_or(NAME_MASK)
                .min(NAME_MASK);
        entry.path = path;
        entry
    }

    let conflicts = index.conflicts()?.collect::<Result<Vec<_>, _>>()?;
    let mut kept = Vec::new();
    for conflict in conflicts {
        let Some(path) = [&conflict.our, &conflict.their, &conflict.ancestor]
            .into_iter()
            .flatten()
            .map(|entry| entry.path.clone())
            .next()
        else {
            continue;
        };
        let path_str = String::from_utf8_lossy(&path).into_owned();
        index.conflict_remove(Path::new(&path_str))?;
        match (conflict.our, conflict.their) {
            (Some(our), Some(their)) => {
                index.add(&merged(their, path.clone()))?;
                match conflict_copy_path(index, &path_str) {
                    Some(copy) => {
                        index.add(&merged(our, copy.clone().into_bytes()))?;
                        kept.push(copy);
                    }
                    // Not an entry (e.g. .gpg-id): the other side's wins.
                    None => log::warn!("merge: kept the other side of a non-entry file"),
                }
            }
            (Some(one), None) | (None, Some(one)) => index.add(&merged(one, path))?,
            (None, None) => {}
        }
    }
    Ok(kept)
}

/// `web/site.gpg` → the first of `web/site (conflict).gpg`,
/// `web/site (conflict 2).gpg`… not in `index`; `None` for a file that is
/// not an entry.
fn conflict_copy_path(index: &git2::Index, path: &str) -> Option<String> {
    let stem = path.strip_suffix(".gpg")?;
    (1..)
        .map(|n| match n {
            1 => format!("{stem} ({}).gpg", gettext("conflict")),
            n => format!("{stem} ({} {n}).gpg", gettext("conflict")),
        })
        .find(|copy| index.get_path(Path::new(copy), 0).is_none())
}

fn current_branch(repo: &Repository) -> Result<String, GitError> {
    let head = repo.head()?;
    head.name()
        .and_then(|name| name.strip_prefix("refs/heads/"))
        .map(str::to_string)
        .ok_or(GitError::NoHeadName)
}

/// Returns the working tree's current branch name, or `None` for an unborn
/// branch / detached HEAD / repository that cannot be opened.
pub fn current_branch_name(project_dir: &Path) -> Option<String> {
    let repo = open_repository(project_dir).ok()?;
    current_branch(&repo).ok()
}

/// Count of local commits that are ahead of the upstream tracking
/// branch — i.e. saved locally but not yet pushed.
///
/// Returns 0 when there is no upstream tracking branch yet (fresh local
/// vault, or remote not configured) so a freshly initialized repo
/// doesn't surface a misleading number in the UI.
pub fn unpushed_count(project_dir: &Path, branch: Option<&str>) -> Result<usize, GitError> {
    let repo = open_repository(project_dir)?;
    // Honor the configured branch override so the badge tracks the
    // branch that push_changes will actually push, rather than HEAD.
    // When no override is set, fall back to HEAD.
    let branch_name = match branch {
        Some(name) => name.to_string(),
        None => current_branch(&repo)?,
    };
    let local_ref = format!("refs/heads/{branch_name}");
    let local = match repo.find_reference(&local_ref) {
        Ok(r) => r.target().ok_or(GitError::NoHeadTarget)?,
        Err(_) => return Ok(0),
    };
    let upstream = match upstream_oid_for_branch(&repo, &branch_name) {
        Ok(oid) => oid,
        Err(_) => return Ok(0),
    };
    let (ahead, _behind) = repo.graph_ahead_behind(local, upstream)?;
    Ok(ahead)
}

fn upstream_oid_for_branch(repo: &Repository, branch: &str) -> Result<Oid, GitError> {
    let remote_name = remote_name_for_branch(repo, branch)?;
    let remote_ref = remote_ref_for_branch(repo, branch)?;
    let remote_tracking_ref = remote_ref
        .strip_prefix("refs/heads/")
        .map(|branch| format!("refs/remotes/{remote_name}/{branch}"))
        .unwrap_or(remote_ref);
    repo.find_reference(&remote_tracking_ref)?
        .target()
        .ok_or(GitError::NoHeadTarget)
}

fn remote_name_for_branch(repo: &Repository, branch: &str) -> Result<String, GitError> {
    let config = repo.config()?;
    Ok(config
        .get_string(&format!("branch.{branch}.remote"))
        .unwrap_or_else(|_| "origin".to_string()))
}

fn remote_ref_for_branch(repo: &Repository, branch: &str) -> Result<String, GitError> {
    let config = repo.config()?;
    let local_ref = format!("refs/heads/{branch}");
    let remote_ref = config
        .get_string(&format!("branch.{branch}.merge"))
        .unwrap_or(local_ref);
    Ok(if remote_ref.starts_with("refs/") {
        remote_ref
    } else {
        format!("refs/heads/{remote_ref}")
    })
}

fn pushed_head(repo: &Repository) -> Result<Oid, GitError> {
    let branch = current_branch(repo)?;
    let remote_name = remote_name_for_branch(repo, &branch)?;
    let remote_ref = remote_ref_for_branch(repo, &branch)?;
    let remote_tracking_ref = remote_ref
        .strip_prefix("refs/heads/")
        .map(|branch| format!("refs/remotes/{remote_name}/{branch}"))
        .unwrap_or(remote_ref);
    repo.find_reference(&remote_tracking_ref)?
        .target()
        .ok_or(GitError::NoHeadTarget)
}

fn push_refspec(
    repo: &Repository,
    remote_name: &str,
    refspec: &str,
    force: bool,
) -> Result<(), GitError> {
    push_refspec_verified(repo, remote_name, refspec, force).map(|_| ())
}

/// Pushes a single refspec and returns the per-ref status reported by the
/// server.
///
/// Returns `Ok(None)` when the server accepted the update, `Ok(Some(reason))`
/// when the server rejected it, and `Err(..)` for transport-level failures.
/// Use this in places where a silent server-side rejection would be unsafe
/// (e.g. before performing a destructive force-push).
fn push_refspec_verified(
    repo: &Repository,
    remote_name: &str,
    refspec: &str,
    force: bool,
) -> Result<Option<String>, GitError> {
    let config = repo.config()?;
    let mut callbacks = RemoteCallbacks::new();
    install_remote_callbacks(&mut callbacks, &config, credential_provider());

    // libgit2's `Remote::push` does not surface per-ref rejections in its
    // return value; it only reports transport errors. Capture rejection
    // reasons via `push_update_reference`, which fires once per pushed ref
    // with `None` for accepted updates and `Some(reason)` when the remote
    // refused.
    let rejections: Rc<RefCell<Vec<(String, String)>>> = Rc::new(RefCell::new(Vec::new()));
    {
        let rejections = Rc::clone(&rejections);
        callbacks.push_update_reference(move |refname, status| {
            if let Some(reason) = status {
                rejections
                    .borrow_mut()
                    .push((refname.to_string(), reason.to_string()));
            }
            Ok(())
        });
    }

    let mut push_options = PushOptions::new();
    push_options.remote_callbacks(callbacks);
    let refspec = if force {
        format!("+{refspec}")
    } else {
        refspec.to_string()
    };

    repo.find_remote(remote_name)?
        .push(&[refspec.as_str()], Some(&mut push_options))?;

    let mut rejections = rejections.borrow_mut();
    if let Some((refname, reason)) = rejections.pop() {
        return Ok(Some(format!("{refname}: {reason}")));
    }

    Ok(None)
}

/// SHA-256 fingerprints (unpadded base64, as `ssh-keygen -lf` prints
/// them) of the SSH host keys published by the hosting services the setup
/// wizard suggests. Sources, checked 2026-11-25:
/// - github.com: <https://api.github.com/meta> (`ssh_key_fingerprints`)
/// - gitlab.com: <https://docs.gitlab.com/user/gitlab_com/>
/// - bitbucket.org: <https://bitbucket.org/site/ssh>
/// - ssh.dev.azure.com: <https://learn.microsoft.com/azure/devops/repos/git/use-ssh-keys-to-authenticate>
const KNOWN_HOST_KEYS: &[(&str, &[&str])] = &[
    (
        "github.com",
        &[
            "+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU",
            "p2QAMXNIC1TJYWeIOttrVc98/R1BUFWu3/LiyKgUfQM",
            "uNiVztksCsDhcc0u9e8BujQXVUpKZIDTMczCvj3tD2s",
        ],
    ),
    (
        "gitlab.com",
        &[
            "eUXGGm1YGsMAS7vkcx6JOJdOGHPem5gQp4taiCfCLB8",
            "HbW3g8zUjNSksFbqTiUWPWg2Bq1x8xdGUrliXFzSnUw",
            "ROQFvPThGrW4RuWLoL9tq9I9zJ42fK4XywyRtbOz/EQ",
        ],
    ),
    (
        "bitbucket.org",
        &[
            "ybgmFkzwOSotHTHLJgHO0QN8L0xErw6vd0VhFA9m3SM",
            "FC73VB6C4OQLSCrjEayhMp9UMxS97caD/Yyi2bhW/J0",
            "46OSHA1Rmj8E8ERTC6xkNcmGOw9oFxYr0WF6zWW8l1E",
        ],
    ),
    (
        "ssh.dev.azure.com",
        &["ohD8VZEXGWo6Ez8GSEJQ9WpafgLFsOfLOtGGQCQo6Og"],
    ),
];

/// True when `host` is one of [`KNOWN_HOST_KEYS`] and presented one of
/// its published keys.
fn is_published_host_key(host: &str, sha256: &[u8; 32]) -> bool {
    let fingerprint = data_encoding::BASE64_NOPAD.encode(sha256);
    KNOWN_HOST_KEYS.iter().any(|(known, keys)| {
        host.eq_ignore_ascii_case(known) && keys.contains(&fingerprint.as_str())
    })
}

/// Wires up the callbacks used for every network operation: host key
/// checks and credentials.
fn install_remote_callbacks<'a>(
    callbacks: &mut RemoteCallbacks<'a>,
    config: &'a git2::Config,
    provider: &'a dyn CredentialProvider,
) {
    // A first connection to github.com & co. would otherwise fail until
    // the user ran `ssh` once in a terminal to fill `known_hosts`. Their
    // published keys are accepted directly; any other host, or a key that
    // does not match, goes through libgit2's usual `known_hosts` check.
    callbacks.certificate_check(|cert, host| {
        let published = cert
            .as_hostkey()
            .and_then(|hostkey| hostkey.hash_sha256())
            .is_some_and(|sha256| is_published_host_key(host, sha256));
        Ok(if published {
            git2::CertificateCheckStatus::CertificateOk
        } else {
            git2::CertificateCheckStatus::CertificatePassthrough
        })
    });
    install_credentials_callback(callbacks, config, provider);
}

/// Wires up the credential callback used for every network operation.
///
/// What to answer comes from `provider` (by default the ssh-agent, then
/// git's credential helpers). libgit2 invokes this callback in a tight loop
/// until it returns `Err` or auth succeeds, so a permanently-rejected key
/// from `ssh-agent` would otherwise hang the app — cap the number of
/// attempts to fail fast in that case.
fn install_credentials_callback<'a>(
    callbacks: &mut RemoteCallbacks<'a>,
    config: &'a git2::Config,
    provider: &'a dyn CredentialProvider,
) {
    let attempts = Cell::new(0u32);
    callbacks.credentials(move |url, username_from_url, allowed_types| {
        // libgit2 first asks for a bare username when an SSH address has
        // none; that is not an authentication attempt.
        let username_only = allowed_types == CredentialType::USERNAME;
        let attempt = attempts.get();
        if !username_only {
            if attempt >= MAX_CREDENTIAL_ATTEMPTS {
                return Err(git2::Error::from_str(
                    "Authentication failed after repeated credential attempts; \
                     check your SSH agent or stored credentials",
                ));
            }
            attempts.set(attempt + 1);
        }

        let request = CredentialRequest {
            url,
            username_from_url,
            ssh_key: allowed_types.intersects(CredentialType::SSH_KEY | CredentialType::SSH_MEMORY),
            user_password: allowed_types.contains(CredentialType::USER_PASS_PLAINTEXT),
            attempt,
        };
        let Some(credential) = provider.credential(&request) else {
            return Err(git2::Error::new(
                ErrorCode::Auth,
                git2::ErrorClass::Net,
                "No credentials for this remote",
            ));
        };
        if username_only {
            if let Some(username) = credential_username(&credential) {
                return Cred::username(username);
            }
        }
        git_credential(credential, config, url, username_from_url)
    });
}

fn credential_username(credential: &Credential) -> Option<&str> {
    match credential {
        Credential::SshAgent { username }
        | Credential::SshKey { username, .. }
        | Credential::UserPassword { username, .. } => Some(username),
        Credential::CredentialHelper => None,
    }
}

fn git_credential(
    credential: Credential,
    config: &git2::Config,
    url: &str,
    username_from_url: Option<&str>,
) -> Result<Cred, git2::Error> {
    match credential {
        Credential::SshAgent { username } => Cred::ssh_key_from_agent(&username)
            .or_else(|_| Cred::credential_helper(config, url, username_from_url)),
        Credential::CredentialHelper => Cred::credential_helper(config, url, username_from_url),
        Credential::SshKey {
            username,
            private_key,
            passphrase,
        } => Cred::ssh_key_from_memory(
            &username,
            None,
            &private_key,
            passphrase.as_deref().map(String::as_str),
        ),
        Credential::UserPassword { username, password } => {
            Cred::userpass_plaintext(&username, &password)
        }
    }
}

fn update_tracking_ref(
    repo: &Repository,
    remote_name: &str,
    remote_ref: &str,
    oid: Oid,
) -> Result<(), GitError> {
    let Some(branch) = remote_ref.strip_prefix("refs/heads/") else {
        return Ok(());
    };
    let tracking_ref = format!("refs/remotes/{remote_name}/{branch}");
    repo.reference(&tracking_ref, oid, true, "Update remote-tracking branch")?;
    Ok(())
}

fn backup_branch_name(branch: &str, head_id: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    let short_id = head_id.chars().take(8).collect::<String>();
    // Random suffix protects against collisions when two rollbacks at
    // the same head land within the same wall-clock second (or on a
    // machine whose clock has been wound backwards).
    let mut entropy = [0u8; 4];
    let suffix = if getrandom::fill(&mut entropy).is_ok() {
        format!(
            "{:02x}{:02x}{:02x}{:02x}",
            entropy[0], entropy[1], entropy[2], entropy[3]
        )
    } else {
        // Fall back to a process-id-derived value: still distinguishes
        // concurrent same-second backups within the same process pair.
        format!("{:08x}", std::process::id())
    };
    format!("reset/backup/{branch}-{short_id}-{timestamp}-{suffix}")
}

/// Returns the current HEAD commit, or `None` for an unborn repository.
fn head_commit(repo: &Repository) -> Result<Option<git2::Commit<'_>>, GitError> {
    match repo.head() {
        Ok(head) => Ok(Some(head.peel_to_commit()?)),
        Err(err) if err.code() == ErrorCode::UnbornBranch || err.code() == ErrorCode::NotFound => {
            Ok(None)
        }
        Err(err) => Err(GitError::Git(err)),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use git2::Repository;

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("passel-{name}-{}-{unique}", std::process::id()))
    }

    /// A working repository on `main` (not on the machine's git default,
    /// so tests behave the same everywhere) with its own identity.
    fn init_repo(dir: &PathBuf) -> Repository {
        init(dir).unwrap();
        let repo = Repository::open(dir).unwrap();
        let mut config = repo.config().unwrap();
        config
            .set_str("user.email", "test@example.invalid")
            .unwrap();
        config.set_str("user.name", "Passel Tests").unwrap();
        drop(config);
        repo
    }

    #[test]
    fn checks_whether_a_remote_can_receive_a_vault() {
        let root = temp_dir("git-check-remote");
        let remote_dir = root.join("remote.git");
        init_bare(&remote_dir, "main").unwrap();
        let url = remote_dir.to_str().unwrap();
        assert!(is_empty_bare_repository(&remote_dir));
        assert_eq!(check_remote(url), RemoteCheck::Empty);

        let work_dir = root.join("work");
        let repo = init_repo(&work_dir);
        fs::write(work_dir.join("entry.gpg"), "encrypted").unwrap();
        add(&work_dir, &work_dir.join("entry.gpg")).unwrap();
        commit(&work_dir, "Add entry").unwrap();
        repo.remote("origin", url).unwrap();
        push(&work_dir, None).unwrap();
        assert!(!is_empty_bare_repository(&remote_dir));
        assert_eq!(check_remote(url), RemoteCheck::NotEmpty);

        let missing = root.join("missing.git");
        assert_eq!(
            check_remote(missing.to_str().unwrap()),
            RemoteCheck::Unreachable
        );
        assert!(!is_empty_bare_repository(&missing));
        assert!(!is_empty_bare_repository(&work_dir));

        fs::remove_dir_all(root).unwrap();
    }

    /// A vault shared by two devices through a bare remote: `desktop`
    /// made the first commit, `phone` cloned it.
    struct SharedVault {
        root: PathBuf,
        remote: PathBuf,
        desktop: PathBuf,
        phone: PathBuf,
    }

    impl SharedVault {
        fn new(name: &str) -> Self {
            let root = temp_dir(name);
            let remote = root.join("remote.git");
            init_bare(&remote, "main").unwrap();
            let desktop = root.join("desktop");
            init_repo(&desktop)
                .remote("origin", remote.to_str().unwrap())
                .unwrap();
            let vault = Self {
                phone: root.join("phone"),
                root,
                remote,
                desktop,
            };
            vault.write(&vault.desktop, "shared.gpg", "v1");
            push(&vault.desktop, None).unwrap();

            clone(vault.remote.to_str().unwrap(), &vault.phone).unwrap();
            set_identity(&vault.phone, "Phone", "phone@example.invalid").unwrap();
            vault
        }

        fn write(&self, device: &Path, file: &str, contents: &str) {
            fs::write(device.join(file), contents).unwrap();
            add(device, &device.join(file)).unwrap();
            commit(device, &format!("Edit {file}")).unwrap();
        }

        fn read(&self, device: &Path, file: &str) -> String {
            fs::read_to_string(device.join(file)).unwrap()
        }
    }

    impl Drop for SharedVault {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    /// A vault made here and pushed to an empty copy must clone, checked
    /// out, on another device: the copy's HEAD has to name the pushed
    /// branch.
    #[test]
    fn a_new_vault_clones_checked_out_elsewhere() {
        let root = temp_dir("git-new-vault");
        let (vault, copy, other) = (
            root.join("vault"),
            root.join("copy.git"),
            root.join("other"),
        );
        init(&vault).unwrap();
        set_identity(&vault, "Phone", "phone@example.invalid").unwrap();
        // An empty repository as GitHub, GitLab or Codeberg create it.
        let mut hosted = git2::RepositoryInitOptions::new();
        hosted.bare(true).initial_head("main");
        Repository::init_opts(&copy, &hosted).unwrap();
        set_origin(&vault, copy.to_str().unwrap()).unwrap();
        fs::write(vault.join(".gpg-id"), "ABCD\n").unwrap();
        add(&vault, &vault.join(".gpg-id")).unwrap();
        commit(&vault, "Initialize vault").unwrap();
        push(&vault, None).unwrap();

        assert_eq!(current_branch_name(&vault).as_deref(), Some("main"));
        clone(copy.to_str().unwrap(), &other).unwrap();
        assert_eq!(fs::read_to_string(other.join(".gpg-id")).unwrap(), "ABCD\n");

        // Folder copies made by Passel name the same branch.
        let folder_copy = root.join("drive.git");
        init_bare(&folder_copy, "main").unwrap();
        let head = Repository::open_bare(&folder_copy).unwrap();
        assert_eq!(
            head.find_reference("HEAD").unwrap().symbolic_target(),
            Some("refs/heads/main")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn clone_brings_the_vault_and_its_upstream() {
        let vault = SharedVault::new("git-clone");
        assert_eq!(vault.read(&vault.phone, "shared.gpg"), "v1");
        assert_eq!(unpushed_count(&vault.phone, None).unwrap(), 0);
        assert_eq!(sync(&vault.phone, None).unwrap(), SyncReport::default());
    }

    #[test]
    fn sync_takes_remote_commits_and_sends_local_ones() {
        let vault = SharedVault::new("git-sync-ff");
        vault.write(&vault.desktop, "desktop.gpg", "from desktop");
        push(&vault.desktop, None).unwrap();

        let report = sync(&vault.phone, None).unwrap();
        assert_eq!(
            report,
            SyncReport {
                received: 1,
                sent: 0
            }
        );
        assert_eq!(vault.read(&vault.phone, "desktop.gpg"), "from desktop");

        vault.write(&vault.phone, "phone.gpg", "from phone");
        let report = sync(&vault.phone, None).unwrap();
        assert_eq!(
            report,
            SyncReport {
                received: 0,
                sent: 1
            }
        );
        assert_eq!(unpushed_count(&vault.phone, None).unwrap(), 0);
        assert_eq!(
            sync(&vault.desktop, None).unwrap(),
            SyncReport {
                received: 1,
                sent: 0
            }
        );
        assert_eq!(vault.read(&vault.desktop, "phone.gpg"), "from phone");
    }

    #[test]
    fn sync_merges_edits_of_different_entries() {
        let vault = SharedVault::new("git-sync-merge");
        vault.write(&vault.desktop, "desktop.gpg", "from desktop");
        push(&vault.desktop, None).unwrap();
        vault.write(&vault.phone, "phone.gpg", "from phone");

        let report = sync(&vault.phone, None).unwrap();
        assert_eq!(
            report,
            SyncReport {
                received: 1,
                sent: 2
            }
        );
        assert_eq!(vault.read(&vault.phone, "desktop.gpg"), "from desktop");
        assert_eq!(vault.read(&vault.phone, "phone.gpg"), "from phone");

        let remote = Repository::open_bare(&vault.remote).unwrap();
        let tip = remote.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(tip.parent_count(), 2, "the merge commit was pushed");
        assert_eq!(tip.author().name(), Some("Phone"));
    }

    #[test]
    fn sync_refuses_conflicting_edits_without_changing_anything() {
        let vault = SharedVault::new("git-sync-conflict");
        vault.write(&vault.desktop, "shared.gpg", "desktop v2");
        push(&vault.desktop, None).unwrap();
        vault.write(&vault.phone, "shared.gpg", "phone v2");
        let before = Repository::open(&vault.phone)
            .unwrap()
            .head()
            .unwrap()
            .target();

        match sync(&vault.phone, None) {
            Err(GitError::MergeConflicts { entries, kept_as }) => {
                assert_eq!(entries, vec!["shared.gpg"]);
                // The phone's version waits on the remote for a merge.
                let side = kept_as.expect("the unmerged commits were sent");
                let remote = Repository::open_bare(&vault.remote).unwrap();
                let sent = remote
                    .find_reference(&format!("refs/heads/{side}"))
                    .unwrap()
                    .target();
                assert_eq!(sent, before);
            }
            other => panic!("expected a conflict, got {other:?}"),
        }
        let after = Repository::open(&vault.phone)
            .unwrap()
            .head()
            .unwrap()
            .target();
        assert_eq!(before, after);
        assert_eq!(vault.read(&vault.phone, "shared.gpg"), "phone v2");

        // A computer merges the side branch (keeping the phone's version)
        // and pushes; the phone's next sync just fast-forwards.
        let desktop = Repository::open(&vault.desktop).unwrap();
        let side = format!("passel-unmerged/{}", &before.unwrap().to_string()[..12]);
        fetch_ref(&desktop, "origin", &format!("refs/heads/{side}")).unwrap();
        let phone_commit = desktop
            .find_reference(&format!("refs/remotes/origin/{side}"))
            .unwrap()
            .peel_to_commit()
            .unwrap();
        let head = desktop.head().unwrap().peel_to_commit().unwrap();
        let signature = desktop.signature().unwrap();
        desktop
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                "Merge the phone's changes",
                &phone_commit.tree().unwrap(),
                &[&head, &phone_commit],
            )
            .unwrap();
        desktop
            .checkout_head(Some(CheckoutBuilder::new().force()))
            .unwrap();
        push(&vault.desktop, None).unwrap();

        let report = sync(&vault.phone, None).unwrap();
        assert_eq!(report.sent, 0);
        assert!(report.received >= 1);
        assert_eq!(vault.read(&vault.phone, "shared.gpg"), "phone v2");
        assert_eq!(unpushed_count(&vault.phone, None).unwrap(), 0);
    }

    /// A vault shared through copies of its folder (as Google Drive keeps
    /// it): `phone` merges `drive`, a copy of the desktop's vault.
    #[test]
    fn merge_from_copy_takes_commits_and_counts_what_to_upload() {
        let vault = SharedVault::new("git-copy-merge");
        let phone = &vault.phone;
        // Never uploaded: everything local is new to the copy.
        assert_eq!(ahead_of_copy(phone, "drive").unwrap(), 1);

        // The copy is the desktop's folder, one commit ahead.
        vault.write(&vault.desktop, "desktop.gpg", "from desktop");
        let merge = merge_from_copy(phone, &vault.desktop, "drive").unwrap();
        assert_eq!(
            merge.report,
            SyncReport {
                received: 1,
                sent: 0
            }
        );
        assert!(merge.kept_copies.is_empty());
        assert_eq!(vault.read(phone, "desktop.gpg"), "from desktop");
        assert_eq!(ahead_of_copy(phone, "drive").unwrap(), 0);

        // Both sides changed different entries: a merge, to upload.
        vault.write(&vault.desktop, "desktop.gpg", "desktop v2");
        vault.write(phone, "phone.gpg", "from phone");
        let merge = merge_from_copy(phone, &vault.desktop, "drive").unwrap();
        assert_eq!(
            merge.report,
            SyncReport {
                received: 1,
                sent: 2
            }
        );
        assert_eq!(vault.read(phone, "desktop.gpg"), "desktop v2");
        assert_eq!(ahead_of_copy(phone, "drive").unwrap(), 2);
        mark_copy_synced(phone, "drive").unwrap();
        assert_eq!(ahead_of_copy(phone, "drive").unwrap(), 0);
        // The git remote is left alone.
        assert_eq!(unpushed_count(phone, None).unwrap(), 4);
    }

    #[test]
    fn merge_from_copy_keeps_both_versions_of_an_entry() {
        let vault = SharedVault::new("git-copy-conflict");
        let phone = &vault.phone;
        vault.write(&vault.desktop, "shared.gpg", "desktop v2");
        vault.write(phone, "shared.gpg", "phone v2");

        let merge = merge_from_copy(phone, &vault.desktop, "drive").unwrap();
        assert_eq!(merge.kept_copies, vec!["shared (conflict).gpg"]);
        assert_eq!(vault.read(phone, "shared.gpg"), "desktop v2");
        assert_eq!(vault.read(phone, "shared (conflict).gpg"), "phone v2");
        assert_eq!(merge.report.received, 1);
        assert_eq!(merge.report.sent, 2, "the phone's commit and the merge");
        let repo = Repository::open(phone).unwrap();
        assert!(
            repo.statuses(None).unwrap().is_empty(),
            "nothing left uncommitted"
        );
        assert_eq!(
            repo.head()
                .unwrap()
                .peel_to_commit()
                .unwrap()
                .parent_count(),
            2
        );

        // A second conflict on the same entry picks the next free name.
        vault.write(&vault.desktop, "shared.gpg", "desktop v3");
        vault.write(phone, "shared.gpg", "phone v3");
        let merge = merge_from_copy(phone, &vault.desktop, "drive").unwrap();
        assert_eq!(merge.kept_copies, vec!["shared (conflict 2).gpg"]);
        assert_eq!(vault.read(phone, "shared (conflict 2).gpg"), "phone v3");
    }

    #[test]
    fn merge_from_copy_fills_an_empty_vault() {
        let vault = SharedVault::new("git-copy-empty");
        let empty = vault.root.join("empty");
        init_repo(&empty);
        let merge = merge_from_copy(&empty, &vault.desktop, "drive").unwrap();
        assert_eq!(
            merge.report,
            SyncReport {
                received: 1,
                sent: 0
            }
        );
        assert_eq!(vault.read(&empty, "shared.gpg"), "v1");

        // And an empty copy only counts the local commits.
        let blank = vault.root.join("blank");
        init_repo(&blank);
        let merge = merge_from_copy(&vault.desktop, &blank, "drive").unwrap();
        assert_eq!(
            merge.report,
            SyncReport {
                received: 0,
                sent: 1
            }
        );
    }

    /// A vault shared through bundle files (as Google Drive or a synced
    /// folder keep it).
    #[test]
    fn bundles_carry_the_vault_both_ways() {
        let vault = SharedVault::new("git-bundle");
        let phone = &vault.phone;
        let desktop_file = vault.root.join("desktop.bundle");
        vault.write(&vault.desktop, "desktop.gpg", "from desktop");
        write_bundle(&vault.desktop, &desktop_file).unwrap();

        // A new device starts from the file alone.
        let tablet = vault.root.join("tablet");
        clone_bundle(&desktop_file, &tablet).unwrap();
        assert_eq!(vault.read(&tablet, "desktop.gpg"), "from desktop");
        assert_eq!(current_branch_name(&tablet).as_deref(), Some("main"));
        let repo = Repository::open(&tablet).unwrap();
        assert!(repo.statuses(None).unwrap().is_empty());
        assert!(repo.remotes().unwrap().is_empty(), "no remote to push to");

        // Stock git clones it too, when installed.
        let cloned = vault.root.join("git-clone");
        if let Ok(output) = std::process::Command::new("git")
            .arg("clone")
            .arg("--quiet")
            .arg(&desktop_file)
            .arg(&cloned)
            .output()
        {
            assert!(output.status.success(), "{output:?}");
            assert_eq!(vault.read(&cloned, "desktop.gpg"), "from desktop");
        }

        // The phone merges it, with a change of its own: both sides kept.
        vault.write(&vault.desktop, "shared.gpg", "desktop v2");
        write_bundle(&vault.desktop, &desktop_file).unwrap();
        vault.write(phone, "shared.gpg", "phone v2");
        let merge = merge_from_bundle(phone, &desktop_file, "drive").unwrap();
        assert_eq!(merge.report.received, 2);
        assert_eq!(merge.kept_copies, vec!["shared (conflict).gpg"]);
        assert_eq!(vault.read(phone, "shared.gpg"), "desktop v2");
        assert_eq!(vault.read(phone, "shared (conflict).gpg"), "phone v2");
        assert_eq!(ahead_of_copy(phone, "drive").unwrap(), 2);

        // Merging the same file again takes nothing new.
        let again = merge_from_bundle(phone, &desktop_file, "drive").unwrap();
        assert_eq!(again.report.received, 0);
        assert!(again.kept_copies.is_empty());

        // The phone's bundle brings the merge back to the desktop.
        let phone_file = vault.root.join("phone.bundle");
        write_bundle(phone, &phone_file).unwrap();
        mark_copy_synced(phone, "drive").unwrap();
        assert_eq!(ahead_of_copy(phone, "drive").unwrap(), 0);
        let back = merge_from_bundle(&vault.desktop, &phone_file, "drive").unwrap();
        assert_eq!(back.report.sent, 0, "a fast-forward");
        assert_eq!(
            vault.read(&vault.desktop, "shared (conflict).gpg"),
            "phone v2"
        );
    }

    #[test]
    fn a_file_that_is_not_a_bundle_is_not_a_vault() {
        let vault = SharedVault::new("git-bundle-junk");
        let junk = vault.root.join("junk.bundle");
        fs::write(&junk, "password-store/\0\0").unwrap();
        assert!(matches!(
            merge_from_bundle(&vault.phone, &junk, "drive"),
            Err(GitError::NotAVault)
        ));
        assert!(matches!(
            clone_bundle(&junk, &vault.root.join("new")),
            Err(GitError::NotAVault)
        ));

        // A real header over a broken pack.
        let mut bytes = Vec::new();
        let tip = Repository::open(&vault.desktop)
            .unwrap()
            .refname_to_id("refs/heads/main")
            .unwrap();
        bytes.extend(format!("# v2 git bundle\n{tip} refs/heads/main\n\nPACKjunk").bytes());
        fs::write(&junk, bytes).unwrap();
        assert!(matches!(
            clone_bundle(&junk, &vault.root.join("new2")),
            Err(GitError::NotAVault)
        ));
    }

    /// A folder copy (a bare repository) written by [`push_to_copy`], read
    /// by [`merge_from_copy`].
    #[test]
    fn folder_copies_carry_the_vault_both_ways() {
        let vault = SharedVault::new("git-folder-copy");
        let phone = &vault.phone;
        let folder = vault.root.join("passwords.git");
        init_bare(&folder, "main").unwrap();
        vault.write(phone, "phone.gpg", "from phone");
        push_to_copy(phone, &folder).unwrap();

        vault.write(&vault.desktop, "desktop.gpg", "from desktop");
        let merge = merge_from_copy(&vault.desktop, &folder, "folder").unwrap();
        assert_eq!(merge.report.received, 1);
        assert_eq!(vault.read(&vault.desktop, "phone.gpg"), "from phone");
        push_to_copy(&vault.desktop, &folder).unwrap();

        // The phone is now behind the folder: a push is refused until
        // it merges.
        vault.write(phone, "phone.gpg", "phone v2");
        assert!(push_to_copy(phone, &folder).is_err());
        merge_from_copy(phone, &folder, "folder").unwrap();
        push_to_copy(phone, &folder).unwrap();
        assert_eq!(vault.read(phone, "desktop.gpg"), "from desktop");
    }

    #[test]
    fn sync_starts_from_a_clone_of_an_empty_remote() {
        let root = temp_dir("git-sync-empty");
        let remote = root.join("remote.git");
        init_bare(&remote, "main").unwrap();
        let phone = root.join("phone");
        clone(remote.to_str().unwrap(), &phone).unwrap();
        set_identity(&phone, "Phone", "phone@example.invalid").unwrap();
        assert_eq!(sync(&phone, None).unwrap(), SyncReport::default());

        fs::write(phone.join("first.gpg"), "x").unwrap();
        add(&phone, &phone.join("first.gpg")).unwrap();
        commit(&phone, "First entry").unwrap();
        assert_eq!(
            sync(&phone, None).unwrap(),
            SyncReport {
                received: 0,
                sent: 1
            }
        );
        assert!(!is_empty_bare_repository(&remote));
        fs::remove_dir_all(root).unwrap();
    }

    /// A smart-HTTP git server for one empty repository that requires
    /// Basic authentication with `expected`. Returns its address and the
    /// `Authorization` headers it received (`None` when absent).
    fn http_remote_requiring(
        expected: &str,
        root: &Path,
    ) -> (
        String,
        std::sync::Arc<std::sync::Mutex<Vec<Option<String>>>>,
    ) {
        use std::io::{BufRead, BufReader, Write};
        use std::sync::{Arc, Mutex};

        let bare = root.join("vault.git");
        init_bare(&bare, "main").unwrap();
        let advertisement = std::process::Command::new("git")
            .args(["upload-pack", "--stateless-rpc", "--advertise-refs"])
            .arg(&bare)
            .output()
            .expect("git is installed")
            .stdout;
        let mut body = b"001e# service=git-upload-pack\n0000".to_vec();
        body.extend_from_slice(&advertisement);

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/vault.git", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (expected, seen_by_server) = (expected.to_string(), Arc::clone(&seen));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut authorization = None;
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                while reader.read_line(&mut line).is_ok_and(|n| n > 0) && line != "\r\n" {
                    if let Some((name, value)) = line.split_once(':') {
                        if name.eq_ignore_ascii_case("authorization") {
                            authorization = Some(value.trim().to_string());
                        }
                    }
                    line.clear();
                }
                let authorized = authorization.as_deref() == Some(expected.as_str());
                seen_by_server.lock().unwrap().push(authorization);
                let _ = if authorized {
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\n\
                         Content-Type: application/x-git-upload-pack-advertisement\r\n\
                         Content-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .and_then(|()| stream.write_all(&body))
                } else {
                    stream.write_all(
                        b"HTTP/1.1 401 Unauthorized\r\n\
                          WWW-Authenticate: Basic realm=\"passel\"\r\n\
                          Content-Length: 0\r\nConnection: close\r\n\r\n",
                    )
                };
            }
        });
        (url, seen)
    }

    fn check_remote_with(url: &str, provider: &dyn CredentialProvider) -> RemoteCheck {
        match remote_has_history(url, provider) {
            Ok(false) => RemoteCheck::Empty,
            Ok(true) => RemoteCheck::NotEmpty,
            Err(err) => classify_remote_error(&err),
        }
    }

    #[test]
    fn sends_a_stored_token_to_an_https_remote() {
        let root = temp_dir("git-http-token");
        let basic = |user_pass: &str| {
            format!(
                "Basic {}",
                data_encoding::BASE64.encode(user_pass.as_bytes())
            )
        };
        let (url, seen) = http_remote_requiring(&basic("me:ghp_token"), &root);
        let stored = StoredCredentials::new();
        stored.set(
            "127.0.0.1",
            Credential::UserPassword {
                username: "me".into(),
                password: zeroize::Zeroizing::new("ghp_token".into()),
            },
        );

        assert_eq!(check_remote_with(&url, &stored), RemoteCheck::Empty);
        assert!(seen.lock().unwrap().contains(&Some(basic("me:ghp_token"))));

        // A revoked token is refused once, then reported, without looping.
        seen.lock().unwrap().clear();
        stored.set(
            "127.0.0.1",
            Credential::UserPassword {
                username: "me".into(),
                password: zeroize::Zeroizing::new("revoked".into()),
            },
        );
        assert_eq!(check_remote_with(&url, &stored), RemoteCheck::AccessDenied);
        let attempts = seen.lock().unwrap().iter().flatten().count();
        assert_eq!(attempts, 1, "the rejected token is not retried");

        // No credential for this host: access denied, nothing sent.
        seen.lock().unwrap().clear();
        stored.remove("127.0.0.1");
        assert_eq!(check_remote_with(&url, &stored), RemoteCheck::AccessDenied);
        assert!(seen.lock().unwrap().iter().all(Option::is_none));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn accepts_only_published_host_keys() {
        let github_ed25519 = data_encoding::BASE64_NOPAD
            .decode(b"+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU")
            .unwrap();
        let github_ed25519: [u8; 32] = github_ed25519.try_into().unwrap();
        assert!(is_published_host_key("github.com", &github_ed25519));
        assert!(is_published_host_key("GitHub.com", &github_ed25519));
        assert!(!is_published_host_key("gitlab.com", &github_ed25519));
        assert!(!is_published_host_key(
            "github.com.evil.example",
            &github_ed25519
        ));
        assert!(!is_published_host_key("github.com", &[0; 32]));
    }

    #[test]
    fn classifies_remote_errors() {
        let error = |code, message: &str| git2::Error::new(code, git2::ErrorClass::Ssh, message);
        assert_eq!(
            classify_remote_error(&error(ErrorCode::Auth, "denied")),
            RemoteCheck::AccessDenied
        );
        assert_eq!(
            classify_remote_error(&error(
                ErrorCode::GenericError,
                "Authentication failed after repeated credential attempts"
            )),
            RemoteCheck::AccessDenied
        );
        assert_eq!(
            classify_remote_error(&error(ErrorCode::Certificate, "unknown")),
            RemoteCheck::UnknownHost
        );
        assert_eq!(
            classify_remote_error(&error(
                ErrorCode::GenericError,
                "invalid or unknown remote ssh hostkey"
            )),
            RemoteCheck::UnknownHost
        );
        assert_eq!(
            classify_remote_error(&error(ErrorCode::NotFound, "repository not found")),
            RemoteCheck::Unreachable
        );
    }

    #[test]
    fn adds_and_commits_file() {
        let dir = temp_dir("git-commit");
        fs::create_dir_all(&dir).unwrap();
        let repo = init_repo(&dir);

        let file_path = dir.join("entry.gpg");
        fs::write(&file_path, "encrypted").unwrap();

        add(&dir, &file_path).unwrap();
        commit(&dir, "Add entry").unwrap();

        let commit = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(commit.message(), Some("Add entry"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn pushes_current_branch_to_origin() {
        let dir = temp_dir("git-push");
        let remote_dir = temp_dir("git-push-remote");
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir_all(&remote_dir).unwrap();
        let repo = init_repo(&dir);
        let _remote_repo = Repository::init_bare(&remote_dir).unwrap();

        let file_path = dir.join("entry.gpg");
        fs::write(&file_path, "encrypted").unwrap();

        add(&dir, &file_path).unwrap();
        commit(&dir, "Add entry").unwrap();

        let refname = repo.head().unwrap().name().unwrap().to_string();
        repo.remote("origin", remote_dir.to_str().unwrap()).unwrap();

        push(&dir, None).unwrap();

        let remote_repo = Repository::open_bare(&remote_dir).unwrap();
        let remote_ref = remote_repo.find_reference(&refname).unwrap();

        assert_eq!(
            remote_ref.target(),
            Some(repo.head().unwrap().peel_to_commit().unwrap().id())
        );

        fs::remove_dir_all(dir).unwrap();
        fs::remove_dir_all(remote_dir).unwrap();
    }

    #[test]
    fn stages_and_commits_file_rename() {
        let dir = temp_dir("git-rename");
        fs::create_dir_all(&dir).unwrap();
        let repo = init_repo(&dir);

        let old_path = dir.join("old.gpg");
        let new_path = dir.join("new.gpg");
        fs::write(&old_path, "encrypted").unwrap();

        add(&dir, &old_path).unwrap();
        commit(&dir, "Add entry").unwrap();
        fs::rename(&old_path, &new_path).unwrap();

        rename(&dir, &old_path, &new_path).unwrap();
        commit(&dir, "rename entry").unwrap();

        let commit = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(commit.message(), Some("rename entry"));
        assert!(repo.revparse_single("HEAD^{tree}:new.gpg").is_ok());
        assert!(repo.revparse_single("HEAD^{tree}:old.gpg").is_err());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stages_and_commits_file_deletion() {
        let dir = temp_dir("git-delete");
        fs::create_dir_all(&dir).unwrap();
        let repo = init_repo(&dir);

        let file_path = dir.join("entry.gpg");
        fs::write(&file_path, "encrypted").unwrap();

        add(&dir, &file_path).unwrap();
        commit(&dir, "Add entry").unwrap();
        fs::remove_file(&file_path).unwrap();

        remove(&dir, &file_path).unwrap();
        commit(&dir, "Delete entry").unwrap();

        let commit = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(commit.message(), Some("Delete entry"));
        assert!(repo.revparse_single("HEAD^{tree}:entry.gpg").is_err());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn lists_branch_changes_newest_first() {
        let dir = temp_dir("git-log");
        fs::create_dir_all(&dir).unwrap();
        let repo = init_repo(&dir);

        let file_path = dir.join("entry.gpg");
        fs::write(&file_path, "first").unwrap();
        add(&dir, &file_path).unwrap();
        commit(&dir, "First entry").unwrap();

        fs::write(&file_path, "second").unwrap();
        add(&dir, &file_path).unwrap();
        commit(&dir, "Second entry").unwrap();

        let changes = branch_changes(&dir, None).unwrap();

        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].summary, "Second entry");
        assert_eq!(changes[1].summary, "First entry");
        assert_eq!(changes[0].short_id.len(), 8);
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let head_author_time = head.author().when();
        assert_eq!(changes[0].author_time_seconds, head_author_time.seconds());
        assert_eq!(
            changes[0].author_time_offset_minutes,
            head_author_time.offset_minutes()
        );

        let second_page = branch_changes_page(&dir, None, 1, 1).unwrap();
        assert_eq!(second_page.len(), 1);
        assert_eq!(second_page[0].summary, "First entry");

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reverts_commit_and_creates_revert_commit() {
        let dir = temp_dir("git-revert");
        fs::create_dir_all(&dir).unwrap();
        let repo = init_repo(&dir);

        let file_path = dir.join("entry.gpg");
        fs::write(&file_path, "first").unwrap();
        add(&dir, &file_path).unwrap();
        commit(&dir, "First entry").unwrap();

        fs::write(&file_path, "second").unwrap();
        add(&dir, &file_path).unwrap();
        commit(&dir, "Second entry").unwrap();
        let commit_id = repo
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string();

        revert_commit(&dir, &commit_id).unwrap();

        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert!(head
            .message()
            .unwrap()
            .starts_with("Revert \"Second entry\""));
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "first");

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rollback_creates_remote_backup_then_force_pushes_branch() {
        let dir = temp_dir("git-rollback");
        let remote_dir = temp_dir("git-rollback-remote");
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir_all(&remote_dir).unwrap();
        let repo = init_repo(&dir);
        let _remote_repo = Repository::init_bare(&remote_dir).unwrap();
        repo.remote("origin", remote_dir.to_str().unwrap()).unwrap();

        let file_path = dir.join("entry.gpg");
        fs::write(&file_path, "first").unwrap();
        add(&dir, &file_path).unwrap();
        commit(&dir, "First entry").unwrap();
        let first_commit = repo
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string();
        push(&dir, None).unwrap();

        fs::write(&file_path, "second").unwrap();
        add(&dir, &file_path).unwrap();
        commit(&dir, "Second entry").unwrap();
        let second_commit = repo
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string();
        push(&dir, None).unwrap();

        let backup_branch = rollback_to_commit(&dir, &first_commit).unwrap();
        let remote_repo = Repository::open_bare(&remote_dir).unwrap();
        let branch_ref = repo.head().unwrap().name().unwrap().to_string();
        let remote_backup_ref = format!("refs/heads/{backup_branch}");

        assert_eq!(fs::read_to_string(&file_path).unwrap(), "first");
        assert_eq!(
            remote_repo
                .find_reference(&remote_backup_ref)
                .unwrap()
                .target()
                .unwrap()
                .to_string(),
            second_commit
        );
        assert_eq!(
            remote_repo
                .find_reference(&branch_ref)
                .unwrap()
                .target()
                .unwrap()
                .to_string(),
            first_commit
        );

        fs::remove_dir_all(dir).unwrap();
        fs::remove_dir_all(remote_dir).unwrap();
    }

    #[test]
    fn rollback_does_not_destroy_remote_when_backup_push_fails() {
        // End-to-end safety: if the backup push fails for any reason — here
        // we point origin at a non-existent path so the backup push errors
        // out — the local HEAD, working copy, and the remote branch must
        // all be unchanged. The destructive reset + force-push must never
        // run when the backup step did not succeed.
        //
        // Note: this exercises the transport-error path. The
        // `BackupRejected` path (where libgit2's `Remote::push` returns
        // Ok(()) but the per-ref status reported via
        // `push_update_reference` is `Some(reason)`) is hard to trigger
        // cleanly with libgit2's "local" transport, which mostly fails
        // bad pushes client-side. The branch is exercised by the same
        // safety logic and surfaced via `GitError::BackupRejected` in
        // production, where the smart HTTP/SSH protocols populate per-ref
        // status messages from the actual remote.
        let dir = temp_dir("git-rollback-no-destroy");
        let remote_dir = temp_dir("git-rollback-no-destroy-remote");
        fs::create_dir_all(&dir).unwrap();
        fs::create_dir_all(&remote_dir).unwrap();
        let repo = init_repo(&dir);
        let remote_repo = Repository::init_bare(&remote_dir).unwrap();
        repo.remote("origin", remote_dir.to_str().unwrap()).unwrap();

        let file_path = dir.join("entry.gpg");
        fs::write(&file_path, "first").unwrap();
        add(&dir, &file_path).unwrap();
        commit(&dir, "First entry").unwrap();
        let first_commit = repo
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string();
        push(&dir, None).unwrap();

        fs::write(&file_path, "second").unwrap();
        add(&dir, &file_path).unwrap();
        commit(&dir, "Second entry").unwrap();
        let second_commit = repo
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string();
        push(&dir, None).unwrap();

        // Repoint origin at a non-existent path so the backup push fails.
        let bad_remote = temp_dir("git-rollback-no-destroy-bad");
        repo.remote_set_url("origin", bad_remote.to_str().unwrap())
            .unwrap();

        let result = rollback_to_commit(&dir, &first_commit);
        assert!(
            result.is_err(),
            "rollback must fail when the backup push fails"
        );

        // Local HEAD must still point at the second commit — the destructive
        // reset must not have run.
        let head_after = repo
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string();
        assert_eq!(head_after, second_commit);
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "second");

        // Restore origin so we can inspect what was/wasn't pushed.
        repo.remote_set_url("origin", remote_dir.to_str().unwrap())
            .unwrap();

        // The original remote branch must still point at the second commit
        // (i.e. we did not silently force-push a rolled-back history).
        let branch_ref = repo.head().unwrap().name().unwrap().to_string();
        let remote_branch = remote_repo.find_reference(&branch_ref).unwrap();
        assert_eq!(remote_branch.target().unwrap().to_string(), second_commit);

        fs::remove_dir_all(dir).unwrap();
        fs::remove_dir_all(remote_dir).unwrap();
    }

    #[test]
    fn backup_rejected_error_redacts_reason() {
        // The variant is distinct (so the UI can branch on it) but its
        // Display deliberately drops the libgit2-supplied reason so we
        // don't echo remote ref names into a UI toast.
        let err = GitError::BackupRejected("refs/heads/foo: denied by server".to_string());
        let message = err.to_string();
        assert!(matches!(err, GitError::BackupRejected(_)));
        assert!(message.contains("rollback backup"));
        assert!(!message.contains("denied by server"));
        assert!(!message.contains("refs/heads/foo"));
    }

    #[test]
    fn reports_open_repository_failure() {
        let dir = temp_dir("git-failure");
        fs::create_dir_all(&dir).unwrap();

        let err = commit(&dir, "No repository").unwrap_err();
        let message = err.to_string();

        assert!(matches!(err, GitError::Git(_)));
        // Display drops the inner libgit2 message; only the categorical
        // "Git operation failed" text reaches the user.
        assert_eq!(message, "Git operation failed");

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

        let err = add(&dir, &outside_file).unwrap_err();

        assert!(matches!(err, GitError::InvalidPath { .. }));

        fs::remove_dir_all(dir).unwrap();
        fs::remove_dir_all(outside_dir).unwrap();
    }

    #[cfg(all(unix, not(any(target_os = "macos", target_os = "ios"))))]
    #[test]
    fn trusts_certificates_given_in_memory() {
        // Two real roots, with text around them as in Android's files.
        let bundle = "ISRG Root X1\n\
-----BEGIN CERTIFICATE-----
MIIFazCCA1OgAwIBAgIRAIIQz7DSQONZRGPgu2OCiwAwDQYJKoZIhvcNAQELBQAw
TzELMAkGA1UEBhMCVVMxKTAnBgNVBAoTIEludGVybmV0IFNlY3VyaXR5IFJlc2Vh
cmNoIEdyb3VwMRUwEwYDVQQDEwxJU1JHIFJvb3QgWDEwHhcNMTUwNjA0MTEwNDM4
WhcNMzUwNjA0MTEwNDM4WjBPMQswCQYDVQQGEwJVUzEpMCcGA1UEChMgSW50ZXJu
ZXQgU2VjdXJpdHkgUmVzZWFyY2ggR3JvdXAxFTATBgNVBAMTDElTUkcgUm9vdCBY
MTCCAiIwDQYJKoZIhvcNAQEBBQADggIPADCCAgoCggIBAK3oJHP0FDfzm54rVygc
h77ct984kIxuPOZXoHj3dcKi/vVqbvYATyjb3miGbESTtrFj/RQSa78f0uoxmyF+
0TM8ukj13Xnfs7j/EvEhmkvBioZxaUpmZmyPfjxwv60pIgbz5MDmgK7iS4+3mX6U
A5/TR5d8mUgjU+g4rk8Kb4Mu0UlXjIB0ttov0DiNewNwIRt18jA8+o+u3dpjq+sW
T8KOEUt+zwvo/7V3LvSye0rgTBIlDHCNAymg4VMk7BPZ7hm/ELNKjD+Jo2FR3qyH
B5T0Y3HsLuJvW5iB4YlcNHlsdu87kGJ55tukmi8mxdAQ4Q7e2RCOFvu396j3x+UC
B5iPNgiV5+I3lg02dZ77DnKxHZu8A/lJBdiB3QW0KtZB6awBdpUKD9jf1b0SHzUv
KBds0pjBqAlkd25HN7rOrFleaJ1/ctaJxQZBKT5ZPt0m9STJEadao0xAH0ahmbWn
OlFuhjuefXKnEgV4We0+UXgVCwOPjdAvBbI+e0ocS3MFEvzG6uBQE3xDk3SzynTn
jh8BCNAw1FtxNrQHusEwMFxIt4I7mKZ9YIqioymCzLq9gwQbooMDQaHWBfEbwrbw
qHyGO0aoSCqI3Haadr8faqU9GY/rOPNk3sgrDQoo//fb4hVC1CLQJ13hef4Y53CI
rU7m2Ys6xt0nUW7/vGT1M0NPAgMBAAGjQjBAMA4GA1UdDwEB/wQEAwIBBjAPBgNV
HRMBAf8EBTADAQH/MB0GA1UdDgQWBBR5tFnme7bl5AFzgAiIyBpY9umbbjANBgkq
hkiG9w0BAQsFAAOCAgEAVR9YqbyyqFDQDLHYGmkgJykIrGF1XIpu+ILlaS/V9lZL
ubhzEFnTIZd+50xx+7LSYK05qAvqFyFWhfFQDlnrzuBZ6brJFe+GnY+EgPbk6ZGQ
3BebYhtF8GaV0nxvwuo77x/Py9auJ/GpsMiu/X1+mvoiBOv/2X/qkSsisRcOj/KK
NFtY2PwByVS5uCbMiogziUwthDyC3+6WVwW6LLv3xLfHTjuCvjHIInNzktHCgKQ5
ORAzI4JMPJ+GslWYHb4phowim57iaztXOoJwTdwJx4nLCgdNbOhdjsnvzqvHu7Ur
TkXWStAmzOVyyghqpZXjFaH3pO3JLF+l+/+sKAIuvtd7u+Nxe5AW0wdeRlN8NwdC
jNPElpzVmbUq4JUagEiuTDkHzsxHpFKVK7q4+63SM1N95R1NbdWhscdCb+ZAJzVc
oyi3B43njTOQ5yOf+1CceWxG1bQVs5ZufpsMljq4Ui0/1lvh+wjChP4kqKOJ2qxq
4RgqsahDYVvTH9w7jXbyLeiNdd8XM2w9U/t7y0Ff/9yi0GE44Za4rF2LN9d11TPA
mRGunUHBcnWEvgJBQl9nJEiU0Zsnvgc/ubhPgXRR4Xq37Z0j4r7g1SgEEzwxA57d
emyPxgcYxn/eR44/KJ4EBs+lVDR3veyJm+kXQ99b21/+jh5Xos1AnX5iItreGCc=
-----END CERTIFICATE-----
Certificate:\n    Data: (text dump)\n";
        assert_eq!(trust_certificates(bundle.as_bytes()).unwrap(), 1);
        assert_eq!(trust_certificates(b"no certificate here").unwrap(), 0);
    }

    /// Android's app folder is reached as /data/user/0/… while libgit2
    /// reports the workdir as /data/data/… (a symlink between them).
    #[cfg(unix)]
    #[test]
    fn stages_paths_reached_through_a_symlinked_folder() {
        let root = temp_dir("git-symlinked");
        let real = root.join("data");
        let alias = root.join("user");
        fs::create_dir_all(&real).unwrap();
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        let vault = alias.join("vault");
        init_repo(&vault);

        fs::write(vault.join("entry.gpg"), "v1").unwrap();
        add(&vault, &vault.join("entry.gpg")).unwrap();
        commit(&vault, "Add entry").unwrap();

        fs::remove_file(vault.join("entry.gpg")).unwrap();
        remove(&vault, &vault.join("entry.gpg")).unwrap();
        commit(&vault, "Remove entry").unwrap();
        assert_eq!(branch_changes(&vault, None).unwrap().len(), 2);

        // Still nothing outside the vault.
        fs::write(root.join("outside.gpg"), "x").unwrap();
        assert!(matches!(
            add(&vault, &alias.join("../outside.gpg")),
            Err(GitError::InvalidPath { .. })
        ));
        fs::remove_dir_all(root).unwrap();
    }
}
