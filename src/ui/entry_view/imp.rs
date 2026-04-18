use std::cell::{Cell, RefCell};

use adw::subclass::prelude::*;
use glib::subclass::InitializingObject;
use gtk::{glib, CompositeTemplate};

use crate::pass::entry::EntryData;

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
    pub current_entry: RefCell<Option<EntryData>>,
    pub modified: Cell<bool>,
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
        self.modified.set(false);
        let obj = self.obj();
        obj.setup_callbacks();
    }
}

impl BinImpl for EntryView {}

impl WidgetImpl for EntryView {}
