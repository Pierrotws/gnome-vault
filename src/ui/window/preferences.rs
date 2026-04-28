//! Preferences dialog: autopush, startup autoload, group-view layout.

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;

use crate::pass::store;

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
        let store_dir_row = builder
            .object::<adw::EntryRow>("store_dir_row")
            .expect("store_dir_row must exist in preferences_dialog.ui");
        let branch_row = builder
            .object::<adw::EntryRow>("branch_row")
            .expect("branch_row must exist in preferences_dialog.ui");

        autopush_row.set_active(self.setting_boolean("autopush", true));
        autoload_row.set_active(self.setting_boolean("autoload", false));
        show_group_view_row.set_active(self.show_group_view_enabled());
        store_dir_row.set_text(&self.setting_string("store-dir", ""));
        // Prefer an explicit user override; otherwise fall back to whatever
        // branch the working tree currently has checked out, so the field is
        // never blank if the repo is in a sensible state.
        let branch_setting = self.setting_string("branch", "");
        let branch_text = if branch_setting.trim().is_empty() {
            store::current_branch_name().unwrap_or_default()
        } else {
            branch_setting
        };
        branch_row.set_text(&branch_text);

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
                window.update_selection_layout(true);
                window.rebuild_vault_tree();
                if row.is_active() {
                    window.show_root_group_content();
                }
            }
        ));

        store_dir_row.connect_apply(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |row| {
                if !window.settings_has_key("store-dir") {
                    window
                        .imp()
                        .entry_view
                        .show_error("GSettings schema is missing store-dir");
                    return;
                }
                let value = row.text().trim().to_string();
                let settings = window.settings();
                if let Err(err) = settings.set_string("store-dir", &value) {
                    window.imp().entry_view.show_error(&err.to_string());
                    return;
                }
                // Update the env var so subsequent store ops resolve to the
                // new directory. The currently-loaded tree, opened entry,
                // and decrypted cache still reflect the old vault until the
                // user reloads or restarts.
                window.apply_store_dir_setting();
                window
                    .imp()
                    .entry_view
                    .show_error("Vault directory updated; restart the app to load it");
            }
        ));

        branch_row.connect_apply(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |row| {
                if !window.settings_has_key("branch") {
                    window
                        .imp()
                        .entry_view
                        .show_error("GSettings schema is missing branch");
                    return;
                }
                let value = row.text().trim().to_string();
                let settings = window.settings();
                if let Err(err) = settings.set_string("branch", &value) {
                    window.imp().entry_view.show_error(&err.to_string());
                    return;
                }
                window.apply_branch_setting();
                if let Err(err) = window.reload_changes_view() {
                    window.imp().entry_view.show_error(&err.to_string());
                }
            }
        ));

        dialog.present(Some(self));
    }
}
