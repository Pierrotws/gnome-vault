mod imp;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use adw::{prelude::*, subclass::prelude::*};
use gtk::{gio, glib};

use crate::app::controller::AppController;
use crate::pass::model::{EntryData, EntryField, PassNode, PassNodeKind};
use crate::ui::generate_password_view::GeneratePasswordView;
use crate::ui::vault_view::{build_selection_from_nodes_with_autoexpand, build_tree_factory};
use crate::ui::EntryView;

const CHANGES_PAGE_SIZE: usize = 50;

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

        if let Err(err) = self.reload_vault_tree() {
            imp.entry_view.show_error(&err.to_string());
        }
        imp.vault_view.set_factory(Some(&build_tree_factory()));
        if let Err(err) = self.reload_changes_view() {
            imp.entry_view.show_error(&err.to_string());
        }

        imp.entry_view.display_empty();
        self.set_edit_unlock_state(false, false, false);
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

            imp.navigation_stack
                .connect_visible_child_name_notify(move |stack| {
                    if stack.visible_child_name().as_deref() == Some("changes") {
                        if let Err(err) = window.reload_changes_view() {
                            window.imp().entry_view.show_error(&err.to_string());
                        }
                    }
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
            let controller = self.controller();
            let entry_view = imp.entry_view.clone();
            let window = self.clone();

            imp.vault_view.connect_entry_selected(move |nav| {
                let Some(node) = nav.selected_node() else {
                    return;
                };

                let result = {
                    let mut controller = controller.borrow_mut();
                    controller.open_node(node)
                };

                match result {
                    Ok(data) => {
                        entry_view.set_entry_data(&data);
                        window.set_edit_unlock_state(true, false, false);
                        let is_dirty = controller.borrow().has_unsaved_changes();
                        entry_view.set_cancellable(is_dirty);
                        let is_valid = controller.borrow().has_valid_changes();
                        entry_view.set_saveable(is_valid);
                    }
                    Err(err) => entry_view.show_error(&err.to_string()),
                }
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

                let result = {
                    let mut controller = controller.borrow_mut();
                    controller.update_current_entry(updated)
                };

                match result {
                    Ok(()) => {
                        let is_dirty = controller.borrow().has_unsaved_changes();
                        view.set_cancellable(is_dirty);
                        let is_valid = controller.borrow().has_valid_changes();
                        view.set_saveable(is_valid);
                        window.set_edit_unlock_state(true, view.is_editable_mode(), is_dirty);
                    }
                    Err(err) => view.show_error(&err.to_string()),
                }
            });
        }

        {
            let controller = self.controller();
            let window = self.clone();

            imp.entry_view.connect_save_requested(move |view| {
                let result = controller.borrow_mut().save_current_entry();

                match result {
                    Ok(()) => {
                        view.set_editable_mode(false);
                        if let Err(err) = window.reload_vault_tree() {
                            view.show_error(&err.to_string());
                        }
                        if let Err(err) = window.reload_changes_view() {
                            view.show_error(&err.to_string());
                        }
                        window.set_edit_unlock_state(true, false, false);
                        let is_dirty = controller.borrow().has_unsaved_changes();
                        view.set_cancellable(is_dirty);
                        let is_valid = controller.borrow().has_valid_changes();
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
                        entry_view.set_entry_data(&entry_data);
                        window.set_edit_unlock_state(true, false, false);
                        let is_dirty = controller.borrow().has_unsaved_changes();
                        entry_view.set_cancellable(is_dirty);
                        let is_valid = controller.borrow().has_valid_changes();
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
                        window.set_edit_unlock_state(true, false, false);
                        dialog.close();
                    }
                    Err(err) => entry_view.show_error(&err.to_string()),
                }
            }
        ));

        dialog.present();
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
        let search_text = self.imp().vault_view.search_entry().text().to_string();
        let nodes = self.controller().borrow().filtered_tree(&search_text);
        let selection =
            build_selection_from_nodes_with_autoexpand(&nodes, !search_text.trim().is_empty());
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

    fn apply_autopush_setting(&self) {
        let autopush = self.settings().boolean("autopush");
        self.controller().borrow_mut().set_autopush(autopush);
    }

    fn show_preferences_dialog(&self) {
        let dialog = adw::PreferencesDialog::builder()
            .title("Preferences")
            .build();
        let page = adw::PreferencesPage::new();
        let group = adw::PreferencesGroup::builder().title("Git").build();
        let autopush_row = adw::SwitchRow::builder()
            .title("Push changes automatically")
            .subtitle("Push to the remote after every saved change")
            .active(self.settings().boolean("autopush"))
            .build();

        group.add(&autopush_row);
        page.add(&group);
        dialog.add(&page);

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

        dialog.present(Some(self));
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
