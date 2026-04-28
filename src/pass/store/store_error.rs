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

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Io(err) => write!(f, "Io error: {err}"),
            StoreError::Git(err) => write!(f, "Git error: {err}"),
            StoreError::Pgp(err) => write!(f, "Gpg error: {err}"),
            StoreError::MissingParent(path) => {
                write!(f, "Missing parent directory for path: {}", path.display())
            }
            StoreError::EmptyFile(path) => {
                write!(f, "Empty file for path: {}", path.display())
            }
            StoreError::InvalidEntryName(name) => {
                write!(f, "Invalid entry name: {name}")
            }
            StoreError::InvalidRecipient => {
                write!(f, "GPG recipient is required")
            }
            StoreError::InvalidFolderPath(path) => {
                write!(f, "Invalid folder path: {}", path.display())
            }
            StoreError::DestinationExists(path) => {
                write!(f, "Destination already exists: {}", path.display())
            }
            StoreError::PushFailed(reason) => {
                write!(f, "Saved locally but failed to push to remote: {reason}")
            }
        }
    }
}

impl std::error::Error for StoreError {}

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
