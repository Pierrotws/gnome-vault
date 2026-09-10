use adw::prelude::*;
use gtk::{glib, subclass::prelude::*};

use crate::pass::model::EntryField;

use super::EntryFieldRow;

mod imp;

const PASSWORD_KEY: &str = "Password";

glib::wrapper! {
    pub struct PasswordFieldRow(ObjectSubclass<imp::PasswordFieldRow>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl PasswordFieldRow {
    pub fn key(&self) -> String {
        self.imp().password_entry.title().to_string()
    }

    pub fn set_key(&self, key: &str) {
        self.imp().password_entry.set_title(key);
    }

    pub fn text(&self) -> glib::GString {
        self.imp().password_entry.text()
    }

    pub fn set_text(&self, text: &str) {
        self.imp().password_entry.set_text(text);
    }

    pub fn set_editable_mode(&self, editable: bool) {
        let imp = self.imp();
        imp.password_entry.set_editable(editable);
        // Keep the generate button's slot so the copy button next to it
        // stays put across the mode switch.
        super::set_control_reserved(&imp.generate_password_button.get(), editable);
    }

    pub fn connect_changed<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&adw::PasswordEntryRow) + 'static,
    {
        self.imp().password_entry.connect_changed(f)
    }

    pub fn connect_copy_clicked<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&gtk::Button) + 'static,
    {
        self.imp().copy_password_button.connect_clicked(f)
    }

    pub fn connect_generate_clicked<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&gtk::Button) + 'static,
    {
        self.imp().generate_password_button.connect_clicked(f)
    }
}

impl EntryFieldRow for PasswordFieldRow {
    fn key(&self) -> String {
        self.key()
    }

    fn entry_field(&self) -> EntryField {
        EntryField::Password(self.text().to_string())
    }

    fn set_entry_field(&self, field: &EntryField) {
        self.set_key(PASSWORD_KEY);
        // Match the variant explicitly. Other field rows (Plain,
        // Multiline, OTP) shouldn't reach this row, but display_value
        // on a non-Password variant would stuff cleartext into the
        // password row — visible after the eye-toggle, copied by the
        // copy button, and saved back as the new primary password.
        // Refuse anything that isn't Password and log so a future
        // caller spotting the warning can route through the correct
        // field row.
        match field {
            EntryField::Password(value) => self.set_text(value),
            other => {
                log::warn!(
                    "PasswordFieldRow::set_entry_field: refusing non-Password variant {:?}",
                    std::mem::discriminant(other)
                );
                self.set_text("");
            }
        }
    }
}
