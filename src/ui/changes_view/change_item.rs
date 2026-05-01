//! `glib::Object` wrapper around [`GitChange`] so changes can live in a
//! `gio::ListStore` and feed `gtk::ListView`'s recycling factory.

use gtk::glib;
use gtk::glib::subclass::prelude::*;

use crate::helpers::git::GitChange;

mod imp {
    use std::cell::RefCell;

    use gtk::glib;
    use gtk::glib::subclass::prelude::*;

    use crate::helpers::git::GitChange;

    #[derive(Default)]
    pub struct ChangeItem {
        pub change: RefCell<Option<GitChange>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ChangeItem {
        const NAME: &'static str = "GnomeVaultChangeItem";
        type Type = super::ChangeItem;
    }

    impl ObjectImpl for ChangeItem {}
}

glib::wrapper! {
    pub struct ChangeItem(ObjectSubclass<imp::ChangeItem>);
}

impl ChangeItem {
    pub fn new(change: GitChange) -> Self {
        let obj: Self = glib::Object::new();
        *obj.imp().change.borrow_mut() = Some(change);
        obj
    }

    /// Returns a clone of the wrapped `GitChange`. Cheaper-than-clone
    /// access via a borrowed `Ref` is intentionally not exposed because
    /// the item factory's bind callback always wants owned strings to
    /// hand to `set_label` / `set_markup`.
    pub fn change(&self) -> GitChange {
        self.imp()
            .change
            .borrow()
            .clone()
            .expect("ChangeItem must be initialized via ChangeItem::new")
    }
}
