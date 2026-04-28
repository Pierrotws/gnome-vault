//! "New entry" dialog: name + parent folder + generated password.

use std::path::PathBuf;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;

use crate::pass::model::{EntryData, EntryField};
use crate::ui::generate_password_view::GeneratePasswordView;

use super::MainWindow;

impl MainWindow {
    pub(super) fn show_new_entry_dialog(&self, folder_path: Option<String>) {
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

        create_button.connect_clicked(glib::clone!(
            #[weak]
            dialog,
            #[weak]
            name_row,
            #[weak]
            folder_row,
            #[weak]
            generator,
            #[weak(rename_to = window)]
            self,
            move |_| {
                let entry_view = window.imp().entry_view.clone();
                let entry = EntryData {
                    password: EntryField::Password(generator.password()),
                    fields: Vec::new(),
                };
                let folder_path = PathBuf::from(folder_row.text().trim());
                let result = window.controller().borrow_mut().create_entry(
                    &folder_path,
                    &name_row.text(),
                    entry,
                );

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
}
