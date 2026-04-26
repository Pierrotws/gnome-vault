use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use git2::{
    build::CheckoutBuilder, Cred, CredentialType, ErrorCode, Oid, PushOptions, RemoteCallbacks,
    Repository, ResetType, Sort,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitChange {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    pub author: String,
    pub parent_count: usize,
    pub is_pushed: bool,
}

#[derive(Debug)]
pub enum GitError {
    Git(git2::Error),
    InvalidPath { path: PathBuf, base: PathBuf },
    MissingWorkdir(PathBuf),
    NoHeadName,
    NoHeadTarget,
    NoChanges,
    Conflicts(String),
}

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GitError::Git(err) => write!(f, "Git error: {err}"),
            GitError::InvalidPath { path, base } => write!(
                f,
                "Path {} is not inside git workdir {}",
                path.display(),
                base.display()
            ),
            GitError::MissingWorkdir(path) => {
                write!(f, "Repository has no workdir: {}", path.display())
            }
            GitError::NoHeadName => write!(f, "Current git HEAD has no branch name"),
            GitError::NoHeadTarget => write!(f, "Current git HEAD has no target commit"),
            GitError::NoChanges => write!(f, "No changes to commit"),
            GitError::Conflicts(commit_id) => {
                write!(f, "Reverting commit {commit_id} produced conflicts")
            }
        }
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
pub fn init(project_dir: &Path) -> Result<(), GitError> {
    Repository::init(project_dir)?;
    Ok(())
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

    if file_path.is_absolute() {
        return file_path
            .strip_prefix(workdir)
            .map(Path::to_path_buf)
            .map_err(|_| GitError::InvalidPath {
                path: file_path.to_path_buf(),
                base: workdir.to_path_buf(),
            });
    }

    Ok(file_path.to_path_buf())
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

/// Stages a file deletion in the repository index.
pub fn remove(project_dir: &Path, file_path: &Path) -> Result<(), GitError> {
    let repo = open_repository(project_dir)?;
    let file_path = relative_workdir_path(&repo, file_path)?;
    let mut index = repo.index()?;

    index.remove_path(&file_path)?;
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
    let signature = repo.signature()?;
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

/// Lists commits reachable from the current branch head.
pub fn current_branch_changes(project_dir: &Path) -> Result<Vec<GitChange>, GitError> {
    current_branch_changes_page(project_dir, 0, usize::MAX)
}

/// Lists a page of commits reachable from the current branch head.
pub fn current_branch_changes_page(
    project_dir: &Path,
    offset: usize,
    limit: usize,
) -> Result<Vec<GitChange>, GitError> {
    let repo = open_repository(project_dir)?;
    let pushed_head = pushed_head(&repo).ok();
    let mut revwalk = repo.revwalk()?;

    if let Err(err) = revwalk.push_head() {
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
                .unwrap_or_else(|| "(no message)".to_string());
            let author = commit
                .author()
                .name()
                .map(str::to_string)
                .unwrap_or_else(|| "Unknown author".to_string());
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
    push_refspec(
        &repo,
        &remote_name,
        &format!("{backup_ref}:{backup_ref}"),
        false,
    )?;

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
pub fn push(project_dir: &Path) -> Result<(), GitError> {
    let repo = open_repository(project_dir)?;
    let branch = current_branch(&repo)?;
    let remote_name = remote_name_for_branch(&repo, &branch)?;
    let remote_ref = remote_ref_for_branch(&repo, &branch)?;
    let refspec = format!("refs/heads/{branch}:{remote_ref}");
    let head_oid = repo.head()?.target().ok_or(GitError::NoHeadTarget)?;

    push_refspec(&repo, &remote_name, &refspec, false)?;
    update_tracking_ref(&repo, &remote_name, &remote_ref, head_oid)?;

    Ok(())
}

fn current_branch(repo: &Repository) -> Result<String, GitError> {
    let head = repo.head()?;
    head.name()
        .and_then(|name| name.strip_prefix("refs/heads/"))
        .map(str::to_string)
        .ok_or(GitError::NoHeadName)
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
    let config = repo.config()?;
    let mut callbacks = RemoteCallbacks::new();

    // Try SSH agent credentials first, then fall back to libgit2's configured
    // credential helpers for HTTPS or custom credential setups.
    callbacks.credentials(move |url, username_from_url, allowed_types| {
        if allowed_types.contains(CredentialType::SSH_KEY) {
            if let Some(username) = username_from_url {
                if let Ok(cred) = Cred::ssh_key_from_agent(username) {
                    return Ok(cred);
                }
            }
        }

        Cred::credential_helper(&config, url, username_from_url)
    });

    let mut push_options = PushOptions::new();
    push_options.remote_callbacks(callbacks);
    let refspec = if force {
        format!("+{refspec}")
    } else {
        refspec.to_string()
    };

    repo.find_remote(remote_name)?
        .push(&[refspec.as_str()], Some(&mut push_options))?;

    Ok(())
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
    format!("reset/backup/{branch}-{short_id}-{timestamp}")
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

        push(&dir).unwrap();

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
    fn lists_current_branch_changes_newest_first() {
        let dir = temp_dir("git-log");
        fs::create_dir_all(&dir).unwrap();
        let _repo = init_repo(&dir);

        let file_path = dir.join("entry.gpg");
        fs::write(&file_path, "first").unwrap();
        add(&dir, &file_path).unwrap();
        commit(&dir, "First entry").unwrap();

        fs::write(&file_path, "second").unwrap();
        add(&dir, &file_path).unwrap();
        commit(&dir, "Second entry").unwrap();

        let changes = current_branch_changes(&dir).unwrap();

        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].summary, "Second entry");
        assert_eq!(changes[1].summary, "First entry");
        assert_eq!(changes[0].short_id.len(), 8);

        let second_page = current_branch_changes_page(&dir, 1, 1).unwrap();
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
        push(&dir).unwrap();

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
        push(&dir).unwrap();

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
    fn reports_open_repository_failure() {
        let dir = temp_dir("git-failure");
        fs::create_dir_all(&dir).unwrap();

        let err = commit(&dir, "No repository").unwrap_err();
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

        let err = add(&dir, &outside_file).unwrap_err();

        assert!(matches!(err, GitError::InvalidPath { .. }));

        fs::remove_dir_all(dir).unwrap();
        fs::remove_dir_all(outside_dir).unwrap();
    }
}
