//Backend operations as functions.

mod store_error;

use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

use crate::{
    helpers::{git, parser, pgp},
    pass::model::{EntryData, PassNode, PassNodeKind},
};

pub use store_error::StoreError;

/// Recursively loads password-store groups and `.gpg` entries.
fn load_dir(root: &Path, dir: &Path) -> io::Result<Vec<PassNode>> {
    let mut nodes = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let _name = entry.file_name();
        let file_name = _name.to_string_lossy();
        // IGNORER les fichiers/dossiers cachés
        if file_name.starts_with('.') {
            continue;
        }
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            let children = load_dir(root, &path)?;

            nodes.push(PassNode {
                name: entry.file_name().to_string_lossy().into_owned(),
                path: relative_path(root, &path),
                kind: PassNodeKind::Group,
                children,
            });
        } else if file_type.is_file() {
            if path.extension().and_then(|e| e.to_str()) == Some("gpg") {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();

                nodes.push(PassNode {
                    name: stem,
                    path: relative_path(root, &path),
                    kind: PassNodeKind::Entry,
                    children: Vec::new(),
                });
            }
        }
    }
    nodes.sort_by(|a, b| match (&a.kind, &b.kind) {
        (PassNodeKind::Group, PassNodeKind::Entry) => std::cmp::Ordering::Less,
        (PassNodeKind::Entry, PassNodeKind::Group) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    //Returns
    Ok(nodes)
}

fn relative_path(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}

pub fn load_password_store() -> io::Result<Vec<PassNode>> {
    let store_dir = password_store_dir();
    log::debug!("Loading password store from: {}", store_dir.display());
    let nodes = load_dir(&store_dir, &store_dir)?;
    log::debug!("Top-level nodes: {}", nodes.len());
    //Returns
    Ok(nodes)
}

/// Loads and decrypts a password-store entry from a tree node.
pub fn load_entry_from_node(node: &PassNode) -> Result<EntryData, StoreError> {
    let path = password_store_dir().join(&node.path);
    let content = pgp::decrypt(&path)?;
    parser::parse_entry(&content).ok_or_else(|| StoreError::EmptyFile(path))
}

/// Deletes an entry file, commits the deletion, and pushes it.
pub fn delete_entry(node: &PassNode) -> Result<(), StoreError> {
    let store_dir = password_store_dir();
    let entry_path = store_dir.join(&node.path);

    fs::remove_file(&entry_path)?;
    git::remove(&store_dir, &entry_path)?;
    git::commit(&store_dir, &format!("Delete entry {}", node.name))?;
    git::push(&store_dir)?;

    Ok(())
}

/// Creates a new entry under a password-store folder.
pub fn create_entry_data(
    folder_path: &Path,
    name: &str,
    entry: &EntryData,
) -> Result<PassNode, StoreError> {
    validate_folder_path(folder_path)?;
    let name = valid_entry_name(name)?;
    let node = PassNode {
        name: name.to_string(),
        path: folder_path.join(format!("{name}.gpg")),
        kind: PassNodeKind::Entry,
        children: Vec::new(),
    };

    let store_dir = password_store_dir();
    let output_path = store_dir.join(&node.path);
    if output_path.exists() {
        return Err(StoreError::DestinationExists(output_path));
    }

    write_entry_data(&node, entry, &format!("Add entry {}", node.name))?;
    Ok(node)
}

/// Renames an entry file, commits the move, and pushes it.
pub fn rename_entry(node: &PassNode, new_name: &str) -> Result<PassNode, StoreError> {
    let new_name = valid_entry_name(new_name)?;

    if node.name == new_name {
        return Ok(node.clone());
    }

    let store_dir = password_store_dir();
    let old_path = store_dir.join(&node.path);
    let new_relative_path = node.path.with_file_name(format!("{new_name}.gpg"));
    let new_path = store_dir.join(&new_relative_path);

    if new_path.exists() {
        return Err(StoreError::DestinationExists(new_path));
    }

    fs::rename(&old_path, &new_path)?;
    git::rename(&store_dir, &old_path, &new_path)?;
    git::commit(&store_dir, "rename entry")?;
    git::push(&store_dir)?;

    Ok(PassNode {
        name: new_name.to_string(),
        path: new_relative_path,
        kind: node.kind.clone(),
        children: node.children.clone(),
    })
}

/// Encrypts, writes, commits, and pushes entry data for a tree node.
pub fn save_entry_data(node: &PassNode, entry: &EntryData) -> Result<(), StoreError> {
    let message = format!("Add/update entry {}", node.name);
    write_entry_data(node, entry, &message)
}

fn write_entry_data(node: &PassNode, entry: &EntryData, message: &str) -> Result<(), StoreError> {
    let plaintext = parser::format_entry(entry);
    let store_dir = password_store_dir();
    let output_path = store_dir.join(&node.path);
    let parent = output_path
        .parent()
        .ok_or_else(|| StoreError::MissingParent(output_path.clone()))?;
    let recipient_ids = pgp::recipient_ids(&store_dir)?;
    let encrypted = pgp::encrypt(&plaintext, &recipient_ids)?;
    fs::create_dir_all(parent)?;
    fs::write(&output_path, encrypted)?;
    git::add(&store_dir, &output_path)?;
    git::commit(&store_dir, message)?;
    git::push(&store_dir)?;
    Ok(())
}

fn valid_entry_name(name: &str) -> Result<&str, StoreError> {
    let name = name.trim();
    if name.is_empty() || name.contains('/') || name.contains('\\') {
        return Err(StoreError::InvalidEntryName(name.to_string()));
    }
    Ok(name)
}

fn validate_folder_path(path: &Path) -> Result<(), StoreError> {
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(StoreError::InvalidFolderPath(path.to_path_buf()));
    }
    Ok(())
}

/// Returns the active password-store directory.
///
/// Honors `PASSWORD_STORE_DIR` and falls back to `~/.password-store`.
pub fn password_store_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("PASSWORD_STORE_DIR") {
        PathBuf::from(dir)
    } else {
        let home = std::env::var("HOME").expect("HOME not set");
        PathBuf::from(home).join(".password-store")
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn validates_entry_names() {
        assert_eq!(valid_entry_name(" example ").unwrap(), "example");
        assert!(matches!(
            valid_entry_name(""),
            Err(StoreError::InvalidEntryName(_))
        ));
        assert!(matches!(
            valid_entry_name("folder/example"),
            Err(StoreError::InvalidEntryName(_))
        ));
        assert!(matches!(
            valid_entry_name("folder\\example"),
            Err(StoreError::InvalidEntryName(_))
        ));
    }

    #[test]
    fn validates_relative_folder_paths() {
        assert!(validate_folder_path(Path::new("")).is_ok());
        assert!(validate_folder_path(Path::new("email/work")).is_ok());
        assert!(matches!(
            validate_folder_path(Path::new("/email/work")),
            Err(StoreError::InvalidFolderPath(_))
        ));
        assert!(matches!(
            validate_folder_path(Path::new("../email")),
            Err(StoreError::InvalidFolderPath(_))
        ));
    }
}
