use gtk::{glib, prelude::*, subclass::prelude::*};

use crate::helpers::clipboard;
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

        imp.key_entry.connect_changed(glib::clone!(
            #[weak]
            entry_view,
            move |_| {
                entry_view.mark_changed();
            }
        ));
        imp.value_entry.connect_changed(glib::clone!(
            #[weak(rename_to = this)]
            obj,
            #[weak]
            entry_view,
            move |_| {
                this.update_value_link();
                entry_view.mark_changed();
            }
        ));
        // The copy button captures the template's value_entry strongly, but
        // value_entry is owned by `obj` (which holds the imp). The closure
        // lives on a button that is also owned by `obj`. This is a sibling
        // capture via the same root, not a parent->child cycle, so a strong
        // reference is fine.
        let value_entry = imp.value_entry.clone();
        imp.copy_button.connect_clicked(move |_| {
            clipboard::copy_secret(&value_entry.text());
        });

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
        self.update_value_link();
    }

    pub fn set_editable_mode(&self, editable: bool) {
        let imp = self.imp();
        imp.drag_handle.set_visible(editable);
        Self::set_entry_editable_mode(&imp.key_entry, editable);
        Self::set_entry_editable_mode(&imp.value_entry, editable);
        imp.delete_button.set_visible(editable);
        self.update_value_link();
    }

    pub fn drag_handle(&self) -> gtk::Widget {
        self.imp().drag_handle.get().upcast()
    }

    fn set_entry_editable_mode(entry: &gtk::Entry, editable: bool) {
        entry.set_editable(editable);
        entry.set_can_focus(editable);
        entry.set_has_frame(editable);
    }

    fn update_value_link(&self) {
        let imp = self.imp();
        let value = imp.value_entry.text();
        let url = value.trim();
        let show_link = !imp.value_entry.is_editable() && Self::is_web_url(url);

        imp.value_entry.set_visible(!show_link);
        imp.value_link_button.set_visible(show_link);

        if show_link {
            imp.value_link_button.set_uri(url);
            imp.value_link_button.set_label(url);
            imp.value_link_button.set_tooltip_text(Some(url));
        } else {
            imp.value_link_button.set_tooltip_text(None);
        }
    }

    fn is_web_url(value: &str) -> bool {
        let Some(rest) = value
            .strip_prefix("https://")
            .or_else(|| value.strip_prefix("http://"))
        else {
            return false;
        };

        !rest.is_empty()
            && rest.contains('.')
            && !rest
                .chars()
                .any(|character| character.is_ascii_whitespace())
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
