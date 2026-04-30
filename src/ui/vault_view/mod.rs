mod imp;

use std::path::{Path, PathBuf};

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
        let factory = build_tree_factory(TreeViewSize::Medium);
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

    pub fn connect_delete_entry_requested<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, String, String) + 'static,
    {
        self.connect_local("delete-entry-requested", false, move |values| {
            let obj = values[0]
                .get::<VaultView>()
                .expect("delete-entry-requested: first arg should be VaultView");
            let entry_path = values[1]
                .get::<String>()
                .expect("delete-entry-requested: second arg should be String");
            let entry_name = values[2]
                .get::<String>()
                .expect("delete-entry-requested: third arg should be String");
            f(&obj, entry_path, entry_name);
            None
        })
    }

    pub fn connect_delete_folder_requested<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, String, String) + 'static,
    {
        self.connect_local("delete-folder-requested", false, move |values| {
            let obj = values[0]
                .get::<VaultView>()
                .expect("delete-folder-requested: first arg should be VaultView");
            let folder_path = values[1]
                .get::<String>()
                .expect("delete-folder-requested: second arg should be String");
            let folder_name = values[2]
                .get::<String>()
                .expect("delete-folder-requested: third arg should be String");
            f(&obj, folder_path, folder_name);
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

    /// Selects the row matching `target_path`, expanding ancestors as needed.
    ///
    /// Walks the tree by ancestor path from the root toward the target;
    /// each ancestor's row is forced expanded so its children become visible
    /// in the flat `TreeListModel` for the next iteration. Returns true if
    /// the target was found and selected.
    pub fn select_node_by_path(&self, target_path: &Path) -> bool {
        let Some(model) = self.imp().tree_view.model() else {
            return false;
        };
        let Ok(selection) = model.downcast::<gtk::SingleSelection>() else {
            return false;
        };
        let Some(tree_model) = selection
            .model()
            .and_then(|m| m.downcast::<gtk::TreeListModel>().ok())
        else {
            return false;
        };

        let mut ancestors: Vec<PathBuf> = Vec::new();
        let mut current = target_path.parent();
        while let Some(parent) = current {
            ancestors.push(parent.to_path_buf());
            current = parent.parent();
        }
        ancestors.reverse();

        for ancestor in &ancestors {
            if let Some(row) = find_tree_row_by_path(&tree_model, ancestor) {
                if !row.is_expanded() {
                    row.set_expanded(true);
                }
            }
        }

        let Some(position) = find_tree_position_by_path(&tree_model, target_path) else {
            return false;
        };
        selection.set_selected(position);
        true
    }
}

fn find_tree_row_by_path(model: &gtk::TreeListModel, target: &Path) -> Option<gtk::TreeListRow> {
    for index in 0..model.n_items() {
        let Some(row) = model.row(index) else {
            continue;
        };
        if tree_row_path_matches(&row, target) {
            return Some(row);
        }
    }
    None
}

fn find_tree_position_by_path(model: &gtk::TreeListModel, target: &Path) -> Option<u32> {
    for index in 0..model.n_items() {
        let Some(row) = model.row(index) else {
            continue;
        };
        if tree_row_path_matches(&row, target) {
            return Some(index);
        }
    }
    None
}

fn tree_row_path_matches(row: &gtk::TreeListRow, target: &Path) -> bool {
    let Some(item) = row.item() else {
        return false;
    };
    let Ok(boxed) = item.downcast::<glib::BoxedAnyObject>() else {
        return false;
    };
    let node = boxed.borrow::<PassNode>();
    node.path == target
}

impl Default for VaultView {
    fn default() -> Self {
        Self::new()
    }
}

/// Visual density of the vault tree view rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeViewSize {
    Small,
    Medium,
    Large,
}

impl TreeViewSize {
    /// Maps a GSettings enum nick (`small` / `medium` / `large`) to the
    /// matching variant; unknown values fall back to `Medium`.
    pub fn from_setting(value: &str) -> Self {
        match value {
            "small" => Self::Small,
            "large" => Self::Large,
            _ => Self::Medium,
        }
    }

    /// Returns `(vertical_margin, horizontal_margin, icon_label_spacing)`
    /// in CSS pixels.
    fn padding(self) -> (i32, i32, i32) {
        match self {
            Self::Small => (6, 6, 8),
            Self::Medium => (10, 8, 12),
            Self::Large => (14, 10, 16),
        }
    }

    /// Pango font scale applied to the row label. 1.0 leaves font size
    /// at the GTK default; Large bumps it to `pango::SCALE_LARGE` (1.2x).
    fn font_scale(self) -> f64 {
        match self {
            Self::Large => gtk::pango::SCALE_LARGE,
            _ => 1.0,
        }
    }
}

pub fn build_tree_factory(size: TreeViewSize) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();

    let (vmargin, hmargin, spacing) = size.padding();
    let scale = size.font_scale();

    factory.connect_setup(move |_, item| {
        let item = item
            .downcast_ref::<gtk::ListItem>()
            .expect("Factory item must be a gtk::ListItem");

        let expander = gtk::TreeExpander::new();
        expander.set_focusable(false);

        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, spacing);
        row_box.set_margin_top(vmargin);
        row_box.set_margin_bottom(vmargin);
        row_box.set_margin_start(hmargin);
        row_box.set_margin_end(hmargin);
        install_row_context_menu(&row_box);

        let icon = gtk::Image::new();
        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        label.set_hexpand(true);
        if (scale - 1.0).abs() > f64::EPSILON {
            let attrs = gtk::pango::AttrList::new();
            attrs.insert(gtk::pango::AttrFloat::new_scale(scale));
            label.set_attributes(Some(&attrs));
        }

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

fn install_row_context_menu(row_box: &gtk::Box) {
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

        let Some(vault_view) = widget
            .ancestor(VaultView::static_type())
            .and_then(|w| w.downcast::<VaultView>().ok())
        else {
            return;
        };
        let popover = gtk::Popover::new();
        popover.set_parent(&widget);
        popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));

        if node.is_group() {
            let menu = gtk::Box::new(gtk::Orientation::Vertical, 0);
            let folder_path = node.path.to_string_lossy().to_string();
            let folder_name = node.name.clone();

            let create = popover_button("Create Entry", false);
            create.connect_clicked(glib::clone!(
                #[weak]
                popover,
                #[weak]
                vault_view,
                #[strong]
                folder_path,
                move |_| {
                    popover.popdown();
                    vault_view.emit_by_name::<()>("create-entry-requested", &[&folder_path]);
                }
            ));
            menu.append(&create);

            menu.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

            let delete = popover_button("Delete Folder", true);
            delete.connect_clicked(glib::clone!(
                #[weak]
                popover,
                #[weak]
                vault_view,
                move |_| {
                    popover.popdown();
                    vault_view.emit_by_name::<()>(
                        "delete-folder-requested",
                        &[&folder_path, &folder_name],
                    );
                }
            ));
            menu.append(&delete);

            popover.set_child(Some(&menu));
        } else {
            let entry_path = node.path.to_string_lossy().to_string();
            let entry_name = node.name.clone();
            let button = popover_button("Delete Entry", true);
            button.connect_clicked(glib::clone!(
                #[weak]
                popover,
                #[weak]
                vault_view,
                move |_| {
                    popover.popdown();
                    vault_view
                        .emit_by_name::<()>("delete-entry-requested", &[&entry_path, &entry_name]);
                }
            ));
            popover.set_child(Some(&button));
        }

        popover.popup();
    });
    row_box.add_controller(gesture);
}

fn popover_button(label: &str, destructive: bool) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.set_margin_top(6);
    button.set_margin_bottom(6);
    button.set_margin_start(6);
    button.set_margin_end(6);
    button.add_css_class("flat");
    if destructive {
        button.add_css_class("destructive-action");
    }
    button
}

pub fn build_store_from_nodes(nodes: &[PassNode]) -> gio::ListStore {
    let store = gio::ListStore::new::<glib::BoxedAnyObject>();

    for node in nodes {
        store.append(&glib::BoxedAnyObject::new(node.clone()));
    }

    store
}

pub fn build_selection_from_nodes_with_autoexpand(
    nodes: &[PassNode],
    autoexpand: bool,
) -> gtk::SingleSelection {
    let root_store = build_store_from_nodes(nodes);

    let tree_model = gtk::TreeListModel::new(root_store.clone(), false, autoexpand, |obj| {
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

pub fn build_group_selection_with_root(
    nodes: &[PassNode],
    autoexpand: bool,
) -> gtk::SingleSelection {
    let root = PassNode {
        name: "Vault".to_string(),
        path: PathBuf::new(),
        kind: PassNodeKind::Group,
        children: nodes.to_vec(),
    };
    let root_store = build_store_from_nodes(&[root]);

    let tree_model = gtk::TreeListModel::new(root_store.clone(), false, autoexpand, |obj| {
        let boxed = obj.downcast_ref::<glib::BoxedAnyObject>()?;
        let node = boxed.borrow::<PassNode>();

        if !node.is_group() {
            return None;
        }

        let child_groups = node
            .children
            .iter()
            .filter(|child| child.is_group())
            .cloned()
            .collect::<Vec<_>>();
        let child_store = build_store_from_nodes(&child_groups);
        Some(child_store.upcast::<gio::ListModel>())
    });

    let selection = gtk::SingleSelection::new(Some(tree_model));
    selection.set_can_unselect(true);
    selection.set_autoselect(false);
    selection.set_selected(gtk::INVALID_LIST_POSITION);
    selection
}
