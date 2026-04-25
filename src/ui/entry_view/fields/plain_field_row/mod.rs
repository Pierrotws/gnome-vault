use gtk::{glib, prelude::*, subclass::prelude::*};

use crate::pass::model::EntryField;

use super::super::EntryView;
use super::EntryFieldRow;

mod imp;

glib::wrapper! {
    pub struct PlainFieldRow(ObjectSubclass<imp::PlainFieldRow>)
        @extends gtk::Widget, gtk::ListBoxRow,
        @implements gtk::Accessible, gtk::Actionable, gtk::Buildable, gtk::ConstraintTarget;
}

impl PlainFieldRow {
    pub fn new_empty(entry_view: &EntryView) -> Self {
        let obj: Self = glib::Object::new();
        let imp = obj.imp();

        let parent = entry_view.clone();
        imp.key_entry.connect_changed(move |_| {
            parent.mark_changed();
        });
        let parent = entry_view.clone();
        imp.value_entry.connect_changed(move |_| {
            parent.mark_changed();
        });
        let value_entry = imp.value_entry.clone();
        imp.copy_button.connect_clicked(move |_| {
            if let Some(display) = gtk::gdk::Display::default() {
                let clipboard = display.clipboard();
                clipboard.set_text(&value_entry.text());
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
        let obj: Self = Self::new_empty(entry_view);
        obj.set_key(key);
        obj.set_value(value);
        obj
    }

    pub fn from_entry_field(entry_view: &EntryView, key: &str, field: &EntryField) -> Option<Self> {
        match field {
            EntryField::Plain(value) => Some(Self::new(entry_view, key, value)),
            _ => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        let imp = self.imp();
        imp.key_entry.text().trim().is_empty() && imp.value_entry.text().trim().is_empty()
    }

    pub fn key(&self) -> String {
        self.imp().key_entry.text().into()
    }

    pub fn value(&self) -> glib::GString {
        self.imp().value_entry.text().into()
    }

    pub fn set_key(&self, key: &str) {
        self.imp().key_entry.set_text(key);
    }

    pub fn set_value(&self, value: &str) {
        self.imp().value_entry.set_text(value);
    }

    pub fn drag_handle(&self) -> gtk::Widget {
        self.imp().drag_handle.get().upcast()
    }
}

impl EntryFieldRow for PlainFieldRow {
    fn key(&self) -> String {
        self.key().trim().to_string()
    }

    fn entry_field(&self) -> EntryField {
        EntryField::Plain(self.value().trim().to_string())
    }

    fn set_entry_field(&self, field: &EntryField) {
        if let EntryField::Plain(value) = field {
            self.set_value(value);
        }
    }
}
