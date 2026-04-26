use adw::subclass::prelude::*;
use gtk::glib;

use gtk::{
    Button, CompositeTemplate, Entry, Image, Label, ScrolledWindow, TemplateChild, TextView,
};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/io/pierrotws/GnomeVault/ui/fields/multiline_field_row.ui")]
pub struct MultilineFieldRow {
    #[template_child]
    pub drag_handle: TemplateChild<Image>,

    #[template_child]
    pub title_entry: TemplateChild<Entry>,

    #[template_child]
    pub value_label: TemplateChild<Label>,

    #[template_child]
    pub value_scrolled_window: TemplateChild<ScrolledWindow>,

    #[template_child]
    pub value_text_view: TemplateChild<TextView>,

    #[template_child]
    pub copy_button: TemplateChild<Button>,

    #[template_child]
    pub delete_button: TemplateChild<Button>,
}

#[glib::object_subclass]
impl ObjectSubclass for MultilineFieldRow {
    const NAME: &'static str = "MultilineFieldRow";
    type Type = super::MultilineFieldRow;
    type ParentType = gtk::ListBoxRow;

    fn class_init(klass: &mut Self::Class) {
        Self::bind_template(klass);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for MultilineFieldRow {}
impl WidgetImpl for MultilineFieldRow {}
impl ListBoxRowImpl for MultilineFieldRow {}
