//! Worker-safe wrappers for the setup wizard.
//!
//! Plan checks and steps touch gpgme, the filesystem and the network, so
//! the wizard runs them on `gio::spawn_blocking` threads. Opening the new
//! vault afterwards happens back on the main loop through
//! [`AppController::open_new_vault`](crate::app::AppController::open_new_vault).

use std::fs;
use std::path::{Path, PathBuf};

use crate::app::app_error::AppError;
use crate::pass::store::{
    self, GpgRecipient, RemoteCheck, SetupPlan, SetupStep, StoreError, StoreStatus,
};

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

/// Connects read-only to a remote address (network: run off the main
/// loop).
pub fn check_remote(remote_url: &str) -> RemoteCheck {
    store::check_remote(remote_url)
}

/// Whether a vault copy can be created at `copy_dir`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyDirStatus {
    Usable,
    /// Inside the vault folder: it would be committed into itself.
    InsideVault,
    /// Something is already there.
    Taken,
}

pub fn copy_dir_status(store_dir: &Path, copy_dir: &Path) -> CopyDirStatus {
    match store::check_copy_dir(store_dir, copy_dir) {
        Ok(()) => CopyDirStatus::Usable,
        Err(StoreError::InvalidFolderPath(_)) => CopyDirStatus::InsideVault,
        Err(_) => CopyDirStatus::Taken,
    }
}

/// A public SSH key found in `~/.ssh`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshPublicKey {
    pub path: PathBuf,
    pub text: String,
}

impl SshPublicKey {
    pub fn is_rsa(&self) -> bool {
        self.text.starts_with("ssh-rsa ")
    }
}

/// The user's default public SSH keys, in the order `ssh` tries them.
pub fn ssh_public_keys() -> Vec<SshPublicKey> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    ["id_ed25519.pub", "id_ecdsa.pub", "id_rsa.pub"]
        .iter()
        .filter_map(|name| {
            let path = home.join(".ssh").join(name);
            let text = fs::read_to_string(&path).ok()?.trim().to_string();
            (!text.is_empty()).then_some(SshPublicKey { path, text })
        })
        .collect()
}
