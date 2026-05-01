mod group_item;
mod imp;

use adw::{prelude::*, subclass::prelude::*};
use gtk::{gio, glib};

pub use group_item::GroupItem;

use crate::pass::model::PassNode;

glib::wrapper! {
    pub struct GroupView(ObjectSubclass<imp::GroupView>)
        @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

#[derive(Clone)]
pub struct GroupEntry {
    pub node: PassNode,
    pub subtitle: Option<String>,
}

impl GroupView {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn setup_callbacks(&self) {
        self.install_list_model();

        self.imp().entries_list.connect_activate(glib::clone!(
            #[weak(rename_to = obj)]
            self,
            move |_, position| {
                let imp = obj.imp();
                let model = imp.model.get().expect("model installed in setup_callbacks");
                let Some(item) = model
                    .item(position)
                    .and_then(|o| o.downcast::<GroupItem>().ok())
                else {
                    return;
                };
                // Emit the node's path string so the listener resolves
                // the match by identity, not by ListView ordinal — any
                // concurrent mutation between emit and handler
                // (set_group, sort, ...) would otherwise re-bind the
                // signal to a different node.
                let path = item.node().path.to_string_lossy().to_string();
                obj.emit_by_name::<()>("entry-activated", &[&path]);
            }
        ));
    }

    /// Installs a `gio::ListStore<GroupItem>` and a recycling factory
    /// behind the ListView. Each row is built once per visible slot
    /// (icon + title + subtitle column); bind populates it from the
    /// current [`GroupItem`].
    fn install_list_model(&self) {
        let imp = self.imp();
        let model = gio::ListStore::new::<GroupItem>();
        let _ = imp.model.set(model.clone());
        let selection = gtk::NoSelection::new(Some(model));
        imp.entries_list.set_model(Some(&selection));

        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            let item = item
                .downcast_ref::<gtk::ListItem>()
                .expect("Factory item is a gtk::ListItem");

            let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            row.set_margin_top(8);
            row.set_margin_bottom(8);
            row.set_margin_start(12);
            row.set_margin_end(12);

            let icon = gtk::Image::from_icon_name("dialog-password-symbolic");
            icon.set_valign(gtk::Align::Center);

            let labels = gtk::Box::new(gtk::Orientation::Vertical, 2);
            labels.set_hexpand(true);
            labels.set_valign(gtk::Align::Center);

            let title = gtk::Label::new(None);
            title.set_xalign(0.0);
            title.set_ellipsize(gtk::pango::EllipsizeMode::End);

            let subtitle = gtk::Label::new(None);
            subtitle.set_xalign(0.0);
            subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
            subtitle.add_css_class("dim-label");
            subtitle.add_css_class("caption");

            labels.append(&title);
            labels.append(&subtitle);
            row.append(&icon);
            row.append(&labels);
            item.set_child(Some(&row));
        });

        factory.connect_bind(|_, item| {
            let item = item
                .downcast_ref::<gtk::ListItem>()
                .expect("Factory item is a gtk::ListItem");
            let group_item = item
                .item()
                .and_then(|o| o.downcast::<GroupItem>().ok())
                .expect("Bound item is a GroupItem");
            let row = item
                .child()
                .and_then(|w| w.downcast::<gtk::Box>().ok())
                .expect("ListItem child is a gtk::Box");

            let icon = row
                .first_child()
                .and_then(|w| w.downcast::<gtk::Image>().ok())
                .expect("First child is the icon");
            let labels = icon
                .next_sibling()
                .and_then(|w| w.downcast::<gtk::Box>().ok())
                .expect("Second child is the labels box");
            let title = labels
                .first_child()
                .and_then(|w| w.downcast::<gtk::Label>().ok())
                .expect("First labels child is the title");
            let subtitle = title
                .next_sibling()
                .and_then(|w| w.downcast::<gtk::Label>().ok())
                .expect("Second labels child is the subtitle");

            title.set_label(&group_item.name());
            match group_item.subtitle() {
                Some(text) => {
                    subtitle.set_label(&text);
                    subtitle.set_visible(true);
                }
                None => {
                    subtitle.set_label("");
                    subtitle.set_visible(false);
                }
            }
        });

        imp.entries_list.set_factory(Some(&factory));
    }

    pub fn set_group(&self, group: &PassNode, entries: &[GroupEntry]) {
        let imp = self.imp();

        imp.group_title_label.set_label(&group.name);
        imp.group_count_label
            .set_label(&format_entry_count(entries.len()));

        let model = imp.model.get().expect("model installed in setup_callbacks");
        model.remove_all();
        let items: Vec<GroupItem> = entries.iter().cloned().map(GroupItem::new).collect();
        model.extend_from_slice(&items);
    }

    pub fn update_entry_subtitle(&self, index: usize, node: &PassNode, subtitle: Option<&str>) {
        let imp = self.imp();
        let model = imp.model.get().expect("model installed in setup_callbacks");
        let Some(item) = model
            .item(index as u32)
            .and_then(|o| o.downcast::<GroupItem>().ok())
        else {
            return;
        };
        if item.node().path != node.path {
            return;
        }
        item.set_subtitle(subtitle.map(|s| s.to_string()));
        // Tell the model the row at `index` has changed so the factory
        // rebinds it. `items_changed(pos, removed=1, added=1)` is the
        // canonical "value updated in place" notification.
        model.items_changed(index as u32, 1, 1);
    }

    pub fn connect_entry_activated<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, PassNode) + 'static,
    {
        self.connect_local("entry-activated", false, move |values| {
            let obj = values[0]
                .get::<GroupView>()
                .expect("entry-activated: first arg should be GroupView");
            let path = values[1]
                .get::<String>()
                .expect("entry-activated: second arg should be String");

            let model = obj
                .imp()
                .model
                .get()
                .expect("model installed in setup_callbacks");
            let mut hit: Option<PassNode> = None;
            for i in 0..model.n_items() {
                if let Some(item) = model.item(i).and_then(|o| o.downcast::<GroupItem>().ok()) {
                    if item.node().path.to_string_lossy() == path {
                        hit = Some(item.node());
                        break;
                    }
                }
            }
            if let Some(node) = hit {
                f(&obj, node);
            }
            None
        })
    }
}

impl Default for GroupView {
    fn default() -> Self {
        Self::new()
    }
}

fn format_entry_count(count: usize) -> String {
    match count {
        0 => "No entries".to_string(),
        1 => "1 entry".to_string(),
        count => format!("{count} entries"),
    }
}
