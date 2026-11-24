//! `glib::Object` wrapper around a [`GroupEntry`] so the entries pane
//! can recycle row widgets through `gtk::ListView` instead of building
//! one `adw::ActionRow` per visible entry.

use gtk::glib;
use gtk::glib::subclass::prelude::*;

use super::GroupEntry;
use crate::pass::model::PassNode;

mod imp {
    use std::cell::RefCell;

    use gtk::glib;
    use gtk::glib::subclass::prelude::*;

    use super::super::GroupEntry;

    #[derive(Default)]
    pub struct GroupItem {
        pub entry: RefCell<Option<GroupEntry>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GroupItem {
        const NAME: &'static str = "PasselGroupItem";
        type Type = super::GroupItem;
    }

    impl ObjectImpl for GroupItem {}
}

glib::wrapper! {
    pub struct GroupItem(ObjectSubclass<imp::GroupItem>);
}

impl GroupItem {
    pub fn new(entry: GroupEntry) -> Self {
        let obj: Self = glib::Object::new();
        *obj.imp().entry.borrow_mut() = Some(entry);
        obj
    }

    pub fn node(&self) -> PassNode {
        self.with_entry(|entry| entry.node.clone())
    }

    pub fn name(&self) -> String {
        self.with_entry(|entry| entry.node.name.clone())
    }

    pub fn subtitle(&self) -> Option<String> {
        self.with_entry(|entry| entry.subtitle.clone())
    }

    fn with_entry<R>(&self, f: impl FnOnce(&GroupEntry) -> R) -> R {
        let borrowed = self.imp().entry.borrow();
        f(borrowed
            .as_ref()
            .expect("GroupItem must be initialized via GroupItem::new"))
    }
}

