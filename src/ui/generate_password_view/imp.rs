use adw::subclass::prelude::*;
use glib::subclass::InitializingObject;
use gtk::prelude::*;
use gtk::{glib, CompositeTemplate, TemplateChild};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/io/pierrotws/GnomeVault/ui/generate_password_view.ui")]
pub struct GeneratePasswordView {
    #[template_child]
    pub length_spin: TemplateChild<gtk::SpinButton>,

    #[template_child]
    pub charset_row: TemplateChild<adw::ComboRow>,

    #[template_child]
    pub preview_password_row: TemplateChild<adw::PasswordEntryRow>,

    #[template_child]
    pub reload_button: TemplateChild<gtk::Button>,
}

#[glib::object_subclass]
impl ObjectSubclass for GeneratePasswordView {
    const NAME: &'static str = "GeneratePasswordView";
    type Type = super::GeneratePasswordView;
    type ParentType = adw::Bin;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for GeneratePasswordView {
    fn constructed(&self) {
        self.parent_constructed();
        self.obj().setup();
    }

    fn dispose(&self) {
        let obj = self.obj();
        while let Some(child) = obj.first_child() {
            child.unparent();
        }
    }
}

impl WidgetImpl for GeneratePasswordView {}
impl BinImpl for GeneratePasswordView {}
