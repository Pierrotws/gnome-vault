//! Preferences dialog: autopush, startup autoload, group-view layout.

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;

use super::MainWindow;

impl MainWindow {
    pub(super) fn show_preferences_dialog(&self) {
        let builder =
            gtk::Builder::from_resource("/io/pierrotws/GnomeVault/ui/preferences_dialog.ui");
        let dialog = builder
            .object::<adw::PreferencesDialog>("preferences_dialog")
            .expect("preferences_dialog must exist in preferences_dialog.ui");
        let autopush_row = builder
            .object::<adw::SwitchRow>("autopush_row")
            .expect("autopush_row must exist in preferences_dialog.ui");
        let autoload_row = builder
            .object::<adw::SwitchRow>("autoload_row")
            .expect("autoload_row must exist in preferences_dialog.ui");
        let show_group_view_row = builder
            .object::<adw::SwitchRow>("show_group_view_row")
            .expect("show_group_view_row must exist in preferences_dialog.ui");

        autopush_row.set_active(self.setting_boolean("autopush", true));
        autoload_row.set_active(self.setting_boolean("autoload", false));
        show_group_view_row.set_active(self.show_group_view_enabled());

        autopush_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |row| {
                let settings = window.settings();
                if let Err(err) = settings.set_boolean("autopush", row.is_active()) {
                    window.imp().entry_view.show_error(&err.to_string());
                    return;
                }
                window.apply_autopush_setting();
                if let Err(err) = window.reload_changes_view() {
                    window.imp().entry_view.show_error(&err.to_string());
                }
            }
        ));

        autoload_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |row| {
                if !window.settings_has_key("autoload") {
                    window
                        .imp()
                        .entry_view
                        .show_error("GSettings schema is missing autoload");
                    return;
                }
                let settings = window.settings();
                if let Err(err) = settings.set_boolean("autoload", row.is_active()) {
                    window.imp().entry_view.show_error(&err.to_string());
                    return;
                }
                if row.is_active() {
                    window.start_autoload_cache();
                }
            }
        ));

        show_group_view_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |row| {
                if !window.settings_has_key("show-group-view") {
                    window
                        .imp()
                        .entry_view
                        .show_error("GSettings schema is missing show-group-view");
                    return;
                }
                let settings = window.settings();
                if let Err(err) = settings.set_boolean("show-group-view", row.is_active()) {
                    window.imp().entry_view.show_error(&err.to_string());
                    return;
                }
                window.update_selection_layout();
                window.rebuild_vault_tree();
                if row.is_active() {
                    window.show_root_group_content();
                }
            }
        ));

        dialog.present(Some(self));
    }
}
