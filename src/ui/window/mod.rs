mod autoload;
mod imp;
mod new_entry_dialog;
mod preferences;
mod setup;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use adw::{prelude::*, subclass::prelude::*};
use gtk::{gio, glib};

use crate::app::controller::AppController;
use crate::pass::model::{PassNode, PassNodeKind};
use crate::pass::store;
use crate::ui::vault_view::{
    build_group_selection_with_root, build_selection_from_nodes_with_autoexpand, build_tree_factory,
};
use crate::ui::{EntryView, GroupEntry};

const CHANGES_PAGE_SIZE: usize = 50;
const SETUP_PROVIDER_NONE: u32 = 0;
const SIMPLE_SELECTION_WIDTH: i32 = 220;
const GROUP_TREE_WIDTH: i32 = 180;
const GROUP_SELECTION_WIDTH: i32 = 520;

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
        let imp = self.imp();

        self.setup_vault_setup_view();
        imp.vault_view.set_factory(Some(&build_tree_factory()));

        if let Err(err) = self.reload_vault_tree() {
            log::debug!("vault unavailable, showing setup view: {err}");
            self.show_setup_view();
            return;
        }
        if let Err(err) = self.reload_changes_view() {
            imp.entry_view.show_error(&err.to_string());
        }

        imp.entry_view.display_empty();
        if self.show_group_view_enabled() {
            self.show_root_group_content();
        } else {
            self.show_empty_content();
        }
        self.set_edit_unlock_state(false, false, false);
        self.show_app_view();
        self.start_autoload_if_enabled();
    }

    pub fn setup_callbacks(&self) {
        let imp = self.imp();

        {
            let window = self.clone();

            imp.new_entry_button.connect_clicked(move |_| {
                window.show_new_entry_dialog(None);
            });
        }

        {
            let window = self.clone();

            imp.preferences_button.connect_clicked(move |_| {
                window.show_preferences_dialog();
            });
        }

        {
            let window = self.clone();

            imp.main_paned.connect_position_notify(move |paned| {
                if window.show_group_view_enabled() && paned.position() < GROUP_TREE_WIDTH {
                    paned.set_position(GROUP_TREE_WIDTH);
                }
            });
        }

        {
            imp.selection_paned.connect_position_notify(move |paned| {
                if paned.position() < GROUP_TREE_WIDTH {
                    paned.set_position(GROUP_TREE_WIDTH);
                }
            });
        }

        {
            let window = self.clone();

            imp.vault_view
                .connect_create_entry_requested(move |_, folder_path| {
                    window.show_new_entry_dialog(Some(folder_path));
                });
        }

        {
            let window = self.clone();

            imp.changes_view
                .connect_revert_change_requested(move |_, commit_id| {
                    window.confirm_revert_change(&commit_id);
                });
        }

        {
            let window = self.clone();

            imp.changes_view
                .connect_rollback_change_requested(move |_, commit_id| {
                    window.confirm_rollback_change(&commit_id);
                });
        }

        {
            let window = self.clone();

            imp.changes_view.connect_push_requested(move |_| {
                window.push_changes();
            });
        }

        {
            let window = self.clone();

            imp.changes_view.connect_load_more_requested(move |_| {
                if let Err(err) = window.load_more_changes() {
                    window.imp().changes_view.set_loading_more(false);
                    window.imp().entry_view.show_error(&err.to_string());
                }
            });
        }

        {
            let window = self.clone();

            imp.changes_button.connect_clicked(move |_| {
                if window.controller().borrow().has_unsaved_changes() {
                    window
                        .imp()
                        .entry_view
                        .show_error("Save or cancel current changes before viewing changes");
                    return;
                }
                if let Err(err) = window.reload_changes_view() {
                    window.imp().entry_view.show_error(&err.to_string());
                    return;
                }
                window.show_changes_content();
            });
        }

        {
            let window = self.clone();

            imp.vault_view
                .connect_delete_entry_requested(move |_, entry_path, entry_name| {
                    let node = PassNode {
                        name: entry_name,
                        path: PathBuf::from(entry_path),
                        kind: PassNodeKind::Entry,
                        children: Vec::new(),
                    };
                    window.delete_entry(node);
                });
        }

        {
            let window = self.clone();

            imp.vault_view.connect_entry_selected(move |nav| {
                let Some(node) = nav.selected_node() else {
                    return;
                };

                window.open_entry_node(node);
            });
        }

        {
            let window = self.clone();

            imp.vault_view.connect_group_activated(move |nav| {
                if !window.show_group_view_enabled() {
                    return;
                }

                let Some(node) = nav.selected_node() else {
                    return;
                };

                if window.controller().borrow().has_unsaved_changes() {
                    window
                        .imp()
                        .entry_view
                        .show_error("Save or cancel current changes before selecting a group");
                    return;
                }

                window.show_group_content(&node);
            });
        }

        {
            let controller = self.controller();
            let window = self.clone();

            imp.entry_view.connect_entry_changed(move |view| {
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
                    Err(err) => view.show_error(&err.to_string()),
                }
            });
        }

        {
            let window = self.clone();

            imp.group_view.connect_entry_activated(move |_, node| {
                window.open_entry_node(node);
            });
        }

        {
            let controller = self.controller();
            let window = self.clone();

            imp.entry_view.connect_save_requested(move |view| {
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
                            view.show_error(&err.to_string());
                        }
                        if let Err(err) = window.reload_changes_view() {
                            view.show_error(&err.to_string());
                        }
                        window.set_edit_unlock_state(true, false, false);
                        view.set_cancellable(is_dirty);
                        view.set_saveable(is_valid);
                    }
                    Err(err) => view.show_error(&err.to_string()),
                }
            });
        }

        {
            let controller = self.controller();
            let entry_view = imp.entry_view.clone();
            let window = self.clone();

            imp.entry_view.connect_delete_requested(move |_| {
                let result = controller.borrow_mut().delete_current_entry();

                match result {
                    Ok(_) => {
                        entry_view.display_empty();
                        if let Err(err) = window.reload_vault_tree() {
                            entry_view.show_error(&err.to_string());
                        }
                        if let Err(err) = window.reload_changes_view() {
                            entry_view.show_error(&err.to_string());
                        }
                        window.show_empty_content();
                        window.set_edit_unlock_state(false, false, false);
                    }
                    Err(err) => entry_view.show_error(&err.to_string()),
                }
            });
        }

        {
            let controller = self.controller();
            let entry_view = imp.entry_view.clone();
            let window = self.clone();

            imp.entry_view.connect_revert_requested(move |_| {
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
                    Err(err) => entry_view.show_error(&err.to_string()),
                }
            });
        }

        {
            let entry_view = imp.entry_view.clone();
            let window = self.clone();

            imp.lock_vault_button.connect_clicked(move |_| {
                let editing = !entry_view.is_editable_mode();
                entry_view.set_editable_mode(editing);
                window.set_edit_unlock_state(true, editing, false);
            });
        }

        {
            let window = self.clone();

            imp.vault_view.connect_search_changed(move |_| {
                window.rebuild_vault_tree();
            });
        }

        {
            let window = self.clone();

            imp.setup_path_row.connect_changed(move |_| {
                window.update_create_vault_button();
            });
        }

        {
            let window = self.clone();

            imp.setup_recipient_row.connect_selected_notify(move |_| {
                window.update_create_vault_button();
            });
        }

        {
            let window = self.clone();

            imp.setup_remote_row.connect_changed(move |_| {
                window.update_create_vault_button();
            });
        }

        {
            let window = self.clone();

            imp.setup_provider_row.connect_selected_notify(move |row| {
                let sync_enabled = row.selected() != SETUP_PROVIDER_NONE;
                window.imp().setup_remote_row.set_sensitive(sync_enabled);
                window.update_create_vault_button();
            });
        }

        {
            let window = self.clone();

            imp.create_vault_button.connect_clicked(move |_| {
                window.create_vault_from_setup();
            });
        }
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
            Err(err) => entry_view.show_error(&err.to_string()),
        }
    }

    fn delete_entry(&self, node: PassNode) {
        if self.controller().borrow().has_unsaved_changes() {
            self.imp()
                .entry_view
                .show_error("Save or cancel current changes before deleting an entry");
            return;
        }

        let deleted_current = match self.controller().borrow_mut().delete_entry(node) {
            Ok(deleted_current) => deleted_current,
            Err(err) => {
                self.imp().entry_view.show_error(&err.to_string());
                return;
            }
        };

        if deleted_current {
            self.imp().entry_view.display_empty();
            self.show_empty_content();
            self.set_edit_unlock_state(false, false, false);
        }
        if let Err(err) = self.reload_vault_tree() {
            self.imp().entry_view.show_error(&err.to_string());
        }
        if let Err(err) = self.reload_changes_view() {
            self.imp().entry_view.show_error(&err.to_string());
        }
    }

    fn confirm_revert_change(&self, commit_id: &str) {
        if self.controller().borrow().has_unsaved_changes() {
            self.imp()
                .entry_view
                .show_error("Save or cancel current changes before undoing a change");
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
                .show_error("Save or cancel current changes before undoing a change");
            return;
        }

        let autopush = self.controller().borrow().autopush();
        let commit_id = commit_id.to_string();
        let window = self.clone();

        glib::spawn_future_local(async move {
            let result =
                match gio::spawn_blocking(move || store::revert_change(&commit_id, autopush)).await
                {
                    Ok(r) => r,
                    Err(_) => {
                        window
                            .imp()
                            .entry_view
                            .show_error("Undo task panicked unexpectedly");
                        return;
                    }
                };

            if let Err(err) = result {
                window.imp().entry_view.show_error(&err.to_string());
                return;
            }

            // Post-op state update — used to live inside
            // AppController::revert_change but is now done here on the main
            // loop after the store call returns from the worker thread.
            {
                let controller = window.controller();
                let mut controller = controller.borrow_mut();
                controller.state_mut().clear_entry_cache();
                controller.state_mut().set_current_session(None);
            }
            window.imp().entry_view.display_empty();
            window.show_empty_content();
            window.set_edit_unlock_state(false, false, false);
            if let Err(err) = window.reload_vault_tree() {
                window.imp().entry_view.show_error(&err.to_string());
            }
            if let Err(err) = window.reload_changes_view() {
                window.imp().entry_view.show_error(&err.to_string());
            }
        });
    }

    fn confirm_rollback_change(&self, commit_id: &str) {
        if self.controller().borrow().has_unsaved_changes() {
            self.imp()
                .entry_view
                .show_error("Save or cancel current changes before rolling back");
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
            let result =
                match gio::spawn_blocking(move || store::rollback_to_change(&commit_id)).await {
                    Ok(r) => r,
                    Err(_) => {
                        window
                            .imp()
                            .entry_view
                            .show_error("Rollback task panicked unexpectedly");
                        return;
                    }
                };

            let backup_branch = match result {
                Ok(b) => b,
                Err(err) => {
                    window.imp().entry_view.show_error(&err.to_string());
                    return;
                }
            };

            // Post-op state update on the main loop.
            {
                let controller = window.controller();
                let mut controller = controller.borrow_mut();
                controller.state_mut().clear_entry_cache();
                controller.state_mut().set_current_session(None);
            }
            window.imp().entry_view.display_empty();
            window.show_empty_content();
            window.set_edit_unlock_state(false, false, false);
            if let Err(err) = window.reload_vault_tree() {
                window.imp().entry_view.show_error(&err.to_string());
            }
            if let Err(err) = window.reload_changes_view() {
                window.imp().entry_view.show_error(&err.to_string());
            }

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
        self.update_selection_layout(false);
        let search_text = self.imp().vault_view.search_entry().text().to_string();
        let nodes = self.controller().borrow().filtered_tree(&search_text);
        let selection = if self.show_group_view_enabled() {
            build_group_selection_with_root(&nodes, true)
        } else {
            build_selection_from_nodes_with_autoexpand(&nodes, !search_text.trim().is_empty())
        };
        self.imp().vault_view.set_selection_model(&selection);
    }

    fn reload_changes_view(&self) -> Result<(), crate::app::app_error::AppError> {
        let changes = self
            .controller()
            .borrow()
            .load_changes_page(0, CHANGES_PAGE_SIZE)?;
        self.imp().loaded_changes_count.set(changes.len());
        self.imp().changes_view.set_changes(&changes);
        self.imp()
            .changes_view
            .set_has_more_changes(changes.len() == CHANGES_PAGE_SIZE);
        Ok(())
    }

    fn load_more_changes(&self) -> Result<(), crate::app::app_error::AppError> {
        let offset = self.imp().loaded_changes_count.get();
        let changes = self
            .controller()
            .borrow()
            .load_changes_page(offset, CHANGES_PAGE_SIZE)?;

        self.imp().loaded_changes_count.set(offset + changes.len());
        self.imp().changes_view.append_changes(&changes);
        self.imp()
            .changes_view
            .set_has_more_changes(changes.len() == CHANGES_PAGE_SIZE);
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
        self.setting_boolean("show-group-view", false)
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

    fn show_group_content(&self, node: &PassNode) {
        let imp = self.imp();
        let entries = self.group_entries_for(node);
        imp.group_view.set_group(node, &entries);
        imp.content_stack.set_visible_child_name("empty");
        imp.lock_vault_button.set_sensitive(false);
        imp.window_title.set_subtitle("Group");
    }

    fn show_root_group_content(&self) {
        let root = self.root_group_node();
        self.show_group_content(&root);
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
        if reset_position {
            imp.main_paned.set_position(GROUP_SELECTION_WIDTH);
            imp.selection_paned.set_position(GROUP_TREE_WIDTH);
        } else if imp.main_paned.position() < GROUP_TREE_WIDTH {
            imp.main_paned.set_position(GROUP_TREE_WIDTH);
        }
        imp.selection_stack.set_visible_child_name("group");
    }

    fn root_group_node(&self) -> PassNode {
        let search_text = self.imp().vault_view.search_entry().text().to_string();
        let children = self.controller().borrow().filtered_tree(&search_text);
        PassNode {
            name: "Vault".to_string(),
            path: PathBuf::new(),
            kind: PassNodeKind::Group,
            children,
        }
    }

    fn group_entries_for(&self, node: &PassNode) -> Vec<GroupEntry> {
        node.children
            .iter()
            .filter(|child| child.is_entry())
            .map(|child| GroupEntry {
                node: child.clone(),
                subtitle: self.entry_subtitle(child),
            })
            .collect()
    }

    fn entry_subtitle(&self, node: &PassNode) -> Option<String> {
        let entry = match self.controller().borrow_mut().preview_entry(node) {
            Ok(entry) => entry,
            Err(err) => {
                log::warn!(
                    "Failed to load entry preview for {}: {err}",
                    node.path.display()
                );
                return None;
            }
        };

        let (key, value) = entry.fields.first()?;
        let value = value.display_value();
        let first_line = value.lines().next().unwrap_or("").trim();

        Some(if first_line.is_empty() {
            format!("{key}:")
        } else {
            format!("{key}: {first_line}")
        })
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
        // duration of the network round-trip. The store call is self-
        // contained (no controller state needed), so we bypass the
        // controller and call store::push_changes directly. The
        // reload_changes_view that follows is fast (filesystem only) and
        // happens back on the main loop.
        let window = self.clone();
        let branch = self.controller().borrow().branch().map(str::to_string);
        glib::spawn_future_local(async move {
            let result =
                match gio::spawn_blocking(move || store::push_changes(branch.as_deref())).await {
                    Ok(r) => r,
                    Err(_) => {
                        window
                            .imp()
                            .entry_view
                            .show_error("Push task panicked unexpectedly");
                        return;
                    }
                };
            if let Err(err) = result {
                window.imp().entry_view.show_error(&err.to_string());
                return;
            }
            if let Err(err) = window.reload_changes_view() {
                window.imp().entry_view.show_error(&err.to_string());
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
