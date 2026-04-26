use std::cell::Cell;

use adw::subclass::prelude::*;
use glib::subclass::*;
use gtk::{glib, CompositeTemplate};

use super::fields::{OtpFieldRow, PasswordFieldRow};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/io/pierrotws/GnomeVault/ui/entry_view.ui")]
pub struct EntryView {
    #[template_child]
    pub content_stack: TemplateChild<gtk::Stack>,

    #[template_child]
    pub title_label: TemplateChild<gtk::Label>,

    #[template_child]
    pub title_entry: TemplateChild<gtk::Entry>,

    #[template_child]
    pub primary_field_stack: TemplateChild<gtk::Stack>,

    #[template_child]
    pub password_field_row: TemplateChild<PasswordFieldRow>,

    #[template_child]
    pub primary_otp_field_row: TemplateChild<OtpFieldRow>,

    #[template_child]
    pub custom_fields_list: TemplateChild<gtk::Box>,

    #[template_child]
    pub add_field_row: TemplateChild<gtk::Box>,

    #[template_child]
    pub add_field_menu_button: TemplateChild<gtk::MenuButton>,

    #[template_child]
    pub footer_actions: TemplateChild<gtk::Box>,

    #[template_child]
    pub delete_button: TemplateChild<gtk::Button>,

    #[template_child]
    pub add_array_field_button: TemplateChild<gtk::Button>,

    #[template_child]
    pub add_otp_field_button: TemplateChild<gtk::Button>,

    #[template_child]
    pub add_plain_field_button: TemplateChild<gtk::Button>,

    #[template_child]
    pub add_multiline_field_button: TemplateChild<gtk::Button>,

    #[template_child]
    pub cancel_button: TemplateChild<gtk::Button>,

    #[template_child]
    pub save_button: TemplateChild<gtk::Button>,

    //non graphical
    pub is_updating_ui: Cell<bool>,
    pub is_editable: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for EntryView {
    const NAME: &'static str = "EntryView";
    type Type = super::EntryView;
    type ParentType = adw::Bin;

    fn class_init(klass: &mut Self::Class) {
        Self::bind_template(klass);
    }

    fn instance_init(obj: &InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for EntryView {
    fn constructed(&self) {
        self.parent_constructed();
        self.is_updating_ui.set(false);
        self.is_editable.set(false);
        let obj = self.obj();
        obj.setup_callbacks();
    }

    fn signals() -> &'static [Signal] {
        use std::sync::OnceLock;
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();

        SIGNALS.get_or_init(|| {
            vec![
                Signal::builder("entry-changed").build(),
                Signal::builder("save-requested").build(),
                Signal::builder("revert-requested").build(),
            ]
        })
    }
}

impl BinImpl for EntryView {}

impl WidgetImpl for EntryView {}
