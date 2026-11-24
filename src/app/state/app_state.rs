// Global state of App

use std::{collections::HashMap, path::Path};

use crate::{
    app::state::EntrySession,
    pass::model::{EntryData, PassNode},
};

// `Clone` is intentionally *not* derived — cloning AppState would
// deep-clone the entire entry_cache, multiplying every plaintext
// EntryField allocation. Use the controller's mutating API instead;
// if a snapshot is ever needed, build it explicitly.
#[derive(Default)]
pub struct AppState {
    tree: Vec<PassNode>,
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

    pub fn remove_cached_entry(&mut self, path: &Path) {
        self.entry_cache.remove(path);
    }

    /// Drops every cached entry whose path lies under `prefix`. Used by
    /// folder-delete to scope eviction to the deleted subtree instead of
    /// nuking the whole cache and forcing the rest of the vault to
    /// re-decrypt on next open.
    ///
    /// An empty prefix matches every path (Path::starts_with("") is
    /// always true) and would silently nuke the whole cache; that is
    /// almost certainly a caller bug — the search-results pseudo-group
    /// is the only PassNode constructed with PathBuf::new() in the UI
    /// layer. Refuse the call rather than defeating the round-2
    /// scoped-eviction protection.
    pub fn evict_cached_entries_under(&mut self, prefix: &Path) {
        if prefix.as_os_str().is_empty() {
            debug_assert!(
                false,
                "evict_cached_entries_under called with empty prefix; refusing to clear whole cache"
            );
            log::warn!("evict_cached_entries_under: empty prefix ignored");
            return;
        }
        self.entry_cache.retain(|path, _| !path.starts_with(prefix));
    }

    pub fn clear_entry_cache(&mut self) {
        self.entry_cache.clear();
    }

    pub fn has_unsaved_changes(&self) -> bool {
        self.current_session
            .as_ref()
            .map(|s| s.is_dirty())
            .unwrap_or(false)
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
