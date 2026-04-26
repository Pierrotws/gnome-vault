use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::subclass::prelude::*;
use gtk::CompositeTemplate;
use gtk::{gio, glib};
use once_cell::unsync::OnceCell;

use crate::app::controller::AppController;
use crate::ui::{ChangesView, EntryView, VaultView};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/io/pierrotws/GnomeVault/ui/window.ui")]
pub struct MainWindow {
    #[template_child]
    pub window_title: TemplateChild<adw::WindowTitle>,

    #[template_child]
    pub main_stack: TemplateChild<gtk::Stack>,

    #[template_child]
    pub navigation_stack: TemplateChild<gtk::Stack>,

    #[template_child]
    pub vault_view: TemplateChild<VaultView>,

    #[template_child]
    pub changes_view: TemplateChild<ChangesView>,

    #[template_child]
    pub entry_view: TemplateChild<EntryView>,

    #[template_child]
    pub new_entry_button: TemplateChild<gtk::Button>,

    #[template_child]
    pub lock_vault_button: TemplateChild<gtk::Button>,

    #[template_child]
    pub preferences_button: TemplateChild<gtk::Button>,

    #[template_child]
    pub setup_path_row: TemplateChild<adw::EntryRow>,

    #[template_child]
    pub setup_recipient_row: TemplateChild<adw::ComboRow>,

    #[template_child]
    pub setup_provider_row: TemplateChild<adw::ComboRow>,

    #[template_child]
    pub setup_remote_row: TemplateChild<adw::EntryRow>,

    #[template_child]
    pub create_vault_button: TemplateChild<gtk::Button>,

    //App Controller
    pub controller: OnceCell<Rc<RefCell<AppController>>>,
    pub settings: OnceCell<gio::Settings>,
    pub loaded_changes_count: Cell<usize>,
    pub setup_recipients: RefCell<Vec<crate::pass::store::GpgRecipient>>,
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
    }
}

impl WidgetImpl for MainWindow {}
impl WindowImpl for MainWindow {}
impl ApplicationWindowImpl for MainWindow {}
impl AdwApplicationWindowImpl for MainWindow {}
