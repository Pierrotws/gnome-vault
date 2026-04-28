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

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::Io(err) => write!(f, "Io error: {err}"),
            AppError::Save(err) => write!(f, "save error: {err}"),
            AppError::NoEntrySelected => write!(f, "No entry selected"),
            AppError::InvalidEntry => write!(f, "Entry is not valid"),
        }
    }
}
