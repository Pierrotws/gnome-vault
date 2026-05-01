//! "New entry" dialog: name + parent folder + generated password.
//!
//! Layout lives in `assets/ui/new_entry_dialog.blp`; this module wires the
//! signals and the create-entry flow.

use std::path::{Component, Path, PathBuf};

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

        // Ensure the GeneratePasswordView GType is registered before the
        // Builder parses the resource — Builder looks types up by name.
        GeneratePasswordView::static_type();

        let builder =
            gtk::Builder::from_resource("/io/pierrotws/GnomeVault/ui/new_entry_dialog.ui");
        let dialog = builder
            .object::<adw::Window>("new_entry_dialog")
            .expect("new_entry_dialog must exist in new_entry_dialog.ui");
        let name_row = builder
            .object::<adw::EntryRow>("name_row")
            .expect("name_row must exist in new_entry_dialog.ui");
        let folder_row = builder
            .object::<adw::EntryRow>("folder_row")
            .expect("folder_row must exist in new_entry_dialog.ui");
        let generator = builder
            .object::<GeneratePasswordView>("generator")
            .expect("generator must exist in new_entry_dialog.ui");
        let cancel_button = builder
            .object::<gtk::Button>("cancel_button")
            .expect("cancel_button must exist in new_entry_dialog.ui");
        let create_button = builder
            .object::<gtk::Button>("create_button")
            .expect("create_button must exist in new_entry_dialog.ui");

        dialog.set_transient_for(Some(self));
        folder_row.set_text(folder_path.as_deref().unwrap_or(""));

        let refresh_create_button = glib::clone!(
            #[weak]
            create_button,
            #[weak]
            name_row,
            #[weak]
            folder_row,
            move || {
                let name_ok = !name_row.text().trim().is_empty();
                let folder_ok = is_valid_folder_input(folder_row.text().trim());
                create_button.set_sensitive(name_ok && folder_ok);
            }
        );
        let refresh_for_name = refresh_create_button.clone();
        name_row.connect_changed(move |_| refresh_for_name());
        let refresh_for_folder = refresh_create_button.clone();
        folder_row.connect_changed(move |_| refresh_for_folder());

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
                // generator.password() is Zeroizing<String>; clone the
                // inner String into the EntryField (itself ZeroizeOnDrop)
                // and let the Zeroizing wrapper wipe its copy at scope end.
                let generated = generator.password();
                let entry = EntryData {
                    password: EntryField::Password((*generated).clone()),
                    fields: Vec::new(),
                };
                // The store does its own validation, but feeding it the
                // raw GString here would let surrounding whitespace land
                // in `valid_entry_name` and trip a confusing
                // "Invalid entry name" error for input that the
                // create-button enable rule already accepted.
                let folder_path = PathBuf::from(folder_row.text().trim());
                let entry_name = name_row.text();
                let result = window.controller().borrow_mut().create_entry(
                    &folder_path,
                    entry_name.trim(),
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

/// Cheap pre-check that mirrors `pass::store::validate_folder_path`. The
/// store re-validates on submit (and rejects symlink escapes via
/// canonicalization, which we can't run from a UI thread without
/// blocking on disk); this only catches the obvious "absolute path or
/// `..` segment" inputs so the create button can be greyed out before
/// the user clicks it. An empty string is accepted — that means
/// "create at the vault root".
fn is_valid_folder_input(text: &str) -> bool {
    if text.is_empty() {
        return true;
    }
    let path = Path::new(text);
    if path.is_absolute() {
        return false;
    }
    !path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
}
