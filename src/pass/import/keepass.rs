//! KeePassXC / KeePass databases (`.kdbx`, KDBX 3 and 4), read with the
//! `keepass` crate. Master password only: key files and hardware keys are
//! not supported yet.

use keepass::db::{fields, GroupRef};
use keepass::error::{DatabaseKeyError, DatabaseOpenError};
use keepass::{Database, DatabaseKey};
use zeroize::Zeroizing;

use super::{entry_from_fields, ImportError, ImportedItem};

pub(super) fn read(
    bytes: &[u8],
    password: &Zeroizing<String>,
) -> Result<Vec<ImportedItem>, ImportError> {
    let key = DatabaseKey::new().with_password(password);
    let db = Database::open(&mut &bytes[..], key).map_err(|err| {
        if is_incorrect_key(&err) {
            ImportError::WrongPassword
        } else {
            ImportError::Unreadable(err.to_string())
        }
    })?;

    let recycle_bin = db.recycle_bin().map(|group| group.id());
    let mut items = Vec::new();
    collect(&db.root(), &mut Vec::new(), recycle_bin, &mut items);
    Ok(items)
}

/// Walks a group, skipping the recycle bin. The root group's own name
/// (usually the database name) is not part of the paths.
fn collect(
    group: &GroupRef<'_>,
    path: &mut Vec<String>,
    recycle_bin: Option<keepass::db::GroupId>,
    items: &mut Vec<ImportedItem>,
) {
    for entry in group.entries() {
        let title = entry.get_title().unwrap_or_default().to_string();
        let password = entry.get_password().unwrap_or_default();
        let mut entry_fields = Vec::new();
        for known in [fields::USERNAME, fields::URL] {
            if let Some(value) = entry.get(known) {
                entry_fields.push((known.to_string(), value.to_string()));
            }
        }
        // Custom fields (including KeePassXC's "otp"), in a stable order.
        let mut custom: Vec<_> = entry
            .fields
            .iter()
            .filter(|(key, _)| !fields::KNOWN_FIELDS.contains(&key.as_str()))
            .map(|(key, value)| (key.clone(), value.get().clone()))
            .collect();
        custom.sort_by(|a, b| a.0.cmp(&b.0));
        entry_fields.extend(custom);
        if let Some(notes) = entry.get(fields::NOTES) {
            entry_fields.push((fields::NOTES.to_string(), notes.to_string()));
        }
        items.push(ImportedItem {
            groups: path.clone(),
            title,
            entry: entry_from_fields(password, entry_fields),
        });
    }
    for child in group.groups() {
        if Some(child.id()) == recycle_bin {
            continue;
        }
        path.push(child.name.clone());
        collect(&child, path, recycle_bin, items);
        path.pop();
    }
}

fn is_incorrect_key(err: &DatabaseOpenError) -> bool {
    matches!(err, DatabaseOpenError::Key(DatabaseKeyError::IncorrectKey))
}

#[cfg(test)]
mod tests {
    use keepass::db::{fields, EntryMut, GroupMut};

    use super::*;
    use crate::pass::model::EntryField;

    fn sample_database(password: &str) -> Vec<u8> {
        let mut db = Database::new();
        let mut root = db.root_mut();
        root.add_entry().edit(|entry: &mut EntryMut<'_>| {
            entry.set_unprotected(fields::TITLE, "Router");
            entry.set_protected(fields::PASSWORD, "admin123");
        });
        root.add_group()
            .edit(|group: &mut GroupMut<'_>| group.name = "Email".into())
            .add_entry()
            .edit(|entry: &mut EntryMut<'_>| {
                entry.set_unprotected(fields::TITLE, "Mail");
                entry.set_unprotected(fields::USERNAME, "jdoe");
                entry.set_protected(fields::PASSWORD, "hunter2");
                entry.set_unprotected(fields::URL, "https://mail.example.org");
                entry.set_unprotected(fields::NOTES, "two\nlines");
                entry.set_protected("otp", "otpauth://totp/Mail?secret=GEZDGNBVGY3TQOJQ");
            });
        let mut bytes = Vec::new();
        db.save(&mut bytes, DatabaseKey::new().with_password(password))
            .unwrap();
        bytes
    }

    #[test]
    fn reads_entries_and_groups() {
        let bytes = sample_database("correct horse");
        let items = read(&bytes, &Zeroizing::new("correct horse".to_string())).unwrap();
        assert_eq!(items.len(), 2);

        let router = items.iter().find(|item| item.title == "Router").unwrap();
        assert!(router.groups.is_empty());
        assert_eq!(
            router.entry.password,
            EntryField::Password("admin123".into())
        );

        let mail = items.iter().find(|item| item.title == "Mail").unwrap();
        assert_eq!(mail.groups, vec!["Email".to_string()]);
        assert_eq!(mail.entry.password, EntryField::Password("hunter2".into()));
        assert_eq!(
            mail.entry.fields,
            vec![
                ("username".to_string(), EntryField::Plain("jdoe".into())),
                (
                    "url".to_string(),
                    EntryField::Plain("https://mail.example.org".into())
                ),
                (
                    "otp".to_string(),
                    EntryField::Otp("otpauth://totp/Mail?secret=GEZDGNBVGY3TQOJQ".into())
                ),
                (
                    "notes".to_string(),
                    EntryField::Multiline("two\nlines".into())
                ),
            ]
        );
    }

    #[test]
    fn reports_a_wrong_password() {
        let bytes = sample_database("correct horse");
        assert!(matches!(
            read(&bytes, &Zeroizing::new("wrong".to_string())),
            Err(ImportError::WrongPassword)
        ));
        assert!(matches!(
            read(b"not a database", &Zeroizing::new("x".to_string())),
            Err(ImportError::Unreadable(_))
        ));
    }
}
