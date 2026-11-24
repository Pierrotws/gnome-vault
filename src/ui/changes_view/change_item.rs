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
        const NAME: &'static str = "PasselChangeItem";
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

    /// Returns a clone of the wrapped `GitChange`. Prefer `with_change`
    /// in hot paths (e.g. the bind callback) — it avoids cloning the
    /// inner `GitChange` for callers that only need to read fields.
    pub fn change(&self) -> GitChange {
        self.with_change(GitChange::clone)
    }

    /// Borrows the wrapped `GitChange` and runs `f` with it. Saves a
    /// per-recycle `GitChange` clone in the bind callback, where the
    /// factory just needs to read four fields (summary, author,
    /// is_pushed, short_id) into the row widgets.
    pub fn with_change<R>(&self, f: impl FnOnce(&GitChange) -> R) -> R {
        let change = self.imp().change.borrow();
        f(change
            .as_ref()
            .expect("ChangeItem must be initialized via ChangeItem::new"))
    }
}
