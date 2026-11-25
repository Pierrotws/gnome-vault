//! Vault setup glue: GSettings → env propagation, the first-run wizard
//! (see [`crate::ui::setup_wizard`]) and the "turn on history" page.

use std::path::{Path, PathBuf};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;

use crate::app::app_error::AppError;
use crate::app::controller::AppController;
use crate::pass::store::StoreError;
use crate::ui::vault_view::{build_tree_factory, TreeViewSize};

use super::MainWindow;

impl MainWindow {
    pub(super) fn apply_store_dir_setting(&self) {
        // Debug builds are pinned to the dev store in main().
        if crate::pass::store::dev_store_dir().is_some() {
            return;
        }
        if !self.settings_has_key("store-dir") {
            if std::env::var_os("PASSWORD_STORE_DIR").is_none() {
                log::warn!(
                    "GSettings schema is missing store-dir; using PASSWORD_STORE_DIR fallback"
                );
            }
            return;
        }

        // A non-empty configured store-dir is authoritative — it wins over
        // any pre-existing PASSWORD_STORE_DIR so that changing the setting
        // immediately retargets store ops at startup. Routed through
        // set_password_store_dir_once so the setter only runs while the
        // process is still single-threaded; the preferences dialog
        // already tells the user a restart is required for changes.
        let store_dir = self.settings().string("store-dir");
        if !store_dir.trim().is_empty() {
            crate::pass::store::set_password_store_dir_once(std::path::Path::new(
                store_dir.as_str(),
            ));
        }
    }

    pub(super) fn apply_autopush_setting(&self) {
        let autopush = self.settings().boolean("autopush");
        self.controller().borrow_mut().set_autopush(autopush);
    }

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

    pub(super) fn apply_branch_setting(&self) {
        if !self.settings_has_key("branch") {
            return;
        }
        let raw = self.settings().string("branch").to_string();
        let branch = raw.trim();
        let value = if branch.is_empty() {
            None
        } else {
            Some(branch.to_string())
        };
        self.controller().borrow_mut().set_branch(value);
        // The unpushed-count badge is computed against this branch
        // override, but we do NOT refresh it here — the caller owns the
        // refresh so the preferences-apply path doesn't double-refresh
        // (it follows up with reload_changes_view, which itself refreshes
        // the badge as part of reloading the change list).
    }

    pub(super) fn setup_vault_setup_view(&self) {
        // Debug builds ignore the shared store-dir setting, which names
        // the installed app's real vault.
        let configured_store_dir = if self.settings_has_key("store-dir")
            && crate::pass::store::dev_store_dir().is_none()
        {
            self.settings().string("store-dir").to_string()
        } else {
            String::new()
        };
        let default_store_dir = if configured_store_dir.trim().is_empty() {
            AppController::default_store_dir()
        } else {
            PathBuf::from(&configured_store_dir)
        };
        let autopush = self.controller().borrow().autopush();
        self.imp().setup_wizard.reset(&default_store_dir, autopush);
    }

    pub(super) fn show_setup_view(&self) {
        let imp = self.imp();
        imp.main_stack.set_visible_child_name("setup");
        imp.new_entry_button.set_sensitive(false);
        imp.lock_vault_button.set_sensitive(false);
        imp.changes_button.set_sensitive(false);
        imp.window_title.set_subtitle("Setup");
    }

    /// Shown when the vault folder holds a pass store that git does not
    /// track yet: offers to put it under git instead of creating a vault.
    pub(super) fn show_enable_history_view(&self) {
        let imp = self.imp();
        let store_dir = AppController::default_store_dir();
        let description = format!(
            "<b>{}</b> already holds a password store, but its changes are not tracked yet. \
             Passel keeps a history of every change so you can undo it and sync it with a remote. \
             Your entries stay as they are.",
            glib::markup_escape_text(&store_dir.to_string_lossy())
        );
        imp.enable_history_page.set_description(Some(&description));
        imp.main_stack.set_visible_child_name("enable-history");
        imp.new_entry_button.set_sensitive(false);
        imp.lock_vault_button.set_sensitive(false);
        imp.changes_button.set_sensitive(false);
        imp.window_title.set_subtitle("Setup");
    }

    pub(super) fn enable_history_from_setup(&self) {
        let result = self.controller().borrow_mut().enable_history();
        match result {
            Ok(()) => {
                self.rebuild_vault_tree();
                self.show_opened_vault();
            }
            Err(err) => self.show_error_dialog(&err.to_string()),
        }
    }

    /// Opens the vault the setup wizard just created at `store_dir`.
    pub(super) fn open_created_vault(&self, store_dir: &Path) {
        let result = self.controller().borrow_mut().open_new_vault(store_dir);
        match result {
            Ok(()) => {
                if let Err(err) = self.persist_store_dir_setting(store_dir) {
                    self.show_error_dialog(&err);
                    return;
                }
                self.rebuild_vault_tree();
                if let Err(err) = self.reload_changes_view() {
                    self.show_error_dialog(&err.to_string());
                }
                self.show_opened_vault();
            }
            // The on-disk vault was created successfully but the running
            // process can no longer retarget PASSWORD_STORE_DIR. Persist
            // the new path to GSettings so the next launch loads it, and
            // tell the user to restart. We deliberately stay on the
            // setup view — switching to the app view would show the
            // *previous* vault's tree, which would confuse the user
            // about whether the wizard succeeded.
            Err(AppError::Save(StoreError::RestartRequired)) => {
                if let Err(err) = self.persist_store_dir_setting(store_dir) {
                    self.show_error_dialog(&err);
                    return;
                }
                self.show_error_dialog(&StoreError::RestartRequired.to_string());
            }
            Err(err) => self.show_error_dialog(&err.to_string()),
        }
    }

    /// Writes the chosen vault path to GSettings so the next launch
    /// resolves to it. Returns the user-facing error message on failure
    /// (logs a warning if the schema is missing the key, in which case
    /// persistence is silently skipped — older builds without the
    /// schema definition still get a working vault for the current
    /// session).
    fn persist_store_dir_setting(&self, store_dir: &Path) -> Result<(), String> {
        // GSettings is shared with the installed app; a debug build must
        // not retarget it at the dev store.
        if crate::pass::store::dev_store_dir().is_some() {
            return Ok(());
        }
        if !self.settings_has_key("store-dir") {
            log::warn!("GSettings schema is missing store-dir; vault path was not persisted");
            return Ok(());
        }
        self.settings()
            .set_string("store-dir", &store_dir.to_string_lossy())
            .map_err(|err| err.to_string())
    }
}
