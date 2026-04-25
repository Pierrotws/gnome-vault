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

        if let EntryField::Password(password) = field {
            self.set_text(password);
        }
    }
}
