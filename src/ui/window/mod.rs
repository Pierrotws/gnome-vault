mod imp;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use adw::{prelude::*, subclass::prelude::*};
use gtk::{gio, glib};

use crate::app::controller::AppController;
use crate::pass::model::{EntryData, EntryField, PassNode, PassNodeKind};
use crate::pass::store::{self, VaultSetup};
use crate::ui::generate_password_view::GeneratePasswordView;
use crate::ui::vault_view::{
    build_group_selection_with_root, build_selection_from_nodes_with_autoexpand, build_tree_factory,
};
use crate::ui::{EntryView, GroupEntry};

const CHANGES_PAGE_SIZE: usize = 50;
const SETUP_PROVIDER_NONE: u32 = 0;

enum AutoloadMessage {
    Loaded(PassNode, EntryData),
    Failed(PathBuf, String),
    Finished,
}

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

            imp.vault_view
                .connect_create_entry_requested(move |_, folder_path| {
                    window.show_new_entry_dialog(Some(folder_path));
                });
        }

        {
            let window = self.clone();

            imp.changes_view
                .connect_revert_change_requested(move |_, commit_id| {
                    window.revert_change(&commit_id);
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

    fn show_new_entry_dialog(&self, folder_path: Option<String>) {
        if self.controller().borrow().has_unsaved_changes() {
            self.imp()
                .entry_view
                .show_error("Save or cancel current changes before creating a new entry");
            return;
        }

        let dialog = adw::Window::builder()
            .title("New Entry")
            .modal(true)
            .resizable(true)
            .default_width(640)
            .default_height(520)
            .transient_for(self)
            .build();

        let main_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .build();

        let form_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(18)
            .margin_top(18)
            .margin_start(18)
            .margin_end(18)
            .build();

        let entry_group = adw::PreferencesGroup::builder().title("Entry").build();
        let name_row = adw::EntryRow::builder().title("Name").build();
        let folder_row = adw::EntryRow::builder().title("Parent Folder").build();
        folder_row.set_text(folder_path.as_deref().unwrap_or(""));
        entry_group.add(&name_row);
        entry_group.add(&folder_row);
        form_box.append(&entry_group);

        let generator = GeneratePasswordView::new();
        form_box.append(&generator);
        main_box.append(&form_box);

        let actions = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .halign(gtk::Align::End)
            .margin_top(18)
            .margin_bottom(18)
            .margin_start(18)
            .margin_end(18)
            .build();

        let cancel_button = gtk::Button::with_label("Cancel");
        let create_button = gtk::Button::with_label("Create");
        create_button.add_css_class("suggested-action");
        create_button.set_sensitive(false);

        actions.append(&cancel_button);
        actions.append(&create_button);
        main_box.append(&actions);
        dialog.set_content(Some(&main_box));

        name_row.connect_changed(glib::clone!(
            #[weak]
            create_button,
            move |row| {
                create_button.set_sensitive(!row.text().trim().is_empty());
            }
        ));

        cancel_button.connect_clicked(glib::clone!(
            #[weak]
            dialog,
            move |_| {
                dialog.close();
            }
        ));

        let controller = self.controller();
        let entry_view = self.imp().entry_view.clone();
        let window = self.clone();
        create_button.connect_clicked(glib::clone!(
            #[weak]
            dialog,
            #[weak]
            name_row,
            #[weak]
            folder_row,
            #[weak]
            generator,
            move |_| {
                let entry = EntryData {
                    password: EntryField::Password(generator.password()),
                    fields: Vec::new(),
                };
                let folder_path = PathBuf::from(folder_row.text().trim());
                let result =
                    controller
                        .borrow_mut()
                        .create_entry(&folder_path, &name_row.text(), entry);

                match result {
                    Ok(entry_data) => {
                        if let Err(err) = window.reload_vault_tree() {
                            entry_view.show_error(&err.to_string());
                        }
                        if let Err(err) = window.reload_changes_view() {
                            entry_view.show_error(&err.to_string());
                        }
                        entry_view.set_entry_data(&entry_data);
                        window.show_entry_content();
                        window.set_edit_unlock_state(true, false, false);
                        dialog.close();
                    }
                    Err(err) => entry_view.show_error(&err.to_string()),
                }
            }
        ));

        dialog.present();
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

    fn revert_change(&self, commit_id: &str) {
        if self.controller().borrow().has_unsaved_changes() {
            self.imp()
                .entry_view
                .show_error("Save or cancel current changes before reverting a commit");
            return;
        }

        if let Err(err) = self.controller().borrow_mut().revert_change(commit_id) {
            self.imp().entry_view.show_error(&err.to_string());
            return;
        }

        self.imp().entry_view.display_empty();
        self.show_empty_content();
        self.set_edit_unlock_state(false, false, false);
        if let Err(err) = self.reload_vault_tree() {
            self.imp().entry_view.show_error(&err.to_string());
        }
        if let Err(err) = self.reload_changes_view() {
            self.imp().entry_view.show_error(&err.to_string());
        }
    }

    fn confirm_rollback_change(&self, commit_id: &str) {
        if self.controller().borrow().has_unsaved_changes() {
            self.imp()
                .entry_view
                .show_error("Save or cancel current changes before rolling back");
            return;
        }

        let dialog = adw::AlertDialog::builder()
            .heading("Rollback Branch?")
            .body("This will create and push a reset backup branch, hard-reset the current branch to this action, and push the reset branch state.")
            .build();
        dialog.add_responses(&[("cancel", "Cancel"), ("rollback", "Rollback")]);
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
        let backup_branch = match self.controller().borrow_mut().rollback_to_change(commit_id) {
            Ok(backup_branch) => backup_branch,
            Err(err) => {
                self.imp().entry_view.show_error(&err.to_string());
                return;
            }
        };

        self.imp().entry_view.display_empty();
        self.show_empty_content();
        self.set_edit_unlock_state(false, false, false);
        if let Err(err) = self.reload_vault_tree() {
            self.imp().entry_view.show_error(&err.to_string());
        }
        if let Err(err) = self.reload_changes_view() {
            self.imp().entry_view.show_error(&err.to_string());
        }

        let dialog = adw::AlertDialog::builder()
            .heading("Rollback Complete")
            .body(format!("Backup branch pushed: {backup_branch}"))
            .build();
        dialog.add_responses(&[("ok", "OK")]);
        dialog.set_default_response(Some("ok"));
        dialog.set_close_response("ok");
        dialog.present(Some(self));
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
        self.update_selection_layout();
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

    fn show_group_view_enabled(&self) -> bool {
        self.setting_boolean("show-group-view", false)
    }

    fn apply_store_dir_setting(&self) {
        if std::env::var_os("PASSWORD_STORE_DIR").is_some() {
            return;
        }

        if !self.settings_has_key("store-dir") {
            log::warn!("GSettings schema is missing store-dir; using PASSWORD_STORE_DIR fallback");
            return;
        }

        let store_dir = self.settings().string("store-dir");
        if !store_dir.trim().is_empty() {
            std::env::set_var("PASSWORD_STORE_DIR", store_dir.as_str());
        }
    }

    fn apply_autopush_setting(&self) {
        let autopush = self.settings().boolean("autopush");
        self.controller().borrow_mut().set_autopush(autopush);
    }

    fn setup_vault_setup_view(&self) {
        let imp = self.imp();
        let provider_model = gtk::StringList::new(&["No Sync", "GitHub", "GitLab", "Custom"]);
        imp.setup_provider_row.set_model(Some(&provider_model));
        imp.setup_provider_row.set_selected(SETUP_PROVIDER_NONE);
        imp.setup_remote_row.set_sensitive(false);
        self.setup_recipient_model();

        let configured_store_dir = if self.settings_has_key("store-dir") {
            self.settings().string("store-dir").to_string()
        } else {
            String::new()
        };
        let default_store_dir = if configured_store_dir.trim().is_empty() {
            store::password_store_dir()
        } else {
            PathBuf::from(&configured_store_dir)
        };
        imp.setup_path_row
            .set_text(&default_store_dir.to_string_lossy());
        self.update_create_vault_button();
    }

    fn show_setup_view(&self) {
        let imp = self.imp();
        imp.main_stack.set_visible_child_name("setup");
        imp.new_entry_button.set_sensitive(false);
        imp.lock_vault_button.set_sensitive(false);
        imp.changes_button.set_sensitive(false);
        imp.window_title.set_subtitle("Setup");
    }

    fn show_app_view(&self) {
        let imp = self.imp();
        self.update_selection_layout();
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

    fn update_selection_layout(&self) {
        if self.show_group_view_enabled() {
            self.show_split_selection_layout();
        } else {
            self.show_tree_selection_layout();
        }
    }

    fn show_tree_selection_layout(&self) {
        let imp = self.imp();
        if imp.simple_vault_bin.child().is_none() {
            imp.group_vault_bin.set_child(Option::<&gtk::Widget>::None);
            imp.simple_vault_bin.set_child(Some(&imp.vault_view.get()));
        }
        imp.selection_zone.set_width_request(200);
        imp.selection_stack.set_visible_child_name("tree");
    }

    fn show_split_selection_layout(&self) {
        let imp = self.imp();
        if imp.group_vault_bin.child().is_none() {
            imp.simple_vault_bin.set_child(Option::<&gtk::Widget>::None);
            imp.group_vault_bin.set_child(Some(&imp.vault_view.get()));
        }
        imp.selection_zone.set_width_request(480);
        imp.selection_paned.set_position(200);
        if imp.main_paned.position() < 200 {
            imp.main_paned.set_position(200);
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

    fn update_create_vault_button(&self) {
        let imp = self.imp();
        let has_path = !imp.setup_path_row.text().trim().is_empty();
        let has_recipient = imp
            .setup_recipients
            .borrow()
            .get(imp.setup_recipient_row.selected() as usize)
            .is_some();
        let needs_remote = imp.setup_provider_row.selected() != SETUP_PROVIDER_NONE;
        let has_remote = !imp.setup_remote_row.text().trim().is_empty();

        imp.create_vault_button
            .set_sensitive(has_path && has_recipient && (!needs_remote || has_remote));
    }

    fn create_vault_from_setup(&self) {
        let imp = self.imp();
        let store_dir = PathBuf::from(imp.setup_path_row.text().trim());
        let Some(recipient) = imp
            .setup_recipients
            .borrow()
            .get(imp.setup_recipient_row.selected() as usize)
            .map(|recipient| recipient.id.clone())
        else {
            self.show_error_dialog("Select a GPG recipient before creating the vault");
            return;
        };
        let remote_url = (imp.setup_provider_row.selected() != SETUP_PROVIDER_NONE)
            .then(|| imp.setup_remote_row.text().trim().to_string())
            .filter(|url| !url.is_empty());

        let setup = VaultSetup {
            store_dir: store_dir.clone(),
            recipient,
            remote_url,
            autopush: self.controller().borrow().autopush(),
        };

        std::env::set_var("PASSWORD_STORE_DIR", &store_dir);

        let setup_result = {
            let controller = self.controller();
            let mut controller = controller.borrow_mut();
            controller.setup_vault(&setup)
        };

        match setup_result {
            Ok(()) => {
                if self.settings_has_key("store-dir") {
                    if let Err(err) = self
                        .settings()
                        .set_string("store-dir", &store_dir.to_string_lossy())
                    {
                        self.show_error_dialog(&err.to_string());
                        return;
                    }
                } else {
                    log::warn!(
                        "GSettings schema is missing store-dir; vault path was not persisted"
                    );
                }
                self.rebuild_vault_tree();
                if let Err(err) = self.reload_changes_view() {
                    self.show_error_dialog(&err.to_string());
                }
                imp.entry_view.display_empty();
                self.show_empty_content();
                self.set_edit_unlock_state(false, false, false);
                self.show_app_view();
                self.start_autoload_if_enabled();
            }
            Err(err) => self.show_error_dialog(&err.to_string()),
        }
    }

    fn setup_recipient_model(&self) {
        let imp = self.imp();
        let recipients = match self.controller().borrow().available_recipients() {
            Ok(recipients) => recipients,
            Err(err) => {
                log::warn!("Failed to list GPG recipients: {err}");
                Vec::new()
            }
        };

        if recipients.is_empty() {
            let model = gtk::StringList::new(&[]);
            imp.setup_recipient_row.set_model(Some(&model));
            imp.setup_recipient_row
                .set_selected(gtk::INVALID_LIST_POSITION);
            imp.setup_recipient_row
                .set_subtitle("No usable local secret encryption key found");
            imp.setup_recipient_row.set_sensitive(false);
        } else {
            let labels = recipients
                .iter()
                .map(|recipient| recipient.label.as_str())
                .collect::<Vec<_>>();
            let model = gtk::StringList::new(&labels);
            imp.setup_recipient_row.set_model(Some(&model));
            imp.setup_recipient_row.set_selected(0);
            imp.setup_recipient_row.set_subtitle("");
            imp.setup_recipient_row.set_sensitive(true);
        }

        imp.setup_recipients.replace(recipients);
        self.update_create_vault_button();
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

    fn show_preferences_dialog(&self) {
        let builder =
            gtk::Builder::from_resource("/io/pierrotws/GnomeVault/ui/preferences_dialog.ui");
        let dialog = builder
            .object::<adw::PreferencesDialog>("preferences_dialog")
            .expect("preferences_dialog must exist in preferences_dialog.ui");
        let autopush_row = builder
            .object::<adw::SwitchRow>("autopush_row")
            .expect("autopush_row must exist in preferences_dialog.ui");
        let autoload_row = builder
            .object::<adw::SwitchRow>("autoload_row")
            .expect("autoload_row must exist in preferences_dialog.ui");
        let show_group_view_row = builder
            .object::<adw::SwitchRow>("show_group_view_row")
            .expect("show_group_view_row must exist in preferences_dialog.ui");

        autopush_row.set_active(self.setting_boolean("autopush", true));
        autoload_row.set_active(self.setting_boolean("autoload", false));
        show_group_view_row.set_active(self.show_group_view_enabled());

        let settings = self.settings();
        let window = self.clone();
        autopush_row.connect_active_notify(move |row| {
            if let Err(err) = settings.set_boolean("autopush", row.is_active()) {
                window.imp().entry_view.show_error(&err.to_string());
                return;
            }
            window.apply_autopush_setting();
            if let Err(err) = window.reload_changes_view() {
                window.imp().entry_view.show_error(&err.to_string());
            }
        });

        let settings = self.settings();
        let window = self.clone();
        autoload_row.connect_active_notify(move |row| {
            if !window.settings_has_key("autoload") {
                window
                    .imp()
                    .entry_view
                    .show_error("GSettings schema is missing autoload");
                return;
            }
            if let Err(err) = settings.set_boolean("autoload", row.is_active()) {
                window.imp().entry_view.show_error(&err.to_string());
                return;
            }
            if row.is_active() {
                window.start_autoload_cache();
            }
        });

        let settings = self.settings();
        let window = self.clone();
        show_group_view_row.connect_active_notify(move |row| {
            if !window.settings_has_key("show-group-view") {
                window
                    .imp()
                    .entry_view
                    .show_error("GSettings schema is missing show-group-view");
                return;
            }
            if let Err(err) = settings.set_boolean("show-group-view", row.is_active()) {
                window.imp().entry_view.show_error(&err.to_string());
                return;
            }
            window.update_selection_layout();
            window.rebuild_vault_tree();
            if row.is_active() {
                window.show_root_group_content();
            }
        });

        dialog.present(Some(self));
    }

    fn start_autoload_if_enabled(&self) {
        if self.setting_boolean("autoload", false) {
            self.start_autoload_cache();
        }
    }

    fn start_autoload_cache(&self) {
        let imp = self.imp();
        if imp.autoload_running.get() {
            return;
        }

        let nodes = self.controller().borrow().uncached_entry_nodes();
        if nodes.is_empty() {
            return;
        }

        imp.autoload_running.set(true);
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            for node in nodes {
                let message = match AppController::load_entry_for_cache(&node) {
                    Ok(entry) => AutoloadMessage::Loaded(node, entry),
                    Err(err) => AutoloadMessage::Failed(node.path.clone(), err.to_string()),
                };
                if sender.send(message).is_err() {
                    return;
                }
            }
            let _ = sender.send(AutoloadMessage::Finished);
        });

        let window = self.clone();
        glib::timeout_add_local(Duration::from_millis(100), move || {
            let mut finished = false;
            let mut loaded_any = false;

            for _ in 0..32 {
                match receiver.try_recv() {
                    Ok(AutoloadMessage::Loaded(node, entry)) => {
                        window
                            .controller()
                            .borrow_mut()
                            .cache_loaded_entry(&node, entry);
                        loaded_any = true;
                    }
                    Ok(AutoloadMessage::Failed(path, err)) => {
                        log::warn!("Failed to autoload {}: {err}", path.display());
                    }
                    Ok(AutoloadMessage::Finished) => {
                        finished = true;
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        finished = true;
                        break;
                    }
                }
            }

            if loaded_any
                && !window
                    .imp()
                    .vault_view
                    .search_entry()
                    .text()
                    .trim()
                    .is_empty()
            {
                window.rebuild_vault_tree();
            }

            if finished {
                window.imp().autoload_running.set(false);
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }

    fn push_changes(&self) {
        if let Err(err) = self.controller().borrow().push_changes() {
            self.imp().entry_view.show_error(&err.to_string());
            return;
        }

        if let Err(err) = self.reload_changes_view() {
            self.imp().entry_view.show_error(&err.to_string());
        }
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
