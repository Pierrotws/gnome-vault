use adw::subclass::prelude::*;
use gtk::glib;

use gtk::{Box, Button, CompositeTemplate, Entry, Image, Label, ProgressBar, TemplateChild};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/io/pierrotws/GnomeVault/ui/fields/otp_field_row.ui")]
pub struct OtpFieldRow {
    #[template_child]
    pub view_box: TemplateChild<Box>,

    #[template_child]
    pub edit_box: TemplateChild<Box>,

    #[template_child]
    pub title_label: TemplateChild<Label>,

    #[template_child]
    pub code_label: TemplateChild<Label>,

    #[template_child]
    pub validity_progress: TemplateChild<ProgressBar>,

    #[template_child]
    pub drag_handle: TemplateChild<Image>,

    #[template_child]
    pub key_entry: TemplateChild<Entry>,

    #[template_child]
    pub url_entry: TemplateChild<Entry>,

    #[template_child]
    pub copy_code_button: TemplateChild<Button>,

    #[template_child]
    pub copy_url_button: TemplateChild<Button>,

    #[template_child]
    pub delete_button: TemplateChild<Button>,
}

#[glib::object_subclass]
impl ObjectSubclass for OtpFieldRow {
    const NAME: &'static str = "OtpFieldRow";
    type Type = super::OtpFieldRow;
    type ParentType = gtk::ListBoxRow;

    fn class_init(klass: &mut Self::Class) {
        Self::bind_template(klass);
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for OtpFieldRow {}
impl WidgetImpl for OtpFieldRow {}
impl ListBoxRowImpl for OtpFieldRow {}
