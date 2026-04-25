//Backend operations as functions.

mod store_error;

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::{
    helpers::{git, parser, pgp},
    pass::model::{EntryData, PassNode, PassNodeKind},
};

pub use store_error::StoreError;

//recursive read function
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
    eprintln!("Loading password store from: {}", store_dir.display());
    let nodes = load_dir(&store_dir, &store_dir)?;
    eprintln!("Top-level nodes: {}", nodes.len());
    //Returns
    Ok(nodes)
}

pub fn load_entry_from_node(node: &PassNode) -> Result<EntryData, StoreError> {
    let path = password_store_dir().join(&node.path);
    let content = pgp::decrypt(&path)?;
    parser::parse_entry(&content).ok_or_else(|| StoreError::EmptyFile(path))
}

pub fn save_entry_data(node: &PassNode, entry: &EntryData) -> Result<(), StoreError> {
    let plaintext = parser::format_entry(entry);
    let store_dir = password_store_dir();
    let output_path = store_dir.join(&node.path);
    let parent = output_path
        .parent()
        .ok_or_else(|| StoreError::MissingParent(output_path.clone()))?;
    //encrypt
    let recipient_ids = pgp::recipient_ids(&store_dir)?;
    let encrypted = pgp::encrypt(&plaintext, &recipient_ids)?;
    //save
    fs::create_dir_all(parent)?;
    fs::write(&output_path, encrypted)?;
    //git
    git::add(&store_dir, &output_path)?;
    let message = format!("Add/update entry {}", node.name);
    git::commit(&store_dir, &message)?;
    git::push(&store_dir)?;
    //end
    Ok(())
}

pub fn password_store_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("PASSWORD_STORE_DIR") {
        PathBuf::from(dir)
    } else {
        let home = std::env::var("HOME").expect("HOME not set");
        PathBuf::from(home).join(".password-store")
    }
}
