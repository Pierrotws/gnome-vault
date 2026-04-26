mod fields;
mod imp;

use adw::prelude::*;
use gtk::glib;
use gtk::subclass::prelude::*;

//use crate::pass::model::*;
use crate::app::state::EntryViewData;
use crate::helpers::otp;
use crate::pass::model::{EntryData, EntryField};
use crate::ui::generate_password_view::GeneratePasswordView;
use fields::{ArrayFieldRow, EntryFieldRow, MultilineFieldRow, OtpFieldRow, PlainFieldRow};

glib::wrapper! {
    pub struct EntryView(ObjectSubclass<imp::EntryView>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for EntryView {
    fn default() -> Self {
        let r = Self::new();
        r.set_editable_mode(false);
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

        imp.primary_otp_field_row.setup_callbacks(self);
        imp.primary_otp_field_row.configure_as_primary_field();
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
            this.append_custom_field_row(&row);
        });
        let this = self.clone();
        imp.add_array_field_button.connect_clicked(move |_| {
            let row = ArrayFieldRow::new(&this, "List", &[]);
            this.append_custom_field_row(&row);
        });
        let this = self.clone();
        imp.add_otp_field_button.connect_clicked(move |_| {
            let row = OtpFieldRow::new(&this, "OTP", "");
            this.append_custom_field_row(&row);
        });
        let this = self.clone();
        imp.add_multiline_field_button.connect_clicked(move |_| {
            let row = MultilineFieldRow::new(&this, "Notes", "");
            this.append_custom_field_row(&row);
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
        imp.delete_button.connect_clicked(move |_| {
            this.emit_by_name::<()>("delete-requested", &[]);
        });
        let this = self.clone();
        imp.password_field_row.connect_changed(move |_| {
            this.mark_changed();
        });
        let this = self.clone();
        imp.title_entry.connect_changed(move |_| {
            this.imp()
                .title_label
                .set_text(&this.imp().title_entry.text());
            this.mark_changed();
        });
    }

    pub fn display_empty(&self) {
        let imp = self.imp();
        imp.is_updating_ui.set(true);
        imp.content_stack.set_visible_child_name("empty");
        imp.title_label.set_text("");
        imp.title_entry.set_text("");
        imp.password_field_row.set_text("");
        imp.primary_otp_field_row.set_url("");
        imp.primary_field_stack.set_visible_child_name("password");
        self.clear_listbox();
        self.set_editable_mode(false);
        self.set_saveable(false);
        self.set_cancellable(false);
        imp.is_updating_ui.set(false);
    }

    pub fn set_entry_data(&self, data: &EntryViewData) {
        let imp = self.imp();

        imp.is_updating_ui.set(true);
        imp.content_stack.set_visible_child_name("content");
        imp.title_label.set_text(&data.title);
        imp.title_entry.set_text(&data.title);

        self.set_primary_field(&data.entry.password);

        self.clear_listbox();
        for (key, value) in &data.entry.fields {
            if let Some(row) = self.build_field_row(key, value) {
                self.append_custom_field_row(&row);
            }
        }

        self.set_editable_mode(false);
        self.set_saveable(false);
        self.set_cancellable(false);
        imp.is_updating_ui.set(false);
    }

    /// Rebuild an EntryViewData
    pub fn to_entry_view_data(&self) -> EntryViewData {
        let imp = self.imp();

        let title = imp.title_entry.text().trim().to_string();
        let password = self.primary_entry_field();

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

    pub fn set_editable_mode(&self, editable: bool) {
        if !editable {
            self.normalize_primary_field_mode();
        }

        let imp = self.imp();
        imp.is_editable.set(editable);
        imp.title_label.set_visible(!editable);
        imp.title_entry.set_visible(editable);
        imp.title_entry.set_editable(editable);
        imp.title_entry.set_can_focus(editable);
        imp.password_field_row.set_editable_mode(editable);
        imp.primary_otp_field_row.set_editable_mode(editable);
        imp.add_field_row.set_visible(editable);
        imp.add_field_menu_button.set_visible(editable);
        imp.footer_actions.set_visible(editable);
        imp.delete_button.set_visible(editable);

        let mut child = imp.custom_fields_list.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            Self::set_field_row_editable(&widget, editable);
        }
    }

    pub fn is_editable_mode(&self) -> bool {
        self.imp().is_editable.get()
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

    pub fn connect_delete_requested<F>(&self, f: F)
    where
        F: Fn(&EntryView) + 'static,
    {
        self.connect_local("delete-requested", false, move |values| {
            let view = values[0]
                .get::<EntryView>()
                .expect("delete-requested: invalid EntryView");
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
        if let Some(row) = ArrayFieldRow::from_entry_field(self, key, field) {
            return Some(row.upcast());
        }
        if let Some(row) = OtpFieldRow::from_entry_field(self, key, field) {
            return Some(row.upcast());
        }
        if let Some(row) = MultilineFieldRow::from_entry_field(self, key, field) {
            return Some(row.upcast());
        }

        None
    }

    fn set_primary_field(&self, field: &EntryField) {
        let imp = self.imp();
        match field {
            EntryField::OTP(_) => {
                imp.primary_otp_field_row.set_entry_field(field);
                imp.primary_field_stack.set_visible_child_name("otp");
            }
            _ => {
                imp.password_field_row.set_entry_field(field);
                imp.primary_field_stack.set_visible_child_name("password");
            }
        }
    }

    fn primary_entry_field(&self) -> EntryField {
        let imp = self.imp();
        if imp.primary_field_stack.visible_child_name().as_deref() == Some("otp") {
            return imp.primary_otp_field_row.entry_field();
        }

        let text = imp.password_field_row.text().to_string();
        if otp::is_otp_url(&text) {
            EntryField::OTP(text)
        } else {
            imp.password_field_row.entry_field()
        }
    }

    fn normalize_primary_field_mode(&self) {
        let imp = self.imp();
        if imp.primary_field_stack.visible_child_name().as_deref() == Some("otp") {
            return;
        }

        let text = imp.password_field_row.text().to_string();
        if !otp::is_otp_url(&text) {
            return;
        }

        let was_updating = imp.is_updating_ui.get();
        imp.is_updating_ui.set(true);
        self.set_primary_field(&EntryField::OTP(text));
        imp.is_updating_ui.set(was_updating);
    }

    fn read_field_row(widget: &gtk::Widget) -> Option<(String, EntryField)> {
        if let Ok(row) = widget.clone().downcast::<PlainFieldRow>() {
            if row.is_empty() {
                return None;
            }

            return Some(row.named_entry_field());
        }
        if let Ok(row) = widget.clone().downcast::<ArrayFieldRow>() {
            if row.is_empty() {
                return None;
            }

            return Some(row.named_entry_field());
        }
        if let Ok(row) = widget.clone().downcast::<OtpFieldRow>() {
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

    fn set_field_row_editable(widget: &gtk::Widget, editable: bool) {
        if let Ok(row) = widget.clone().downcast::<PlainFieldRow>() {
            row.set_editable_mode(editable);
            return;
        }

        if let Ok(row) = widget.clone().downcast::<ArrayFieldRow>() {
            row.set_editable_mode(editable);
            return;
        }

        if let Ok(row) = widget.clone().downcast::<OtpFieldRow>() {
            row.set_editable_mode(editable);
            return;
        }

        if let Ok(row) = widget.clone().downcast::<MultilineFieldRow>() {
            row.set_editable_mode(editable);
        }
    }

    fn append_custom_field_row<R>(&self, row: &R)
    where
        R: IsA<gtk::ListBoxRow>,
    {
        let row = row.upcast_ref::<gtk::ListBoxRow>();
        self.setup_custom_field_drag(row);
        Self::set_field_row_editable(
            row.upcast_ref::<gtk::Widget>(),
            self.imp().is_editable.get(),
        );
        self.insert_custom_field_before_add_button(row);
    }

    fn insert_custom_field_before_add_button(&self, row: &gtk::ListBoxRow) {
        let imp = self.imp();
        let list = imp.custom_fields_list.get();
        let add_row = imp.add_field_row.get();

        match add_row.prev_sibling() {
            Some(previous_sibling) => list.insert_child_after(row, Some(&previous_sibling)),
            None => list.prepend(row),
        }
    }

    fn setup_custom_field_drag(&self, row: &gtk::ListBoxRow) {
        let Some(handle) = Self::field_row_drag_handle(row) else {
            return;
        };

        let drag_source = gtk::DragSource::builder()
            .actions(gtk::gdk::DragAction::MOVE)
            .build();
        let list_for_drag = self.imp().custom_fields_list.get();
        let row_for_drag = row.clone();
        drag_source.connect_prepare(move |_, _, _| {
            // Store the source row index in the drag payload. The row object
            // itself cannot be transferred through GTK's content provider.
            Self::box_child_index(&list_for_drag, row_for_drag.upcast_ref())
                .map(|index| gtk::gdk::ContentProvider::for_value(&index.to_value()))
        });
        handle.add_controller(drag_source);

        let drop_target = gtk::DropTarget::new(i32::static_type(), gtk::gdk::DragAction::MOVE);
        let this = self.clone();
        let target_row = row.clone();
        drop_target.connect_drop(move |_, value, _, y| {
            let Ok(source_index) = value.get::<i32>() else {
                return false;
            };

            this.move_custom_field_row(source_index, &target_row, y)
        });
        row.add_controller(drop_target);
    }

    fn field_row_drag_handle(row: &gtk::ListBoxRow) -> Option<gtk::Widget> {
        if let Ok(row) = row.clone().downcast::<PlainFieldRow>() {
            return Some(row.drag_handle());
        }

        if let Ok(row) = row.clone().downcast::<ArrayFieldRow>() {
            return Some(row.drag_handle());
        }

        if let Ok(row) = row.clone().downcast::<OtpFieldRow>() {
            return Some(row.drag_handle());
        }

        if let Ok(row) = row.clone().downcast::<MultilineFieldRow>() {
            return Some(row.drag_handle());
        }

        None
    }

    fn move_custom_field_row(
        &self,
        source_index: i32,
        target_row: &gtk::ListBoxRow,
        drop_y: f64,
    ) -> bool {
        if !self.imp().is_editable.get() {
            return false;
        }

        if source_index < 0 {
            return false;
        }

        let list = self.imp().custom_fields_list.get();
        let Some(source_row) = Self::box_child_at_index(&list, source_index) else {
            return false;
        };

        let Some(target_index) = Self::box_child_index(&list, target_row.upcast_ref()) else {
            return false;
        };

        let insert_after_target = drop_y > f64::from(target_row.height()) / 2.0;
        let mut insert_index = target_index + i32::from(insert_after_target);
        if source_index < insert_index {
            insert_index -= 1;
        }

        if source_index == insert_index {
            return false;
        }

        list.remove(&source_row);
        Self::box_insert_child_at_index(&list, &source_row, insert_index);
        self.mark_changed();

        true
    }

    fn box_child_index(list: &gtk::Box, target: &gtk::Widget) -> Option<i32> {
        let mut index = 0;
        let mut child = list.first_child();

        while let Some(widget) = child {
            if widget == *target {
                return Some(index);
            }

            child = widget.next_sibling();
            index += 1;
        }

        None
    }

    fn box_child_at_index(list: &gtk::Box, target_index: i32) -> Option<gtk::Widget> {
        let mut index = 0;
        let mut child = list.first_child();

        while let Some(widget) = child {
            if index == target_index {
                return Some(widget);
            }

            child = widget.next_sibling();
            index += 1;
        }

        None
    }

    fn box_insert_child_at_index(list: &gtk::Box, child: &gtk::Widget, index: i32) {
        if index <= 0 {
            list.prepend(child);
            return;
        }

        let previous_index = index - 1;
        let Some(previous_sibling) = Self::box_child_at_index(list, previous_index) else {
            list.append(child);
            return;
        };

        list.insert_child_after(child, Some(&previous_sibling));
    }

    fn clear_listbox(&self) {
        let imp = self.imp();
        let list = &imp.custom_fields_list;
        let add_row: gtk::Widget = imp.add_field_row.get().upcast();

        while let Some(child) = list.first_child() {
            if child == add_row {
                break;
            }

            list.remove(&child);
        }
    }
}
