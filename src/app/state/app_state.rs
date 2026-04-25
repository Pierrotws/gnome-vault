// Global state of App

use std::{collections::HashMap, path::Path};

use crate::{
    app::state::EntrySession,
    pass::model::{EntryData, PassNode},
};

#[derive(Clone, Default)]
pub struct AppState {
    tree: Vec<PassNode>,
    selected_node: Option<PassNode>,
    current_session: Option<EntrySession>,
    /// Decrypted entry cache keyed by password-store relative path.
    ///
    /// This avoids repeatedly opening and decrypting the same `.gpg` file while
    /// the application is running. The cache stores clean entry data only:
    /// opened entries and entries after a successful save.
    entry_cache: HashMap<std::path::PathBuf, EntryData>,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tree(&self) -> &[PassNode] {
        &self.tree
    }

    pub fn set_tree(&mut self, tree: Vec<PassNode>) {
        self.tree = tree;
    }

    pub fn selected_node(&self) -> Option<&PassNode> {
        self.selected_node.as_ref()
    }

    pub fn set_selected_node(&mut self, node: Option<PassNode>) {
        self.selected_node = node;
    }

    pub fn current_session(&self) -> Option<&EntrySession> {
        self.current_session.as_ref()
    }

    pub fn current_session_mut(&mut self) -> Option<&mut EntrySession> {
        self.current_session.as_mut()
    }

    pub fn set_current_session(&mut self, session: Option<EntrySession>) {
        self.current_session = session;
    }

    /// Returns cached entry data for a password-store relative path.
    pub fn cached_entry(&self, path: &Path) -> Option<&EntryData> {
        self.entry_cache.get(path)
    }

    /// Stores decrypted entry data for the node path, replacing older data.
    pub fn cache_entry(&mut self, node: &PassNode, entry: EntryData) {
        self.entry_cache.insert(node.path.clone(), entry);
    }

    pub fn has_unsaved_changes(&self) -> bool {
        self.current_session
            .as_ref()
            .map(|s| s.is_dirty())
            .unwrap_or(false)
    }

    pub fn clear_current_entry(&mut self) {
        self.selected_node = None;
        self.current_session = None;
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::pass::model::{EntryField, PassNodeKind};

    use super::*;

    fn entry(value: &str) -> EntryData {
        EntryData {
            password: EntryField::Password(value.into()),
            fields: Vec::new(),
        }
    }

    fn node(path: &str) -> PassNode {
        PassNode {
            name: path.into(),
            path: PathBuf::from(path),
            kind: PassNodeKind::Entry,
            children: Vec::new(),
        }
    }

    #[test]
    fn caches_entries_by_node_path() {
        let node = node("email/example.gpg");
        let data = entry("secret");
        let mut state = AppState::new();

        state.cache_entry(&node, data.clone());

        assert_eq!(state.cached_entry(&node.path), Some(&data));
    }

    #[test]
    fn cache_entry_replaces_existing_value() {
        let node = node("email/example.gpg");
        let mut state = AppState::new();

        state.cache_entry(&node, entry("old"));
        state.cache_entry(&node, entry("new"));

        assert_eq!(state.cached_entry(&node.path), Some(&entry("new")));
    }
}
