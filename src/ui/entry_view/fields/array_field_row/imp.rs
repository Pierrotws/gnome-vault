use adw::subclass::prelude::*;
use gtk::glib;

use gtk::{Button, CompositeTemplate, Entry, ListBox, TemplateChild};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/io/pierrotws/GnomeVault/ui/fields/array_field_row.ui")]
pub struct ArrayFieldRow {
    #[template_child]
    pub title_entry: TemplateChild<Entry>,

    #[template_child]
    pub value_list: TemplateChild<ListBox>,

    #[template_child]
    pub add_item_button: TemplateChild<Button>,

    #[template_child]
    pub copy_button: TemplateChild<Button>,

    #[template_child]
    pub delete_button: TemplateChild<Button>,
}

#[glib::object_subclass]
impl ObjectSubclass for ArrayFieldRow {
    const NAME: &'static str = "ArrayFieldRow";
    type Type = super::ArrayFieldRow;
    type ParentType = gtk::ListBoxRow;

    fn class_init(klass: &mut Self::Class) {
        Self::bind_template(klass);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for ArrayFieldRow {}
impl WidgetImpl for ArrayFieldRow {}
impl ListBoxRowImpl for ArrayFieldRow {}
