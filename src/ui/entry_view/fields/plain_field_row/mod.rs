use gtk::{glib, prelude::*, subclass::prelude::*};

use crate::helpers::clipboard;
use crate::pass::model::EntryField;

use super::super::EntryView;
use super::{CustomFieldRow, EntryFieldRow};

mod imp;

/// libadwaita's own "opens outside the app" icon, the same one
/// `AdwAboutDialog` uses for external links.
const LINK_ICON_NAME: &str = "adw-external-link-symbolic";

glib::wrapper! {
    pub struct PlainFieldRow(ObjectSubclass<imp::PlainFieldRow>)
        @extends gtk::Widget, gtk::ListBoxRow,
        @implements gtk::Accessible, gtk::Actionable, gtk::Buildable, gtk::ConstraintTarget;
}

impl PlainFieldRow {
    pub fn new_empty(entry_view: &EntryView) -> Self {
        let obj: Self = glib::Object::new();
        let imp = obj.imp();

        imp.key_entry.connect_changed(glib::clone!(
            #[weak]
            entry_view,
            move |_| {
                entry_view.mark_changed();
            }
        ));
        imp.value_entry.connect_changed(glib::clone!(
            #[weak(rename_to = this)]
            obj,
            #[weak]
            entry_view,
            move |_| {
                this.update_value_link();
                entry_view.mark_changed();
            }
        ));
        // The copy button captures the template's value_entry strongly, but
        // value_entry is owned by `obj` (which holds the imp). The closure
        // lives on a button that is also owned by `obj`. This is a sibling
        // capture via the same root, not a parent->child cycle, so a strong
        // reference is fine.
        let value_entry = imp.value_entry.clone();
        imp.copy_button.connect_clicked(move |_| {
            clipboard::copy_secret(&value_entry.text());
        });

        imp.value_entry.connect_icon_release(|entry, position| {
            if position != gtk::EntryIconPosition::Secondary {
                return;
            }
            let uri = entry.text();
            let uri = uri.trim();
            if !PlainFieldRow::is_web_url(uri) {
                return;
            }
            if let Err(error) =
                gtk::gio::AppInfo::launch_default_for_uri(uri, None::<&gtk::gio::AppLaunchContext>)
            {
                log::warn!("Failed to open link: {error}");
            }
        });

        super::wire_delete_button(&imp.delete_button, obj.upcast_ref(), entry_view);
        obj
    }

    pub fn new(entry_view: &EntryView, key: &str, value: &str) -> Self {
        let obj: Self = Self::new_empty(entry_view);
        obj.set_key(key);
        obj.set_value(value);
        obj
    }

    pub fn from_entry_field(entry_view: &EntryView, key: &str, field: &EntryField) -> Option<Self> {
        match field {
            EntryField::Plain(value) => Some(Self::new(entry_view, key, value)),
            _ => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        let imp = self.imp();
        imp.key_entry.text().trim().is_empty() && imp.value_entry.text().trim().is_empty()
    }

    pub fn key(&self) -> String {
        self.imp().key_entry.text().into()
    }

    pub fn value(&self) -> glib::GString {
        self.imp().value_entry.text().into()
    }

    pub fn set_key(&self, key: &str) {
        self.imp().key_entry.set_text(key);
    }

    pub fn set_value(&self, value: &str) {
        self.imp().value_entry.set_text(value);
        self.update_value_link();
    }

    pub fn set_editable_mode(&self, editable: bool) {
        let imp = self.imp();
        super::set_control_reserved(&imp.drag_handle.get(), editable);
        super::set_entry_editable(&imp.key_entry, editable);
        super::set_entry_editable(&imp.value_entry, editable);
        super::set_control_reserved(&imp.delete_button.get(), editable);
        self.update_value_link();
    }

    pub fn drag_handle(&self) -> gtk::Widget {
        self.imp().drag_handle.get().upcast()
    }

    /// Shows a clickable "open link" icon inside the value entry when
    /// the value is a web URL.
    ///
    /// This used to swap the entry out for a `LinkButton` in read-only
    /// mode, which meant a URL field rendered as blue underlined link
    /// text when reading and as plain entry text when editing — the
    /// same value in two different shapes. Keeping the entry and hanging
    /// the affordance off its secondary icon means the value never moves
    /// or restyles, and the link stays reachable in both modes.
    fn update_value_link(&self) {
        let imp = self.imp();
        let value = imp.value_entry.text();
        let url = value.trim();
        let position = gtk::EntryIconPosition::Secondary;

        if Self::is_web_url(url) {
            imp.value_entry
                .set_icon_from_icon_name(position, Some(LINK_ICON_NAME));
            imp.value_entry.set_icon_activatable(position, true);
            imp.value_entry.set_icon_sensitive(position, true);
            imp.value_entry.set_icon_tooltip_text(position, Some(url));
        } else {
            imp.value_entry.set_icon_from_icon_name(position, None);
            imp.value_entry.set_icon_tooltip_text(position, None);
        }
    }

    fn is_web_url(value: &str) -> bool {
        let Some(rest) = value
            .strip_prefix("https://")
            .or_else(|| value.strip_prefix("http://"))
        else {
            return false;
        };

        !rest.is_empty()
            && rest.contains('.')
            && !rest
                .chars()
                .any(|character| character.is_ascii_whitespace())
    }
}

impl EntryFieldRow for PlainFieldRow {
    fn key(&self) -> String {
        self.key().trim().to_string()
    }

    fn entry_field(&self) -> EntryField {
        EntryField::Plain(self.value().trim().to_string())
    }

    fn set_entry_field(&self, field: &EntryField) {
        if let EntryField::Plain(value) = field {
            self.set_value(value);
        }
    }
}

impl CustomFieldRow for PlainFieldRow {
    fn set_editable_mode(&self, editable: bool) {
        PlainFieldRow::set_editable_mode(self, editable);
    }

    fn is_empty(&self) -> bool {
        PlainFieldRow::is_empty(self)
    }

    fn drag_handle(&self) -> gtk::Widget {
        PlainFieldRow::drag_handle(self)
    }
}
