use adw::subclass::prelude::*;
use gtk::glib;

use gtk::{Button, CompositeTemplate, TemplateChild};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/io/pierrotws/GnomeVault/ui/fields/password_field_row.ui")]
pub struct PasswordFieldRow {
    #[template_child]
    pub password_entry: TemplateChild<adw::PasswordEntryRow>,

    #[template_child]
    pub copy_password_button: TemplateChild<Button>,

    #[template_child]
    pub generate_password_button: TemplateChild<Button>,
}

#[glib::object_subclass]
impl ObjectSubclass for PasswordFieldRow {
    const NAME: &'static str = "PasswordFieldRow";
    type Type = super::PasswordFieldRow;
    type ParentType = adw::Bin;

    fn class_init(klass: &mut Self::Class) {
        Self::bind_template(klass);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for PasswordFieldRow {}
impl BinImpl for PasswordFieldRow {}
impl WidgetImpl for PasswordFieldRow {}
