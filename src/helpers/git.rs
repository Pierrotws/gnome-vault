use std::path::{Path, PathBuf};

use git2::{Cred, CredentialType, ErrorCode, PushOptions, RemoteCallbacks, Repository};

#[derive(Debug)]
pub enum GitError {
    Git(git2::Error),
    InvalidPath { path: PathBuf, base: PathBuf },
    MissingWorkdir(PathBuf),
    NoHeadName,
    NoChanges,
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
            GitError::NoChanges => write!(f, "No changes to commit"),
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

/// Pushes the current branch to its configured remote.
///
/// If the branch has no upstream configuration, this falls back to pushing to
/// `origin` using the same local branch name.
pub fn push(project_dir: &Path) -> Result<(), GitError> {
    let repo = open_repository(project_dir)?;
    let head = repo.head()?;
    let refname = head
        .name()
        .filter(|name| name.starts_with("refs/heads/"))
        .ok_or(GitError::NoHeadName)?
        .to_string();
    let branch_name = refname
        .strip_prefix("refs/heads/")
        .ok_or(GitError::NoHeadName)?;
    let config = repo.config()?;
    let remote_name = config
        .get_string(&format!("branch.{branch_name}.remote"))
        .unwrap_or_else(|_| "origin".to_string());
    let remote_ref = config
        .get_string(&format!("branch.{branch_name}.merge"))
        .unwrap_or_else(|_| refname.clone());
    let remote_ref = if remote_ref.starts_with("refs/") {
        remote_ref
    } else {
        format!("refs/heads/{remote_ref}")
    };
    let refspec = format!("{refname}:{remote_ref}");
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

    repo.find_remote(&remote_name)?
        .push(&[refspec.as_str()], Some(&mut push_options))?;

    Ok(())
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
