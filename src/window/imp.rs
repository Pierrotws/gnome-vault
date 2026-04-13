use adw::subclass::prelude::*;
use gtk::glib;
use gtk::{CompositeTemplate, TemplateChild};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/io/pierrotws/GnomeVault/window.ui")]
pub struct MainWindow {
    #[template_child]
    pub tree_search_entry: TemplateChild<gtk::SearchEntry>,

    #[template_child]
    pub tree_view: TemplateChild<gtk::ListView>,

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
        obj.setup_tree_view();
        obj.setup_callbacks();
    }
}

impl gtk::subclass::widget::WidgetImpl for MainWindow {}
impl gtk::subclass::window::WindowImpl for MainWindow {}
impl gtk::subclass::application_window::ApplicationWindowImpl for MainWindow {}
impl adw::subclass::application_window::AdwApplicationWindowImpl for MainWindow {}
