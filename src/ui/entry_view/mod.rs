mod fields;
mod imp;

use adw::prelude::*;
use gtk::glib;
use gtk::subclass::prelude::*;

//use crate::pass::model::*;
use crate::app::state::EntryViewData;
use crate::helpers::{clipboard, otp};
use crate::i18n::gettext;
use crate::pass::model::{EntryData, EntryField};
use crate::ui::generate_password_view::GeneratePasswordView;
use fields::{
    with_custom_field_row, CustomFieldRow, EntryFieldRow, MultilineFieldRow, OtpFieldRow,
    PlainFieldRow,
};

/// Maximum number of characters the title `EntryRow` widens to before
/// it stops growing. Past this point the title visibly truncates with
/// an ellipsis, but the headerbar layout stays stable.
const MAX_TITLE_WIDTH_CHARS: i32 = 48;

glib::wrapper! {
    pub struct EntryView(ObjectSubclass<imp::EntryView>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for EntryView {
    fn default() -> Self {
        let r = Self::new();
        r.set_editable_mode(false);
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
        // password_field_row is a TemplateChild owned by self; capturing it
        // strongly into its own copy-button closure does not form a cycle
        // through self.
        imp.password_field_row.connect_copy_clicked({
            let password_field_row = imp.password_field_row.clone();
            move |_| {
                clipboard::copy_secret(&password_field_row.text());
            }
        });
        imp.password_field_row
            .connect_generate_clicked(glib::clone!(
                #[weak(rename_to = this)]
                self,
                move |_| {
                    this.show_generate_password_dialog();
                }
            ));
        imp.add_plain_field_button.connect_clicked(glib::clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                let row = PlainFieldRow::new_empty(&this);
                this.append_custom_field_row(&row);
            }
        ));
        imp.add_otp_field_button.connect_clicked(glib::clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                let row = OtpFieldRow::new(&this, "OTP", "");
                this.append_custom_field_row(&row);
            }
        ));
        imp.add_multiline_field_button.connect_clicked(glib::clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                let row = MultilineFieldRow::new(&this, "Notes", "");
                this.append_custom_field_row(&row);
            }
        ));
        imp.edit_button.connect_clicked(glib::clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                this.set_editable_mode(true);
                // Edit is hidden now; keep keyboard focus in the same row.
                this.imp().cancel_button.grab_focus();
            }
        ));
        imp.cancel_button.connect_clicked(glib::clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                this.emit_by_name::<()>("revert-requested", &[]);
            }
        ));
        imp.save_button.connect_clicked(glib::clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                this.emit_by_name::<()>("save-requested", &[]);
            }
        ));
        imp.delete_button.connect_clicked(glib::clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                this.emit_by_name::<()>("delete-requested", &[]);
            }
        ));
        imp.password_field_row.connect_changed(glib::clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                this.mark_changed();
            }
        ));
        imp.title_entry.connect_changed(glib::clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                this.resize_title_entry();
                this.mark_changed();
            }
        ));
        // Enter-to-save: pressing Enter in the title entry behaves like
        // clicking the save button when the entry is in saveable state.
        // Falling silently otherwise mirrors how toolbar Save reacts to
        // the disabled button.
        imp.title_entry.connect_activate(glib::clone!(
            #[weak(rename_to = this)]
            self,
            move |_| {
                if this.imp().save_button.is_sensitive() {
                    this.emit_by_name::<()>("save-requested", &[]);
                }
            }
        ));
    }

    pub fn display_empty(&self) {
        let imp = self.imp();
        imp.is_updating_ui.set(true);
        imp.content_stack.set_visible_child_name("empty");
        imp.title_entry.set_text("");
        imp.password_field_row.set_text("");
        imp.primary_otp_field_row.set_url("");
        imp.primary_field_stack.set_visible_child_name("password");
        self.clear_listbox();
        self.set_editable_mode(false);
        self.set_saveable(false);
        imp.is_updating_ui.set(false);
    }

    pub fn set_entry_data(&self, data: &EntryViewData) {
        let imp = self.imp();

        imp.is_updating_ui.set(true);
        imp.content_stack.set_visible_child_name("content");
        imp.title_entry.set_text(&data.title);
        self.resize_title_entry();

        self.set_primary_field(&data.entry.password);

        self.clear_listbox();
        for (key, value) in &data.entry.fields {
            if let Some(row) = self.build_field_row(key, value) {
                self.append_custom_field_row(&row);
            }
        }

        self.set_editable_mode(false);
        self.set_saveable(false);
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

    /// Save is shown throughout edit mode, enabled once there are valid
    /// changes to save.
    pub fn set_saveable(&self, val: bool) {
        self.imp().save_button.set_sensitive(val);
    }

    pub fn set_editable_mode(&self, editable: bool) {
        if !editable {
            self.normalize_primary_field_mode();
        }

        let imp = self.imp();
        imp.is_editable.set(editable);
        imp.edit_actions_stack
            .set_visible_child_name(if editable { "edit" } else { "view" });
        // The title is one always-visible frameless entry, so the only
        // thing that changes here is whether it takes input.
        fields::set_entry_editable(&imp.title_entry, editable);
        imp.password_field_row.set_editable_mode(editable);
        imp.primary_otp_field_row.set_editable_mode(editable);
        // The "add field" row sits after every field, so hiding it
        // outright costs nothing above it. The footer is different: it
        // is anchored under the scrolled field list, and dropping it
        // let the list grow into the freed space, so the whole pane
        // shifted the moment edit mode opened. Reserve its height and
        // only its contents come and go.
        imp.add_field_row.set_visible(editable);
        fields::set_control_reserved(&imp.footer_actions.get(), editable);

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

    /// Pops an `adw::AlertDialog` with the supplied message, parented on
    /// this `EntryView`. Despite the name's hint at an inline label, this
    /// is the standard error-dialog helper for cases where the
    /// `EntryView` is the nearest reachable widget host (i.e. an entry is
    /// open or the entry pane is wired up). For early-startup paths
    /// without an `EntryView` (e.g. the setup wizard), see
    /// `MainWindow::show_error_dialog`.
    pub fn show_error_dialog(&self, msg: &str) {
        let dialog = adw::AlertDialog::builder()
            .heading(gettext("Error"))
            .body(msg)
            .build();
        dialog.add_responses(&[("ok", &gettext("OK"))]);
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
            .title(gettext("Generate Password"))
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

        let cancel_button = gtk::Button::with_label(&gettext("Cancel"));
        let ok_button = gtk::Button::with_label(&gettext("OK"));
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
            // The OK closure captures the dialog and the GeneratePasswordView
            // strongly, but neither is owned by `self`. `this` is held weakly
            // so the dialog cannot keep this EntryView alive.
            ok_button.connect_clicked(glib::clone!(
                #[weak(rename_to = this)]
                self,
                #[strong]
                dialog,
                #[strong]
                content,
                move |_| {
                    let password = content.password();
                    this.imp().password_field_row.set_text(&password);
                    this.mark_changed();
                    dialog.close();
                }
            ));
        }

        dialog.present();
    }

    fn build_field_row(&self, key: &str, field: &EntryField) -> Option<gtk::ListBoxRow> {
        if let Some(row) = PlainFieldRow::from_entry_field(self, key, field) {
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
            EntryField::Otp(_) => {
                imp.primary_otp_field_row.set_entry_field(field);
                imp.primary_field_stack.set_visible_child_name("otp");
            }
            _ => {
                imp.password_field_row.set_entry_field(field);
                imp.primary_field_stack.set_visible_child_name("password");
            }
        }
    }

    /// Horizontal size group shared by the key entries of every plain
    /// key/value row, so the key column is as wide as its longest key
    /// and all rows put their separator at the same x.
    pub fn key_size_group(&self) -> gtk::SizeGroup {
        self.imp()
            .key_size_group
            .get_or_init(|| gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal))
            .clone()
    }

    fn resize_title_entry(&self) {
        let width = self
            .imp()
            .title_entry
            .text()
            .chars()
            .count()
            .clamp(1, MAX_TITLE_WIDTH_CHARS as usize) as i32;
        let title_entry = &self.imp().title_entry;
        title_entry.set_width_chars(width);
        title_entry.set_max_width_chars(width);
    }

    fn primary_entry_field(&self) -> EntryField {
        let imp = self.imp();
        if imp.primary_field_stack.visible_child_name().as_deref() == Some("otp") {
            return imp.primary_otp_field_row.entry_field();
        }

        let text = imp.password_field_row.text().to_string();
        if otp::is_otp_url(&text) {
            EntryField::Otp(text)
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
        self.set_primary_field(&EntryField::Otp(text));
        imp.is_updating_ui.set(was_updating);
    }

    fn read_field_row(widget: &gtk::Widget) -> Option<(String, EntryField)> {
        with_custom_field_row(widget, |row| {
            if CustomFieldRow::is_empty(row) {
                None
            } else {
                Some(row.named_entry_field())
            }
        })
        .flatten()
    }

    fn set_field_row_editable(widget: &gtk::Widget, editable: bool) {
        with_custom_field_row(widget, |row| row.set_editable_mode(editable));
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
        // Carry the row GObject itself through the content provider so
        // the drop handler resolves the source by identity (re-querying
        // its current index at drop time). The previous payload was the
        // index captured at drag start, which pointed at the wrong row
        // if anything mutated the list mid-drag.
        let row_for_drag = row.clone();
        drag_source.connect_prepare(move |_, _, _| {
            Some(gtk::gdk::ContentProvider::for_value(
                &row_for_drag.to_value(),
            ))
        });
        handle.add_controller(drag_source);

        let drop_target =
            gtk::DropTarget::new(gtk::ListBoxRow::static_type(), gtk::gdk::DragAction::MOVE);
        let target_row = row.clone();
        drop_target.connect_drop(glib::clone!(
            #[weak(rename_to = this)]
            self,
            #[upgrade_or]
            false,
            move |_, value, _, y| {
                let Ok(source_row) = value.get::<gtk::ListBoxRow>() else {
                    return false;
                };
                this.move_custom_field_row(&source_row, &target_row, y)
            }
        ));
        row.add_controller(drop_target);
    }

    fn field_row_drag_handle(row: &gtk::ListBoxRow) -> Option<gtk::Widget> {
        with_custom_field_row(row.upcast_ref::<gtk::Widget>(), |row| row.drag_handle())
    }

    fn move_custom_field_row(
        &self,
        source_row: &gtk::ListBoxRow,
        target_row: &gtk::ListBoxRow,
        drop_y: f64,
    ) -> bool {
        if !self.imp().is_editable.get() {
            return false;
        }

        let list = self.imp().custom_fields_list.get();
        // Re-query the source row's current index instead of trusting a
        // stale value captured at drag start; if it has been removed
        // from the list since then, abort.
        let Some(source_index) = Self::box_child_index(&list, source_row.upcast_ref()) else {
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

        list.remove(source_row);
        Self::box_insert_child_at_index(&list, source_row.upcast_ref(), insert_index);
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
