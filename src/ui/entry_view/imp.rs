use std::cell::Cell;

use adw::subclass::prelude::*;
use glib::subclass::*;
use gtk::{glib, CompositeTemplate};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/io/pierrotws/GnomeVault/entry_view.ui")]
pub struct EntryView {
    #[template_child]
    pub content_stack: TemplateChild<gtk::Stack>,

    #[template_child]
    pub title_label: TemplateChild<gtk::Label>,

    #[template_child]
    pub password_row: TemplateChild<adw::PasswordEntryRow>,

    #[template_child]
    pub copy_password_button: TemplateChild<gtk::Button>,

    #[template_child]
    pub generate_password_button: TemplateChild<gtk::Button>,

    #[template_child]
    pub custom_fields_list: TemplateChild<gtk::ListBox>,

    #[template_child]
    pub add_field_button: TemplateChild<gtk::Button>,

    #[template_child]
    pub cancel_button: TemplateChild<gtk::Button>,

    #[template_child]
    pub save_button: TemplateChild<gtk::Button>,

    //non graphical
    pub is_updating_ui: Cell<bool>,
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
