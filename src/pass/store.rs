use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum PassNodeKind {
    Group,
    Entry,
}

#[derive(Debug, Clone)]
pub struct PassNode {
    pub name: String,
    pub path: PathBuf,
    pub kind: PassNodeKind,
    pub children: Vec<PassNode>,
}

impl PassNode {
    pub fn is_group(&self) -> bool {
        matches!(self.kind, PassNodeKind::Group)
    }

    pub fn is_entry(&self) -> bool {
        matches!(self.kind, PassNodeKind::Entry)
    }
}

pub fn load_password_store() -> io::Result<Vec<PassNode>> {
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .map_err(|e| io::Error::new(io::ErrorKind::NotFound, format!("HOME not set: {e}")))?;

    let store_dir = home.join(".password-store");
    eprintln!("Loading password store from: {}", store_dir.display());

    let nodes = load_dir(&store_dir, &store_dir)?;
    eprintln!("Top-level nodes: {}", nodes.len());

    Ok(nodes)
}

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

    Ok(nodes)
}

fn relative_path(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root).unwrap_or(path).to_path_buf()
}
