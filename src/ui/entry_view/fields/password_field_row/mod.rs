use adw::prelude::*;
use gtk::{glib, subclass::prelude::*};

mod imp;

glib::wrapper! {
    pub struct PasswordFieldRow(ObjectSubclass<imp::PasswordFieldRow>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl PasswordFieldRow {
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
