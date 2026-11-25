//! Writing entries read from another password manager into a vault, in
//! one commit.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::helpers::{git, parser, pgp};
use crate::pass::import::ImportedItem;

use super::{ensure_inside_store, has_history, push_if_synced, write_atomically, StoreError};

/// What an import wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportSummary {
    pub written: usize,
    /// Entries whose name was taken and got a number appended.
    pub renamed: usize,
}

/// Where each item lands: `destination/<groups…>/<title>.gpg`, relative
/// to the vault. Names are made valid for pass (no `/`, no leading dot,
/// which would hide them), and a name already used on disk or earlier
/// in the import gets ` (2)`, ` (3)`…
pub fn plan_import(
    store_dir: &Path,
    destination: &Path,
    items: &[ImportedItem],
) -> (Vec<PathBuf>, usize) {
    let mut taken: HashSet<PathBuf> = HashSet::new();
    let mut renamed = 0;
    let paths = items
        .iter()
        .map(|item| {
            let mut folder = destination.to_path_buf();
            for group in &item.groups {
                folder.push(safe_name(group, "Group"));
            }
            let base = safe_name(&item.title, "Untitled");
            let free = |name: &str| {
                let path = folder.join(format!("{name}.gpg"));
                !taken.contains(&path) && !store_dir.join(&path).exists()
            };
            let name = if free(&base) {
                base
            } else {
                renamed += 1;
                (2..)
                    .map(|n| format!("{base} ({n})"))
                    .find(|name| free(name))
                    .expect("an unused name exists")
            };
            let path = folder.join(format!("{name}.gpg"));
            taken.insert(path.clone());
            path
        })
        .collect();
    (paths, renamed)
}

/// A file or folder name for pass: path separators and control
/// characters replaced, leading dots and dashes dropped.
fn safe_name(name: &str, fallback: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_start_matches(['.', '-']).trim();
    if cleaned.is_empty() || cleaned == ".gpg-id" {
        fallback.to_string()
    } else {
        cleaned.to_string()
    }
}

/// Encrypts and writes every item under `destination` (a folder of the
/// vault), then records them in one commit and pushes when the vault
/// syncs. If writing fails, the files written so far are removed.
pub fn import_entries(
    store_dir: &Path,
    destination: &Path,
    items: &[ImportedItem],
    message: &str,
    autopush: bool,
) -> Result<ImportSummary, StoreError> {
    super::validate_folder_path(destination)?;
    let (paths, renamed) = plan_import(store_dir, destination, items);

    let mut written: Vec<PathBuf> = Vec::new();
    let result = (|| -> Result<(), StoreError> {
        for (item, relative) in items.iter().zip(&paths) {
            let output = store_dir.join(relative);
            let parent = output
                .parent()
                .ok_or_else(|| StoreError::MissingParent(output.clone()))?;
            fs::create_dir_all(parent)?;
            ensure_inside_store(store_dir, parent)?;
            let recipients = pgp::recipient_ids_for(store_dir, parent)?;
            let ciphertext = pgp::encrypt(&parser::format_entry(&item.entry), &recipients)?;
            write_atomically(&output.with_extension("gpg.tmp"), &output, &ciphertext)?;
            written.push(output);
        }
        if has_history(store_dir) && !written.is_empty() {
            for path in &written {
                git::add(store_dir, path)?;
            }
            git::commit(store_dir, message)?;
        }
        Ok(())
    })();

    if let Err(err) = result {
        for path in &written {
            if let Err(remove_err) = fs::remove_file(path) {
                log::warn!("Failed to remove a partly imported entry: {remove_err}");
            }
            if has_history(store_dir) {
                let _ = git::remove(store_dir, path);
            }
        }
        return Err(err);
    }

    // The entries are committed: a failed upload must not undo them.
    if let Err(err) = push_if_synced(store_dir, autopush) {
        return Err(StoreError::PushFailed(err.to_string()));
    }
    Ok(ImportSummary {
        written: written.len(),
        renamed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pass::model::{EntryData, EntryField};

    fn item(groups: &[&str], title: &str) -> ImportedItem {
        ImportedItem {
            groups: groups.iter().map(|group| group.to_string()).collect(),
            title: title.to_string(),
            entry: EntryData {
                password: EntryField::Password("x".into()),
                fields: Vec::new(),
            },
        }
    }

    #[test]
    fn plans_valid_unique_paths() {
        let store = std::env::temp_dir().join(format!("passel-plan-import-{}", std::process::id()));
        fs::create_dir_all(store.join("Imported/Email")).unwrap();
        fs::write(store.join("Imported/Email/Mail.gpg"), "existing").unwrap();

        let items = [
            item(&["Email"], "Mail"),
            item(&["Email"], "Mail"),
            item(&["a/b"], ".hidden"),
            item(&[], "  "),
            item(&[".."], "-rf"),
        ];
        let (paths, renamed) = plan_import(&store, Path::new("Imported"), &items);
        assert_eq!(
            paths,
            vec![
                PathBuf::from("Imported/Email/Mail (2).gpg"),
                PathBuf::from("Imported/Email/Mail (3).gpg"),
                PathBuf::from("Imported/a-b/hidden.gpg"),
                PathBuf::from("Imported/Untitled.gpg"),
                PathBuf::from("Imported/Group/rf.gpg"),
            ]
        );
        assert_eq!(renamed, 2);
        fs::remove_dir_all(store).unwrap();
    }
}
