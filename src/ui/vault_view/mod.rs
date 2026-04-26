mod imp;

use gtk::{gdk, gio, glib, ListItemFactory, SelectionModel};
use gtk::{prelude::*, subclass::prelude::*};

use crate::pass::model::{PassNode, PassNodeKind};

glib::wrapper! {
    pub struct VaultView(ObjectSubclass<imp::VaultView>)
        @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl VaultView {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_model(&self, model: Option<&impl glib::object::IsA<SelectionModel>>) {
        self.imp().tree_view.set_model(model);
    }

    pub fn set_factory(&self, factory: Option<&impl glib::object::IsA<ListItemFactory>>) {
        self.imp().tree_view.set_factory(factory);
    }

    pub fn tree_view(&self) -> gtk::ListView {
        self.imp().tree_view.get()
    }

    pub fn search_entry(&self) -> gtk::SearchEntry {
        self.imp().search_entry.get()
    }

    pub fn setup_callbacks(&self) {
        let imp = self.imp();

        imp.search_entry.connect_search_changed(glib::clone!(
            #[weak(rename_to = obj)]
            self,
            move |_| {
                obj.emit_by_name::<()>("search-changed", &[]);
            }
        ));
    }

    /// Convenience method if parent code constructs the selection model
    /// and wants VaultView to both install and wire it.
    pub fn set_selection_model(&self, selection: &gtk::SingleSelection) {
        self.imp().tree_view.set_model(Some(selection));
        self.connect_selection(selection);
    }

    /// Convenience method if parent code wants the default tree factory.
    pub fn use_default_factory(&self) {
        let factory = build_tree_factory();
        self.imp().tree_view.set_factory(Some(&factory));
    }

    fn connect_selection(&self, selection: &gtk::SingleSelection) {
        selection.connect_selected_notify(glib::clone!(
            #[weak(rename_to = obj)]
            self,
            move |selection| {
                let position = selection.selected();
                if position == gtk::INVALID_LIST_POSITION {
                    return;
                }

                let Some(selection_item) = selection.item(position) else {
                    return;
                };
                let Ok(row) = selection_item.downcast::<gtk::TreeListRow>() else {
                    return;
                };
                let Some(row_item) = row.item() else {
                    return;
                };
                let Ok(boxed) = row_item.downcast::<glib::BoxedAnyObject>() else {
                    return;
                };

                let node = boxed.borrow::<PassNode>();
                match node.kind {
                    PassNodeKind::Group => {
                        row.set_expanded(!row.is_expanded());
                        obj.emit_by_name::<()>("group-activated", &[]);
                    }
                    PassNodeKind::Entry => {
                        obj.emit_by_name::<()>("entry-selected", &[]);
                    }
                }
            }
        ));
    }

    pub fn handle_search_changed(&self) {
        let text = self.imp().search_entry.text();
        log::debug!("VaultView search changed: {text}");
    }

    pub fn set_mode_simple(&self) {
        // Placeholder for future mode switching.
    }

    pub fn set_mode_split(&self) {
        // Placeholder for future 3-pane navigation mode.
    }

    pub fn connect_entry_selected<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_local("entry-selected", false, move |values| {
            let obj = values[0]
                .get::<VaultView>()
                .expect("entry-selected: first arg should be VaultView");
            f(&obj);
            None
        })
    }

    pub fn connect_group_activated<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_local("group-activated", false, move |values| {
            let obj = values[0]
                .get::<VaultView>()
                .expect("group-activated: first arg should be VaultView");
            f(&obj);
            None
        })
    }

    pub fn connect_create_entry_requested<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, String) + 'static,
    {
        self.connect_local("create-entry-requested", false, move |values| {
            let obj = values[0]
                .get::<VaultView>()
                .expect("create-entry-requested: first arg should be VaultView");
            let folder_path = values[1]
                .get::<String>()
                .expect("create-entry-requested: second arg should be String");
            f(&obj, folder_path);
            None
        })
    }

    pub fn connect_search_changed<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_local("search-changed", false, move |values| {
            let obj = values[0]
                .get::<VaultView>()
                .expect("search-changed: first arg should be VaultView");
            f(&obj);
            None
        })
    }

    /// Since PassNode is not a GObject, signals stay parameterless and parent
    /// code can pull the current node through this getter.
    pub fn selected_node(&self) -> Option<PassNode> {
        let model = self.imp().tree_view.model()?;
        let selection = model.downcast::<gtk::SingleSelection>().ok()?;

        let position = selection.selected();
        if position == gtk::INVALID_LIST_POSITION {
            return None;
        }

        let selection_item = selection.item(position)?;
        let row = selection_item.downcast::<gtk::TreeListRow>().ok()?;
        let row_item = row.item()?;
        let boxed = row_item.downcast::<glib::BoxedAnyObject>().ok()?;
        let node = boxed.borrow::<PassNode>();

        Some(node.clone())
    }
}

impl Default for VaultView {
    fn default() -> Self {
        Self::new()
    }
}

pub fn build_tree_factory() -> gtk::SignalListItemFactory {
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
        install_folder_context_menu(&row_box);

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

fn install_folder_context_menu(row_box: &gtk::Box) {
    let gesture = gtk::GestureClick::new();
    gesture.set_button(gdk::BUTTON_SECONDARY);
    gesture.connect_pressed(move |gesture, _, x, y| {
        let Some(widget) = gesture.widget() else {
            return;
        };
        let Some(expander) = widget
            .ancestor(gtk::TreeExpander::static_type())
            .and_then(|w| w.downcast::<gtk::TreeExpander>().ok())
        else {
            return;
        };
        let Some(row) = expander.list_row() else {
            return;
        };
        let Some(row_item) = row.item() else {
            return;
        };
        let Ok(boxed) = row_item.downcast::<glib::BoxedAnyObject>() else {
            return;
        };
        let node = boxed.borrow::<PassNode>();
        if !node.is_group() {
            return;
        }

        let Some(vault_view) = widget
            .ancestor(VaultView::static_type())
            .and_then(|w| w.downcast::<VaultView>().ok())
        else {
            return;
        };
        let folder_path = node.path.to_string_lossy().to_string();
        let popover = gtk::Popover::new();
        popover.set_parent(&widget);
        popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));

        let button = gtk::Button::with_label("Create Entry");
        button.set_margin_top(6);
        button.set_margin_bottom(6);
        button.set_margin_start(6);
        button.set_margin_end(6);

        button.connect_clicked(glib::clone!(
            #[weak]
            popover,
            #[weak]
            vault_view,
            move |_| {
                popover.popdown();
                vault_view.emit_by_name::<()>("create-entry-requested", &[&folder_path]);
            }
        ));

        popover.set_child(Some(&button));
        popover.popup();
    });
    row_box.add_controller(gesture);
}

pub fn build_store_from_nodes(nodes: &[PassNode]) -> gio::ListStore {
    let store = gio::ListStore::new::<glib::BoxedAnyObject>();

    for node in nodes {
        store.append(&glib::BoxedAnyObject::new(node.clone()));
    }

    store
}

pub fn build_selection_from_nodes(nodes: &[PassNode]) -> gtk::SingleSelection {
    let root_store = build_store_from_nodes(nodes);

    let tree_model = gtk::TreeListModel::new(root_store.clone(), false, false, |obj| {
        let boxed = obj.downcast_ref::<glib::BoxedAnyObject>()?;
        let node = boxed.borrow::<PassNode>();

        if !node.is_group() {
            return None;
        }

        let child_store = build_store_from_nodes(&node.children);
        Some(child_store.upcast::<gio::ListModel>())
    });

    let selection = gtk::SingleSelection::new(Some(tree_model));
    selection.set_can_unselect(true);
    selection.set_autoselect(false);
    selection.set_selected(gtk::INVALID_LIST_POSITION);
    selection
}
