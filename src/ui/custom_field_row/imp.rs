use adw::subclass::prelude::*;
use gtk::glib;

use gtk::{Button, CompositeTemplate, Entry, TemplateChild};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/io/pierrotws/GnomeVault/custom_field_row.ui")]
pub struct CustomFieldRow {
    #[template_child]
    pub key_entry: TemplateChild<Entry>,

    #[template_child]
    pub value_entry: TemplateChild<Entry>,

    #[template_child]
    pub copy_button: TemplateChild<Button>,

    #[template_child]
    pub delete_button: TemplateChild<Button>,
}

#[glib::object_subclass]
impl ObjectSubclass for CustomFieldRow {
    const NAME: &'static str = "CustomFieldRow";
    type Type = super::CustomFieldRow;
    type ParentType = gtk::ListBoxRow;

    fn class_init(klass: &mut Self::Class) {
        Self::bind_template(klass);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for CustomFieldRow {
    fn constructed(&self) {
        self.parent_constructed();
        // let obj = self.obj();
    }
}

impl WidgetImpl for CustomFieldRow {}
impl ListBoxRowImpl for CustomFieldRow {}
