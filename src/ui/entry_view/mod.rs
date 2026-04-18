mod imp;

use gtk::subclass::prelude::*;
use gtk::{glib, prelude::*};

use crate::pass::entry::EntryData;

glib::wrapper! {
    pub struct EntryView(ObjectSubclass<imp::EntryView>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl EntryView {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn setup_callbacks(&self) {
        let imp = self.imp();

        imp.copy_password_button.connect_clicked({
            let password_row = imp.password_row.clone();
            move |_| {
                if let Some(display) = gtk::gdk::Display::default() {
                    display.clipboard().set_text(&password_row.text());
                }
            }
        });

        imp.generate_password_button.connect_clicked({
            //let password_row = imp.password_row.clone();
            move |_| {
                //TODO:
                //let new_password = generate_password();
                //password_row.set_text(&new_password);
            }
        });

        imp.add_field_button.connect_clicked(|_| {
            println!("Add field clicked");
        });
    }

    pub fn display_entry(&self, title: &str, entry: &EntryData) {
        let imp = self.imp();

        imp.content_stack.set_visible_child_name("content");

        imp.title_label.set_text(title);
        imp.password_row.set_text(&entry.password);

        clear_listbox(&imp.custom_fields_list);

        for (key, value) in &entry.fields {
            let row = build_custom_field_row(&key, &value);
            imp.custom_fields_list.append(&row);
        }
    }
}

//remove all childs of list
fn clear_listbox(list: &gtk::ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

fn build_custom_field_row(key: &str, value: &str) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();

    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    hbox.set_margin_top(8);
    hbox.set_margin_bottom(8);
    hbox.set_margin_start(8);
    hbox.set_margin_end(8);

    let key_label = gtk::Entry::new();
    key_label.set_hexpand(true);
    key_label.set_width_chars(14);
    key_label.set_text(key);
    //key_label.set_xalign(0.0);
    //key_label.set_halign(gtk::Align::Start);
    //key_label.set_valign(gtk::Align::Center);

    let value_entry = gtk::Entry::new();
    value_entry.set_hexpand(true);
    value_entry.set_width_chars(24);
    value_entry.set_text(value);
    value_entry.set_placeholder_text(Some("Value"));

    let copy_button = gtk::Button::builder()
        .icon_name("edit-copy-symbolic")
        .tooltip_text("Copy value")
        .valign(gtk::Align::Center)
        .build();

    let delete_button = gtk::Button::builder()
        .icon_name("user-trash-symbolic")
        .tooltip_text("Delete field")
        .valign(gtk::Align::Center)
        .build();

    copy_button.connect_clicked({
        let value_entry = value_entry.clone();
        move |_| {
            if let Some(display) = gtk::gdk::Display::default() {
                let clipboard = display.clipboard();
                clipboard.set_text(&value_entry.text());
            }
        }
    });

    delete_button.connect_clicked({
        let row = row.clone();
        move |_| {
            if let Some(parent) = row.parent() {
                if let Ok(listbox) = parent.downcast::<gtk::ListBox>() {
                    listbox.remove(&row);
                }
            }
        }
    });

    hbox.append(&key_label);
    hbox.append(&value_entry);
    hbox.append(&copy_button);
    hbox.append(&delete_button);

    row.set_child(Some(&hbox));
    row
}
