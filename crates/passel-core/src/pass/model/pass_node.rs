use std::path::PathBuf;

/// Kind of node in the password-store tree.
///
/// `Group` carries its children inline so the type system enforces the
/// "only groups have children" invariant — entries can't even
/// accidentally hold a child vec, so callers don't have to defensively
/// check `is_entry()` before reading `children`.
#[derive(Debug, Clone, PartialEq)]
pub enum PassNodeKind {
    Group(Vec<PassNode>),
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
}

impl PassNode {
    /// Returns `true` when this node represents a directory/group.
    pub fn is_group(&self) -> bool {
        matches!(self.kind, PassNodeKind::Group(_))
    }

    /// Returns `true` when this node represents a `.gpg` entry.
    pub fn is_entry(&self) -> bool {
        matches!(self.kind, PassNodeKind::Entry)
    }

    /// Returns the child nodes if this node is a group, or an empty slice
    /// for an entry. Callers that previously relied on the always-empty
    /// `children: Vec<PassNode>` field for entries can read this without
    /// branching on the kind.
    pub fn children(&self) -> &[PassNode] {
        match &self.kind {
            PassNodeKind::Group(children) => children,
            PassNodeKind::Entry => &[],
        }
    }
}
