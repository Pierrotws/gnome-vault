mod imp;

use adw::subclass::prelude::*;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;

glib::wrapper! {
    pub struct MainWindow(ObjectSubclass<imp::MainWindow>)
        @extends adw::ApplicationWindow, gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk::Accessible, gtk::Buildable,
                    gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl MainWindow {
    pub fn new(app: &adw::Application) -> Self {
        glib::Object::builder()
            .property("application", app)
            .build()
    }

    pub fn setup_callbacks(&self) {
        let imp = self.imp();

        imp.add_field_button.connect_clicked(|_| {
            println!("Add field clicked");
        });

        imp.tree_search_entry.connect_search_changed(|entry: &gtk::SearchEntry| {
            println!("Search: {}", entry.text());
        });
    }
}
