//This file defines application-level errors.

use std::io;

use crate::pass::store::StoreError;

#[derive(Debug)]
pub enum AppError {
    Io(io::Error),
    Save(StoreError),
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
        AppError::Save(err)
    }
}

// `Display` is intentionally redacted — see StoreError's Display
// comment. Use `{err:?}` (Debug) for log diagnostics.
impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::Io(err) => write!(f, "I/O error ({})", err.kind()),
            AppError::Save(err) => write!(f, "{err}"),
            AppError::NoEntrySelected => f.write_str("No entry selected"),
            AppError::InvalidEntry => f.write_str("Entry is not valid"),
        }
    }
}
