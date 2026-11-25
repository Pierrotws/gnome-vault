//! Preferences dialog: startup autoload, group-view layout, tree size.
//! Per-vault settings (branch, auto-push, remote) are in each vault's
//! settings dialog.

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;

use super::MainWindow;

impl MainWindow {
    pub(super) fn show_preferences_dialog(&self) {
        let builder = gtk::Builder::from_resource("/io/pierrotws/Passel/ui/preferences_dialog.ui");
        let dialog = builder
            .object::<adw::PreferencesDialog>("preferences_dialog")
            .expect("preferences_dialog must exist in preferences_dialog.ui");
        let autoload_row = builder
            .object::<adw::SwitchRow>("autoload_row")
            .expect("autoload_row must exist in preferences_dialog.ui");
        let show_group_view_row = builder
            .object::<adw::SwitchRow>("show_group_view_row")
            .expect("show_group_view_row must exist in preferences_dialog.ui");
        let interpret_multiline_as_markdown_row = builder
            .object::<adw::SwitchRow>("interpret_multiline_as_markdown_row")
            .expect("interpret_multiline_as_markdown_row must exist in preferences_dialog.ui");
        let tree_view_size_row = builder
            .object::<adw::ComboRow>("tree_view_size_row")
            .expect("tree_view_size_row must exist in preferences_dialog.ui");

        autoload_row.set_active(self.setting_boolean("autoload", false));
        show_group_view_row.set_active(self.show_group_view_enabled());
        interpret_multiline_as_markdown_row
            .set_active(self.setting_boolean("interpret-multiline-as-markdown", true));
        tree_view_size_row.set_selected(tree_view_size_index(
            &self.setting_string("tree-view-size", "medium"),
        ));

        autoload_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |row| {
                if !window.settings_has_key("autoload") {
                    window
                        .imp()
                        .entry_view
                        .show_error_dialog("GSettings schema is missing autoload");
                    return;
                }
                let settings = window.settings();
                if let Err(err) = settings.set_boolean("autoload", row.is_active()) {
                    window.imp().entry_view.show_error_dialog(&err.to_string());
                    return;
                }
                if row.is_active() {
                    window.start_autoload_cache();
                } else {
                    window.stop_autoload_cache();
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
                        .show_error_dialog("GSettings schema is missing show-group-view");
                    return;
                }
                let settings = window.settings();
                if let Err(err) = settings.set_boolean("show-group-view", row.is_active()) {
                    window.imp().entry_view.show_error_dialog(&err.to_string());
                    return;
                }
                window.update_selection_layout(true);
                window.rebuild_vault_tree();
                if row.is_active() {
                    window.show_root_group_content();
                }
            }
        ));

        interpret_multiline_as_markdown_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |row| {
                if !window.settings_has_key("interpret-multiline-as-markdown") {
                    window.imp().entry_view.show_error_dialog(
                        "GSettings schema is missing interpret-multiline-as-markdown",
                    );
                    return;
                }
                let settings = window.settings();
                if let Err(err) =
                    settings.set_boolean("interpret-multiline-as-markdown", row.is_active())
                {
                    window.imp().entry_view.show_error_dialog(&err.to_string());
                }
            }
        ));

        tree_view_size_row.connect_selected_notify(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |row| {
                if !window.settings_has_key("tree-view-size") {
                    window
                        .imp()
                        .entry_view
                        .show_error_dialog("GSettings schema is missing tree-view-size");
                    return;
                }
                let nick = match row.selected() {
                    0 => "small",
                    2 => "large",
                    _ => "medium",
                };
                let settings = window.settings();
                if let Err(err) = settings.set_string("tree-view-size", nick) {
                    window.imp().entry_view.show_error_dialog(&err.to_string());
                    return;
                }
                window.apply_tree_view_size_setting();
            }
        ));

        dialog.present(Some(self));
    }
}

fn tree_view_size_index(nick: &str) -> u32 {
    match nick {
        "small" => 0,
        "medium" => 1,
        "large" => 2,
        unknown => {
            // Schema migration / hand-edited GSettings can leave a nick
            // we don't recognise. Snap to medium so the dropdown still
            // shows a sensible default, but leave a breadcrumb in the
            // journal so the misconfiguration isn't completely silent.
            log::debug!(
                "tree_view_size_index: unknown tree-view-size nick {unknown:?}; defaulting to medium"
            );
            1
        }
    }
}
