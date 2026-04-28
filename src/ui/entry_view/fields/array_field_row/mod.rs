use gtk::{glib, prelude::*, subclass::prelude::*};

use crate::helpers::clipboard;
use crate::pass::model::EntryField;

use super::super::EntryView;
use super::EntryFieldRow;

mod imp;

glib::wrapper! {
    pub struct ArrayFieldRow(ObjectSubclass<imp::ArrayFieldRow>)
        @extends gtk::Widget, gtk::ListBoxRow,
        @implements gtk::Accessible, gtk::Actionable, gtk::Buildable, gtk::ConstraintTarget;
}

impl ArrayFieldRow {
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

        imp.add_item_button.connect_clicked(glib::clone!(
            #[weak(rename_to = this)]
            obj,
            #[weak]
            entry_view,
            move |_| {
                this.append_value_entry("", Some(&entry_view));
                entry_view.mark_changed();
            }
        ));

        imp.copy_button.connect_clicked(glib::clone!(
            #[weak(rename_to = this)]
            obj,
            move |_| {
                clipboard::copy_secret(&this.values().join("\n"));
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

        obj
    }

    pub fn new(entry_view: &EntryView, key: &str, values: &[String]) -> Self {
        let obj = Self::new_empty(entry_view);
        obj.set_key(key);
        obj.set_values(values, Some(entry_view));
        obj
    }

    pub fn from_entry_field(entry_view: &EntryView, key: &str, field: &EntryField) -> Option<Self> {
        match field {
            EntryField::Array(values) => Some(Self::new(entry_view, key, values)),
            _ => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.key().trim().is_empty() && self.values().iter().all(|value| value.trim().is_empty())
    }

    pub fn key(&self) -> String {
        self.imp().title_entry.text().into()
    }

    pub fn set_key(&self, key: &str) {
        self.imp().title_entry.set_text(key);
    }

    pub fn set_editable_mode(&self, editable: bool) {
        let imp = self.imp();
        imp.drag_handle.set_visible(editable);
        Self::set_entry_editable_mode(&imp.title_entry, editable);
        imp.add_item_button.set_visible(editable);
        imp.delete_button.set_visible(editable);

        let mut child = imp.value_list.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            Self::set_value_row_editable(&widget, editable);
        }
    }

    pub fn drag_handle(&self) -> gtk::Widget {
        self.imp().drag_handle.get().upcast()
    }

    pub fn values(&self) -> Vec<String> {
        let list = &self.imp().value_list;
        let mut values = Vec::new();
        let mut child = list.first_child();

        while let Some(widget) = child {
            child = widget.next_sibling();

            if let Some(value) = Self::read_value_row(&widget) {
                values.push(value);
            }
        }

        values
    }

    pub fn set_values(&self, values: &[String], entry_view: Option<&EntryView>) {
        self.clear_values();

        if values.is_empty() {
            self.append_value_entry("", entry_view);
            return;
        }

        for value in values {
            self.append_value_entry(value, entry_view);
        }
    }

    fn append_value_entry(&self, value: &str, entry_view: Option<&EntryView>) {
        let row = gtk::ListBoxRow::new();
        let box_ = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(6)
            .margin_end(6)
            .build();
        let entry = gtk::Entry::builder()
            .hexpand(true)
            .placeholder_text("Value")
            .text(value)
            .build();
        let delete_button = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .tooltip_text("Delete item")
            .valign(gtk::Align::Center)
            .build();

        if let Some(entry_view) = entry_view {
            entry.connect_changed(glib::clone!(
                #[weak]
                entry_view,
                move |_| {
                    entry_view.mark_changed();
                }
            ));
        }

        // The per-value delete button captures `row` strongly because the row
        // is a freshly-created local widget owned only by `value_list`; the
        // closure needs to keep the row reference for `.parent()` lookup. The
        // EntryView reference, however, is captured weakly to avoid a cycle.
        let row_to_delete = row.clone();
        let entry_view_weak = entry_view.map(|view| view.downgrade());
        delete_button.connect_clicked(move |_| {
            if let Some(list) = row_to_delete.parent() {
                if let Ok(listbox) = list.downcast::<gtk::ListBox>() {
                    listbox.remove(&row_to_delete);
                    if let Some(parent) = entry_view_weak.as_ref().and_then(|w| w.upgrade()) {
                        parent.mark_changed();
                    }
                }
            }
        });

        box_.append(&entry);
        box_.append(&delete_button);
        row.set_child(Some(&box_));
        self.imp().value_list.append(&row);
    }

    fn set_value_row_editable(widget: &gtk::Widget, editable: bool) -> Option<()> {
        let row = widget.clone().downcast::<gtk::ListBoxRow>().ok()?;
        let child = row.child()?;
        let container = child.downcast::<gtk::Box>().ok()?;
        let entry = container.first_child()?.downcast::<gtk::Entry>().ok()?;
        let delete_button = entry.next_sibling()?.downcast::<gtk::Button>().ok()?;

        Self::set_entry_editable_mode(&entry, editable);
        delete_button.set_visible(editable);
        Some(())
    }

    fn set_entry_editable_mode(entry: &gtk::Entry, editable: bool) {
        entry.set_editable(editable);
        entry.set_can_focus(editable);
        entry.set_has_frame(editable);
    }

    fn read_value_row(widget: &gtk::Widget) -> Option<String> {
        let row = widget.clone().downcast::<gtk::ListBoxRow>().ok()?;
        let child = row.child()?;
        let container = child.downcast::<gtk::Box>().ok()?;
        let entry = container.first_child()?.downcast::<gtk::Entry>().ok()?;

        Some(entry.text().trim().to_string())
    }

    fn clear_values(&self) {
        let list = &self.imp().value_list;
        while let Some(child) = list.first_child() {
            list.remove(&child);
        }
    }
}

impl EntryFieldRow for ArrayFieldRow {
    fn key(&self) -> String {
        self.key().trim().to_string()
    }

    fn entry_field(&self) -> EntryField {
        EntryField::Array(
            self.values()
                .into_iter()
                .filter(|value| !value.is_empty())
                .collect(),
        )
    }

    fn set_entry_field(&self, field: &EntryField) {
        if let EntryField::Array(values) = field {
            self.set_values(values, None);
        }
    }
}
