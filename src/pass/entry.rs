use std::{fs, path::PathBuf};

use crate::{
    helpers::{git, pgp},
    pass::store::PassNode,
};

#[derive(Debug, Clone)]
pub struct EntryData {
    pub node: PassNode,
    pub password: String,
    pub fields: Vec<(String, String)>,
}

impl From<&EntryData> for String {
    fn from(entry: &EntryData) -> Self {
        let mut out = String::new();
        out.push_str(&entry.password);
        out.push('\n');
        for (key, value) in &entry.fields {
            if key.trim().is_empty() && value.trim().is_empty() {
                continue;
            }
            out.push_str(key);
            out.push_str(": ");
            out.push_str(value);
            out.push('\n');
        }

        out
    }
}

#[derive(Debug)]
pub enum SaveEntryError {
    Io(std::io::Error),
    Git(git::GitError),
    Pgp(pgp::PgpError),
    MissingParent(PathBuf),
}

impl std::fmt::Display for SaveEntryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveEntryError::Io(err) => write!(f, "Io error: {err}"),
            SaveEntryError::Git(err) => write!(f, "Git error: {err}"),
            SaveEntryError::Pgp(err) => write!(f, "Gpg error: {err}"),
            SaveEntryError::MissingParent(path) => {
                write!(f, "Missing parent directory for path: {}", path.display())
            }
        }
    }
}

impl std::error::Error for SaveEntryError {}

impl From<std::io::Error> for SaveEntryError {
    fn from(e: std::io::Error) -> Self {
        SaveEntryError::Io(e)
    }
}

impl From<git::GitError> for SaveEntryError {
    fn from(e: git::GitError) -> Self {
        SaveEntryError::Git(e)
    }
}

impl From<pgp::PgpError> for SaveEntryError {
    fn from(e: pgp::PgpError) -> Self {
        SaveEntryError::Pgp(e)
    }
}

pub fn save_entry_data(entry: &EntryData) -> Result<(), SaveEntryError> {
    let plaintext: String = entry.into();
    let store_dir = password_store_dir();
    let parent = entry
        .node
        .path
        .parent()
        .ok_or_else(|| SaveEntryError::MissingParent(entry.node.path.clone()))?;
    //encrypt
    let recipient_ids = pgp::recipient_ids(&store_dir)?;
    let encrypted = pgp::encrypt(&plaintext, &recipient_ids)?;
    //save
    fs::create_dir_all(parent)?;
    let output_path = store_dir.join(&entry.node.path);
    fs::write(&output_path, encrypted)?;
    //git
    git::add(&store_dir, &output_path)?;
    let message = format!("Add/update entry {}", entry.node.name);
    git::commit(&store_dir, &message)?;
    git::push(&store_dir)?;
    //end
    Ok(())
}

pub fn load_entry_from_node(node: &PassNode) -> Result<EntryData, String> {
    let path = password_store_dir().join(&node.path);
    let content = pgp::decrypt(&path).map_err(|e| e.to_string())?;
    let mut lines = content.lines();
    let password = lines
        .next()
        .ok_or_else(|| "Empty pass entry".to_string())?
        .to_string();
    let mut fields = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            fields.push((k.trim().to_string(), v.trim().to_string()));
        } else {
            fields.push(("note".to_string(), trimmed.to_string()));
        }
    }
    //Returns
    Ok(EntryData {
        node: node.clone(),
        password,
        fields,
    })
}

pub fn password_store_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("PASSWORD_STORE_DIR") {
        PathBuf::from(dir)
    } else {
        let home = std::env::var("HOME").expect("HOME not set");
        PathBuf::from(home).join(".password-store")
    }
}
