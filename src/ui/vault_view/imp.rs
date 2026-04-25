use gtk::subclass::prelude::*;
use gtk::{
    glib::{self, subclass::*},
    CompositeTemplate, TemplateChild,
};

#[derive(Debug, Default, CompositeTemplate)]
#[template(resource = "/io/pierrotws/GnomeVault/ui/vault_view.ui")]
pub struct VaultView {
    #[template_child]
    pub search_entry: TemplateChild<gtk::SearchEntry>,

    #[template_child]
    pub tree_view: TemplateChild<gtk::ListView>,
}

#[glib::object_subclass]
impl ObjectSubclass for VaultView {
    const NAME: &'static str = "VaultView";
    type Type = super::VaultView;
    type ParentType = gtk::Box;

    fn class_init(klass: &mut Self::Class) {
        Self::bind_template(klass);
    }

    fn instance_init(obj: &InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for VaultView {
    fn signals() -> &'static [Signal] {
        static SIGNALS: std::sync::OnceLock<Vec<Signal>> = std::sync::OnceLock::new();

        SIGNALS.get_or_init(|| {
            vec![
                Signal::builder("entry-selected").build(),
                Signal::builder("group-activated").build(),
                Signal::builder("search-changed").build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();

        let obj = self.obj();
        obj.setup_callbacks();
    }
}

impl WidgetImpl for VaultView {}
impl BoxImpl for VaultView {}
