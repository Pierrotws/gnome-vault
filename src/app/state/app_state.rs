// Global state of App

use crate::{app::state::EntrySession, pass::model::PassNode};

#[derive(Clone, Default)]
pub struct AppState {
    tree: Vec<PassNode>,
    selected_node: Option<PassNode>,
    current_session: Option<EntrySession>,
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
