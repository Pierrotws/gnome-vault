use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum PassNodeKind {
    Group,
    Entry,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PassNode {
    pub name: String,
    pub path: PathBuf,
    pub kind: PassNodeKind,
    pub children: Vec<PassNode>,
}

impl PassNode {
    pub fn is_group(&self) -> bool {
        matches!(self.kind, PassNodeKind::Group)
    }

    pub fn is_entry(&self) -> bool {
        matches!(self.kind, PassNodeKind::Entry)
    }
}
