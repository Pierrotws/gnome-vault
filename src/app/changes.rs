//! Worker-safe wrappers over change-history store operations.
//!
//! These run on `gio::spawn_blocking` threads, so they take owned/borrowed
//! primitives instead of borrowing the controller. Post-op state cleanup
//! (cache eviction, session reset, tree reload) happens back on the main
//! loop via [`AppController::clear_loaded_entry_state`] and
//! [`AppController::reload_tree`].

use crate::app::app_error::AppError;
use crate::pass::store;

pub fn revert(commit_id: &str, autopush: bool) -> Result<(), AppError> {
    Ok(store::revert_change(commit_id, autopush)?)
}

pub fn rollback(commit_id: &str) -> Result<String, AppError> {
    Ok(store::rollback_to_change(commit_id)?)
}

pub fn push(branch: Option<&str>) -> Result<(), AppError> {
    Ok(store::push_changes(branch)?)
}
