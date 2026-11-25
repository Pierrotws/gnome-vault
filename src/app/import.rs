//! Worker-safe wrappers for importing another password manager's vault:
//! reading (and decrypting) the file, then writing its entries into the
//! open vault. Both run on `gio::spawn_blocking` threads.

use std::path::Path;

use zeroize::Zeroizing;

use crate::app::app_error::AppError;
use crate::pass::import::{self, ImportSource, ImportedItem};
use crate::pass::store::{self, ImportSummary};

/// Decrypts the file with its master password. The error is a message
/// for the user (wrong password, not a vault of that kind…).
pub fn read_file(
    source: ImportSource,
    path: &Path,
    password: &Zeroizing<String>,
) -> Result<Vec<ImportedItem>, String> {
    import::read_vault_file(source, path, password).map_err(|err| {
        log::debug!("import read failed: {err}");
        err.to_string()
    })
}

/// Writes the items under `destination` in the vault, in one commit.
pub fn write(
    store_dir: &Path,
    destination: &Path,
    items: &[ImportedItem],
    source: ImportSource,
    autopush: bool,
) -> Result<ImportSummary, AppError> {
    let message = format!("Import {} entries from {}", items.len(), source.name());
    Ok(store::import_entries(
        store_dir,
        destination,
        items,
        &message,
        autopush,
    )?)
}

/// Number of distinct groups among the items, for the preview.
pub fn group_count(items: &[ImportedItem]) -> usize {
    let mut groups: Vec<&[String]> = items
        .iter()
        .map(|item| item.groups.as_slice())
        .filter(|groups| !groups.is_empty())
        .collect();
    groups.sort();
    groups.dedup();
    groups.len()
}
