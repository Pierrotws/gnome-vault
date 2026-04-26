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
    },
};
use std::path::Path;

pub struct AppController {
    state: AppState,
    autopush: bool,
}

impl AppController {
    /// Creates a controller with empty application state.
    pub fn new() -> Self {
        Self {
            state: AppState::new(),
            autopush: true,
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

    /// Reloads the password-store tree from disk.
    ///
    /// This refreshes the visible node tree only. Cached decrypted entries are
    /// kept so reopening an already viewed entry does not decrypt the file again.
    pub fn reload_tree(&mut self) -> Result<(), AppError> {
        let tree = pass::store::load_password_store()?;
        self.state.set_tree(tree);
        Ok(())
    }

    /// Lists commits reachable from the current password-store branch.
    pub fn load_changes(&self) -> Result<Vec<GitChange>, AppError> {
        Ok(pass::store::load_changes()?)
    }

    /// Lists one page of commits reachable from the current password-store branch.
    pub fn load_changes_page(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<GitChange>, AppError> {
        Ok(pass::store::load_changes_page(offset, limit)?)
    }

    /// Reverts a password-store commit, then clears stale entry state.
    pub fn revert_change(&mut self, commit_id: &str) -> Result<(), AppError> {
        pass::store::revert_change(commit_id, self.autopush)?;
        self.state.clear_entry_cache();
        self.state.set_current_session(None);
        self.reload_tree()?;
        Ok(())
    }

    /// Rolls the branch back to a commit after creating a remote backup branch.
    pub fn rollback_to_change(&mut self, commit_id: &str) -> Result<String, AppError> {
        let backup_branch = pass::store::rollback_to_change(commit_id)?;
        self.state.clear_entry_cache();
        self.state.set_current_session(None);
        self.reload_tree()?;
        Ok(backup_branch)
    }

    /// Pushes committed local password-store changes.
    pub fn push_changes(&self) -> Result<(), AppError> {
        Ok(pass::store::push_changes()?)
    }

    /// Opens an entry node and returns view data for the UI.
    ///
    /// Decrypted entries are cached by node path. The first open reads and
    /// decrypts the `.gpg` file; later opens reuse the cached [`EntryData`].
    pub fn open_node(&mut self, node: PassNode) -> Result<EntryViewData, AppError> {
        let title = node.name.clone();
        let entry = match self.state.cached_entry(&node.path) {
            Some(entry) => {
                log::debug!("entry cache hit: {}", node.path.display());
                entry.clone()
            }
            None => {
                log::debug!("entry cache miss: {}", node.path.display());
                let entry = pass::store::load_entry_from_node(&node)?;
                self.state.cache_entry(&node, entry.clone());
                entry
            }
        };
        let session = EntrySession::new(node, entry.clone());
        self.state.set_current_session(Some(session));

        Ok(EntryViewData { title, entry })
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
    pub fn save_current_entry(&mut self) -> Result<(), AppError> {
        let (entry_changed, rename) = {
            let session = self
                .state
                .current_session()
                .ok_or(AppError::NoEntrySelected)?;
            let rename = session
                .has_name_changes()
                .then(|| (session.node().clone(), session.current_name().to_string()));

            (session.has_entry_changes(), rename)
        };

        let old_path = if let Some((node, name)) = rename {
            let old_path = node.path.clone();
            let renamed_node = pass::store::rename_entry(&node, &name, self.autopush)?;
            let session = self
                .state
                .current_session_mut()
                .ok_or(AppError::NoEntrySelected)?;
            session.replace_node(renamed_node);
            Some(old_path)
        } else {
            None
        };

        let session = self
            .state
            .current_session_mut()
            .ok_or(AppError::NoEntrySelected)?;

        //no validate yet
        //session.current().validate()?;
        if entry_changed {
            pass::store::save_entry_data(session.node(), session.current(), self.autopush)?;
        }
        let node = session.node().clone();
        let entry = session.current().clone();
        session.mark_saved();
        if let Some(old_path) = old_path {
            self.state.remove_cached_entry(&old_path);
        }
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
}
