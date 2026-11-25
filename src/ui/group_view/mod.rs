mod group_item;
mod group_row;
mod imp;

use adw::{prelude::*, subclass::prelude::*};
use gtk::{gdk, gio, glib};

pub use group_item::GroupItem;
use group_row::GroupRow;

use crate::i18n::{gettext, ngettext_f};
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
        let view_weak = self.downgrade();
        factory.connect_setup(move |_, item| {
            let item = item
                .downcast_ref::<gtk::ListItem>()
                .expect("Factory item is a gtk::ListItem");

            let row = GroupRow::new();
            row.set_orientation(gtk::Orientation::Horizontal);
            row.set_spacing(12);
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

            // Per-row right-click menu: GroupRow holds a typed cell
            // refreshed by `connect_bind` to the current GroupItem so a
            // single GestureClick controller serves every recycled use
            // of this row template.
            let gesture = gtk::GestureClick::new();
            gesture.set_button(gdk::BUTTON_SECONDARY);
            let view_for_gesture = view_weak.clone();
            let row_for_gesture = row.downgrade();
            gesture.connect_pressed(move |_, _, x, y| {
                let Some(view) = view_for_gesture.upgrade() else {
                    return;
                };
                let Some(row) = row_for_gesture.upgrade() else {
                    return;
                };
                let Some(group_item) = row.current() else {
                    return;
                };
                let node = group_item.node();
                view.show_row_popover(row.upcast_ref::<gtk::Widget>(), x, y, &node);
            });
            row.add_controller(gesture);

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
                .and_then(|w| w.downcast::<GroupRow>().ok())
                .expect("ListItem child is a GroupRow");

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
            // Set the text but never the visibility: subtitles are
            // decrypted in the background and arrive one row at a time,
            // so hiding the empty label made every row start one line
            // short and grow as its preview landed, shuffling the list
            // under the user's cursor. An empty GtkLabel still measures
            // one line, so leaving it in place gives every row its final
            // height the moment the list is built.
            subtitle.set_label(&group_item.subtitle().unwrap_or_default());

            // Refresh the per-row pointer so the right-click gesture
            // resolves to *this* GroupItem while the row is bound.
            row.set_current(Some(glib::object::ObjectExt::downgrade(&group_item)));
        });

        factory.connect_unbind(|_, item| {
            let item = item
                .downcast_ref::<gtk::ListItem>()
                .expect("Factory item is a gtk::ListItem");
            let row = item
                .child()
                .and_then(|w| w.downcast::<GroupRow>().ok())
                .expect("ListItem child is a GroupRow");
            // Drop the weak ref so a stale gesture firing during
            // recycling doesn't resolve to the wrong entry.
            row.set_current(None);
        });

        imp.entries_list.set_factory(Some(&factory));
    }

    /// Builds and presents the per-row right-click popover. Pulled out
    /// of the gesture closure so the heavy widget construction lives
    /// in `&self` rather than a per-row factory captured closure.
    fn show_row_popover(&self, widget: &gtk::Widget, x: f64, y: f64, node: &PassNode) {
        let popover = gtk::Popover::new();
        popover.set_parent(widget);
        popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        // popdown() does not unparent; without this every right-click
        // would accumulate a fresh popover under the row widget.
        popover.connect_closed(|popover| popover.unparent());

        let entry_path = node.path.to_string_lossy().to_string();
        let entry_name = node.name.clone();
        let button = gtk::Button::with_label(&gettext("Delete Entry"));
        button.set_margin_top(6);
        button.set_margin_bottom(6);
        button.set_margin_start(6);
        button.set_margin_end(6);
        button.add_css_class("flat");
        button.add_css_class("destructive-action");
        button.connect_clicked(glib::clone!(
            #[weak]
            popover,
            #[weak(rename_to = obj)]
            self,
            move |_| {
                popover.popdown();
                obj.emit_by_name::<()>("delete-entry-requested", &[&entry_path, &entry_name]);
            }
        ));
        popover.set_child(Some(&button));
        popover.popup();
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
        // Replace the GroupItem in place with a fresh one carrying the
        // new subtitle. We can't just mutate the existing GroupItem and
        // call `items_changed(index, 1, 1)`: GtkListView optimizes that
        // case by detecting the same GObject at the same position and
        // skips the unbind/bind cycle, so the row's label never picks
        // up the new subtitle. Splicing in a new GObject forces the
        // rebind.
        let entry = GroupEntry {
            node: item.node(),
            subtitle: subtitle.map(|s| s.to_string()),
        };
        model.splice(index as u32, 1, &[GroupItem::new(entry)]);
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

    pub fn connect_delete_entry_requested<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, String, String) + 'static,
    {
        self.connect_local("delete-entry-requested", false, move |values| {
            let obj = values[0]
                .get::<GroupView>()
                .expect("delete-entry-requested: first arg should be GroupView");
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
}

impl Default for GroupView {
    fn default() -> Self {
        Self::new()
    }
}

fn format_entry_count(count: usize) -> String {
    match count {
        0 => gettext("No entries"),
        count => ngettext_f(
            "{n} entry",
            "{n} entries",
            count as u64,
            &[("n", &count.to_string())],
        ),
    }
}
