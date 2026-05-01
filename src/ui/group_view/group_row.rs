//! Subclassed `gtk::Box` row widget for [`GroupView`]'s list factory.
//!
//! Holds a typed per-row cell pointing at the currently-bound
//! [`super::GroupItem`] so the right-click gesture can resolve to the
//! entry under the cursor while the row is bound. The same shape as
//! [`crate::ui::changes_view::ChangeRow`] — a typed field instead of
//! `glib::Object::set_data` keeps the lookup safe.

use gtk::glib;
use gtk::glib::subclass::prelude::*;

use super::GroupItem;

mod imp {
    use std::cell::RefCell;

    use gtk::glib;
    use gtk::glib::subclass::prelude::*;
    use gtk::subclass::prelude::*;

    use super::GroupItem;

    #[derive(Default)]
    pub struct GroupRow {
        pub current: RefCell<Option<glib::WeakRef<GroupItem>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GroupRow {
        const NAME: &'static str = "GnomeVaultGroupRow";
        type Type = super::GroupRow;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for GroupRow {}
    impl WidgetImpl for GroupRow {}
    impl BoxImpl for GroupRow {}
}

glib::wrapper! {
    pub struct GroupRow(ObjectSubclass<imp::GroupRow>)
        @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl GroupRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_current(&self, item: Option<glib::WeakRef<GroupItem>>) {
        *self.imp().current.borrow_mut() = item;
    }

    pub fn current(&self) -> Option<GroupItem> {
        self.imp()
            .current
            .borrow()
            .as_ref()
            .and_then(|weak| weak.upgrade())
    }
}

impl Default for GroupRow {
    fn default() -> Self {
        Self::new()
    }
}
