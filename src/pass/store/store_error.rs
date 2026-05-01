use std::path::PathBuf;

use crate::helpers::{git, pgp};

#[derive(Debug)]
pub enum StoreError {
    Io(std::io::Error),
    Git(git::GitError),
    Pgp(pgp::PgpError),
    MissingParent(PathBuf),
    EmptyFile(PathBuf),
    InvalidEntryName(String),
    InvalidRecipient,
    InvalidFolderPath(PathBuf),
    DestinationExists(PathBuf),
    /// The local commit succeeded but pushing to the remote failed.
    ///
    /// Distinguished from [`StoreError::Git`] so the UI can tell the user
    /// "your change was saved locally but did not reach the remote" rather
    /// than implying the save itself failed.
    PushFailed(String),
}

// `Display` produces the user-facing message and intentionally avoids
// including entry paths or names (which inside a password store ARE the
// secret labels), gpg stderr, or git stderr that may echo refs/paths.
// `GitError::Display` and `PgpError::Display` are themselves redacted —
// see the comments on those impls — so it's safe to forward them here
// for category context. Logs MUST use `{err}` (Display); the auto-
// derived `Debug` re-exposes the upstream messages that Display drops,
// so reserve `{err:?}` for ad-hoc developer inspection only.
impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Io(err) => write!(f, "I/O error ({})", err.kind()),
            StoreError::Git(err) => write!(f, "Git error: {err}"),
            StoreError::Pgp(err) => write!(f, "GPG error: {err}"),
            StoreError::MissingParent(_) => f.write_str("Missing parent directory"),
            StoreError::EmptyFile(_) => f.write_str("Encrypted file is empty"),
            StoreError::InvalidEntryName(_) => f.write_str("Invalid entry name"),
            StoreError::InvalidRecipient => f.write_str("GPG recipient is required"),
            StoreError::InvalidFolderPath(_) => f.write_str("Invalid folder path"),
            StoreError::DestinationExists(_) => f.write_str("Destination already exists"),
            StoreError::PushFailed(_) => {
                f.write_str("Saved locally but failed to push to remote")
            }
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StoreError::Io(err) => Some(err),
            StoreError::Git(err) => Some(err),
            StoreError::Pgp(err) => Some(err),
            StoreError::MissingParent(_)
            | StoreError::EmptyFile(_)
            | StoreError::InvalidEntryName(_)
            | StoreError::InvalidRecipient
            | StoreError::InvalidFolderPath(_)
            | StoreError::DestinationExists(_)
            | StoreError::PushFailed(_) => None,
        }
    }
}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e)
    }
}

impl From<git::GitError> for StoreError {
    fn from(e: git::GitError) -> Self {
        StoreError::Git(e)
    }
}

impl From<pgp::PgpError> for StoreError {
    fn from(e: pgp::PgpError) -> Self {
        StoreError::Pgp(e)
    }
}
