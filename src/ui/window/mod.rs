mod imp;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use adw::{prelude::*, subclass::prelude::*};
use gtk::{gio, glib};

use crate::app::controller::AppController;
use crate::pass::model::{EntryData, EntryField};
use crate::ui::generate_password_view::GeneratePasswordView;
use crate::ui::vault_view::{build_selection_from_nodes, build_tree_factory};
use crate::ui::EntryView;

glib::wrapper! {
    pub struct MainWindow(ObjectSubclass<imp::MainWindow>)
        @extends adw::ApplicationWindow, gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk::Accessible, gtk::Buildable,
                    gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl MainWindow {
    pub fn new(app: &adw::Application, controller: Rc<RefCell<AppController>>) -> Self {
        let obj: Self = glib::Object::builder().property("application", app).build();

        let _ = obj.imp().controller.set(controller);

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

            imp.vault_view
                .connect_create_entry_requested(move |_, folder_path| {
                    window.show_new_entry_dialog(Some(folder_path));
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

        imp.vault_view.connect_search_changed(move |nav| {
            nav.handle_search_changed();
        });
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

    fn reload_vault_tree(&self) -> Result<(), crate::app::app_error::AppError> {
        let controller = self.controller();
        let nodes = {
            let mut controller = controller.borrow_mut();
            controller.reload_tree()?;
            controller.state().tree().to_vec()
        };
        let selection = build_selection_from_nodes(&nodes);
        self.imp().vault_view.set_selection_model(&selection);
        Ok(())
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
