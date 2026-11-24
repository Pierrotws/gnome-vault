use adw::subclass::prelude::*;
use glib::subclass::InitializingObject;
use gtk::{glib, CompositeTemplate, TemplateChild};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/io/pierrotws/Passel/ui/generate_password_view.ui")]
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

    // No manual unparent loop here. `adw::Bin` already owns its single
    // child via a `child` property and unparents it in its own dispose
    // handler. Doing it ourselves first would leave Bin's priv->child
    // pointing at an already-unparented (and possibly freed) widget,
    // tripping `gtk_widget_unparent: assertion 'GTK_IS_WIDGET(widget)'
    // failed` when Bin's dispose runs next.
}

impl WidgetImpl for GeneratePasswordView {}
impl BinImpl for GeneratePasswordView {}
