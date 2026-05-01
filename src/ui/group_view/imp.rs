use std::cell::OnceCell;

use adw::subclass::prelude::*;
use gtk::gio;
use gtk::prelude::*;
use gtk::{
    glib,
    glib::{subclass::InitializingObject, subclass::Signal},
    CompositeTemplate, TemplateChild,
};

#[derive(Default, CompositeTemplate)]
#[template(resource = "/io/pierrotws/GnomeVault/ui/group_view.ui")]
pub struct GroupView {
    #[template_child]
    pub group_title_label: TemplateChild<gtk::Label>,

    #[template_child]
    pub group_count_label: TemplateChild<gtk::Label>,

    #[template_child]
    pub entries_list: TemplateChild<gtk::ListView>,

    /// Backing store for the ListView. Holds [`super::GroupItem`]
    /// objects so the SignalListItemFactory recycles rows.
    pub model: OnceCell<gio::ListStore>,
}

#[glib::object_subclass]
impl ObjectSubclass for GroupView {
    const NAME: &'static str = "GroupView";
    type Type = super::GroupView;
    type ParentType = gtk::Box;

    fn class_init(klass: &mut Self::Class) {
        klass.bind_template();
    }

    fn instance_init(obj: &InitializingObject<Self>) {
        obj.init_template();
    }
}

impl ObjectImpl for GroupView {
    fn signals() -> &'static [Signal] {
        static SIGNALS: std::sync::OnceLock<Vec<Signal>> = std::sync::OnceLock::new();

        SIGNALS.get_or_init(|| {
            // Emit the entry's relative path so the receiver looks up
            // the matching PassNode by identity rather than by current
            // row position. Drop / reorder operations between emit and
            // handler invocation otherwise pointed at the wrong node.
            vec![
                Signal::builder("entry-activated")
                    .param_types([String::static_type()])
                    .build(),
                // Mirrors the `vault_view` shape: (entry_path,
                // entry_name) so the window handler can rebuild the
                // PassNode the same way.
                Signal::builder("delete-entry-requested")
                    .param_types([String::static_type(), String::static_type()])
                    .build(),
            ]
        })
    }

    fn constructed(&self) {
        self.parent_constructed();
        self.obj().setup_callbacks();
    }

    fn dispose(&self) {
        let obj = self.obj();
        while let Some(child) = obj.first_child() {
            child.unparent();
        }
    }
}

impl WidgetImpl for GroupView {}
impl BoxImpl for GroupView {}
