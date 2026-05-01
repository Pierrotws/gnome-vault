use std::cell::Cell;

use adw::subclass::prelude::*;
use gtk::glib;
use gtk::prelude::*;

use gtk::{
    Box, Button, CompositeTemplate, Entry, Image, Label, ProgressBar, TemplateChild, ToggleButton,
};

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
    pub reveal_url_button: TemplateChild<ToggleButton>,

    #[template_child]
    pub copy_url_button: TemplateChild<Button>,

    #[template_child]
    pub delete_button: TemplateChild<Button>,

    pub show_row_controls: Cell<bool>,
    pub key_editable: Cell<bool>,
    pub already_setup: Cell<bool>,
    pub timer_source: Cell<Option<glib::SourceId>>,
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

impl ObjectImpl for OtpFieldRow {
    fn constructed(&self) {
        self.parent_constructed();
        // The otpauth:// URL embeds the long-lived shared secret in
        // its `?secret=...` query parameter; that's strictly worse to
        // leak than any single OTP code. Tell input methods,
        // accessibility tooling (Orca will read entry contents
        // character-by-character), and spellcheck/candidate-window
        // subsystems that this entry is not for general text input.
        // Mirrors the hint set used on PasswordFieldRow's
        // password_entry.
        self.url_entry.set_input_hints(
            gtk::InputHints::PRIVATE | gtk::InputHints::NO_EMOJI | gtk::InputHints::NO_SPELLCHECK,
        );
        self.url_entry.set_input_purpose(gtk::InputPurpose::Password);
    }

    fn dispose(&self) {
        if let Some(source) = self.timer_source.take() {
            source.remove();
        }
        let obj = self.obj();
        while let Some(child) = obj.first_child() {
            child.unparent();
        }
    }
}
impl WidgetImpl for OtpFieldRow {}
impl ListBoxRowImpl for OtpFieldRow {}
