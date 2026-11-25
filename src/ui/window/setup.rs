//! Vault setup glue: the setup wizard's dialog (see
//! [`crate::ui::setup_wizard`]) with the "No Vault Yet" page behind it,
//! and the "turn on history" page. The vault list itself lives in
//! [`super::vaults`].

use std::path::Path;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;

use crate::ui::setup_wizard::SetupWizard;
use crate::ui::vault_view::{build_tree_factory, TreeViewSize};

use super::MainWindow;

impl MainWindow {
    /// Re-installs the vault-tree factory using the configured row size.
    /// Safe to call repeatedly: replacing the factory triggers GTK to
    /// rebuild visible rows with the new padding.
    pub(super) fn apply_tree_view_size_setting(&self) {
        let size = self.tree_view_size_setting();
        self.imp()
            .vault_view
            .set_factory(Some(&build_tree_factory(size)));
    }

    pub(super) fn tree_view_size_setting(&self) -> TreeViewSize {
        if !self.settings_has_key("tree-view-size") {
            return TreeViewSize::Medium;
        }
        let value = self.settings().string("tree-view-size");
        TreeViewSize::from_setting(&value)
    }

    /// Opens the setup wizard in a dialog, on a suggested new folder.
    pub(super) fn present_setup_wizard(&self) {
        self.present_setup_wizard_in(&self.default_new_vault_dir());
    }

    /// Opens the setup wizard in a dialog on `store_dir` (once: a second
    /// call while it is open does nothing).
    pub(super) fn present_setup_wizard_in(&self, store_dir: &Path) {
        let imp = self.imp();
        if imp.setup_dialog.upgrade().is_some() {
            return;
        }
        let wizard = SetupWizard::new();
        // New vaults push automatically when they have somewhere to push.
        wizard.reset(store_dir, true);

        let dialog = adw::Dialog::builder()
            .title("New Vault")
            .content_width(620)
            .content_height(720)
            .child(&wizard)
            .build();
        wizard.connect_locked_changed(glib::clone!(
            #[weak]
            dialog,
            move |_, locked| dialog.set_can_close(!locked)
        ));
        wizard.connect_vault_created(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[weak]
            dialog,
            move |_, store_dir| {
                if window.add_created_vault(&store_dir) {
                    dialog.force_close();
                }
            }
        ));
        imp.setup_dialog.set(Some(&dialog));
        dialog.present(Some(self));
    }

    /// Shows the "No Vault Yet" page and opens the wizard over it.
    pub(super) fn show_setup_view(&self) {
        let imp = self.imp();
        imp.main_stack.set_visible_child_name("setup");
        // At startup the window is not shown yet: a dialog needs a mapped
        // parent, so open it once the main loop runs.
        glib::idle_add_local_once(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move || window.present_setup_wizard()
        ));
        self.disable_vault_actions();
    }

    /// Shown when a vault folder holds a pass store that git does not
    /// track yet, and the user has not answered for it: turn on history,
    /// or open it without.
    pub(super) fn show_enable_history_view(&self, store_dir: &Path) {
        let imp = self.imp();
        let description = format!(
            "<b>{}</b> holds a password store whose changes are not recorded. \
             Passel can keep a history of every change so you can undo it and sync it with a \
             remote. Your entries stay as they are.",
            glib::markup_escape_text(&store_dir.to_string_lossy())
        );
        imp.enable_history_page.set_description(Some(&description));
        imp.main_stack.set_visible_child_name("enable-history");
        self.disable_vault_actions();
    }
}
