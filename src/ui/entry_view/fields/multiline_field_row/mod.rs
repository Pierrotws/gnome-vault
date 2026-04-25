use gtk::{glib, prelude::*, subclass::prelude::*};

use crate::pass::model::EntryField;

use super::super::EntryView;
use super::EntryFieldRow;

mod imp;

glib::wrapper! {
    pub struct MultilineFieldRow(ObjectSubclass<imp::MultilineFieldRow>)
        @extends gtk::Widget, gtk::ListBoxRow,
        @implements gtk::Accessible, gtk::Actionable, gtk::Buildable, gtk::ConstraintTarget;
}

impl MultilineFieldRow {
    pub fn new_empty(entry_view: &EntryView) -> Self {
        let obj: Self = glib::Object::new();
        let imp = obj.imp();

        let parent = entry_view.clone();
        imp.title_entry.connect_changed(move |_| {
            parent.mark_changed();
        });

        let buffer = imp.value_text_view.buffer();
        let parent = entry_view.clone();
        buffer.connect_changed(move |_| {
            parent.mark_changed();
        });

        let this = obj.clone();
        imp.copy_button.connect_clicked(move |_| {
            if let Some(display) = gtk::gdk::Display::default() {
                display.clipboard().set_text(&this.value());
            }
        });

        let this = obj.clone();
        let parent = entry_view.clone();
        imp.delete_button.connect_clicked(move |_| {
            if let Some(list) = this.parent() {
                if let Ok(listbox) = list.downcast::<gtk::ListBox>() {
                    listbox.remove(&this);
                    parent.mark_changed();
                }
            }
        });

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
    }

    fn set_entry_editable_mode(entry: &gtk::Entry, editable: bool) {
        entry.set_editable(editable);
        entry.set_can_focus(editable);
        entry.set_has_frame(editable);
    }
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
