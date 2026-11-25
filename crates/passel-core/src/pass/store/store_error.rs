use std::path::PathBuf;

use crate::helpers::{git, pgp};
use crate::i18n::{gettext, gettext_f};

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

// Hand-rolled `Debug` matches `Display`'s redaction posture so a
// stray `log::warn!("{err:?}")` or any panic via `.unwrap()` /
// `.expect()` (both render their cause via `Debug`) doesn't put
// secret-bearing path/name strings into journalctl. Tests get
// verbose Debug under `cfg(test)` to keep `assert!(matches!(...))`
// failure messages useful — see the `EntryField` template.
#[cfg(not(test))]
impl std::fmt::Debug for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            StoreError::Io(_) => "Io",
            StoreError::Git(_) => "Git",
            StoreError::Pgp(_) => "Pgp",
            StoreError::MissingParent(_) => "MissingParent",
            StoreError::EmptyFile(_) => "EmptyFile",
            StoreError::InvalidEntryName(_) => "InvalidEntryName",
            StoreError::InvalidRecipient => "InvalidRecipient",
            StoreError::InvalidFolderPath(_) => "InvalidFolderPath",
            StoreError::DestinationExists(_) => "DestinationExists",
            StoreError::PushFailed(_) => "PushFailed",
        };
        write!(f, "StoreError::{name}(<redacted>)")
    }
}

#[cfg(test)]
impl std::fmt::Debug for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Io(err) => f.debug_tuple("Io").field(err).finish(),
            StoreError::Git(err) => f.debug_tuple("Git").field(err).finish(),
            StoreError::Pgp(err) => f.debug_tuple("Pgp").field(err).finish(),
            StoreError::MissingParent(p) => f.debug_tuple("MissingParent").field(p).finish(),
            StoreError::EmptyFile(p) => f.debug_tuple("EmptyFile").field(p).finish(),
            StoreError::InvalidEntryName(s) => f.debug_tuple("InvalidEntryName").field(s).finish(),
            StoreError::InvalidRecipient => f.debug_struct("InvalidRecipient").finish(),
            StoreError::InvalidFolderPath(p) => {
                f.debug_tuple("InvalidFolderPath").field(p).finish()
            }
            StoreError::DestinationExists(p) => {
                f.debug_tuple("DestinationExists").field(p).finish()
            }
            StoreError::PushFailed(s) => f.debug_tuple("PushFailed").field(s).finish(),
        }
    }
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
        let message = match self {
            StoreError::Io(err) => gettext_f(
                "I/O error ({reason})",
                &[("reason", &err.kind().to_string())],
            ),
            StoreError::Git(err) => {
                gettext_f("Git error: {reason}", &[("reason", &err.to_string())])
            }
            StoreError::Pgp(err) => {
                gettext_f("GPG error: {reason}", &[("reason", &err.to_string())])
            }
            StoreError::MissingParent(_) => gettext("Missing parent directory"),
            StoreError::EmptyFile(_) => gettext("Encrypted file is empty"),
            StoreError::InvalidEntryName(_) => gettext("Invalid entry name"),
            StoreError::InvalidRecipient => gettext("GPG recipient is required"),
            StoreError::InvalidFolderPath(_) => gettext("Invalid folder path"),
            StoreError::DestinationExists(_) => gettext("Destination already exists"),
            StoreError::PushFailed(_) => gettext("Saved locally but failed to push to remote"),
        };
        f.write_str(&message)
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
