///Represents entry in pass
use crate::pass::model::EntryField;

#[derive(Debug, Clone, PartialEq)]
pub struct EntryData {
    pub password: EntryField,
    pub fields: Vec<(String, EntryField)>,
}

impl From<&EntryData> for String {
    fn from(entry: &EntryData) -> Self {
        let mut out = String::new();
        let password_str = &entry.password.to_str();
        out.push_str(&password_str);
        out.push('\n');
        for (key, value) in &entry.fields {
            let value = value.to_str();
            if key.trim().is_empty() && value.trim().is_empty() {
                continue;
            }
            out.push_str(key);
            out.push_str(": ");
            out.push_str(&value);
            out.push('\n');
        }
        out
    }
}
