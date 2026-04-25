use std::path::PathBuf;

/// Kind of node in the password-store tree.
#[derive(Debug, Clone, PartialEq)]
pub enum PassNodeKind {
    Group,
    Entry,
}

/// A group or entry discovered in the password-store tree.
#[derive(Debug, Clone, PartialEq)]
pub struct PassNode {
    /// Display name without the `.gpg` extension for entries.
    pub name: String,
    /// Path relative to the password-store root.
    pub path: PathBuf,
    pub kind: PassNodeKind,
    /// Child nodes for groups. Entries always have no children.
    pub children: Vec<PassNode>,
}

impl PassNode {
    /// Returns `true` when this node represents a directory/group.
    pub fn is_group(&self) -> bool {
        matches!(self.kind, PassNodeKind::Group)
    }

    /// Returns `true` when this node represents a `.gpg` entry.
    pub fn is_entry(&self) -> bool {
        matches!(self.kind, PassNodeKind::Entry)
    }
}
