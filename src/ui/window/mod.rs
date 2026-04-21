mod imp;

use adw::subclass::prelude::*;
use gtk::{gio, glib};

use crate::{pass::entry::load_entry_from_node, ui::entry_view::EntryView};

glib::wrapper! {
    pub struct MainWindow(ObjectSubclass<imp::MainWindow>)
        @extends adw::ApplicationWindow, gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk::Accessible, gtk::Buildable,
                    gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl MainWindow {
    pub fn new(app: &adw::Application) -> Self {
        glib::Object::builder().property("application", app).build()
    }

    pub fn setup_callbacks(&self) {
        let imp = self.imp();
        let entry_view = imp.entry_view.clone();
        imp.vault_view.connect_entry_selected(move |nav| {
            eprintln!("Launch callback connected to signal entry-selected");
            let Some(node) = nav.selected_node() else {
                return;
            };
            match load_entry_from_node(&node) {
                Ok(entry_data) => entry_view.display_entry(&entry_data),
                Err(err) => eprintln!("{err}"),
            }
        });
    }

    pub fn get_entry_view(&self) -> EntryView {
        self.imp().entry_view.clone()
    }
}
