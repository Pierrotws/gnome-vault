//! A vault kept on Google Drive, for the desktop app (the Android app
//! talks to Drive in Kotlin, with the same steps): one bundle file per
//! vault, merged and uploaded through any [`DriveApi`] (HTTP in the app,
//! a fake in tests).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::helpers::git;
use crate::i18n::gettext;
use crate::pass::store::{
    self, CopyMerge, SetupPlan, SetupStep, StoreError, SyncTarget, DRIVE_FILE_KEY, DRIVE_STORAGE,
    DRIVE_VERSION_KEY,
};

/// Name suffix of a vault file on Drive: `Passwords.bundle`.
pub const SUFFIX: &str = ".bundle";

/// Syncs retried when another device uploads at the same time.
const ATTEMPTS: usize = 3;

/// A vault file on Google Drive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveFile {
    pub id: String,
    pub name: String,
    /// Changes whenever the file does.
    pub version: String,
    /// RFC 3339, e.g. `2026-11-27T10:58:18.000Z`.
    pub modified_time: Option<String>,
}

impl DriveFile {
    /// `Passwords.bundle` → `Passwords`.
    pub fn vault_name(&self) -> &str {
        self.name.strip_suffix(SUFFIX).unwrap_or(&self.name)
    }
}

#[derive(Debug)]
pub enum DriveError {
    /// No Google account is signed in, or its access was withdrawn.
    SignIn,
    /// This build has no Google client to sign in with.
    NotConfigured,
    /// Drive refused a request even with a fresh sign-in.
    Refused,
    Full,
    Unreachable,
    /// Another device kept uploading during the sync.
    Changed,
    Store(StoreError),
}

impl std::fmt::Display for DriveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            DriveError::SignIn => gettext("Sign in to Google Drive again"),
            DriveError::NotConfigured => gettext("Google Drive is not available in this build"),
            DriveError::Refused => gettext("Google Drive refused access"),
            DriveError::Full => gettext("Your Google Drive is full"),
            DriveError::Unreachable => gettext("Could not reach Google Drive"),
            DriveError::Changed => gettext("Google Drive changed during the sync: sync again"),
            DriveError::Store(err) => return err.fmt(f),
        };
        f.write_str(&message)
    }
}

impl std::error::Error for DriveError {}

impl From<StoreError> for DriveError {
    fn from(err: StoreError) -> Self {
        DriveError::Store(err)
    }
}

impl From<git::GitError> for DriveError {
    fn from(err: git::GitError) -> Self {
        DriveError::Store(StoreError::Git(err))
    }
}

/// The few Drive calls a vault needs; each blocks.
pub trait DriveApi {
    /// The vault files Passel made on this Drive, newest first.
    fn vaults(&self) -> Result<Vec<DriveFile>, DriveError>;
    /// The file's current state; `None` once deleted or trashed.
    fn file(&self, id: &str) -> Result<Option<DriveFile>, DriveError>;
    fn download(&self, id: &str, to: &Path) -> Result<(), DriveError>;
    /// Uploads a new vault file named after `vault_name`.
    fn create(&self, vault_name: &str, from: &Path) -> Result<DriveFile, DriveError>;
    /// Replaces the contents of file `id`.
    fn update(&self, id: &str, from: &Path) -> Result<DriveFile, DriveError>;
}

/// Exchanges the vault at `store_dir` with its Drive file: downloads and
/// merges it when it changed, then uploads the vault when Drive lacks
/// some of its changes (a vault never uploaded, or whose file was
/// deleted, gets a new file named `vault_name`). Another device
/// uploading in between is caught by checking the file's version just
/// before uploading. Temporary files go in `scratch`.
pub fn sync(
    store_dir: &Path,
    vault_name: &str,
    api: &dyn DriveApi,
    scratch: &Path,
) -> Result<CopyMerge, DriveError> {
    let mut merged = CopyMerge::default();
    for _ in 0..ATTEMPTS {
        let file_id = git::local_config(store_dir, DRIVE_FILE_KEY);
        let known_version = git::local_config(store_dir, DRIVE_VERSION_KEY);
        let current = match &file_id {
            Some(id) => api.file(id)?,
            None => None,
        };
        if let Some(current) = current
            .as_ref()
            .filter(|c| Some(&c.version) != known_version.as_ref())
        {
            let merge = with_temp_file(scratch, |bundle| {
                api.download(&current.id, bundle)?;
                Ok(store::merge_vault_bundle(store_dir, bundle, DRIVE_STORAGE)?)
            })?;
            git::set_local_config(store_dir, DRIVE_VERSION_KEY, &current.version)?;
            merged.report.received += merge.report.received;
            merged.kept_copies.extend(merge.kept_copies);
        }
        let unsent = store::copy_unsent_count(store_dir, DRIVE_STORAGE)?;
        merged.report.sent = unsent;
        if current.is_some() && unsent == 0 {
            return Ok(merged);
        }
        let uploaded = with_temp_file(scratch, |bundle| {
            store::write_vault_bundle(store_dir, bundle)?;
            match &current {
                None => api.create(vault_name, bundle).map(Some),
                // Changed meanwhile: merge that first.
                Some(current)
                    if api.file(&current.id)?.as_ref().map(|f| &f.version)
                        != Some(&current.version) =>
                {
                    Ok(None)
                }
                Some(current) => api.update(&current.id, bundle).map(Some),
            }
        })?;
        if let Some(file) = uploaded {
            uploaded_to(store_dir, &file)?;
            return Ok(merged);
        }
    }
    Err(DriveError::Changed)
}

/// Makes a vault at `dir` (missing or empty) from the Drive file `file`.
pub fn download(
    api: &dyn DriveApi,
    file: &DriveFile,
    dir: &Path,
    scratch: &Path,
) -> Result<(), DriveError> {
    with_temp_file(scratch, |bundle| {
        api.download(&file.id, bundle)?;
        git::clone_bundle(bundle, dir)?;
        Ok(())
    })?;
    if !dir.join(".gpg-id").is_file() {
        return Err(git::GitError::NotAVault.into());
    }
    store::set_storage(dir, DRIVE_STORAGE)?;
    uploaded_to(dir, file)?;
    Ok(())
}

/// Runs the steps of a Google Drive plan that reach Drive (the others
/// run through [`store::run_setup_step`]).
pub fn run_setup_step(
    plan: &SetupPlan,
    step: SetupStep,
    api: &dyn DriveApi,
    scratch: &Path,
) -> Result<(), DriveError> {
    let dir = plan.store_dir.as_path();
    match (step, &plan.sync) {
        (SetupStep::Download, Some(SyncTarget::Drive(Some(id)))) => {
            let file = api.file(id)?.ok_or(git::GitError::NotAVault)?;
            let downloaded = download(api, &file, dir, scratch);
            if downloaded.is_err() && dir.exists() {
                // Leave the folder empty, as before the step.
                if let Err(err) = store::remove_contents(dir) {
                    log::warn!("could not clean a failed download: {}", err.kind());
                }
            }
            downloaded
        }
        (SetupStep::Upload, Some(SyncTarget::Drive(_))) => {
            let name = dir
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Passwords".to_string());
            sync(dir, &name, api, scratch).map(|_| ())
        }
        _ => Ok(store::run_setup_step(plan, step)?),
    }
}

/// True when the plan has a step that reaches Google Drive.
pub fn is_drive_step(plan: &SetupPlan, step: SetupStep) -> bool {
    matches!(plan.sync, Some(SyncTarget::Drive(_)))
        && matches!(step, SetupStep::Upload | SetupStep::Download)
}

/// The vault now matches Drive file `file`.
fn uploaded_to(dir: &Path, file: &DriveFile) -> Result<(), DriveError> {
    git::set_local_config(dir, DRIVE_FILE_KEY, &file.id)?;
    git::set_local_config(dir, DRIVE_VERSION_KEY, &file.version)?;
    store::copy_written(dir, DRIVE_STORAGE)?;
    Ok(())
}

/// Runs `block` with a fresh path in `scratch`, removed afterwards.
fn with_temp_file<T>(
    scratch: &Path,
    block: impl FnOnce(&Path) -> Result<T, DriveError>,
) -> Result<T, DriveError> {
    std::fs::create_dir_all(scratch).map_err(|e| DriveError::Store(StoreError::Io(e)))?;
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let path: PathBuf = scratch.join(format!("passel-{}-{unique}{SUFFIX}", std::process::id()));
    let result = block(&path);
    let _ = std::fs::remove_file(&path);
    result
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::fs;

    use super::*;

    /// Drive in memory: files by id, each version a counter.
    #[derive(Default)]
    struct FakeDrive {
        files: RefCell<HashMap<String, (DriveFile, Vec<u8>)>>,
        /// Uploads made "by another device" right before the next update.
        race: RefCell<usize>,
    }

    impl FakeDrive {
        fn put(&self, id: &str, name: &str, bytes: Vec<u8>) -> DriveFile {
            let mut files = self.files.borrow_mut();
            let version = files
                .get(id)
                .map(|(file, _)| file.version.parse::<u32>().unwrap() + 1)
                .unwrap_or(1);
            let file = DriveFile {
                id: id.to_string(),
                name: name.to_string(),
                version: version.to_string(),
                modified_time: None,
            };
            files.insert(id.to_string(), (file.clone(), bytes));
            file
        }
    }

    impl DriveApi for FakeDrive {
        fn vaults(&self) -> Result<Vec<DriveFile>, DriveError> {
            Ok(self
                .files
                .borrow()
                .values()
                .map(|(f, _)| f.clone())
                .collect())
        }
        fn file(&self, id: &str) -> Result<Option<DriveFile>, DriveError> {
            if *self.race.borrow() > 0 {
                *self.race.borrow_mut() -= 1;
                let (file, bytes) = self.files.borrow()[id].clone();
                self.put(id, &file.name, bytes);
            }
            Ok(self.files.borrow().get(id).map(|(f, _)| f.clone()))
        }
        fn download(&self, id: &str, to: &Path) -> Result<(), DriveError> {
            fs::write(to, &self.files.borrow()[id].1).unwrap();
            Ok(())
        }
        fn create(&self, vault_name: &str, from: &Path) -> Result<DriveFile, DriveError> {
            let id = format!("id{}", self.files.borrow().len() + 1);
            Ok(self.put(
                &id,
                &format!("{vault_name}{SUFFIX}"),
                fs::read(from).unwrap(),
            ))
        }
        fn update(&self, id: &str, from: &Path) -> Result<DriveFile, DriveError> {
            let name = self.files.borrow()[id].0.name.clone();
            Ok(self.put(id, &name, fs::read(from).unwrap()))
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "passel-drive-{name}-{}-{unique}",
            std::process::id()
        ))
    }

    fn vault(dir: &Path, file: &str, contents: &str) {
        if !dir.join(".git").exists() {
            git::init(dir).unwrap();
            let repo = git2::Repository::open(dir).unwrap();
            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Tests").unwrap();
            config.set_str("user.email", "t@example.invalid").unwrap();
            fs::write(dir.join(".gpg-id"), "KEY\n").unwrap();
            git::add(dir, &dir.join(".gpg-id")).unwrap();
            git::commit(dir, "Initialize vault").unwrap();
            store::set_storage(dir, DRIVE_STORAGE).unwrap();
        }
        fs::write(dir.join(file), contents).unwrap();
        git::add(dir, &dir.join(file)).unwrap();
        git::commit(dir, &format!("Edit {file}")).unwrap();
    }

    #[test]
    fn two_computers_share_a_vault_through_drive() {
        let root = temp_dir("share");
        let scratch = root.join("tmp");
        let drive = FakeDrive::default();
        let desk = root.join("desk");
        vault(&desk, "bank.gpg", "desk v1");
        assert_eq!(store::unpushed_count(&desk, None).unwrap(), 2);

        // First sync: a new Drive file.
        let first = sync(&desk, "Passwords", &drive, &scratch).unwrap();
        assert_eq!(first.report.sent, 2);
        assert_eq!(store::unpushed_count(&desk, None).unwrap(), 0);
        let file = drive.vaults().unwrap().pop().unwrap();
        assert_eq!(file.vault_name(), "Passwords");

        // Another computer downloads it and changes the same entry.
        let laptop = root.join("laptop");
        download(&drive, &file, &laptop, &scratch).unwrap();
        git::set_identity(&laptop, "Laptop", "l@example.invalid").unwrap();
        assert_eq!(
            fs::read_to_string(laptop.join("bank.gpg")).unwrap(),
            "desk v1"
        );
        vault(&laptop, "bank.gpg", "laptop v2");
        sync(&laptop, "Passwords", &drive, &scratch).unwrap();

        // Meanwhile the first one changed it too: both versions kept.
        vault(&desk, "bank.gpg", "desk v2");
        let merged = sync(&desk, "Passwords", &drive, &scratch).unwrap();
        assert_eq!(merged.report.received, 1);
        assert_eq!(merged.kept_copies, vec!["bank (conflict).gpg"]);
        assert_eq!(
            fs::read_to_string(desk.join("bank.gpg")).unwrap(),
            "laptop v2"
        );
        assert_eq!(store::unpushed_count(&desk, None).unwrap(), 0, "uploaded");

        // Nothing new: nothing downloaded or uploaded.
        let version = drive.file(&file.id).unwrap().unwrap().version;
        let idle = sync(&desk, "Passwords", &drive, &scratch).unwrap();
        assert_eq!((idle.report.received, idle.report.sent), (0, 0));
        assert_eq!(drive.file(&file.id).unwrap().unwrap().version, version);

        // Another device uploading just before ours: merged, then sent.
        vault(&laptop, "mail.gpg", "x");
        *drive.race.borrow_mut() = 1;
        let raced = sync(&laptop, "Passwords", &drive, &scratch).unwrap();
        assert!(raced.report.sent >= 1);
        assert_eq!(store::unpushed_count(&laptop, None).unwrap(), 0);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_drive_file_that_is_not_a_vault_is_refused() {
        let root = temp_dir("junk");
        let drive = FakeDrive::default();
        let file = drive.put("junk", "Junk.bundle", b"not a bundle".to_vec());
        let dir = root.join("vault");
        assert!(download(&drive, &file, &dir, &root.join("tmp")).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
