mod fields;
mod imp;

use adw::prelude::*;
use gtk::glib;
use gtk::subclass::prelude::*;

//use crate::pass::model::*;
use crate::app::state::EntryViewData;
use crate::pass::model::{EntryData, EntryField};
use crate::ui::generate_password_view::GeneratePasswordView;
use fields::{EntryFieldRow, MultilineFieldRow, PlainFieldRow};

glib::wrapper! {
    pub struct EntryView(ObjectSubclass<imp::EntryView>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for EntryView {
    fn default() -> Self {
        let r = Self::new();
        r.set_cancellable(false);
        r.set_saveable(false);
        r
    }
}

impl EntryView {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn setup_callbacks(&self) {
        let imp = self.imp();

        imp.password_field_row.connect_copy_clicked({
            let password_field_row = imp.password_field_row.clone();
            move |_| {
                if let Some(display) = gtk::gdk::Display::default() {
                    display.clipboard().set_text(&password_field_row.text());
                }
            }
        });
        let this = self.clone();
        imp.password_field_row.connect_generate_clicked(move |_| {
            this.show_generate_password_dialog();
        });
        let this = self.clone();
        imp.add_plain_field_button.connect_clicked(move |_| {
            let row = PlainFieldRow::new_empty(&this);
            this.imp().custom_fields_list.append(&row);
        });
        imp.add_array_field_button.connect_clicked(move |_| {
            todo!("Add Array field row");
        });
        imp.add_otp_field_button.connect_clicked(move |_| {
            todo!("Add OTP field row");
        });
        let this = self.clone();
        imp.add_multiline_field_button.connect_clicked(move |_| {
            let row = MultilineFieldRow::new(&this, "Notes", "");
            this.imp().custom_fields_list.append(&row);
        });
        let this = self.clone();
        imp.cancel_button.connect_clicked(move |_| {
            this.emit_by_name::<()>("revert-requested", &[]);
        });
        let this = self.clone();
        imp.save_button.connect_clicked(move |_| {
            this.emit_by_name::<()>("save-requested", &[]);
        });
        let this = self.clone();
        imp.password_field_row.connect_changed(move |_| {
            this.mark_changed();
        });
    }

    pub fn display_empty(&self) {
        let imp = self.imp();
        imp.content_stack.set_visible_child_name("empty");
        imp.title_label.set_text("");
        imp.password_field_row.set_text("");
        self.clear_listbox();
        self.set_saveable(false);
        self.set_cancellable(false);
    }

    pub fn set_entry_data(&self, data: &EntryViewData) {
        let imp = self.imp();

        imp.is_updating_ui.set(true);
        imp.content_stack.set_visible_child_name("content");
        imp.title_label.set_text(&data.title);

        imp.password_field_row.set_entry_field(&data.entry.password);

        self.clear_listbox();
        for (key, value) in &data.entry.fields {
            if let Some(row) = self.build_field_row(key, value) {
                imp.custom_fields_list.append(&row);
            }
        }

        imp.is_updating_ui.set(false);
    }

    /// Rebuild an EntryViewData
    pub fn to_entry_view_data(&self) -> EntryViewData {
        let imp = self.imp();

        let title = imp.title_label.get().text().to_string();
        let (_password_key, password) = imp.password_field_row.named_entry_field();

        let mut fields = Vec::new();
        let mut child = imp.custom_fields_list.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();

            if let Some((key, field)) = Self::read_field_row(&widget) {
                fields.push((key, field));
            }
        }
        //Returns
        EntryViewData {
            title,
            entry: EntryData { password, fields },
        }
    }

    pub fn set_saveable(&self, val: bool) {
        let imp = self.imp();
        //imp.save_button.set_visible(val);
        let save = imp.save_button.get();
        save.set_opacity(if val { 1.0 } else { 0.0 });
        save.set_sensitive(val);
    }

    pub fn set_cancellable(&self, val: bool) {
        let imp = self.imp();
        imp.cancel_button.set_visible(val);
    }

    pub fn mark_changed(&self) {
        let imp = self.imp();
        if imp.is_updating_ui.get() {
            return;
        }
        self.emit_by_name::<()>("entry-changed", &[]);
    }

    pub fn show_error(&self, msg: &str) {
        let dialog = adw::AlertDialog::builder()
            .heading("Error")
            .body(msg)
            .build();
        dialog.add_responses(&[("ok", "OK")]);
        dialog.set_default_response(Some("ok"));
        dialog.set_close_response("ok");
        dialog.present(Some(self));
    }

    pub fn connect_entry_changed<F>(&self, f: F)
    where
        F: Fn(&EntryView) + 'static,
    {
        self.connect_local("entry-changed", false, move |values| {
            let view = values[0]
                .get::<EntryView>()
                .expect("entry-changed: invalid EntryView");
            f(&view);
            None
        });
    }

    pub fn connect_save_requested<F>(&self, f: F)
    where
        F: Fn(&EntryView) + 'static,
    {
        self.connect_local("save-requested", false, move |values| {
            let view = values[0]
                .get::<EntryView>()
                .expect("save-requested: invalid EntryView");
            f(&view);
            None
        });
    }

    pub fn connect_revert_requested<F>(&self, f: F)
    where
        F: Fn(&EntryView) + 'static,
    {
        self.connect_local("revert-requested", false, move |values| {
            let view = values[0]
                .get::<EntryView>()
                .expect("revert-requested: invalid EntryView");
            f(&view);
            None
        });
    }

    fn show_generate_password_dialog(&self) {
        let parent = self
            .root()
            .and_then(|root| root.downcast::<gtk::Window>().ok());

        let dialog = adw::Window::builder()
            .title("Generate Password")
            .modal(true)
            .resizable(true)
            .default_width(640)
            .default_height(320)
            .build();

        if let Some(parent) = parent.as_ref() {
            dialog.set_transient_for(Some(parent));
        }

        let main_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .build();

        let content = GeneratePasswordView::new();

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
        let ok_button = gtk::Button::with_label("OK");
        ok_button.add_css_class("suggested-action");

        actions.append(&cancel_button);
        actions.append(&ok_button);
        main_box.append(&content);
        main_box.append(&actions);

        dialog.set_content(Some(&main_box));

        {
            let dialog = dialog.clone();
            cancel_button.connect_clicked(move |_| {
                dialog.close();
            });
        }

        {
            let this = self.clone();
            let dialog = dialog.clone();
            let content = content.clone();

            ok_button.connect_clicked(move |_| {
                let password = content.password();
                this.imp().password_field_row.set_text(&password);
                this.mark_changed();
                dialog.close();
            });
        }

        dialog.present();
    }

    fn build_field_row(&self, key: &str, field: &EntryField) -> Option<gtk::ListBoxRow> {
        if let Some(row) = PlainFieldRow::from_entry_field(self, key, field) {
            return Some(row.upcast());
        }
        if let Some(row) = MultilineFieldRow::from_entry_field(self, key, field) {
            return Some(row.upcast());
        }

        None
    }

    fn read_field_row(widget: &gtk::Widget) -> Option<(String, EntryField)> {
        if let Ok(row) = widget.clone().downcast::<PlainFieldRow>() {
            if row.is_empty() {
                return None;
            }

            return Some(row.named_entry_field());
        }
        if let Ok(row) = widget.clone().downcast::<MultilineFieldRow>() {
            if row.is_empty() {
                return None;
            }

            return Some(row.named_entry_field());
        }

        None
    }

    fn clear_listbox(&self) {
        let list = &self.imp().custom_fields_list;
        while let Some(child) = list.first_child() {
            list.remove(&child);
        }
    }
}
