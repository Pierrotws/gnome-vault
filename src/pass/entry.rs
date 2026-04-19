use std::path::PathBuf;

use crate::{helpers::gpg, pass::store::PassNode};

#[derive(Debug, Clone)]
pub struct EntryData {
    pub node: PassNode,
    pub password: String,
    pub fields: Vec<(String, String)>,
}

pub fn save_entry_data(entry: &EntryData) -> Result<(), Box<dyn std::error::Error>> {
    println!("Saving entry at {}", entry.node.path.display());
    println!("  password: {}", entry.password);
    println!("Saved.");
    //Err("Someting failed".into())
    Ok(())
}

pub fn load_entry_from_node(node: &PassNode) -> Result<EntryData, String> {
    let path = password_store_dir().join(&node.path);
    let content = gpg::decrypt(&path)?;
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
