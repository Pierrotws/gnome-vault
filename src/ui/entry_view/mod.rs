mod imp;

use adw::prelude::*;
use gtk::glib;
use gtk::subclass::prelude::*;

use crate::pass::entry::EntryData;
use crate::ui::generate_password_view::GeneratePasswordView;

glib::wrapper! {
    pub struct EntryView(ObjectSubclass<imp::EntryView>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl EntryView {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_modified(&self, val: bool) {
        let imp = self.imp();
        imp.modified.set(val);
        imp.cancel_button.set_visible(val);
        imp.save_button.set_visible(val);
    }

    pub fn is_modified(&self) -> bool {
        self.imp().modified.get()
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

        let this = self.clone();
        imp.generate_password_button.connect_clicked(move |_| {
            this.show_generate_password_dialog();
        });

        imp.add_field_button.connect_clicked(|_| {
            println!("Add field clicked");
        });

        let this = self.clone();
        imp.cancel_button.connect_clicked(move |_| {
            this.reload_from_entry();
        });
    }

    pub fn display_entry(&self, entry: &EntryData) {
        let imp = self.imp();

        imp.content_stack.set_visible_child_name("content");
        *imp.current_entry.borrow_mut() = Some(entry.clone());

        self.reload_from_entry();
    }

    pub fn reload_from_entry(&self) {
        let imp = self.imp();
        let entry = imp.current_entry.borrow();
        let Some(entry) = entry.as_ref() else {
            return;
        };

        imp.title_label.set_text(&entry.node.name);
        imp.password_row.set_text(&entry.password);

        clear_listbox(&imp.custom_fields_list);
        for (key, value) in &entry.fields {
            let row = self.build_custom_field_row(&key, &value);
            imp.custom_fields_list.append(&row);
        }
        self.set_modified(false);

        // let model = imp.custom_fields_list.observe_children();
        // let this = self.clone();
        // model.connect_items_changed(move |_, _, _, _| {
        //     println!("Custom fields changed");
        //     this.set_modified();
        // });
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
                this.set_modified(true);
                let imp = this.imp();
                imp.password_row.set_text(&password);
                dialog.close();
            });
        }
        dialog.present();
    }

    fn build_custom_field_row(&self, key: &str, value: &str) -> gtk::ListBoxRow {
        let row = gtk::ListBoxRow::new();

        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        hbox.set_margin_top(8);
        hbox.set_margin_bottom(8);
        hbox.set_margin_start(8);
        hbox.set_margin_end(8);

        let key_entry = gtk::Entry::new();
        key_entry.set_hexpand(true);
        key_entry.set_width_chars(14);
        key_entry.set_text(key);
        //key_entry.set_xalign(0.0);
        //key_entry.set_valign(gtk::Align::Center);
        //key_entry.set_halign(gtk::Align::Start);

        let this = self.clone();
        key_entry.connect_changed(move |_| {
            this.set_modified(true);
        });

        let value_entry = gtk::Entry::new();
        value_entry.set_hexpand(true);
        value_entry.set_width_chars(24);
        value_entry.set_text(value);
        value_entry.set_placeholder_text(Some("Value"));

        let this = self.clone();
        value_entry.connect_changed(move |_| {
            this.set_modified(true);
        });

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

        let this = self.clone();
        delete_button.connect_clicked({
            let row = row.clone();
            move |_| {
                if let Some(parent) = row.parent() {
                    if let Ok(listbox) = parent.downcast::<gtk::ListBox>() {
                        listbox.remove(&row);
                        this.set_modified(true);
                    }
                }
            }
        });

        hbox.append(&key_entry);
        hbox.append(&value_entry);
        hbox.append(&copy_button);
        hbox.append(&delete_button);

        row.set_child(Some(&hbox));
        row
    }
}

//remove all childs of list
fn clear_listbox(list: &gtk::ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}
