use adw::subclass::prelude::*;
use gtk::glib;
use gtk::subclass::prelude::*;
use gtk::{CompositeTemplate, TemplateChild};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/io/pierrotws/GnomeVault/window.ui")]
pub struct MainWindow {
    #[template_child]
    pub tree_search_entry: TemplateChild<gtk::SearchEntry>,

    #[template_child]
    pub tree_list: TemplateChild<gtk::ListBox>,

    #[template_child]
    pub password_row: TemplateChild<adw::PasswordEntryRow>,

    #[template_child]
    pub custom_fields_list: TemplateChild<gtk::ListBox>,

    #[template_child]
    pub add_field_button: TemplateChild<gtk::Button>,
}

#[glib::object_subclass]
impl ObjectSubclass for MainWindow {
    const NAME: &'static str = "MainWindow";
    type Type = super::MainWindow;
    type ParentType = adw::ApplicationWindow;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for MainWindow {
    fn constructed(&self) {
        self.parent_constructed();
        let obj = self.obj();
        obj.setup_callbacks();
    }
}

impl WidgetImpl for MainWindow {}
impl WindowImpl for MainWindow {}
impl ApplicationWindowImpl for MainWindow {}
impl AdwApplicationWindowImpl for MainWindow {}
