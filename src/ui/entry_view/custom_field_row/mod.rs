use gtk::{glib, prelude::*, subclass::prelude::*};

use crate::pass::entry::EntryField;

use super::EntryView;

mod imp;

glib::wrapper! {
    pub struct CustomFieldRow(ObjectSubclass<imp::CustomFieldRow>)
        @extends gtk::Widget, gtk::ListBoxRow,
        @implements gtk::Accessible, gtk::Actionable, gtk::Buildable, gtk::ConstraintTarget;
}

impl CustomFieldRow {
    pub fn new_empty(entry_view: &EntryView) -> Self {
        let obj: Self = glib::Object::new();
        let imp = obj.imp();

        let parent = entry_view.clone();
        imp.key_entry.connect_changed(move |_| {
            parent.set_modified(true);
        });
        let parent = entry_view.clone();
        imp.value_entry.connect_changed(move |_| {
            parent.set_modified(true);
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
                    if !this.is_empty() {
                        parent.set_modified(true);
                    }
                }
            }
        });
        obj
    }

    pub fn new(entry_view: &EntryView, key: &str, value: &EntryField) -> Self {
        let obj: Self = Self::new_empty(entry_view);
        obj.set_key(key);
        obj.set_value(&value.to_str());
        obj
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
}
