mod imp;

use crate::pass_entry::{entry_path_to_gpg_file, load_entry_from_gpg_file};
use crate::pass_store::{load_password_store, PassNode, PassNodeKind};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::prelude::*;
use gtk::{gio, glib};

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

        imp.tree_search_entry
            .connect_search_changed(|entry: &gtk::SearchEntry| {
                println!("Search: {}", entry.text());
            });
    }

    pub fn setup_tree_view(&self) {
        let imp = self.imp();

        let nodes = match load_password_store() {
            Ok(nodes) => nodes,
            Err(err) => {
                eprintln!("Failed to load password store: {err}");
                Vec::new()
            }
        };

        let root_store = build_store_from_nodes(&nodes);

        let tree_model = gtk::TreeListModel::new(root_store.clone(), false, false, |obj| {
            let boxed = obj.downcast_ref::<glib::BoxedAnyObject>()?;
            let node = boxed.borrow::<PassNode>();

            if !node.is_group() {
                return None;
            }

            let child_store = build_store_from_nodes(&node.children);
            Some(child_store.upcast::<gio::ListModel>())
        });

        let selection = gtk::SingleSelection::new(Some(tree_model.clone()));
        let factory = build_tree_factory();

        imp.tree_view.set_model(Some(&selection));
        imp.tree_view.set_factory(Some(&factory));

        let win = self.clone();

        imp.tree_view
            .connect_activate(move |list_view: &gtk::ListView, position: u32| {
                let Some(model): Option<gtk::SelectionModel> = list_view.model() else {
                    return;
                };

                let Ok(selection) = model.downcast::<gtk::SingleSelection>() else {
                    return;
                };

                let Some(item): Option<glib::Object> = selection.item(position) else {
                    return;
                };

                let Ok(row) = item.downcast::<gtk::TreeListRow>() else {
                    return;
                };

                let Some(item): Option<glib::Object> = row.item() else {
                    return;
                };

                let Ok(boxed) = item.downcast::<glib::BoxedAnyObject>() else {
                    return;
                };

                let node = boxed.borrow::<PassNode>();

                match node.kind {
                    PassNodeKind::Group => {
                        row.set_expanded(!row.is_expanded());
                    }
                    PassNodeKind::Entry => {
                        let full_path = entry_path_to_gpg_file(&node.path);

                        match load_entry_from_gpg_file(&full_path) {
                            Ok(entry) => {
                                win.display_entry(&entry.password, &entry.fields);
                            }
                            Err(err) => {
                                eprintln!("{err}");
                            }
                        }
                    }
                }
            });
    }

    pub fn display_entry(&self, password: &str, fields: &[(String, String)]) {
        let imp = self.imp();

        imp.password_row.set_text(password);

        clear_listbox(&imp.custom_fields_list);

        for (key, value) in fields {
            let row = build_custom_field_row(key, value);
            imp.custom_fields_list.append(&row);
        }
    }
}

fn build_custom_field_row(key: &str, value: &str) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();

    let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    hbox.set_margin_top(8);
    hbox.set_margin_bottom(8);
    hbox.set_margin_start(8);
    hbox.set_margin_end(8);

    let key_label = gtk::Label::new(Some(key));
    key_label.set_xalign(0.0);
    key_label.set_width_chars(14);
    key_label.set_halign(gtk::Align::Start);
    key_label.set_valign(gtk::Align::Center);
    key_label.set_hexpand(true);

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

fn build_store_from_nodes(nodes: &[PassNode]) -> gio::ListStore {
    let store = gio::ListStore::new::<glib::BoxedAnyObject>();

    for node in nodes {
        store.append(&glib::BoxedAnyObject::new(node.clone()));
    }

    store
}

fn build_tree_factory() -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();

    factory.connect_setup(|_, item| {
        let item = item
            .downcast_ref::<gtk::ListItem>()
            .expect("Factory item must be a gtk::ListItem");

        let expander = gtk::TreeExpander::new();
        expander.set_focusable(false);

        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row_box.set_margin_top(6);
        row_box.set_margin_bottom(6);
        row_box.set_margin_start(6);
        row_box.set_margin_end(6);

        let icon = gtk::Image::new();
        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        label.set_hexpand(true);

        row_box.append(&icon);
        row_box.append(&label);

        expander.set_child(Some(&row_box));
        item.set_child(Some(&expander));
    });

    factory.connect_bind(|_, item| {
        let item = item
            .downcast_ref::<gtk::ListItem>()
            .expect("Factory item must be a gtk::ListItem");

        let expander = item
            .child()
            .and_then(|w| w.downcast::<gtk::TreeExpander>().ok())
            .expect("ListItem child must be a TreeExpander");

        let row = item
            .item()
            .and_then(|o| o.downcast::<gtk::TreeListRow>().ok())
            .expect("ListItem item must be a TreeListRow");

        expander.set_list_row(Some(&row));

        let boxed = row
            .item()
            .and_then(|o| o.downcast::<glib::BoxedAnyObject>().ok())
            .expect("TreeListRow item must be a BoxedAnyObject");

        let node = boxed.borrow::<PassNode>();

        let row_box = expander
            .child()
            .and_then(|w| w.downcast::<gtk::Box>().ok())
            .expect("TreeExpander child must be a Box");

        let icon = row_box
            .first_child()
            .and_then(|w| w.downcast::<gtk::Image>().ok())
            .expect("First row child must be an Image");

        let label = icon
            .next_sibling()
            .and_then(|w| w.downcast::<gtk::Label>().ok())
            .expect("Second row child must be a Label");

        label.set_label(&node.name);

        match node.kind {
            PassNodeKind::Group => {
                icon.set_icon_name(Some("folder-symbolic"));
            }
            PassNodeKind::Entry => {
                icon.set_icon_name(Some("dialog-password-symbolic"));
            }
        }

        item.set_selectable(true);
        item.set_activatable(true);
    });

    factory
}

fn clear_listbox(list: &gtk::ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}
