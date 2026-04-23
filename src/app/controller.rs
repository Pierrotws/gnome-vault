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

    pub fn reload_tree(&mut self) -> Result<(), AppError> {
        let tree = pass::store::load_password_store()?;
        self.state.set_tree(tree);
        Ok(())
    }

    pub fn open_node(&mut self, node: PassNode) -> Result<EntryViewData, AppError> {
        let title = node.name.clone();
        let entry = pass::store::load_entry_from_node(&node)?;
        let session = EntrySession::new(node, entry.clone());
        self.state.set_current_session(Some(session));

        Ok(EntryViewData { title, entry })
    }

    pub fn update_current_entry(&mut self, data: EntryViewData) -> Result<(), AppError> {
        eprintln!("update current entry");
        let session = self
            .state
            .current_session_mut()
            .ok_or(AppError::NoEntrySelected)?;

        session.replace_current(data.title, data.entry);
        Ok(())
    }

    pub fn save_current_entry(&mut self) -> Result<(), AppError> {
        let session = self
            .state
            .current_session_mut()
            .ok_or(AppError::NoEntrySelected)?;

        //no validate yet
        //session.current().validate()?;
        pass::store::save_entry_data(session.node(), session.current())?;
        session.mark_saved();
        Ok(())
    }

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
