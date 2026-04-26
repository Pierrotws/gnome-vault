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

        for (str, entry) in &self.current.fields {
            if str.is_empty() {
                return false;
            }
            match &entry {
                EntryField::Plain(str) => {
                    if str.is_empty() {
                        return false;
                    }
                }
                EntryField::OTP(str) => {
                    if !otp::is_otp_url(str) {
                        return false;
                    }
                }
                EntryField::Password(str) => {
                    if str.is_empty() {
                        return false;
                    }
                }
                EntryField::Multiline(str) => {
                    if str.is_empty() {
                        return false;
                    }
                }
                EntryField::Array(arr) => {
                    if arr.len() == 0 {
                        return false;
                    }
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
}
