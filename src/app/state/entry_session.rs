// Context of current edition

use crate::{
    helpers::otp,
    pass::model::{EntryData, EntryField, PassNode},
};

#[derive(Debug, Clone, PartialEq)]
pub struct EntrySession {
    node: PassNode,
    name: String,
    original: EntryData,
    current: EntryData,
}

impl EntrySession {
    pub fn new(node: PassNode, data: EntryData) -> Self {
        Self {
            name: node.name.clone(),
            node,
            original: data.clone(),
            current: data,
        }
    }

    pub fn node(&self) -> &PassNode {
        &self.node
    }

    pub fn original(&self) -> &EntryData {
        &self.original
    }

    pub fn current(&self) -> &EntryData {
        &self.current
    }

    pub fn current_mut(&mut self) -> &mut EntryData {
        &mut self.current
    }

    pub fn current_name(&self) -> &str {
        &self.name
    }

    pub fn has_entry_changes(&self) -> bool {
        self.original != self.current
    }

    pub fn has_name_changes(&self) -> bool {
        self.node.name != self.name
    }

    /// Validation rules for persisting an entry.
    ///
    /// These are intentionally lenient to match real `pass(1)` entries:
    /// - the entry name is non-empty and not path-like
    /// - the password slot is a non-empty `Password` or a parsable OTP URI
    /// - field keys are non-empty (anonymous lines are not persisted as fields)
    /// - OTP field values must be parsable as `otpauth://` URIs
    /// - `Plain`, `Multiline`, `Array`, and `Password` field values may be
    ///   empty — many real pass entries store empty placeholder values like
    ///   `comment:` and rejecting those would break round-tripping.
    pub fn is_valid(&self) -> bool {
        if self.name.trim().is_empty() || self.name.contains('/') || self.name.contains('\\') {
            return false;
        }

        match &self.current.password {
            EntryField::Password(str) => {
                if str.is_empty() {
                    return false;
                }
            }
            EntryField::OTP(str) => {
                if !otp::is_otp_url(str) {
                    return false;
                }
            }
            _ => return false,
        }

        for (key, entry) in &self.current.fields {
            if key.is_empty() {
                return false;
            }
            // Only OTP values must parse; everything else is allowed to be
            // empty so that comment-style placeholder fields round-trip.
            if let EntryField::OTP(value) = entry {
                if !otp::is_otp_url(value) {
                    return false;
                }
            }
        }
        true
    }

    pub fn is_dirty(&self) -> bool {
        (self.original != self.current) || (self.node.name != self.name)
    }

    pub fn revert(&mut self) {
        self.current = self.original.clone();
        self.name = self.node.name.clone();
    }

    pub fn mark_saved(&mut self) {
        self.original = self.current.clone();
        self.node.name = self.name.clone();
    }

    pub fn replace_node(&mut self, node: PassNode) {
        self.node = node;
    }

    pub fn replace_current(&mut self, name: String, data: EntryData) {
        self.name = name;
        self.current = data;
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::pass::model::{EntryField, PassNodeKind};

    use super::*;

    fn node() -> PassNode {
        PassNode {
            name: "old".into(),
            path: PathBuf::from("folder/old.gpg"),
            kind: PassNodeKind::Entry,
            children: Vec::new(),
        }
    }

    fn entry() -> EntryData {
        EntryData {
            password: EntryField::Password("secret".into()),
            fields: Vec::new(),
        }
    }

    #[test]
    fn title_changes_make_session_dirty() {
        let mut session = EntrySession::new(node(), entry());

        session.replace_current("new".into(), entry());

        assert!(session.has_name_changes());
        assert!(session.is_dirty());
    }

    #[test]
    fn rejects_empty_or_path_like_entry_names() {
        let mut session = EntrySession::new(node(), entry());

        session.replace_current("".into(), entry());
        assert!(!session.is_valid());

        session.replace_current("folder/name".into(), entry());
        assert!(!session.is_valid());
    }

    #[test]
    fn accepts_baseline_entry() {
        let session = EntrySession::new(node(), entry());
        assert!(session.is_valid());
    }

    #[test]
    fn rejects_empty_password() {
        let mut session = EntrySession::new(node(), entry());
        session.replace_current(
            "ok".into(),
            EntryData {
                password: EntryField::Password(String::new()),
                fields: Vec::new(),
            },
        );
        assert!(!session.is_valid());
    }

    #[test]
    fn rejects_invalid_otp_password() {
        let mut session = EntrySession::new(node(), entry());
        session.replace_current(
            "ok".into(),
            EntryData {
                password: EntryField::OTP("not a url".into()),
                fields: Vec::new(),
            },
        );
        assert!(!session.is_valid());
    }

    #[test]
    fn accepts_otp_password_url() {
        let mut session = EntrySession::new(node(), entry());
        session.replace_current(
            "ok".into(),
            EntryData {
                password: EntryField::OTP(
                    "otpauth://totp/Example:user?secret=JBSWY3DPEHPK3PXP".into(),
                ),
                fields: Vec::new(),
            },
        );
        assert!(session.is_valid());
    }

    #[test]
    fn allows_empty_plain_and_multiline_field_values() {
        // Real pass entries often have placeholder fields like `comment:`
        // with no value. The session must not reject those.
        let mut session = EntrySession::new(node(), entry());
        session.replace_current(
            "ok".into(),
            EntryData {
                password: EntryField::Password("secret".into()),
                fields: vec![
                    ("comment".into(), EntryField::Plain(String::new())),
                    ("notes".into(), EntryField::Multiline(String::new())),
                    ("tags".into(), EntryField::Array(Vec::new())),
                ],
            },
        );
        assert!(session.is_valid());
    }

    #[test]
    fn rejects_empty_field_key() {
        let mut session = EntrySession::new(node(), entry());
        session.replace_current(
            "ok".into(),
            EntryData {
                password: EntryField::Password("secret".into()),
                fields: vec![(String::new(), EntryField::Plain("v".into()))],
            },
        );
        assert!(!session.is_valid());
    }

    #[test]
    fn rejects_invalid_otp_field_value() {
        let mut session = EntrySession::new(node(), entry());
        session.replace_current(
            "ok".into(),
            EntryData {
                password: EntryField::Password("secret".into()),
                fields: vec![("totp".into(), EntryField::OTP("not a url".into()))],
            },
        );
        assert!(!session.is_valid());
    }
}
