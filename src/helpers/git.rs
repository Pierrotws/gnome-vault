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

fn open_repository(project_dir: &Path) -> Result<Repository, GitError> {
    Repository::open(project_dir).map_err(GitError::from)
}

fn workdir_path(repo: &Repository) -> Result<&Path, GitError> {
    repo.workdir()
        .ok_or_else(|| GitError::MissingWorkdir(repo.path().to_path_buf()))
}

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

pub fn add(project_dir: &Path, file_path: &Path) -> Result<(), GitError> {
    let repo = open_repository(project_dir)?;
    let file_path = relative_workdir_path(&repo, file_path)?;
    let mut index = repo.index()?;

    index.add_path(&file_path)?;
    index.write()?;

    Ok(())
}

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

pub fn push(project_dir: &Path) -> Result<(), GitError> {
    let repo = open_repository(project_dir)?;
    let head = repo.head()?;
    let refname = head.name().ok_or(GitError::NoHeadName)?.to_string();
    let refspec = format!("{refname}:{refname}");
    let config = repo.config()?;
    let mut callbacks = RemoteCallbacks::new();

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

    repo.find_remote("origin")?
        .push(&[refspec.as_str()], Some(&mut push_options))?;

    Ok(())
}

fn head_commit(repo: &Repository) -> Result<Option<git2::Commit<'_>>, GitError> {
    match repo.head() {
        Ok(head) => Ok(Some(head.peel_to_commit()?)),
        Err(err) if err.code() == ErrorCode::UnbornBranch || err.code() == ErrorCode::NotFound => {
            Ok(None)
        }
        Err(err) => Err(GitError::Git(err)),
    }
}
