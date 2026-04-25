// This file is the API used by the UI.
//
// The UI should not call pass::store::* directly.
// The UI should mostly talk to AppController.

use crate::{
    app::{
        app_error::AppError,
        state::{AppState, EntrySession, EntryViewData},
    },
    pass::{
        self,
        model::{EntryData, PassNode},
    },
};

pub struct AppController {
    state: AppState,
}

impl AppController {
    /// Creates a controller with empty application state.
    pub fn new() -> Self {
        Self {
            state: AppState::new(),
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

    /// Reloads the password-store tree from disk.
    ///
    /// This refreshes the visible node tree only. Cached decrypted entries are
    /// kept so reopening an already viewed entry does not decrypt the file again.
    pub fn reload_tree(&mut self) -> Result<(), AppError> {
        let tree = pass::store::load_password_store()?;
        self.state.set_tree(tree);
        Ok(())
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

    /// Persists the current entry, marks the session clean, and updates cache.
    pub fn save_current_entry(&mut self) -> Result<(), AppError> {
        let session = self
            .state
            .current_session_mut()
            .ok_or(AppError::NoEntrySelected)?;

        //no validate yet
        //session.current().validate()?;
        pass::store::save_entry_data(session.node(), session.current())?;
        let node = session.node().clone();
        let entry = session.current().clone();
        session.mark_saved();
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
