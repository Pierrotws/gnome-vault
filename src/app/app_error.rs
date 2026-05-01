//This file defines application-level errors.

use std::io;

use crate::pass::store::StoreError;

#[derive(Debug)]
pub enum AppError {
    Io(io::Error),
    Save(StoreError),
    /// The local commit succeeded but pushing to the remote failed.
    ///
    /// Lifted out of `Save(StoreError::PushFailed)` so callers can
    /// branch on "saved locally, push failed" without re-introspecting
    /// `StoreError`. The wrapped `String` is the redacted display
    /// message from `StoreError`; do not re-render it through
    /// `StoreError::Display` in callers.
    PushFailed(String),
    NoEntrySelected,
    /// The current edit session does not satisfy the validation rules in
    /// [`EntrySession::is_valid`](crate::app::state::EntrySession::is_valid).
    /// Returned by mutation paths so they cannot persist incoherent data
    /// even if the UI layer forgets to gate the save button.
    InvalidEntry,
}

impl From<std::io::Error> for AppError {
    fn from(err: io::Error) -> Self {
        AppError::Io(err)
    }
}

impl From<StoreError> for AppError {
    fn from(err: StoreError) -> Self {
        match err {
            // Promote PushFailed so the UI can show "saved locally, push
            // failed" without pattern-matching back through StoreError.
            StoreError::PushFailed(msg) => AppError::PushFailed(msg),
            other => AppError::Save(other),
        }
    }
}

// `Display` is intentionally redacted — see StoreError's Display
// comment. Logs that may surface to the user (journalctl, `--log`
// output) MUST use `{err}` (Display); the auto-derived `Debug` for
// `Save(StoreError)` re-expands paths, gpgme stderr, and libgit2
// messages that Display deliberately drops. Reserve `{err:?}` for
// ad-hoc developer inspection.
impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::Io(err) => write!(f, "I/O error ({})", err.kind()),
            AppError::Save(err) => write!(f, "{err}"),
            AppError::PushFailed(_) => {
                f.write_str("Saved locally but failed to push to remote")
            }
            AppError::NoEntrySelected => f.write_str("No entry selected"),
            AppError::InvalidEntry => f.write_str("Entry is not valid"),
        }
    }
}
