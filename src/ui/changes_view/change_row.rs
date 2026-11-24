//! Subclassed `gtk::Box` row widget for [`ChangesView`]'s list factory.
//!
//! Holds a typed per-row cell pointing at the currently-bound
//! [`super::ChangeItem`] so the right-click gesture can resolve to *this*
//! commit while the row is bound. A subclass with a typed field replaces
//! the previous `glib::Object::set_data` / `data::<T>()` shape, which
//! relied on a string key and a blind `unsafe` cast to `T`.

use gtk::glib;
use gtk::glib::subclass::prelude::*;

use super::ChangeItem;

mod imp {
    use std::cell::RefCell;

    use gtk::glib;
    use gtk::glib::subclass::prelude::*;
    use gtk::subclass::prelude::*;

    use super::ChangeItem;

    #[derive(Default)]
    pub struct ChangeRow {
        pub current: RefCell<Option<glib::WeakRef<ChangeItem>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ChangeRow {
        const NAME: &'static str = "PasselChangeRow";
        type Type = super::ChangeRow;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for ChangeRow {}
    impl WidgetImpl for ChangeRow {}
    impl BoxImpl for ChangeRow {}
}

glib::wrapper! {
    pub struct ChangeRow(ObjectSubclass<imp::ChangeRow>)
        @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl ChangeRow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_current(&self, item: Option<glib::WeakRef<ChangeItem>>) {
        *self.imp().current.borrow_mut() = item;
    }

    pub fn current(&self) -> Option<ChangeItem> {
        self.imp()
            .current
            .borrow()
            .as_ref()
            .and_then(|weak| weak.upgrade())
    }
}

impl Default for ChangeRow {
    fn default() -> Self {
        Self::new()
    }
}
