use std::cell::RefCell;

use adw::subclass::prelude::*;
use gtk::prelude::*;
use gtk::{
    glib,
    glib::{subclass::InitializingObject, subclass::Signal},
    CompositeTemplate, TemplateChild,
};

use crate::pass::model::PassNode;

#[derive(Default, CompositeTemplate)]
#[template(resource = "/io/pierrotws/GnomeVault/ui/group_view.ui")]
pub struct GroupView {
    #[template_child]
    pub group_title_label: TemplateChild<gtk::Label>,

    #[template_child]
    pub group_count_label: TemplateChild<gtk::Label>,

    #[template_child]
    pub entries_list: TemplateChild<gtk::ListBox>,

    pub entries: RefCell<Vec<PassNode>>,
}

#[glib::object_subclass]
impl ObjectSubclass for GroupView {
    const NAME: &'static str = "GroupView";
    type Type = super::GroupView;
    type ParentType = gtk::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for GroupView {
    fn signals() -> &'static [Signal] {
        static SIGNALS: std::sync::OnceLock<Vec<Signal>> = std::sync::OnceLock::new();

        SIGNALS.get_or_init(|| {
            vec![Signal::builder("entry-activated")
                .param_types([u32::static_type()])
                .build()]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        self.obj().setup_callbacks();
    }

    fn dispose(&self) {
        let obj = self.obj();
        while let Some(child) = obj.first_child() {
            child.unparent();
        }
    }
}

impl WidgetImpl for GroupView {}
impl BoxImpl for GroupView {}
