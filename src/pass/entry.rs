use gpgme::{Context, Data, Protocol};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct EntryData {
    pub password: String,
    pub fields: Vec<(String, String)>,
}

pub fn load_entry_from_gpg_file(rel_path: &Path) -> Result<EntryData, String> {
    let path = password_store_dir().join(rel_path);
    let mut ctx = Context::from_protocol(Protocol::OpenPgp)
        .map_err(|e| format!("Failed to initialize GPGME OpenPGP context: {e}"))?;

    let path_str = path
        .to_str()
        .ok_or_else(|| format!("Non-UTF-8 path: {}", path.display()))?;

    let mut cipher = Data::load(path_str)
        .map_err(|e| format!("Failed to open encrypted file {}: {e}", path.display()))?;

    let mut plain = Vec::<u8>::new();

    ctx.decrypt(&mut cipher, &mut plain)
        .map_err(|e| format!("Failed to decrypt {}: {e}", path.display()))?;

    let content = String::from_utf8(plain).map_err(|e| {
        format!(
            "Decrypted content is not valid UTF-8 for {}: {e}",
            path.display()
        )
    })?;

    parse_pass_entry(&content)
}

fn parse_pass_entry(content: &str) -> Result<EntryData, String> {
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

    Ok(EntryData { password, fields })
}

pub fn password_store_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("PASSWORD_STORE_DIR") {
        PathBuf::from(dir)
    } else {
        let home = std::env::var("HOME").expect("HOME not set");
        PathBuf::from(home).join(".password-store")
    }
}
