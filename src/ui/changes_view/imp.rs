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
    pub changes_box: TemplateChild<gtk::Box>,
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
            ]
        })
    }
}

impl WidgetImpl for ChangesView {}
impl BoxImpl for ChangesView {}
