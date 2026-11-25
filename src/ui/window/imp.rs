use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use adw::subclass::prelude::*;
use gtk::CompositeTemplate;
use gtk::{gio, glib};
use once_cell::unsync::OnceCell;

use crate::app::controller::AppController;
use crate::ui::{ChangesView, EntryView, GroupView, VaultView};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/io/pierrotws/Passel/ui/window.ui")]
pub struct MainWindow {
    #[template_child]
    pub window_title: TemplateChild<adw::WindowTitle>,

    #[template_child]
    pub main_stack: TemplateChild<gtk::Stack>,

    #[template_child]
    pub content_stack: TemplateChild<gtk::Stack>,

    #[template_child]
    pub selection_stack: TemplateChild<gtk::Stack>,

    #[template_child]
    pub main_paned: TemplateChild<gtk::Paned>,

    #[template_child]
    pub selection_zone: TemplateChild<gtk::Box>,

    #[template_child]
    pub simple_vault_bin: TemplateChild<adw::Bin>,

    #[template_child]
    pub group_vault_bin: TemplateChild<adw::Bin>,

    #[template_child]
    pub selection_paned: TemplateChild<gtk::Paned>,

    #[template_child]
    pub entry_bin: TemplateChild<adw::Bin>,

    #[template_child]
    pub vault_view: TemplateChild<VaultView>,

    #[template_child]
    pub group_view: TemplateChild<GroupView>,

    #[template_child]
    pub changes_view: TemplateChild<ChangesView>,

    #[template_child]
    pub entry_view: TemplateChild<EntryView>,

    #[template_child]
    pub new_entry_button: TemplateChild<gtk::Button>,

    #[template_child]
    pub changes_button: TemplateChild<gtk::Button>,

    #[template_child]
    pub changes_badge: TemplateChild<gtk::Label>,

    #[template_child]
    pub lock_vault_button: TemplateChild<gtk::Button>,

    #[template_child]
    pub preferences_button: TemplateChild<gtk::Button>,

    #[template_child]
    pub create_vault_button: TemplateChild<gtk::Button>,

    #[template_child]
    pub enable_history_page: TemplateChild<adw::StatusPage>,

    #[template_child]
    pub enable_history_button: TemplateChild<gtk::Button>,

    //App Controller
    pub controller: OnceCell<Rc<RefCell<AppController>>>,
    pub settings: OnceCell<gio::Settings>,
    pub loaded_changes_count: Cell<usize>,
    pub autoload_running: Cell<bool>,
    /// Shared with the autoload worker thread; the polling tick and the
    /// worker both honor it so a "Load entries at startup" toggle in
    /// preferences can stop an in-flight cache warmup.
    pub autoload_cancel: RefCell<Option<Arc<AtomicBool>>>,
    pub group_preview_generation: Cell<u64>,
    /// The setup wizard's dialog while it is open.
    pub setup_dialog: glib::WeakRef<adw::Dialog>,
    /// Shared with the in-flight group-preview worker. Navigating away
    /// from a group (or starting a new preview load) sets the previous
    /// flag so the worker stops decrypting the rest of the queue
    /// instead of running to completion in the background.
    pub group_preview_cancel: RefCell<Option<Arc<AtomicBool>>>,
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
