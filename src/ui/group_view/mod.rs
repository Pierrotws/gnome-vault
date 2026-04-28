mod imp;

use adw::{prelude::*, subclass::prelude::*};
use gtk::glib;

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
        self.imp().entries_list.connect_row_activated(glib::clone!(
            #[weak(rename_to = obj)]
            self,
            move |_, row| {
                obj.emit_by_name::<()>("entry-activated", &[&(row.index() as u32)]);
            }
        ));
    }

    pub fn set_group(&self, group: &PassNode, entries: &[GroupEntry]) {
        let imp = self.imp();

        imp.group_title_label.set_label(&group.name);
        imp.group_count_label
            .set_label(&format_entry_count(entries.len()));

        while let Some(child) = imp.entries_list.first_child() {
            imp.entries_list.remove(&child);
        }

        imp.entries
            .replace(entries.iter().map(|entry| entry.node.clone()).collect());

        for entry in entries {
            imp.entries_list.append(&build_entry_row(entry));
        }
    }

    pub fn update_entry_subtitle(&self, index: usize, node: &PassNode, subtitle: Option<&str>) {
        let imp = self.imp();
        let entries = imp.entries.borrow();
        let Some(current) = entries.get(index) else {
            return;
        };
        if current.path != node.path {
            return;
        }
        drop(entries);

        let Some(row) = imp.entries_list.row_at_index(index as i32) else {
            return;
        };
        let Ok(row) = row.downcast::<adw::ActionRow>() else {
            return;
        };

        row.set_subtitle(subtitle.unwrap_or(""));
    }

    pub fn connect_entry_activated<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, PassNode) + 'static,
    {
        self.connect_local("entry-activated", false, move |values| {
            let obj = values[0]
                .get::<GroupView>()
                .expect("entry-activated: first arg should be GroupView");
            let index = values[1]
                .get::<u32>()
                .expect("entry-activated: second arg should be u32");

            if let Some(node) = obj.imp().entries.borrow().get(index as usize).cloned() {
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

fn build_entry_row(entry: &GroupEntry) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(&entry.node.name)
        .activatable(true)
        .build();

    if let Some(subtitle) = &entry.subtitle {
        row.set_subtitle(subtitle);
    }

    let icon = gtk::Image::from_icon_name("dialog-password-symbolic");
    row.add_prefix(&icon);
    row
}

fn format_entry_count(count: usize) -> String {
    match count {
        0 => "No entries".to_string(),
        1 => "1 entry".to_string(),
        count => format!("{count} entries"),
    }
}
