//! Buttercup vaults (`.bcup`).
//!
//! Buttercup has no published file format; this follows its sources
//! (buttercup-core `source/io/`, and the iocane library it encrypts with):
//!
//! ```text
//! file      = "b~>buttercup/" ("a" | "b") payload
//! payload   = content "$" iv "$" salt "$" auth ["$" rounds ["$" method]]
//!   content   base64 ciphertext
//!   iv        hex, 16 bytes
//!   salt      text, used as is
//!   auth      hex: HMAC-SHA256 (cbc) or the GCM tag (gcm)
//!   rounds    PBKDF2 iterations, 1000 when missing
//!   method    "cbc" (default) or "gcm"
//! keys      = PBKDF2-HMAC-SHA256(password, salt, rounds) → 64 bytes:
//!             AES-256 key, then HMAC key (cbc; gcm uses the first 32)
//! cbc       : HMAC(hmac key, content ‖ iv ‖ salt) = auth, then AES-256-CBC
//! gcm       : AES-256-GCM, 16-byte nonce, AAD = iv ‖ salt
//! plaintext : format b = base64(gzip(JSON vault))
//!             format a = gzip bytes as a "binary string" → history lines
//! ```
//!
//! Format b is what current Buttercup writes; format a is the older
//! command-history format.

use std::collections::HashMap;
use std::io::Read;

use aes::cipher::{block_padding::Pkcs7, BlockModeDecrypt, KeyIvInit};
use data_encoding::{BASE64, HEXLOWER_PERMISSIVE};
use pbkdf2::hmac::{Hmac, KeyInit, Mac};
use serde_json::Value;
use sha2::Sha256;
use zeroize::Zeroizing;

use super::{entry_from_fields, ImportError, ImportedItem};
use crate::i18n::gettext;

const SIGNATURE_A: &str = "b~>buttercup/a";
const SIGNATURE_B: &str = "b~>buttercup/b";
const DEFAULT_ROUNDS: u32 = 1000;
/// Upper bound on PBKDF2 rounds read from a file, so a crafted file
/// cannot make the import run for hours.
const MAX_ROUNDS: u32 = 10_000_000;
const TRASH_ROLE: (&str, &str) = ("bc_group_role", "trash");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    A,
    B,
}

pub(super) fn read(
    bytes: &[u8],
    password: &Zeroizing<String>,
) -> Result<Vec<ImportedItem>, ImportError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| unreadable(gettext("not a Buttercup vault")))?
        .trim();
    let (format, payload) = if let Some(payload) = text.strip_prefix(SIGNATURE_B) {
        (Format::B, payload)
    } else if let Some(payload) = text.strip_prefix(SIGNATURE_A) {
        (Format::A, payload)
    } else {
        return Err(unreadable(gettext("not a Buttercup vault")));
    };
    let plaintext = decrypt(payload, password)?;
    match format {
        Format::B => {
            let gzip = BASE64
                .decode(plaintext.trim().as_bytes())
                .map_err(|_| unreadable(gettext("unexpected content")))?;
            let json = gunzip(&gzip)?;
            read_format_b(&json)
        }
        Format::A => {
            // pako's gzip "to string" output: one char per byte.
            let gzip = plaintext
                .chars()
                .map(|c| u8::try_from(u32::from(c)))
                .collect::<Result<Vec<u8>, _>>()
                .map_err(|_| unreadable(gettext("unexpected content")))?;
            let history = gunzip(&gzip)?;
            Ok(read_format_a(&history))
        }
    }
}

fn unreadable(reason: String) -> ImportError {
    ImportError::Unreadable(reason)
}

struct Payload<'a> {
    content: &'a str,
    iv: &'a str,
    salt: &'a str,
    auth: &'a str,
    rounds: u32,
    gcm: bool,
}

fn parse_payload(payload: &str) -> Result<Payload<'_>, ImportError> {
    let parts: Vec<&str> = payload.split('$').collect();
    if !(4..=6).contains(&parts.len()) {
        return Err(unreadable(gettext("unexpected encryption format")));
    }
    let rounds = match parts.get(4).filter(|rounds| !rounds.is_empty()) {
        Some(rounds) => rounds
            .parse::<u32>()
            .ok()
            .filter(|rounds| (1..=MAX_ROUNDS).contains(rounds))
            .ok_or_else(|| unreadable(gettext("unexpected key derivation")))?,
        None => DEFAULT_ROUNDS,
    };
    let gcm = match parts.get(5).copied().unwrap_or("") {
        "" | "cbc" => false,
        "gcm" => true,
        _ => return Err(unreadable(gettext("unsupported encryption method"))),
    };
    Ok(Payload {
        content: parts[0],
        iv: parts[1],
        salt: parts[2],
        auth: parts[3],
        rounds,
        gcm,
    })
}

fn decrypt(payload: &str, password: &Zeroizing<String>) -> Result<Zeroizing<String>, ImportError> {
    let payload = parse_payload(payload)?;
    let ciphertext = BASE64
        .decode(payload.content.as_bytes())
        .map_err(|_| unreadable(gettext("damaged content")))?;
    let iv = HEXLOWER_PERMISSIVE
        .decode(payload.iv.as_bytes())
        .map_err(|_| unreadable(gettext("damaged content")))?;
    let auth = HEXLOWER_PERMISSIVE
        .decode(payload.auth.as_bytes())
        .map_err(|_| unreadable(gettext("damaged content")))?;

    let mut keys = Zeroizing::new([0u8; 64]);
    pbkdf2::pbkdf2_hmac::<Sha256>(
        password.as_bytes(),
        payload.salt.as_bytes(),
        payload.rounds,
        &mut keys[..],
    );
    let (key, hmac_key) = keys.split_at(32);

    let plain = if payload.gcm {
        decrypt_gcm(key, &iv, &ciphertext, &auth, payload)?
    } else {
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(hmac_key)
            .map_err(|_| unreadable(gettext("key derivation failed")))?;
        mac.update(payload.content.as_bytes());
        mac.update(payload.iv.as_bytes());
        mac.update(payload.salt.as_bytes());
        // A different password derives a different HMAC key: this is how
        // a wrong password shows up.
        mac.verify_slice(&auth)
            .map_err(|_| ImportError::WrongPassword)?;
        let iv: [u8; 16] = iv
            .as_slice()
            .try_into()
            .map_err(|_| unreadable(gettext("damaged content")))?;
        let key: [u8; 32] = key.try_into().expect("32-byte half of 64");
        Zeroizing::new(
            cbc::Decryptor::<aes::Aes256>::new(&key.into(), &iv.into())
                .decrypt_padded_vec::<Pkcs7>(&ciphertext)
                .map_err(|_| unreadable(gettext("damaged content")))?,
        )
    };
    let text =
        String::from_utf8(plain.to_vec()).map_err(|_| unreadable(gettext("unexpected content")))?;
    Ok(Zeroizing::new(text))
}

fn decrypt_gcm(
    key: &[u8],
    iv: &[u8],
    ciphertext: &[u8],
    tag: &[u8],
    payload: Payload<'_>,
) -> Result<Zeroizing<Vec<u8>>, ImportError> {
    use aes_gcm::aead::{consts::U16, Aead, Payload as AeadPayload};
    use aes_gcm::{AesGcm, KeyInit as _};

    let cipher = AesGcm::<aes::Aes256, U16>::new_from_slice(key)
        .map_err(|_| unreadable(gettext("key derivation failed")))?;
    let nonce =
        aes_gcm::Nonce::<U16>::try_from(iv).map_err(|_| unreadable(gettext("damaged content")))?;
    let mut message = ciphertext.to_vec();
    message.extend_from_slice(tag);
    let aad = format!("{}{}", payload.iv, payload.salt);
    let plain = cipher
        .decrypt(
            &nonce,
            AeadPayload {
                msg: &message,
                aad: aad.as_bytes(),
            },
        )
        .map_err(|_| ImportError::WrongPassword)?;
    Ok(Zeroizing::new(plain))
}

fn gunzip(bytes: &[u8]) -> Result<Zeroizing<String>, ImportError> {
    let mut text = Zeroizing::new(String::new());
    flate2::read::GzDecoder::new(bytes)
        .read_to_string(&mut text)
        .map_err(|_| unreadable(gettext("damaged content")))?;
    Ok(text)
}

/// A group or entry gathered from either format.
#[derive(Default)]
struct Node {
    title: String,
    parent: String,
    properties: Vec<(String, String)>,
    attributes: HashMap<String, String>,
}

/// Format b: the whole vault as one JSON object (possibly wrapped in a
/// one-item history array).
fn read_format_b(json: &str) -> Result<Vec<ImportedItem>, ImportError> {
    let value: Value =
        serde_json::from_str(json.trim()).map_err(|_| unreadable(gettext("unexpected content")))?;
    let vault = match value {
        Value::Array(mut lines) if !lines.is_empty() => match lines.swap_remove(0) {
            Value::String(inner) => serde_json::from_str(&inner)
                .map_err(|_| unreadable(gettext("unexpected content")))?,
            other => other,
        },
        other => other,
    };

    let live = |value: &Value| value.get("deleted").is_none_or(Value::is_null);
    let values = |object: Option<&Value>| -> Vec<(String, String)> {
        object
            .and_then(Value::as_object)
            .map(|map| {
                map.iter()
                    .filter(|(_, value)| live(value))
                    .filter_map(|(key, value)| {
                        let text = value.get("value")?.as_str()?;
                        Some((key.clone(), text.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let id_of = |value: &Value, key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };

    let mut groups = HashMap::new();
    for group in vault
        .get("g")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        groups.insert(
            id_of(group, "id"),
            Node {
                title: id_of(group, "t"),
                parent: id_of(group, "g"),
                attributes: values(group.get("a")).into_iter().collect(),
                ..Node::default()
            },
        );
    }
    let entries = vault
        .get("e")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|entry| Node {
            parent: id_of(entry, "g"),
            properties: values(entry.get("p")),
            attributes: values(entry.get("a")).into_iter().collect(),
            ..Node::default()
        })
        .collect::<Vec<_>>();
    Ok(items_from(&groups, entries))
}

/// Format a: one command per line (create group, set property…).
fn read_format_a(history: &str) -> Vec<ImportedItem> {
    let mut groups: HashMap<String, Node> = HashMap::new();
    let mut entries: HashMap<String, Node> = HashMap::new();
    let mut entry_order: Vec<String> = Vec::new();

    for line in history.lines() {
        let args = command_args(line);
        let arg = |index: usize| {
            args.get(index)
                .map(|arg| decode_value(arg))
                .unwrap_or_default()
        };
        match args.first().map(String::as_str) {
            Some("cgr") => {
                groups.insert(
                    arg(2),
                    Node {
                        parent: arg(1),
                        ..Node::default()
                    },
                );
            }
            Some("tgr") => {
                if let Some(group) = groups.get_mut(&arg(1)) {
                    group.title = arg(2);
                }
            }
            Some("mgr") => {
                if let Some(group) = groups.get_mut(&arg(1)) {
                    group.parent = arg(2);
                }
            }
            Some("dgr") => {
                groups.remove(&arg(1));
            }
            Some("sga") => {
                if let Some(group) = groups.get_mut(&arg(1)) {
                    group.attributes.insert(arg(2), arg(3));
                }
            }
            Some("dga") => {
                if let Some(group) = groups.get_mut(&arg(1)) {
                    group.attributes.remove(&arg(2));
                }
            }
            Some("cen") => {
                let id = arg(2);
                entry_order.push(id.clone());
                entries.insert(
                    id,
                    Node {
                        parent: arg(1),
                        ..Node::default()
                    },
                );
            }
            Some("men") => {
                if let Some(entry) = entries.get_mut(&arg(1)) {
                    entry.parent = arg(2);
                }
            }
            Some("den") => {
                entries.remove(&arg(1));
            }
            Some("sep") => {
                if let Some(entry) = entries.get_mut(&arg(1)) {
                    let (key, value) = (arg(2), arg(3));
                    match entry.properties.iter_mut().find(|(name, _)| *name == key) {
                        Some(existing) => existing.1 = value,
                        None => entry.properties.push((key, value)),
                    }
                }
            }
            Some("dep") => {
                if let Some(entry) = entries.get_mut(&arg(1)) {
                    let key = arg(2);
                    entry.properties.retain(|(name, _)| *name != key);
                }
            }
            Some("sea") => {
                if let Some(entry) = entries.get_mut(&arg(1)) {
                    entry.attributes.insert(arg(2), arg(3));
                }
            }
            Some("dea") => {
                if let Some(entry) = entries.get_mut(&arg(1)) {
                    entry.attributes.remove(&arg(2));
                }
            }
            // aid, fmt, pad, vault attributes: nothing to import.
            _ => {}
        }
    }

    let ordered = entry_order
        .into_iter()
        .filter_map(|id| entries.remove(&id))
        .collect();
    items_from(&groups, ordered)
}

/// Splits a history line into words; double-quoted words may hold
/// spaces and backslash escapes.
fn command_args(line: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut escaped = false;
    let mut started = false;
    for c in line.chars() {
        if escaped {
            current.push(c);
            escaped = false;
        } else if in_quotes && c == '\\' {
            escaped = true;
        } else if c == '"' {
            in_quotes = !in_quotes;
            started = true;
        } else if c.is_whitespace() && !in_quotes {
            if started {
                args.push(std::mem::take(&mut current));
                started = false;
            }
        } else {
            current.push(c);
            started = true;
        }
    }
    if started {
        args.push(current);
    }
    args
}

/// Format a writes names and values as `utf8+base64:<base64>`.
fn decode_value(value: &str) -> String {
    match value.strip_prefix("utf8+base64:") {
        Some(encoded) => BASE64
            .decode(encoded.as_bytes())
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .unwrap_or_default(),
        None => value.to_string(),
    }
}

/// Group path from the top down to `id`, or `None` inside the trash.
fn group_path<'a>(groups: &'a HashMap<String, Node>, mut id: &'a str) -> Option<Vec<String>> {
    let mut path = Vec::new();
    while let Some(group) = groups.get(id) {
        if group.attributes.get(TRASH_ROLE.0).map(String::as_str) == Some(TRASH_ROLE.1) {
            return None;
        }
        path.push(group.title.clone());
        id = &group.parent;
        if path.len() > groups.len() {
            break; // A cycle in a damaged file.
        }
    }
    path.reverse();
    Some(path)
}

/// Turns groups and entries into items, skipping the trash and
/// anything inside it.
fn items_from(groups: &HashMap<String, Node>, entries: Vec<Node>) -> Vec<ImportedItem> {
    let path_of = |id: &str| group_path(groups, id);

    entries
        .into_iter()
        .filter_map(|entry| {
            let path = path_of(&entry.parent)?;
            let mut title = String::new();
            let mut password = String::new();
            let mut fields = Vec::new();
            for (key, value) in &entry.properties {
                match key.to_lowercase().as_str() {
                    "title" => title = value.clone(),
                    "password" => password = value.clone(),
                    // Other properties (URL, notes, custom ones) become
                    // fields; an `otpauth://` value becomes an OTP field.
                    _ => fields.push((key.clone(), value.clone())),
                }
            }
            Some(ImportedItem {
                groups: path,
                title,
                entry: entry_from_fields(&password, fields),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use aes::cipher::BlockModeEncrypt;
    use flate2::{write::GzEncoder, Compression};
    use std::io::Write;

    use super::*;
    use crate::pass::model::EntryField;

    const VAULT_B: &str = r#"{
        "id": "v1",
        "a": {},
        "g": [
            {"id": "g1", "g": "0", "t": "General", "a": {}},
            {"id": "g2", "g": "g1", "t": "Email", "a": {}},
            {"id": "g3", "g": "0", "t": "Trash", "a": {"bc_group_role": {"value": "trash"}}}
        ],
        "e": [
            {"id": "e1", "g": "g2", "p": {
                "title": {"value": "Mail"},
                "username": {"value": "jdoe"},
                "password": {"value": "hunter2"},
                "URL": {"value": "https://mail.example.org"},
                "Old": {"value": "gone", "deleted": 1700000000000}
            }, "a": {}},
            {"id": "e2", "g": "g3", "p": {"title": {"value": "Deleted"}}, "a": {}}
        ],
        "del": {"e": {}, "g": {}}
    }"#;

    /// Encrypts like iocane's CBC mode, to build test vaults.
    fn encrypt_cbc(plaintext: &str, password: &str, salt: &str, rounds: u32) -> String {
        let mut keys = [0u8; 64];
        pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), salt.as_bytes(), rounds, &mut keys);
        let (key, hmac_key) = keys.split_at(32);
        let iv = [7u8; 16];
        let key: [u8; 32] = key.try_into().unwrap();
        let ciphertext = cbc::Encryptor::<aes::Aes256>::new(&key.into(), &iv.into())
            .encrypt_padded_vec::<Pkcs7>(plaintext.as_bytes());
        let content = BASE64.encode(&ciphertext);
        let iv_hex = HEXLOWER_PERMISSIVE.encode(&iv);
        let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(hmac_key).unwrap();
        mac.update(content.as_bytes());
        mac.update(iv_hex.as_bytes());
        mac.update(salt.as_bytes());
        let auth = HEXLOWER_PERMISSIVE.encode(&mac.finalize().into_bytes());
        format!("{content}${iv_hex}${salt}${auth}${rounds}$cbc")
    }

    fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    fn format_b_file(json: &str, password: &str) -> Vec<u8> {
        let plaintext = BASE64.encode(&gzip(json.as_bytes()));
        format!(
            "{SIGNATURE_B}{}",
            encrypt_cbc(&plaintext, password, "c2FsdHNhbHQ", 1000)
        )
        .into_bytes()
    }

    fn secret(password: &str) -> Zeroizing<String> {
        Zeroizing::new(password.to_string())
    }

    #[test]
    fn reads_a_format_b_vault() {
        let items = read(&format_b_file(VAULT_B, "pw"), &secret("pw")).unwrap();
        assert_eq!(items.len(), 1, "the trashed entry is skipped");
        let mail = &items[0];
        assert_eq!(mail.title, "Mail");
        assert_eq!(
            mail.groups,
            vec!["General".to_string(), "Email".to_string()]
        );
        assert_eq!(mail.entry.password, EntryField::Password("hunter2".into()));
        let keys: Vec<_> = mail
            .entry
            .fields
            .iter()
            .map(|(key, _)| key.as_str())
            .collect();
        assert!(keys.contains(&"username") && keys.contains(&"url"));
        assert!(!keys.contains(&"old"), "deleted properties are skipped");
    }

    #[test]
    fn reads_a_format_b_vault_wrapped_in_history() {
        let wrapped = serde_json::to_string(&vec![VAULT_B.to_string()]).unwrap();
        let items = read(&format_b_file(&wrapped, "pw"), &secret("pw")).unwrap();
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn rejects_a_wrong_password_or_another_file() {
        assert!(matches!(
            read(&format_b_file(VAULT_B, "pw"), &secret("nope")),
            Err(ImportError::WrongPassword)
        ));
        assert!(matches!(
            read(b"hello", &secret("pw")),
            Err(ImportError::Unreadable(_))
        ));
        assert!(matches!(
            read(
                format!("{SIGNATURE_B}a$b$c$d$99999999999$cbc").as_bytes(),
                &secret("pw")
            ),
            Err(ImportError::Unreadable(_))
        ));
    }

    #[test]
    fn reads_a_format_a_history() {
        let encoded = |text: &str| format!("utf8+base64:{}", BASE64.encode(text.as_bytes()));
        let history = [
            "fmt buttercup/a".to_string(),
            "aid 1234".to_string(),
            "cgr 0 g1".to_string(),
            format!("tgr g1 {}", encoded("Banking & Bills")),
            "cen g1 e1".to_string(),
            format!("sep e1 title {}", encoded("My Bank")),
            format!("sep e1 password {}", encoded("s3cret")),
            format!("sep e1 username {}", encoded("jdoe")),
            "cgr 0 g2".to_string(),
            "sga g2 bc_group_role trash".to_string(),
            "cen g2 e2".to_string(),
            format!("sep e2 title {}", encoded("Old")),
            "pad 5678".to_string(),
        ]
        .join("\n");
        let gzip_bytes = gzip(history.as_bytes());
        let binary_string: String = gzip_bytes.iter().map(|&byte| char::from(byte)).collect();
        let file = format!(
            "{SIGNATURE_A}{}",
            encrypt_cbc(&binary_string, "pw", "salt", 1000)
        );

        let items = read(file.as_bytes(), &secret("pw")).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "My Bank");
        assert_eq!(items[0].groups, vec!["Banking & Bills".to_string()]);
        assert_eq!(
            items[0].entry.password,
            EntryField::Password("s3cret".into())
        );
    }

    #[test]
    fn splits_quoted_history_arguments() {
        assert_eq!(
            command_args(r#"sep e1 "two words" "a \"quote\"""#),
            vec!["sep", "e1", "two words", "a \"quote\""]
        );
    }
}
