use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use adw::subclass::prelude::*;
use glib::subclass::{InitializingObject, Signal};
use gtk::prelude::*;
use gtk::{glib, CompositeTemplate, TemplateChild};

use crate::pass::store::GpgRecipient;

use super::SetupRun;

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/io/pierrotws/Passel/ui/setup_wizard.ui")]
pub struct SetupWizard {
    #[template_child]
    pub navigation: TemplateChild<adw::NavigationView>,

    #[template_child]
    pub create_button: TemplateChild<gtk::Button>,

    #[template_child]
    pub vault_group: TemplateChild<adw::PreferencesGroup>,

    #[template_child]
    pub path_row: TemplateChild<adw::EntryRow>,

    #[template_child]
    pub recipient_row: TemplateChild<adw::ComboRow>,

    #[template_child]
    pub vault_back_button: TemplateChild<gtk::Button>,

    #[template_child]
    pub vault_next_button: TemplateChild<gtk::Button>,

    #[template_child]
    pub provider_row: TemplateChild<adw::ComboRow>,

    #[template_child]
    pub remote_row: TemplateChild<adw::EntryRow>,

    #[template_child]
    pub sync_back_button: TemplateChild<gtk::Button>,

    #[template_child]
    pub sync_next_button: TemplateChild<gtk::Button>,

    #[template_child]
    pub review_page: TemplateChild<adw::NavigationPage>,

    #[template_child]
    pub steps_list: TemplateChild<gtk::ListBox>,

    #[template_child]
    pub error_label: TemplateChild<gtk::Label>,

    #[template_child]
    pub review_back_button: TemplateChild<gtk::Button>,

    #[template_child]
    pub skip_button: TemplateChild<gtk::Button>,

    #[template_child]
    pub apply_button: TemplateChild<gtk::Button>,

    pub recipients: RefCell<Vec<GpgRecipient>>,
    pub autopush: Cell<bool>,
    /// The plan shown on the review page and how far it got. Kept across
    /// retries so steps that already succeeded are not run again.
    pub run: RefCell<Option<SetupRun>>,
    pub running: Cell<bool>,
}

#[glib::object_subclass]
impl ObjectSubclass for SetupWizard {
    const NAME: &'static str = "SetupWizard";
    type Type = super::SetupWizard;
    type ParentType = adw::Bin;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for SetupWizard {
    fn constructed(&self) {
        self.parent_constructed();
        self.obj().setup();
    }

    fn signals() -> &'static [Signal] {
        static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
        SIGNALS.get_or_init(|| {
            vec![
                // Every step succeeded (or the user skipped the sync steps
                // after the vault itself was created). Carries the folder.
                Signal::builder("vault-created")
                    .param_types([String::static_type()])
                    .build(),
            ]
        })
    }
}

impl WidgetImpl for SetupWizard {}
impl BinImpl for SetupWizard {}
