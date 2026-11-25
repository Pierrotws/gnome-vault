//! Reading other password managers' vault files, to import their entries
//! into a pass vault: KeePassXC (`.kdbx`) and Buttercup (`.bcup`).
//!
//! Readers only decrypt and map entries; writing them into the vault is
//! [`crate::pass::store::import_entries`]. Master passwords are handled
//! as [`Zeroizing`] strings and never logged.

mod buttercup;
mod keepass;

use std::path::Path;

use zeroize::Zeroizing;

use crate::i18n::{gettext, gettext_f};
use crate::pass::model::{EntryData, EntryField};

/// Where an import comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportSource {
    KeePassXc,
    Buttercup,
}

impl ImportSource {
    pub fn name(self) -> &'static str {
        match self {
            ImportSource::KeePassXc => "KeePassXC",
            ImportSource::Buttercup => "Buttercup",
        }
    }
}

/// One entry read from another password manager.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedItem {
    /// Group names from the top of the source vault down to the entry.
    pub groups: Vec<String>,
    pub title: String,
    pub entry: EntryData,
}

#[derive(Debug)]
pub enum ImportError {
    /// The master password does not open this file.
    WrongPassword,
    /// The file is not a vault of the chosen kind, or is damaged.
    Unreadable(String),
    Io(std::io::Error),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            ImportError::WrongPassword => gettext("The master password is not correct"),
            ImportError::Unreadable(reason) => {
                gettext_f("This file cannot be read: {reason}", &[("reason", reason)])
            }
            ImportError::Io(err) => gettext_f(
                "This file cannot be opened ({reason})",
                &[("reason", &err.kind().to_string())],
            ),
        };
        f.write_str(&message)
    }
}

impl std::error::Error for ImportError {}

impl From<std::io::Error> for ImportError {
    fn from(err: std::io::Error) -> Self {
        ImportError::Io(err)
    }
}

/// Decrypts a vault file and returns its entries (trash excluded).
pub fn read_vault_file(
    source: ImportSource,
    path: &Path,
    password: &Zeroizing<String>,
) -> Result<Vec<ImportedItem>, ImportError> {
    let bytes = Zeroizing::new(std::fs::read(path)?);
    match source {
        ImportSource::KeePassXc => keepass::read(&bytes, password),
        ImportSource::Buttercup => buttercup::read(&bytes, password),
    }
}

/// Builds the pass entry for an item: the password on the first line,
/// then the other fields in the order given, empty ones dropped.
/// Values with line breaks become multiline fields, `otpauth://` values
/// OTP fields.
fn entry_from_fields(password: &str, fields: Vec<(String, String)>) -> EntryData {
    let fields = fields
        .into_iter()
        .filter(|(key, value)| !key.trim().is_empty() && !value.trim().is_empty())
        .map(|(key, value)| {
            let field = if value.contains('\n') {
                EntryField::Multiline(value)
            } else if crate::helpers::otp::is_otp_url(&value) {
                EntryField::Otp(value)
            } else {
                EntryField::Plain(value)
            };
            (field_key(&key), field)
        })
        .collect();
    EntryData {
        password: EntryField::Password(first_line(password)),
        fields,
    }
}

/// pass keys are single words in practice: `Two Words` → `two-words`.
fn field_key(key: &str) -> String {
    key.split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .to_lowercase()
}

/// The pass format keeps the secret on one line.
fn first_line(value: &str) -> String {
    value.lines().next().unwrap_or_default().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_fields_to_pass_entries() {
        let entry = entry_from_fields(
            "hunter2",
            vec![
                ("UserName".into(), "jdoe".into()),
                ("URL".into(), "https://example.org".into()),
                ("Notes".into(), "line one\nline two".into()),
                (
                    "otp".into(),
                    "otpauth://totp/x?secret=GEZDGNBVGY3TQOJQ".into(),
                ),
                ("Recovery Code".into(), "1234".into()),
                ("Empty".into(), "  ".into()),
            ],
        );
        assert_eq!(entry.password, EntryField::Password("hunter2".into()));
        assert_eq!(
            entry.fields,
            vec![
                ("username".to_string(), EntryField::Plain("jdoe".into())),
                (
                    "url".to_string(),
                    EntryField::Plain("https://example.org".into())
                ),
                (
                    "notes".to_string(),
                    EntryField::Multiline("line one\nline two".into())
                ),
                (
                    "otp".to_string(),
                    EntryField::Otp("otpauth://totp/x?secret=GEZDGNBVGY3TQOJQ".into())
                ),
                (
                    "recovery-code".to_string(),
                    EntryField::Plain("1234".into())
                ),
            ]
        );
    }

    #[test]
    fn keeps_the_secret_on_one_line() {
        let entry = entry_from_fields("first\nsecond", Vec::new());
        assert_eq!(entry.password, EntryField::Password("first".into()));
    }
}
