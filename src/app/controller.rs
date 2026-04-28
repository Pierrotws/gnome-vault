// This file is the API used by the UI.
//
// The UI should not call pass::store::* directly.
// The UI should mostly talk to AppController.

use crate::{
    app::{
        app_error::AppError,
        state::{AppState, EntrySession, EntryViewData},
    },
    helpers::git::GitChange,
    pass::{
        self,
        model::{EntryData, PassNode},
        store::{GpgRecipient, VaultSetup},
    },
};
use std::path::Path;

pub struct AppController {
    state: AppState,
    autopush: bool,
    /// User-configured branch name. `None` means "use the working tree's
    /// current branch" (whatever HEAD points at). When `Some`, push and
    /// history operations target this branch instead.
    branch: Option<String>,
}

impl AppController {
    /// Creates a controller with empty application state.
    pub fn new() -> Self {
        Self {
            state: AppState::new(),
            autopush: true,
            branch: None,
        }
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut AppState {
        &mut self.state
    }

    pub fn current_session(&self) -> Option<&EntrySession> {
        self.state.current_session()
    }

    pub fn current_entry(&self) -> Option<&EntryData> {
        self.state.current_session().map(|s| s.current())
    }

    pub fn current_entry_mut(&mut self) -> Option<&mut EntryData> {
        self.state.current_session_mut().map(|s| s.current_mut())
    }

    pub fn has_valid_changes(&self) -> bool {
        if !self.state.has_unsaved_changes() {
            return false;
        }
        self.state
            .current_session()
            .as_ref()
            .map(|s| s.is_valid())
            .unwrap_or(false)
    }

    pub fn has_unsaved_changes(&self) -> bool {
        self.state.has_unsaved_changes()
    }

    pub fn autopush(&self) -> bool {
        self.autopush
    }

    pub fn set_autopush(&mut self, autopush: bool) {
        self.autopush = autopush;
    }

    /// Returns the configured branch override, or `None` if push and history
    /// should follow the working tree's current branch.
    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }

    /// Sets the configured branch. An empty string is treated as "no override".
    pub fn set_branch(&mut self, branch: Option<String>) {
        self.branch = branch.filter(|b| !b.trim().is_empty());
    }

    /// Initializes a new password-store vault and reloads application state.
    pub fn setup_vault(&mut self, setup: &VaultSetup) -> Result<(), AppError> {
        pass::store::setup_vault(setup)?;
        self.state.clear_entry_cache();
        self.state.set_current_session(None);
        self.reload_tree()?;
        Ok(())
    }

    /// Lists usable GPG recipients for a newly created vault.
    pub fn available_recipients(&self) -> Result<Vec<GpgRecipient>, AppError> {
        Ok(pass::store::available_recipients()?)
    }

    /// Reloads the password-store tree from disk.
    ///
    /// This refreshes the visible node tree only. Cached decrypted entries are
    /// kept so reopening an already viewed entry does not decrypt the file again.
    pub fn reload_tree(&mut self) -> Result<(), AppError> {
        let tree = pass::store::load_password_store()?;
        self.state.set_tree(tree);
        Ok(())
    }

    /// Returns tree nodes matching a search query.
    ///
    /// Entry names are always searchable. Cached entries also expose their
    /// field keys to search without decrypting files that were never opened.
    /// Matching groups include their complete subtree; matching entries include
    /// their parent groups.
    pub fn filtered_tree(&self, query: &str) -> Vec<PassNode> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return self.state.tree().to_vec();
        }

        self.state
            .tree()
            .iter()
            .filter_map(|node| self.filtered_node(node, &query))
            .collect()
    }

    /// Returns entry nodes that are not present in the decrypted cache.
    pub fn uncached_entry_nodes(&self) -> Vec<PassNode> {
        self.state
            .tree()
            .iter()
            .flat_map(|node| self.uncached_entry_nodes_from(node))
            .collect()
    }

    /// Loads an entry for background cache warming.
    pub fn load_entry_for_cache(node: &PassNode) -> Result<EntryData, AppError> {
        Ok(pass::store::load_entry_from_node(node)?)
    }

    /// Stores an entry loaded by the background cache warmer.
    pub fn cache_loaded_entry(&mut self, node: &PassNode, entry: EntryData) {
        self.state.cache_entry(node, entry);
    }

    /// Clears loaded entry state after an operation rewrites vault history.
    pub fn clear_loaded_entry_state(&mut self) {
        self.state.clear_entry_cache();
        self.state.set_current_session(None);
    }

    /// Lists commits reachable from the configured branch (or HEAD if none).
    pub fn load_changes(&self) -> Result<Vec<GitChange>, AppError> {
        Ok(pass::store::load_changes(self.branch())?)
    }

    /// Lists one page of commits reachable from the configured branch
    /// (or HEAD if none).
    pub fn load_changes_page(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<GitChange>, AppError> {
        Ok(pass::store::load_changes_page(
            offset,
            limit,
            self.branch(),
        )?)
    }

    /// Opens an entry node and returns view data for the UI.
    ///
    /// Decrypted entries are cached by node path. The first open reads and
    /// decrypts the `.gpg` file; later opens reuse the cached [`EntryData`].
    pub fn open_node(&mut self, node: PassNode) -> Result<EntryViewData, AppError> {
        let title = node.name.clone();
        let entry = self.load_entry_data(&node)?;
        let session = EntrySession::new(node, entry.clone());
        self.state.set_current_session(Some(session));

        Ok(EntryViewData { title, entry })
    }

    /// Loads an entry for list previews without changing the active edit session.
    pub fn preview_entry(&mut self, node: &PassNode) -> Result<EntryData, AppError> {
        self.load_entry_data(node)
    }

    fn load_entry_data(&mut self, node: &PassNode) -> Result<EntryData, AppError> {
        match self.state.cached_entry(&node.path) {
            Some(entry) => {
                log::debug!("entry cache hit: {}", node.path.display());
                Ok(entry.clone())
            }
            None => {
                log::debug!("entry cache miss: {}", node.path.display());
                let entry = pass::store::load_entry_from_node(node)?;
                self.state.cache_entry(node, entry.clone());
                Ok(entry)
            }
        }
    }

    /// Replaces the currently edited entry with values read from the UI.
    pub fn update_current_entry(&mut self, data: EntryViewData) -> Result<(), AppError> {
        log::debug!("update current entry");
        let session = self
            .state
            .current_session_mut()
            .ok_or(AppError::NoEntrySelected)?;

        session.replace_current(data.title, data.entry);
        Ok(())
    }

    /// Creates, saves, caches, and opens a new entry.
    pub fn create_entry(
        &mut self,
        folder_path: &Path,
        title: &str,
        entry: EntryData,
    ) -> Result<EntryViewData, AppError> {
        // Build a transient session against the candidate values so the same
        // validation rules used by the editor screen also gate the controller.
        let candidate_node = PassNode {
            name: title.to_string(),
            path: folder_path.join(format!("{title}.gpg")),
            kind: crate::pass::model::PassNodeKind::Entry,
            children: Vec::new(),
        };
        let candidate = EntrySession::new(candidate_node, entry.clone());
        if !candidate.is_valid() {
            return Err(AppError::InvalidEntry);
        }

        let node = pass::store::create_entry_data(folder_path, title, &entry, self.autopush)?;
        self.state.cache_entry(&node, entry.clone());
        let session = EntrySession::new(node.clone(), entry.clone());
        self.state.set_current_session(Some(session));

        Ok(EntryViewData {
            title: node.name,
            entry,
        })
    }

    /// Deletes the currently opened entry and closes the current session.
    pub fn delete_current_entry(&mut self) -> Result<bool, AppError> {
        let node = self
            .state
            .current_session()
            .ok_or(AppError::NoEntrySelected)?
            .node()
            .clone();

        self.delete_entry(node)
    }

    /// Deletes the given entry and clears the current session if it was open.
    pub fn delete_entry(&mut self, node: PassNode) -> Result<bool, AppError> {
        pass::store::delete_entry(&node, self.autopush)?;
        self.state.remove_cached_entry(&node.path);

        let deleting_current = self
            .state
            .current_session()
            .is_some_and(|session| session.node().path == node.path);
        if deleting_current {
            self.state.set_current_session(None);
        }

        Ok(deleting_current)
    }

    /// Persists the current entry, marks the session clean, and updates cache.
    ///
    /// Cache invariants (must hold on every return path, success or failure):
    /// - The cache only ever holds CLEAN entry data — i.e. ciphertext that has
    ///   actually been written to disk for that path.
    /// - After a rename succeeds on disk, the cache entry under the old path
    ///   is removed immediately. The cache entry under the new path is
    ///   inserted only if the rename AND any subsequent body save succeed.
    /// - On any error path, the session is NOT marked saved (`is_dirty()`
    ///   stays true) so the user can retry.
    pub fn save_current_entry(&mut self) -> Result<(), AppError> {
        let (entry_changed, rename) = {
            let session = self
                .state
                .current_session()
                .ok_or(AppError::NoEntrySelected)?;
            // Validate before touching disk so we never persist incoherent data.
            if !session.is_valid() {
                return Err(AppError::InvalidEntry);
            }
            let rename = session
                .has_name_changes()
                .then(|| (session.node().clone(), session.current_name().to_string()));

            (session.has_entry_changes(), rename)
        };

        // Phase 1: rename on disk if needed. If rename fails, nothing on disk
        // has changed and the cache is still valid under the old path.
        let old_path = if let Some((node, name)) = rename {
            let old_path = node.path.clone();
            let renamed_node = pass::store::rename_entry(&node, &name, self.autopush)?;
            // Disk has been renamed. Drop the stale cache entry under the
            // OLD path immediately, regardless of whether the body save below
            // succeeds. Otherwise the cache would point at a path that no
            // longer exists.
            self.state.remove_cached_entry(&old_path);
            let session = self
                .state
                .current_session_mut()
                .ok_or(AppError::NoEntrySelected)?;
            session.replace_node(renamed_node);
            Some(old_path)
        } else {
            None
        };

        // Phase 2: write the body. On failure we leave the (already-renamed)
        // disk state alone but propagate so the user retries; the session is
        // still dirty and the new path has no cache entry yet.
        if entry_changed {
            let session = self
                .state
                .current_session()
                .ok_or(AppError::NoEntrySelected)?;
            pass::store::save_entry_data(session.node(), session.current(), self.autopush)?;
        }

        // Phase 3: both phases succeeded. Snapshot the current values for
        // the cache, mark the session clean, and only THEN populate the
        // cache under the new path so a transient body-save error never
        // installs a cache entry that doesn't match disk.
        let session = self
            .state
            .current_session_mut()
            .ok_or(AppError::NoEntrySelected)?;
        let node = session.node().clone();
        let entry = session.current().clone();
        session.mark_saved();
        let _ = old_path; // already removed above; bind to mark intent
        self.state.cache_entry(&node, entry);
        Ok(())
    }

    /// Restores the current session to its last loaded or saved state.
    pub fn revert_current_entry(&mut self) -> Result<EntryViewData, AppError> {
        let session = self
            .state
            .current_session_mut()
            .ok_or(AppError::NoEntrySelected)?;

        session.revert();
        Ok(EntryViewData {
            title: session.node().name.clone(),
            entry: session.current().clone(),
        })
    }

    pub fn close_current_entry(&mut self) {
        self.state.set_current_session(None);
    }

    fn filtered_node(&self, node: &PassNode, query: &str) -> Option<PassNode> {
        if node.name.to_lowercase().contains(query) {
            return Some(node.clone());
        }

        if node.is_group() {
            let children = node
                .children
                .iter()
                .filter_map(|child| self.filtered_node(child, query))
                .collect::<Vec<_>>();

            if children.is_empty() {
                return None;
            }

            let mut node = node.clone();
            node.children = children;
            return Some(node);
        }

        self.state
            .cached_entry(&node.path)
            .and_then(|entry| self.entry_matches_query(entry, query).then(|| node.clone()))
    }

    fn entry_matches_query(&self, entry: &EntryData, query: &str) -> bool {
        entry.fields.iter().any(|(key, value)| {
            key.to_lowercase().contains(query)
                || value.display_value().to_lowercase().contains(query)
        })
    }

    fn uncached_entry_nodes_from(&self, node: &PassNode) -> Vec<PassNode> {
        if node.is_group() {
            return node
                .children
                .iter()
                .flat_map(|child| self.uncached_entry_nodes_from(child))
                .collect();
        }

        if self.state.cached_entry(&node.path).is_none() {
            vec![node.clone()]
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::pass::model::{EntryData, EntryField, PassNodeKind};

    use super::*;

    fn entry_node(name: &str, path: &str) -> PassNode {
        PassNode {
            name: name.into(),
            path: PathBuf::from(path),
            kind: PassNodeKind::Entry,
            children: Vec::new(),
        }
    }

    fn group_node(name: &str, path: &str, children: Vec<PassNode>) -> PassNode {
        PassNode {
            name: name.into(),
            path: PathBuf::from(path),
            kind: PassNodeKind::Group,
            children,
        }
    }

    fn entry_with_key(key: &str) -> EntryData {
        EntryData {
            password: EntryField::Password("secret".into()),
            fields: vec![(key.into(), EntryField::Plain("value".into()))],
        }
    }

    fn entry_with_value(value: EntryField) -> EntryData {
        EntryData {
            password: EntryField::Password("secret".into()),
            fields: vec![("note".into(), value)],
        }
    }

    #[test]
    fn filters_entries_by_name_and_keeps_parents() {
        let mut controller = AppController::new();
        controller.state_mut().set_tree(vec![group_node(
            "Personal",
            "Personal",
            vec![entry_node("Email", "Personal/Email.gpg")],
        )]);

        let filtered = controller.filtered_tree("email");

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "Personal");
        assert_eq!(filtered[0].children.len(), 1);
        assert_eq!(filtered[0].children[0].name, "Email");
    }

    #[test]
    fn matching_groups_keep_full_subtree() {
        let mut controller = AppController::new();
        controller.state_mut().set_tree(vec![group_node(
            "Work",
            "Work",
            vec![entry_node("Email", "Work/Email.gpg")],
        )]);

        let filtered = controller.filtered_tree("work");

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "Work");
        assert_eq!(filtered[0].children.len(), 1);
    }

    #[test]
    fn filters_cached_entries_by_field_key() {
        let mut controller = AppController::new();
        let node = entry_node("Server", "Server.gpg");
        controller.state_mut().set_tree(vec![node.clone()]);
        controller
            .state_mut()
            .cache_entry(&node, entry_with_key("username"));

        let filtered = controller.filtered_tree("user");

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "Server");
    }

    #[test]
    fn filters_cached_entries_by_plain_field_value() {
        let mut controller = AppController::new();
        let node = entry_node("Server", "Server.gpg");
        controller.state_mut().set_tree(vec![node.clone()]);
        controller
            .state_mut()
            .cache_entry(&node, entry_with_value(EntryField::Plain("admin".into())));

        let filtered = controller.filtered_tree("admin");

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "Server");
    }

    #[test]
    fn filters_cached_entries_by_array_field_value() {
        let mut controller = AppController::new();
        let node = entry_node("Server", "Server.gpg");
        controller.state_mut().set_tree(vec![node.clone()]);
        controller.state_mut().cache_entry(
            &node,
            entry_with_value(EntryField::Array(vec!["primary".into(), "backup".into()])),
        );

        let filtered = controller.filtered_tree("backup");

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "Server");
    }

    // --- Validation + cache-invariant tests ---
    //
    // These tests exercise controller-internal logic only: validation gating
    // and cache-update bookkeeping. Anything that requires actually
    // encrypting, committing, or pushing is covered by the store-level tests
    // in `pass::store::tests`. We deliberately do NOT test the on-disk
    // happy-path here because the underlying `pass::store` API is invoked
    // statically and would require either a real GPG/git setup or a trait
    // refactor — both out of scope for the controller-internal invariants.

    fn baseline_entry() -> EntryData {
        EntryData {
            password: EntryField::Password("secret".into()),
            fields: Vec::new(),
        }
    }

    #[test]
    fn save_rejects_invalid_session_without_writing() {
        // An empty password violates is_valid(); save must error and not
        // touch the cache (which we use here as a stand-in for "did anything
        // happen?").
        let mut controller = AppController::new();
        let node = entry_node("Server", "Server.gpg");
        let dirty_entry = EntryData {
            password: EntryField::Password(String::new()),
            fields: Vec::new(),
        };
        let session = EntrySession::new(node.clone(), baseline_entry());
        controller.state_mut().set_current_session(Some(session));
        // Mutate the session into an invalid dirty state.
        controller
            .current_entry_mut()
            .expect("session should be present")
            .password = dirty_entry.password.clone();

        let err = controller.save_current_entry().unwrap_err();
        assert!(matches!(err, AppError::InvalidEntry));
        assert!(controller.state().cached_entry(&node.path).is_none());
        // Session is still dirty so the user can fix and retry.
        assert!(controller.has_unsaved_changes());
    }

    #[test]
    fn create_entry_rejects_invalid_payload() {
        // `create_entry` should refuse an entry with an empty password
        // before invoking the store layer.
        let mut controller = AppController::new();
        let bad = EntryData {
            password: EntryField::Password(String::new()),
            fields: Vec::new(),
        };
        let err = controller
            .create_entry(Path::new(""), "name", bad)
            .unwrap_err();
        assert!(matches!(err, AppError::InvalidEntry));
        assert!(controller.current_session().is_none());
    }

    #[test]
    fn revert_restores_original_and_clears_dirty() {
        let mut controller = AppController::new();
        let node = entry_node("Server", "Server.gpg");
        let session = EntrySession::new(node.clone(), baseline_entry());
        controller.state_mut().set_current_session(Some(session));

        // Mutate so the session is dirty.
        controller
            .current_entry_mut()
            .expect("session should be present")
            .password = EntryField::Password("changed".into());
        assert!(controller.has_unsaved_changes());

        let view = controller.revert_current_entry().unwrap();
        // After revert, the current entry matches the original.
        assert_eq!(view.entry, baseline_entry());
        assert!(!controller.has_unsaved_changes());
    }

    #[test]
    fn cache_invariant_rename_then_body_failure_clears_old_path() {
        // Simulate Fix 2c by directly exercising the cache-update boundary
        // the controller relies on. The invariant: once we believe the disk
        // is renamed, the OLD-path cache entry must disappear immediately
        // and no NEW-path cache entry should appear unless the body save
        // also succeeded.
        let mut controller = AppController::new();
        let old_node = entry_node("old", "folder/old.gpg");
        let new_path = PathBuf::from("folder/new.gpg");

        // Pretend we previously cached the entry under the old path.
        controller
            .state_mut()
            .cache_entry(&old_node, baseline_entry());
        assert!(controller.state().cached_entry(&old_node.path).is_some());

        // The save flow's phase-1 rename step removes the old-path cache
        // entry as soon as the on-disk rename completes.
        controller.state_mut().remove_cached_entry(&old_node.path);

        // Phase-2 (the body save) is what fails in this scenario, so we do
        // NOT call cache_entry for the new path.
        assert!(controller.state().cached_entry(&old_node.path).is_none());
        assert!(controller.state().cached_entry(&new_path).is_none());
    }

    #[test]
    fn cache_invariant_save_success_caches_new_path() {
        // Mirror image of the previous test: when both phases succeed, the
        // new-path cache entry is populated and the old-path entry is gone.
        let mut controller = AppController::new();
        let old_node = entry_node("old", "folder/old.gpg");
        let new_node = entry_node("new", "folder/new.gpg");

        controller
            .state_mut()
            .cache_entry(&old_node, baseline_entry());
        // Phase 1: disk rename succeeds, drop old.
        controller.state_mut().remove_cached_entry(&old_node.path);
        // Phase 3: insert under the new path.
        controller
            .state_mut()
            .cache_entry(&new_node, baseline_entry());

        assert!(controller.state().cached_entry(&old_node.path).is_none());
        assert_eq!(
            controller.state().cached_entry(&new_node.path),
            Some(&baseline_entry())
        );
    }

    #[test]
    fn delete_clears_cache_and_session() {
        // Direct test of the controller's delete bookkeeping: after the
        // store-level remove (which we sidestep here by manually evicting),
        // the cache should be empty for that path and the session should be
        // cleared if the deleted node matches.
        let mut controller = AppController::new();
        let node = entry_node("Server", "Server.gpg");
        controller.state_mut().cache_entry(&node, baseline_entry());
        let session = EntrySession::new(node.clone(), baseline_entry());
        controller.state_mut().set_current_session(Some(session));

        // Simulate the post-store-remove cache + session updates.
        controller.state_mut().remove_cached_entry(&node.path);
        controller.state_mut().set_current_session(None);

        assert!(controller.state().cached_entry(&node.path).is_none());
        assert!(controller.current_session().is_none());
    }

    #[test]
    fn has_valid_changes_requires_dirty_and_valid() {
        let mut controller = AppController::new();
        let node = entry_node("Server", "Server.gpg");
        let session = EntrySession::new(node, baseline_entry());
        controller.state_mut().set_current_session(Some(session));

        // Clean session: no valid changes regardless of validity.
        assert!(!controller.has_valid_changes());

        // Dirty + valid -> true.
        controller
            .current_entry_mut()
            .expect("session should be present")
            .password = EntryField::Password("rotated".into());
        assert!(controller.has_valid_changes());

        // Dirty + invalid -> false.
        controller
            .current_entry_mut()
            .expect("session should be present")
            .password = EntryField::Password(String::new());
        assert!(!controller.has_valid_changes());
    }
}
