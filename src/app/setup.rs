//! Worker-safe wrappers for the setup wizard.
//!
//! Plan checks and steps touch gpgme, the filesystem and the network, so
//! the wizard runs them on `gio::spawn_blocking` threads. Opening the new
//! vault afterwards happens back on the main loop through
//! [`AppController::open_new_vault`](crate::app::AppController::open_new_vault).

use std::path::Path;

use crate::app::app_error::AppError;
use crate::pass::store::{self, GpgRecipient, SetupPlan, SetupStep, StoreStatus};

pub fn check_plan(plan: &SetupPlan) -> Result<(), AppError> {
    Ok(store::check_setup_plan(plan)?)
}

pub fn run_step(plan: &SetupPlan, step: SetupStep) -> Result<(), AppError> {
    Ok(store::run_setup_step(plan, step)?)
}

/// Lists usable local encryption keys; an empty list when there are none
/// or gpgme cannot be reached (the reason is logged).
pub fn available_recipients() -> Vec<GpgRecipient> {
    store::available_recipients().unwrap_or_else(|err| {
        log::warn!("Failed to list GPG recipients: {err}");
        Vec::new()
    })
}

/// Classifies a folder typed into the wizard.
pub fn folder_status(dir: &Path) -> StoreStatus {
    store::check_store(dir)
}
