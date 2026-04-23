//This file defines application-level errors.

use std::io;

use crate::pass::store::StoreError;

pub enum AppError {
    Io(io::Error),
    Save(StoreError),
    NoEntrySelected,
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
        }
    }
}
