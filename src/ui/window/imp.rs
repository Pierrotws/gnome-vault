use std::cell::RefCell;
use std::rc::Rc;

use adw::subclass::prelude::*;
use gtk::glib;
use gtk::CompositeTemplate;
use once_cell::unsync::OnceCell;

use crate::app::controller::AppController;
use crate::ui::{EntryView, VaultView};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/io/pierrotws/GnomeVault/window.ui")]
pub struct MainWindow {
    #[template_child]
    pub vault_view: TemplateChild<VaultView>,

    #[template_child]
    pub entry_view: TemplateChild<EntryView>,

    //App Controller
    pub controller: OnceCell<Rc<RefCell<AppController>>>,
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
