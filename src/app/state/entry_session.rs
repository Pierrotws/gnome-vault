// Context of current edition

use crate::pass::model::{EntryData, EntryField, PassNode};

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

    pub fn is_valid(&self) -> bool {
        match &self.current.password {
            EntryField::Password(str) => {
                if str.is_empty() {
                    return false;
                }
            }
            EntryField::OTP(str) => {
                if str.is_empty() {
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
                    if str.is_empty() {
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

    pub fn replace_current(&mut self, name: String, data: EntryData) {
        self.name = name;
        self.current = data;
    }
}
