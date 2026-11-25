mod autoload;
mod group_content;
mod imp;
mod new_entry_dialog;
mod preferences;
mod setup;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use adw::{prelude::*, subclass::prelude::*};
use gtk::{gio, glib};

use crate::app::changes;
use crate::app::controller::AppController;
use crate::pass::model::{PassNode, PassNodeKind};
use crate::pass::store::StoreStatus;
use crate::ui::vault_view::{
    build_group_selection_with_root, build_selection_from_nodes_with_autoexpand,
};
use crate::ui::EntryView;

const CHANGES_PAGE_SIZE: usize = 50;
const SIMPLE_SELECTION_WIDTH: i32 = 220;
const GROUP_TREE_WIDTH: i32 = 170;

glib::wrapper! {
    pub struct MainWindow(ObjectSubclass<imp::MainWindow>)
        @extends adw::ApplicationWindow, gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk::Accessible, gtk::Buildable,
                    gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl MainWindow {
    pub fn new(app: &adw::Application, controller: Rc<RefCell<AppController>>) -> Self {
        let obj: Self = glib::Object::builder().property("application", app).build();
        let settings = gio::Settings::new(crate::APP_ID);

        let _ = obj.imp().controller.set(controller);
        let _ = obj.imp().settings.set(settings);
        obj.apply_store_dir_setting();
        obj.apply_autopush_setting();
        obj.apply_branch_setting();

        obj.setup_views();
        obj.setup_callbacks();
        obj
    }

    fn controller(&self) -> Rc<RefCell<AppController>> {
        self.imp()
            .controller
            .get()
            .expect("MainWindow controller must be set")
            .clone()
    }

    fn setup_views(&self) {
        self.setup_vault_setup_view();
        self.apply_tree_view_size_setting();

        match AppController::store_status() {
            StoreStatus::Ready => {}
            StoreStatus::PassWithoutGit => {
                self.show_enable_history_view();
                return;
            }
            status @ (StoreStatus::Missing | StoreStatus::Empty | StoreStatus::Unrelated) => {
                log::debug!("no vault to open ({status:?}), showing setup view");
                self.show_setup_view();
                return;
            }
        }
        if let Err(err) = self.reload_vault_tree() {
            log::debug!("vault unavailable, showing setup view: {err}");
            self.show_setup_view();
            return;
        }
        self.show_opened_vault();
    }

    /// Switches to the app view on a vault whose tree the controller has
    /// just loaded: at startup, or once setup has created or adopted one.
    fn show_opened_vault(&self) {
        let imp = self.imp();
        imp.entry_view.display_empty();
        if self.show_group_view_enabled() {
            self.show_root_group_content();
        } else {
            self.show_empty_content();
        }
        self.set_edit_unlock_state(false, false, false);
        self.show_app_view();
        self.refresh_changes_badge();
        self.start_autoload_if_enabled();
    }

    pub fn setup_callbacks(&self) {
        let imp = self.imp();

        imp.new_entry_button.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                window.show_new_entry_dialog(None);
            }
        ));

        imp.preferences_button.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                window.show_preferences_dialog();
            }
        ));

        imp.vault_view.connect_create_entry_requested(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, folder_path| {
                window.show_new_entry_dialog(Some(folder_path));
            }
        ));

        imp.changes_view
            .connect_revert_change_requested(glib::clone!(
                #[weak(rename_to = window)]
                self,
                move |_, commit_id| {
                    window.confirm_revert_change(&commit_id);
                }
            ));

        imp.changes_view
            .connect_rollback_change_requested(glib::clone!(
                #[weak(rename_to = window)]
                self,
                move |_, commit_id| {
                    window.confirm_rollback_change(&commit_id);
                }
            ));

        imp.changes_view.connect_push_requested(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                window.push_changes();
            }
        ));

        imp.changes_view.connect_load_more_requested(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                if let Err(err) = window.load_more_changes() {
                    window.imp().changes_view.set_loading_more(false);
                    window.imp().entry_view.show_error_dialog(&err.to_string());
                }
            }
        ));

        imp.changes_button.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                if window.controller().borrow().has_unsaved_changes() {
                    window
                        .imp()
                        .entry_view
                        .show_error_dialog("Save or cancel current changes before viewing changes");
                    return;
                }
                if let Err(err) = window.reload_changes_view() {
                    window.imp().entry_view.show_error_dialog(&err.to_string());
                    return;
                }
                window.show_changes_content();
            }
        ));

        imp.vault_view.connect_delete_entry_requested(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, entry_path, entry_name| {
                let node = PassNode {
                    name: entry_name,
                    path: PathBuf::from(entry_path),
                    kind: PassNodeKind::Entry,
                };
                window.confirm_delete_entry(node);
            }
        ));

        // Same delete flow as the vault tree, just sourced from a
        // right-click in the group view's entries list.
        imp.group_view.connect_delete_entry_requested(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, entry_path, entry_name| {
                let node = PassNode {
                    name: entry_name,
                    path: PathBuf::from(entry_path),
                    kind: PassNodeKind::Entry,
                };
                window.confirm_delete_entry(node);
            }
        ));

        imp.vault_view.connect_delete_folder_requested(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, folder_path, folder_name| {
                let node = PassNode {
                    name: folder_name,
                    path: PathBuf::from(folder_path),
                    kind: PassNodeKind::Group(Vec::new()),
                };
                window.confirm_delete_folder(node);
            }
        ));

        imp.vault_view.connect_entry_selected(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |nav| {
                let Some(node) = nav.selected_node() else {
                    return;
                };

                // Refuse to switch entries while the open one has unsaved
                // edits; otherwise the dirty session is silently overwritten
                // when open_entry_node replaces the current EntrySession.
                if window.controller().borrow().has_unsaved_changes() {
                    window.imp().entry_view.show_error_dialog(
                        "Save or cancel current changes before opening another entry",
                    );
                    return;
                }

                window.open_entry_node(node);
            }
        ));

        imp.vault_view.connect_group_activated(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |nav| {
                if !window.show_group_view_enabled() {
                    return;
                }

                let Some(node) = nav.selected_node() else {
                    return;
                };

                if window.controller().borrow().has_unsaved_changes() {
                    window.imp().entry_view.show_error_dialog(
                        "Save or cancel current changes before selecting a group",
                    );
                    return;
                }

                // While searching, the right pane belongs to the search
                // results; programmatic and manual group selection both
                // leave it untouched.
                if !window
                    .imp()
                    .vault_view
                    .search_entry()
                    .text()
                    .trim()
                    .is_empty()
                {
                    return;
                }

                window.show_group_content(&node);
            }
        ));

        imp.entry_view.connect_entry_changed(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |view| {
                let controller = window.controller();
                if controller.borrow().current_session().is_none() {
                    return;
                }

                let updated = view.to_entry_view_data();

                let result = controller.borrow_mut().update_current_entry(updated);

                match result {
                    Ok(()) => {
                        // Snapshot the bools we need in a single borrow before
                        // any widget setter runs, so a setter-driven signal
                        // can never re-enter the controller while a borrow is
                        // still live.
                        let (is_dirty, is_valid) = {
                            let controller = controller.borrow();
                            (
                                controller.has_unsaved_changes(),
                                controller.has_valid_changes(),
                            )
                        };
                        view.set_cancellable(is_dirty);
                        view.set_saveable(is_valid);
                        window.set_edit_unlock_state(true, view.is_editable_mode(), is_dirty);
                    }
                    Err(err) => view.show_error_dialog(&err.to_string()),
                }
            }
        ));

        imp.group_view.connect_entry_activated(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, node| {
                let search_active = !window
                    .imp()
                    .vault_view
                    .search_entry()
                    .text()
                    .trim()
                    .is_empty();
                // Path::parent yields Some("") for top-level entries, so filter
                // out the empty path — there's no parent group to select.
                let parent_path = node
                    .path
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .map(|p| p.to_path_buf());
                window.open_entry_node(node);
                if search_active {
                    if let Some(parent) = parent_path {
                        // The parent group may have been filtered out of the
                        // current tree by the active search; in that case the
                        // previous selection silently persists. Log so the
                        // mismatch is debuggable without spamming on the happy
                        // path.
                        if !window.imp().vault_view.select_node_by_path(&parent) {
                            log::debug!("select_node_by_path: parent not in filtered tree");
                        }
                    }
                }
            }
        ));

        imp.entry_view.connect_save_requested(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |view| {
                let controller = window.controller();
                let result = controller.borrow_mut().save_current_entry();

                match result {
                    Ok(()) => {
                        // Snapshot bools before any widget setter runs;
                        // setters can synchronously emit signals that
                        // re-enter the controller and would otherwise panic.
                        let (is_dirty, is_valid) = {
                            let controller = controller.borrow();
                            (
                                controller.has_unsaved_changes(),
                                controller.has_valid_changes(),
                            )
                        };
                        view.set_editable_mode(false);
                        if let Err(err) = window.reload_vault_tree() {
                            view.show_error_dialog(&err.to_string());
                        }
                        if let Err(err) = window.reload_changes_view() {
                            view.show_error_dialog(&err.to_string());
                        }
                        window.set_edit_unlock_state(true, false, false);
                        view.set_cancellable(is_dirty);
                        view.set_saveable(is_valid);
                    }
                    Err(err) => view.show_error_dialog(&err.to_string()),
                }
            }
        ));

        imp.entry_view.connect_delete_requested(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                window.confirm_delete_current_entry();
            }
        ));

        imp.entry_view.connect_revert_requested(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                let controller = window.controller();
                let entry_view = window.imp().entry_view.clone();
                let result = controller.borrow_mut().revert_current_entry();

                match result {
                    Ok(entry_data) => {
                        // Snapshot bools before any widget setter runs.
                        let (is_dirty, is_valid) = {
                            let controller = controller.borrow();
                            (
                                controller.has_unsaved_changes(),
                                controller.has_valid_changes(),
                            )
                        };
                        entry_view.set_entry_data(&entry_data);
                        window.set_edit_unlock_state(true, false, false);
                        entry_view.set_cancellable(is_dirty);
                        entry_view.set_saveable(is_valid);
                    }
                    Err(err) => entry_view.show_error_dialog(&err.to_string()),
                }
            }
        ));

        imp.lock_vault_button.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                let entry_view = window.imp().entry_view.clone();
                let editing = !entry_view.is_editable_mode();
                entry_view.set_editable_mode(editing);
                // Read the live dirty flag rather than hard-coding `false`.
                // The lock button is gated insensitive while dirty so this
                // path is only reachable when has_unsaved_changes() is
                // false, but threading the snapshot through avoids lying
                // to set_edit_unlock_state if that gating ever loosens.
                let is_dirty = window.controller().borrow().has_unsaved_changes();
                window.set_edit_unlock_state(true, editing, is_dirty);
            }
        ));

        imp.vault_view.connect_search_changed(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                window.rebuild_vault_tree();
            }
        ));

        imp.setup_wizard.connect_vault_created(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, store_dir| {
                window.open_created_vault(&store_dir);
            }
        ));

        imp.enable_history_button.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_| {
                window.enable_history_from_setup();
            }
        ));
    }

    pub fn get_entry_view(&self) -> EntryView {
        self.imp().entry_view.clone()
    }

    fn open_entry_node(&self, node: PassNode) {
        let result = self.controller().borrow_mut().open_node(node);
        let entry_view = self.imp().entry_view.clone();

        match result {
            Ok(data) => {
                // Snapshot bools before any widget setter runs.
                let (is_dirty, is_valid) = {
                    let controller = self.controller();
                    let controller = controller.borrow();
                    (
                        controller.has_unsaved_changes(),
                        controller.has_valid_changes(),
                    )
                };
                entry_view.set_entry_data(&data);
                self.show_entry_content();
                self.set_edit_unlock_state(true, false, false);
                entry_view.set_cancellable(is_dirty);
                entry_view.set_saveable(is_valid);
            }
            Err(err) => entry_view.show_error_dialog(&err.to_string()),
        }
    }

    fn confirm_delete_entry(&self, node: PassNode) {
        if self.controller().borrow().has_unsaved_changes() {
            self.imp()
                .entry_view
                .show_error_dialog("Save or cancel current changes before deleting an entry");
            return;
        }

        let dialog = adw::AlertDialog::builder()
            .heading(format!("Delete \u{201C}{}\u{201D}?", node.name))
            .body("The entry is removed and a single commit is recorded.")
            .build();
        dialog.add_responses(&[("cancel", "Cancel"), ("delete", "Delete")]);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);

        let window = self.clone();
        dialog.connect_response(None, move |_, response| {
            if response == "delete" {
                window.delete_entry(node.clone());
            }
        });
        dialog.present(Some(self));
    }

    fn delete_entry(&self, node: PassNode) {
        let deleted_current = match self.controller().borrow_mut().delete_entry(node) {
            Ok(deleted_current) => deleted_current,
            Err(err) => {
                self.imp().entry_view.show_error_dialog(&err.to_string());
                return;
            }
        };

        if deleted_current {
            self.imp().entry_view.display_empty();
            self.show_empty_content();
            self.set_edit_unlock_state(false, false, false);
        }
        if let Err(err) = self.reload_vault_tree() {
            self.imp().entry_view.show_error_dialog(&err.to_string());
        }
        if let Err(err) = self.reload_changes_view() {
            self.imp().entry_view.show_error_dialog(&err.to_string());
        }
    }

    fn confirm_delete_current_entry(&self) {
        if self.controller().borrow().has_unsaved_changes() {
            self.imp()
                .entry_view
                .show_error_dialog("Save or cancel current changes before deleting an entry");
            return;
        }

        // Snapshot the entry name now: the dialog outlives this borrow,
        // and the AlertDialog body is only allowed owned data.
        let name = self
            .controller()
            .borrow()
            .current_session()
            .map(|s| s.node().name.clone());
        let Some(name) = name else {
            return;
        };

        let dialog = adw::AlertDialog::builder()
            .heading(format!("Delete \u{201C}{name}\u{201D}?"))
            .body("The entry is removed and a single commit is recorded.")
            .build();
        dialog.add_responses(&[("cancel", "Cancel"), ("delete", "Delete")]);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);

        let window = self.clone();
        dialog.connect_response(None, move |_, response| {
            if response == "delete" {
                window.delete_current_entry();
            }
        });
        dialog.present(Some(self));
    }

    fn delete_current_entry(&self) {
        let entry_view = self.imp().entry_view.clone();
        let result = self.controller().borrow_mut().delete_current_entry();
        match result {
            Ok(_) => {
                entry_view.display_empty();
                if let Err(err) = self.reload_vault_tree() {
                    entry_view.show_error_dialog(&err.to_string());
                }
                if let Err(err) = self.reload_changes_view() {
                    entry_view.show_error_dialog(&err.to_string());
                }
                self.show_empty_content();
                self.set_edit_unlock_state(false, false, false);
            }
            Err(err) => entry_view.show_error_dialog(&err.to_string()),
        }
    }

    fn confirm_delete_folder(&self, node: PassNode) {
        if self.controller().borrow().has_unsaved_changes() {
            self.imp()
                .entry_view
                .show_error_dialog("Save or cancel current changes before deleting a folder");
            return;
        }

        let dialog = adw::AlertDialog::builder()
            .heading(format!("Delete \u{201C}{}\u{201D}?", node.name))
            .body(
                "Every entry inside this folder is removed and a single \
                 commit is recorded.",
            )
            .build();
        dialog.add_responses(&[("cancel", "Cancel"), ("delete", "Delete")]);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);

        let window = self.clone();
        dialog.connect_response(None, move |_, response| {
            if response == "delete" {
                window.delete_folder(node.clone());
            }
        });
        dialog.present(Some(self));
    }

    fn delete_folder(&self, node: PassNode) {
        let closed_session = match self.controller().borrow_mut().delete_folder(node) {
            Ok(closed) => closed,
            Err(err) => {
                self.imp().entry_view.show_error_dialog(&err.to_string());
                return;
            }
        };

        if closed_session {
            self.imp().entry_view.display_empty();
            self.show_empty_content();
            self.set_edit_unlock_state(false, false, false);
        }
        if let Err(err) = self.reload_vault_tree() {
            self.imp().entry_view.show_error_dialog(&err.to_string());
        }
        if let Err(err) = self.reload_changes_view() {
            self.imp().entry_view.show_error_dialog(&err.to_string());
        }
    }

    fn confirm_revert_change(&self, commit_id: &str) {
        if self.controller().borrow().has_unsaved_changes() {
            self.imp()
                .entry_view
                .show_error_dialog("Save or cancel current changes before undoing a change");
            return;
        }

        let dialog = adw::AlertDialog::builder()
            .heading("Undo this change?")
            .body(
                "A new change will be recorded that reverses the selected one. \
                 The original change is kept in history.",
            )
            .build();
        dialog.add_responses(&[("cancel", "Cancel"), ("revert", "Undo")]);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        dialog.set_response_appearance("revert", adw::ResponseAppearance::Destructive);

        let window = self.clone();
        let commit_id = commit_id.to_string();
        dialog.connect_response(None, move |_, response| {
            if response == "revert" {
                window.revert_change(&commit_id);
            }
        });
        dialog.present(Some(self));
    }

    fn revert_change(&self, commit_id: &str) {
        if self.controller().borrow().has_unsaved_changes() {
            self.imp()
                .entry_view
                .show_error_dialog("Save or cancel current changes before undoing a change");
            return;
        }

        let autopush = self.controller().borrow().autopush();
        let commit_id = commit_id.to_string();
        let window = self.clone();

        glib::spawn_future_local(async move {
            let result =
                match gio::spawn_blocking(move || changes::revert(&commit_id, autopush)).await {
                    Ok(r) => r,
                    Err(_) => {
                        window
                            .imp()
                            .entry_view
                            .show_error_dialog("Undo task panicked unexpectedly");
                        return;
                    }
                };

            if let Err(err) = result {
                window.imp().entry_view.show_error_dialog(&err.to_string());
                return;
            }

            window.controller().borrow_mut().clear_loaded_entry_state();
            window.imp().entry_view.display_empty();
            window.show_empty_content();
            window.set_edit_unlock_state(false, false, false);
            if let Err(err) = window.reload_vault_tree() {
                window.imp().entry_view.show_error_dialog(&err.to_string());
            }
            if let Err(err) = window.reload_changes_view() {
                window.imp().entry_view.show_error_dialog(&err.to_string());
            }
            // Re-warm the cache the user just lost: clear_loaded_entry_state
            // dropped every previously decrypted entry, so without this a
            // user with "Load entries at startup" enabled would finish the
            // revert with a cold cache until the next launch.
            window.start_autoload_if_enabled();
        });
    }

    fn confirm_rollback_change(&self, commit_id: &str) {
        if self.controller().borrow().has_unsaved_changes() {
            self.imp().entry_view.show_error_dialog(
                "Save or cancel current changes before discarding later changes",
            );
            return;
        }

        let dialog = adw::AlertDialog::builder()
            .heading("Discard changes after this point?")
            .body(
                "Every change after the selected one will be removed from the \
                 vault. A backup is saved to the remote first so you can \
                 recover if something goes wrong.",
            )
            .build();
        dialog.add_responses(&[("cancel", "Cancel"), ("rollback", "Discard later changes")]);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        dialog.set_response_appearance("rollback", adw::ResponseAppearance::Destructive);

        let window = self.clone();
        let commit_id = commit_id.to_string();
        dialog.connect_response(None, move |_, response| {
            if response == "rollback" {
                window.rollback_change(&commit_id);
            }
        });
        dialog.present(Some(self));
    }

    fn rollback_change(&self, commit_id: &str) {
        let commit_id = commit_id.to_string();
        let window = self.clone();

        glib::spawn_future_local(async move {
            let result = match gio::spawn_blocking(move || changes::rollback(&commit_id)).await {
                Ok(r) => r,
                Err(_) => {
                    window
                        .imp()
                        .entry_view
                        .show_error_dialog("Discard-changes task panicked unexpectedly");
                    return;
                }
            };

            let backup_branch = match result {
                Ok(b) => b,
                Err(err) => {
                    window.imp().entry_view.show_error_dialog(&err.to_string());
                    return;
                }
            };

            window.controller().borrow_mut().clear_loaded_entry_state();
            window.imp().entry_view.display_empty();
            window.show_empty_content();
            window.set_edit_unlock_state(false, false, false);
            if let Err(err) = window.reload_vault_tree() {
                window.imp().entry_view.show_error_dialog(&err.to_string());
            }
            if let Err(err) = window.reload_changes_view() {
                window.imp().entry_view.show_error_dialog(&err.to_string());
            }
            // Same rationale as revert_change: clear_loaded_entry_state
            // dropped every previously decrypted entry, so without this
            // an autoload user finishes with a cold cache until next launch.
            window.start_autoload_if_enabled();

            let dialog = adw::AlertDialog::builder()
                .heading("Changes discarded")
                .body(format!(
                    "A backup was saved to the remote as \"{backup_branch}\" \
                     in case you need to recover."
                ))
                .build();
            dialog.add_responses(&[("ok", "OK")]);
            dialog.set_default_response(Some("ok"));
            dialog.set_close_response("ok");
            dialog.present(Some(&window));
        });
    }

    fn reload_vault_tree(&self) -> Result<(), crate::app::app_error::AppError> {
        let controller = self.controller();
        {
            let mut controller = controller.borrow_mut();
            controller.reload_tree()?;
        }
        self.rebuild_vault_tree();
        Ok(())
    }

    fn rebuild_vault_tree(&self) {
        // The selection layout (tree vs split) is set in show_app_view
        // and refreshed when the show-group-view preference toggles —
        // it doesn't change between search keystrokes. Skip the call
        // here so each keystroke only swaps the selection model.
        let search_text = self.imp().vault_view.search_entry().text().to_string();
        let nodes = self.controller().borrow().filtered_tree(&search_text);
        let group_view = self.show_group_view_enabled();
        let selection = if group_view {
            build_group_selection_with_root(&nodes, true)
        } else {
            build_selection_from_nodes_with_autoexpand(&nodes, !search_text.trim().is_empty())
        };
        self.imp().vault_view.set_selection_model(&selection);
        if group_view {
            self.show_root_group_content();
        }
    }

    fn reload_changes_view(&self) -> Result<(), crate::app::app_error::AppError> {
        // Probe one extra commit so we can tell "exactly PAGE_SIZE
        // changes total" apart from "more changes available". The
        // previous heuristic (`changes.len() == CHANGES_PAGE_SIZE`)
        // incorrectly armed load-more in the former case.
        let mut changes = self
            .controller()
            .borrow()
            .load_changes_page(0, CHANGES_PAGE_SIZE + 1)?;
        let has_more = changes.len() > CHANGES_PAGE_SIZE;
        if has_more {
            changes.truncate(CHANGES_PAGE_SIZE);
        }
        self.imp().loaded_changes_count.set(changes.len());
        self.imp().changes_view.set_changes(&changes, has_more);
        self.refresh_changes_badge();
        Ok(())
    }

    /// Reads the unpushed-commit count from the controller and updates
    /// the small badge on the Changes button. Hidden when there is
    /// nothing ahead of upstream; otherwise shows the count.
    fn refresh_changes_badge(&self) {
        let badge = &self.imp().changes_badge;
        let count = match self.controller().borrow().unpushed_count() {
            Ok(n) => n,
            Err(err) => {
                log::debug!("unable to compute unpushed count: {err}");
                badge.set_visible(false);
                return;
            }
        };
        if count == 0 {
            badge.set_visible(false);
        } else {
            badge.set_label(&count.to_string());
            badge.set_visible(true);
        }
    }

    fn load_more_changes(&self) -> Result<(), crate::app::app_error::AppError> {
        let offset = self.imp().loaded_changes_count.get();
        let mut changes = self
            .controller()
            .borrow()
            .load_changes_page(offset, CHANGES_PAGE_SIZE + 1)?;
        let has_more = changes.len() > CHANGES_PAGE_SIZE;
        if has_more {
            changes.truncate(CHANGES_PAGE_SIZE);
        }
        self.imp().loaded_changes_count.set(offset + changes.len());
        self.imp().changes_view.append_changes(&changes, has_more);
        Ok(())
    }

    fn settings(&self) -> gio::Settings {
        self.imp()
            .settings
            .get()
            .expect("MainWindow settings must be set")
            .clone()
    }

    fn settings_has_key(&self, key: &str) -> bool {
        gio::SettingsSchemaSource::default()
            .and_then(|source| source.lookup(crate::APP_ID, true))
            .is_some_and(|schema| schema.has_key(key))
    }

    fn setting_boolean(&self, key: &str, default: bool) -> bool {
        if self.settings_has_key(key) {
            self.settings().boolean(key)
        } else {
            default
        }
    }

    fn setting_string(&self, key: &str, default: &str) -> String {
        if self.settings_has_key(key) {
            self.settings().string(key).into()
        } else {
            default.to_string()
        }
    }

    fn show_group_view_enabled(&self) -> bool {
        self.setting_boolean("show-group-view", true)
    }

    fn show_app_view(&self) {
        let imp = self.imp();
        self.update_selection_layout(true);
        imp.main_stack.set_visible_child_name("app");
        imp.new_entry_button.set_sensitive(true);
        imp.changes_button.set_sensitive(true);
        imp.lock_vault_button.set_sensitive(false);
        imp.window_title.set_subtitle("Read-only");
    }

    fn show_empty_content(&self) {
        let imp = self.imp();
        imp.content_stack.set_visible_child_name("empty");
        imp.lock_vault_button.set_sensitive(false);
        imp.window_title.set_subtitle("Read-only");
    }

    fn show_entry_content(&self) {
        let imp = self.imp();
        imp.content_stack.set_visible_child_name("entry");
        let has_entry = self.controller().borrow().current_session().is_some();
        let editing = imp.entry_view.is_editable_mode();
        let is_dirty = self.controller().borrow().has_unsaved_changes();
        self.set_edit_unlock_state(has_entry, editing, is_dirty);
    }

    fn update_selection_layout(&self, reset_position: bool) {
        if self.show_group_view_enabled() {
            self.show_split_selection_layout(reset_position);
        } else {
            self.show_tree_selection_layout(reset_position);
        }
    }

    fn show_tree_selection_layout(&self, reset_position: bool) {
        let imp = self.imp();
        if imp.simple_vault_bin.child().is_none() {
            imp.group_vault_bin.set_child(Option::<&gtk::Widget>::None);
            imp.simple_vault_bin.set_child(Some(&imp.vault_view.get()));
        }
        imp.selection_zone.set_width_request(-1);
        // The selection area is just the tree here: keep its width on
        // window resizes and let the entry pane (the second column)
        // take the difference.
        imp.main_paned.set_resize_start_child(false);
        if reset_position {
            imp.main_paned.set_position(SIMPLE_SELECTION_WIDTH);
        }
        imp.selection_stack.set_visible_child_name("tree");
    }

    fn show_split_selection_layout(&self, reset_position: bool) {
        let imp = self.imp();
        if imp.group_vault_bin.child().is_none() {
            imp.simple_vault_bin.set_child(Option::<&gtk::Widget>::None);
            imp.group_vault_bin.set_child(Some(&imp.vault_view.get()));
        }
        imp.selection_zone.set_width_request(-1);
        // Window resizes are shared between the selection area and the
        // entry pane; within the selection area, selection_paned hands
        // all of it to the entries list so the tree keeps its width.
        imp.main_paned.set_resize_start_child(true);
        if reset_position {
            // Columns take a third of the window, the entry pane the
            // other two. Main-paned resizes are proportional in this
            // layout (resize-start-child), so the ratio holds as the
            // window is resized. Before the first allocation width() is
            // 0, so fall back to the default width.
            let window_width = match self.width() {
                0 => self.default_width(),
                width => width,
            };
            imp.main_paned.set_position(window_width / 3);
            imp.selection_paned.set_position(GROUP_TREE_WIDTH);
        }
        imp.selection_stack.set_visible_child_name("group");
    }

    fn show_changes_content(&self) {
        let imp = self.imp();
        imp.content_stack.set_visible_child_name("changes");
        imp.lock_vault_button.set_sensitive(false);
        imp.window_title.set_subtitle("Changes");
    }

    fn show_error_dialog(&self, message: &str) {
        let dialog = adw::AlertDialog::builder()
            .heading("Error")
            .body(message)
            .build();
        dialog.add_responses(&[("ok", "OK")]);
        dialog.set_default_response(Some("ok"));
        dialog.set_close_response("ok");
        dialog.present(Some(self));
    }

    fn push_changes(&self) {
        // Push runs on a worker thread so the UI does not freeze for the
        // duration of the network round-trip. The reload_changes_view that
        // follows is fast (filesystem only) and happens back on the main loop.
        let window = self.clone();
        let branch = self.controller().borrow().branch().map(str::to_string);
        glib::spawn_future_local(async move {
            let result = match gio::spawn_blocking(move || changes::push(branch.as_deref())).await {
                Ok(r) => r,
                Err(_) => {
                    window
                        .imp()
                        .entry_view
                        .show_error_dialog("Sync-to-remote task panicked unexpectedly");
                    return;
                }
            };
            if let Err(err) = result {
                window.imp().entry_view.show_error_dialog(&err.to_string());
                return;
            }
            if let Err(err) = window.reload_changes_view() {
                window.imp().entry_view.show_error_dialog(&err.to_string());
            }
        });
    }

    fn set_edit_unlock_state(&self, has_entry: bool, editing: bool, is_dirty: bool) {
        let imp = self.imp();
        imp.lock_vault_button.set_sensitive(has_entry && !is_dirty);
        imp.lock_vault_button.set_icon_name(if editing {
            "changes-allow-symbolic"
        } else {
            "system-lock-screen-symbolic"
        });
        imp.lock_vault_button.set_tooltip_text(Some(if editing {
            "Editing unlocked"
        } else {
            "Unlock editing"
        }));
        imp.window_title
            .set_subtitle(if editing { "Editing" } else { "Read-only" });
    }
}
