use gtk::{gio, glib, prelude::*, subclass::prelude::*};
use gtk_markdown::MarkdownTextView;

use crate::helpers::clipboard;
use crate::pass::model::EntryField;

use super::super::EntryView;
use super::EntryFieldRow;

mod imp;

const MARKDOWN_SETTING_KEY: &str = "interpret-multiline-as-markdown";

/// Offset applied to gtk-markdown heading-level CSS classes so a markdown
/// `#` heading renders as `title-2` (custom field keys already use
/// `title-2`, and headings inside the value should not be visually larger).
const HEADING_LEVEL_OFFSET: u32 = 1;

glib::wrapper! {
    pub struct MultilineFieldRow(ObjectSubclass<imp::MultilineFieldRow>)
        @extends gtk::Widget, gtk::ListBoxRow,
        @implements gtk::Accessible, gtk::Actionable, gtk::Buildable, gtk::ConstraintTarget;
}

impl MultilineFieldRow {
    pub fn new_empty(entry_view: &EntryView) -> Self {
        let obj: Self = glib::Object::new();
        let imp = obj.imp();

        imp.title_entry.connect_changed(glib::clone!(
            #[weak]
            entry_view,
            move |_| {
                entry_view.mark_changed();
            }
        ));

        let buffer = imp.value_text_view.buffer();
        buffer.connect_changed(glib::clone!(
            #[weak(rename_to = this)]
            obj,
            #[weak]
            entry_view,
            move |_| {
                this.sync_value_view();
                entry_view.mark_changed();
            }
        ));

        imp.copy_button.connect_clicked(glib::clone!(
            #[weak(rename_to = this)]
            obj,
            move |_| {
                clipboard::copy_secret(&this.value());
            }
        ));

        imp.delete_button.connect_clicked(glib::clone!(
            #[weak(rename_to = this)]
            obj,
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

        if settings_has_key(MARKDOWN_SETTING_KEY) {
            let settings = gio::Settings::new(crate::APP_ID);
            settings.connect_changed(
                Some(MARKDOWN_SETTING_KEY),
                glib::clone!(
                    #[weak(rename_to = this)]
                    obj,
                    move |_, _| {
                        this.sync_value_view();
                    }
                ),
            );
            let _ = imp.settings.set(settings);
        }

        obj
    }

    pub fn new(entry_view: &EntryView, key: &str, value: &str) -> Self {
        let obj = Self::new_empty(entry_view);
        obj.set_key(key);
        obj.set_value(value);
        obj
    }

    pub fn from_entry_field(entry_view: &EntryView, key: &str, field: &EntryField) -> Option<Self> {
        match field {
            EntryField::Multiline(value) => Some(Self::new(entry_view, key, value)),
            _ => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.key().trim().is_empty() && self.value().trim().is_empty()
    }

    pub fn key(&self) -> String {
        self.imp().title_entry.text().into()
    }

    pub fn set_key(&self, key: &str) {
        let title_entry = &self.imp().title_entry;
        title_entry.set_text(key);
    }

    pub fn set_editable_mode(&self, editable: bool) {
        let imp = self.imp();
        imp.drag_handle.set_visible(editable);
        Self::set_entry_editable_mode(&imp.title_entry, editable);
        imp.value_view_box.set_visible(!editable);
        imp.value_scrolled_window.set_visible(editable);
        imp.value_text_view.set_editable(editable);
        imp.value_text_view.set_can_focus(editable);
        imp.value_text_view.set_cursor_visible(editable);
        imp.delete_button.set_visible(editable);
    }

    pub fn drag_handle(&self) -> gtk::Widget {
        self.imp().drag_handle.get().upcast()
    }

    pub fn value(&self) -> String {
        let buffer = self.imp().value_text_view.buffer();
        let start = buffer.start_iter();
        let end = buffer.end_iter();
        buffer.text(&start, &end, true).to_string()
    }

    pub fn set_value(&self, value: &str) {
        self.imp().value_text_view.buffer().set_text(value);
        self.sync_value_view();
    }

    fn sync_value_view(&self) {
        let value_view_box = &self.imp().value_view_box;
        let value = self.value();
        clear_box(value_view_box);

        if self.markdown_enabled() {
            let view = MarkdownTextView::new();
            view.set_heading_level_offset(HEADING_LEVEL_OFFSET);
            view.set_markdown(&value);
            value_view_box.append(&view);
        } else {
            value_view_box.append(&plain_text_label(&value));
        }
    }

    fn set_entry_editable_mode(entry: &gtk::Entry, editable: bool) {
        entry.set_editable(editable);
        entry.set_can_focus(editable);
        entry.set_has_frame(editable);
    }

    fn markdown_enabled(&self) -> bool {
        self.imp()
            .settings
            .get()
            .is_some_and(|settings| settings.boolean(MARKDOWN_SETTING_KEY))
    }
}

fn plain_text_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_wrap(true);
    label.set_xalign(0.0);
    label.set_selectable(true);
    label
}

fn clear_box(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

fn settings_has_key(key: &str) -> bool {
    gio::SettingsSchemaSource::default()
        .and_then(|source| source.lookup(crate::APP_ID, true))
        .is_some_and(|schema| schema.has_key(key))
}

impl EntryFieldRow for MultilineFieldRow {
    fn key(&self) -> String {
        self.key().trim().to_string()
    }

    fn entry_field(&self) -> EntryField {
        EntryField::Multiline(self.value().trim().to_string())
    }

    fn set_entry_field(&self, field: &EntryField) {
        if let EntryField::Multiline(value) = field {
            self.set_value(value);
        }
    }
}
