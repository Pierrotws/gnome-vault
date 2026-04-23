mod custom_field_row;
mod imp;

use adw::prelude::*;
use gtk::glib;
use gtk::subclass::prelude::*;

use crate::pass::entry::*;
use crate::ui::generate_password_view::GeneratePasswordView;
use custom_field_row::CustomFieldRow;

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
        let this = self.clone();
        imp.add_field_button.connect_clicked(move |_| {
            let row = CustomFieldRow::new_empty(&this);
            let in_imp = this.imp();
            in_imp.custom_fields_list.append(&row);
        });
        let this = self.clone();
        imp.cancel_button.connect_clicked(move |_| {
            this.reload_from_entry();
        });
        let this = self.clone();
        imp.save_button.connect_clicked(move |_| {
            if let Err(err) = save_entry_data(&(&this).into()) {
                this.show_error(&err.to_string());
            } else {
                this.set_modified(false);
            }
        });
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
        let password_str = (&entry.password).to_str();
        imp.password_row.set_text(&password_str);
        clear_listbox(&imp.custom_fields_list);
        for (key, value) in &entry.fields {
            let row = CustomFieldRow::new(&self, &key, value);
            imp.custom_fields_list.append(&row);
        }
        self.set_modified(false);
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

    fn read_field_row(widget: &gtk::Widget) -> Option<(String, String)> {
        let row = widget.clone().downcast::<gtk::ListBoxRow>().ok()?;
        let row_child = row.child()?;
        let container = row_child.downcast::<gtk::Box>().ok()?;

        let first = container.first_child()?;
        let second = first.next_sibling()?;

        let key = first.downcast::<gtk::Entry>().ok()?.text().to_string();
        let value = second.downcast::<gtk::Entry>().ok()?.text().to_string();

        if key.is_empty() || value.is_empty() {
            return None;
        }
        Some((key, value))
    }
}

impl From<&EntryView> for EntryData {
    fn from(view: &EntryView) -> EntryData {
        let imp = view.imp();
        let password = EntryField::Password(imp.password_row.text().to_string());
        let node = imp.current_entry.borrow().as_ref().unwrap().node.clone();
        //Fields
        let mut fields = Vec::new();
        let mut child = imp.custom_fields_list.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if let Some((key, value)) = EntryView::read_field_row(&widget) {
                if !key.is_empty() || !value.is_empty() {
                    fields.push((key, EntryField::Plain(value)));
                }
            }
        }
        EntryData {
            node,
            password,
            fields,
        }
    }
}

//remove all childs of list
fn clear_listbox(list: &gtk::ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}
