use std::cell::Cell;

use adw::subclass::prelude::*;
use gtk::glib;
use gtk::{
    glib::{subclass::InitializingObject, subclass::Signal},
    prelude::StaticType,
    CompositeTemplate, TemplateChild,
};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/io/pierrotws/GnomeVault/ui/changes_view.ui")]
pub struct ChangesView {
    #[template_child]
    pub empty_label: TemplateChild<gtk::Label>,

    #[template_child]
    pub changes_scrolled_window: TemplateChild<gtk::ScrolledWindow>,

    #[template_child]
    pub changes_box: TemplateChild<gtk::Box>,

    pub is_loading_more: Cell<bool>,
    pub has_more_changes: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for ChangesView {
    const NAME: &'static str = "ChangesView";
    type Type = super::ChangesView;
    type ParentType = gtk::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for ChangesView {
    fn signals() -> &'static [Signal] {
        static SIGNALS: std::sync::OnceLock<Vec<Signal>> = std::sync::OnceLock::new();

        SIGNALS.get_or_init(|| {
            vec![
                Signal::builder("revert-change-requested")
                    .param_types([String::static_type()])
                    .build(),
                Signal::builder("rollback-change-requested")
                    .param_types([String::static_type()])
                    .build(),
                Signal::builder("push-requested").build(),
                Signal::builder("load-more-requested").build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        self.obj().setup_callbacks();
    }
}

impl WidgetImpl for ChangesView {}
impl BoxImpl for ChangesView {}
