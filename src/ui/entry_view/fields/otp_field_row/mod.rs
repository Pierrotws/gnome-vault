use gtk::{glib, prelude::*, subclass::prelude::*};

use crate::{
    helpers::{clipboard, otp},
    pass::model::EntryField,
};

use super::super::EntryView;
use super::EntryFieldRow;

mod imp;

glib::wrapper! {
    pub struct OtpFieldRow(ObjectSubclass<imp::OtpFieldRow>)
        @extends gtk::Widget, gtk::ListBoxRow,
        @implements gtk::Accessible, gtk::Actionable, gtk::Buildable, gtk::ConstraintTarget;
}

impl OtpFieldRow {
    pub fn new_empty(entry_view: &EntryView) -> Self {
        let obj: Self = glib::Object::new();
        obj.configure_as_custom_field();
        obj.setup_callbacks(entry_view);
        obj.update_url_validation();
        obj.update_otp_display();
        obj
    }

    pub fn setup_callbacks(&self, entry_view: &EntryView) {
        let imp = self.imp();

        // Guard against double-registration of signal handlers if setup_callbacks
        // is ever called more than once for the same row.
        if imp.already_setup.replace(true) {
            return;
        }

        imp.key_entry.connect_changed(glib::clone!(
            #[weak(rename_to = this)]
            self,
            #[weak]
            entry_view,
            move |entry| {
                this.imp().title_label.set_text(&entry.text());
                entry_view.mark_changed();
            }
        ));

        imp.url_entry.connect_changed(glib::clone!(
            #[weak(rename_to = this)]
            self,
            #[weak]
            entry_view,
            move |_| {
                this.update_url_validation();
                this.update_otp_display();
                entry_view.mark_changed();
            }
        ));

        imp.copy_code_button.connect_clicked(glib::clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                clipboard::copy_secret(&this.current_code());
            }
        ));

        imp.copy_url_button.connect_clicked(glib::clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                clipboard::copy_secret(&this.url());
            }
        ));

        imp.reveal_url_button.connect_toggled(glib::clone!(
            #[weak(rename_to = this)]
            self,
            move |button| {
                this.set_url_revealed(button.is_active());
            }
        ));

        imp.delete_button.connect_clicked(glib::clone!(
            #[weak(rename_to = this)]
            self,
            #[weak]
            entry_view,
            move |_| {
                if let Some(list) = this.parent() {
                    if let Ok(container) = list.downcast::<gtk::Box>() {
                        container.remove(&this);
                        entry_view.mark_changed();
                    }
                }
            }
        ));

        // Tie the 1Hz OTP refresh tick to map/unmap so it only runs while
        // the row is on screen. This avoids wasted wakeups and ensures the
        // timer never outlives the widget (dispose also clears it).
        self.connect_map(|row| {
            let imp = row.imp();
            // Replace any prior source with the new one; if a previous timer
            // somehow survived, remove it before installing a fresh tick.
            let existing = imp.timer_source.take();
            if let Some(source) = existing {
                source.remove();
            }
            let source = glib::timeout_add_seconds_local(
                1,
                glib::clone!(
                    #[weak]
                    row,
                    #[upgrade_or]
                    glib::ControlFlow::Break,
                    move || {
                        row.update_otp_display();
                        glib::ControlFlow::Continue
                    }
                ),
            );
            imp.timer_source.set(Some(source));
        });

        self.connect_unmap(|row| {
            if let Some(source) = row.imp().timer_source.take() {
                source.remove();
            }
        });

        self.update_url_validation();
        self.update_otp_display();
    }

    pub fn new(entry_view: &EntryView, key: &str, url: &str) -> Self {
        let obj = Self::new_empty(entry_view);
        obj.set_key(key);
        obj.set_url(url);
        obj
    }

    pub fn from_entry_field(entry_view: &EntryView, key: &str, field: &EntryField) -> Option<Self> {
        match field {
            EntryField::OTP(value) => Some(Self::new(entry_view, key, value)),
            _ => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.key().trim().is_empty() && self.url().trim().is_empty()
    }

    pub fn key(&self) -> String {
        self.imp().key_entry.text().into()
    }

    pub fn set_key(&self, key: &str) {
        let imp = self.imp();
        imp.key_entry.set_text(key);
        imp.title_label.set_text(key);
    }

    pub fn url(&self) -> glib::GString {
        self.imp().url_entry.text()
    }

    pub fn set_url(&self, url: &str) {
        self.imp().url_entry.set_text(url);
        self.update_url_validation();
        self.update_otp_display();
    }

    pub fn set_editable_mode(&self, editable: bool) {
        let imp = self.imp();
        let key_editable = editable && imp.key_editable.get();
        let row_controls_visible = editable && imp.show_row_controls.get();

        imp.view_box.set_visible(!editable);
        imp.edit_box.set_visible(editable);
        imp.key_entry.set_editable(key_editable);
        imp.key_entry.set_can_focus(key_editable);
        imp.key_entry.set_has_frame(key_editable);
        imp.url_entry.set_editable(editable);
        imp.url_entry.set_can_focus(editable);
        imp.url_entry.set_has_frame(editable);
        imp.drag_handle.set_visible(row_controls_visible);
        imp.delete_button.set_visible(row_controls_visible);

        if !editable {
            imp.reveal_url_button.set_active(false);
            self.set_url_revealed(false);
        }
    }

    fn set_url_revealed(&self, revealed: bool) {
        let imp = self.imp();
        imp.url_entry.set_visibility(revealed);
        if revealed {
            imp.reveal_url_button.set_icon_name("view-conceal-symbolic");
            imp.reveal_url_button.set_tooltip_text(Some("Hide URL"));
        } else {
            imp.reveal_url_button.set_icon_name("view-reveal-symbolic");
            imp.reveal_url_button.set_tooltip_text(Some("Show URL"));
        }
    }

    pub fn drag_handle(&self) -> gtk::Widget {
        self.imp().drag_handle.get().upcast()
    }

    pub fn configure_as_primary_field(&self) {
        let imp = self.imp();
        imp.show_row_controls.set(false);
        imp.key_editable.set(false);
        self.set_key("Password");
    }

    fn configure_as_custom_field(&self) {
        let imp = self.imp();
        imp.show_row_controls.set(true);
        imp.key_editable.set(true);
    }

    fn current_code(&self) -> String {
        otp::generate_totp_from_url(self.url().as_str())
            .map(|code| code.code)
            .unwrap_or_default()
    }

    fn update_otp_display(&self) {
        let imp = self.imp();
        match otp::generate_totp_from_url(self.url().as_str()) {
            Ok(code) => {
                imp.code_label.set_text(&code.code);
                imp.validity_progress
                    .set_fraction(code.remaining as f64 / code.period as f64);
            }
            Err(_) => {
                imp.code_label.set_text("------");
                imp.validity_progress.set_fraction(0.0);
            }
        }
    }

    fn update_url_validation(&self) {
        let url_entry = &self.imp().url_entry;
        let is_valid = otp::is_otp_url(url_entry.text().as_str());

        if is_valid {
            url_entry.remove_css_class("error");
            url_entry.set_tooltip_text(None);
        } else {
            url_entry.add_css_class("error");
            url_entry.set_tooltip_text(Some("OTP URL must start with otpauth://"));
        }
    }
}

impl EntryFieldRow for OtpFieldRow {
    fn key(&self) -> String {
        self.key().trim().to_string()
    }

    fn entry_field(&self) -> EntryField {
        EntryField::OTP(self.url().trim().to_string())
    }

    fn set_entry_field(&self, field: &EntryField) {
        if let EntryField::OTP(value) = field {
            self.set_url(value);
        }
    }
}
