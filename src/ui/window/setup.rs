//! Vault setup wizard: GSettings → env propagation, recipient lookup, and
//! the first-run "create a vault here" form.

use std::path::PathBuf;

use adw::prelude::*;
use adw::subclass::prelude::*;

use crate::pass::store::{self, VaultSetup};

use super::{MainWindow, SETUP_PROVIDER_NONE};

impl MainWindow {
    pub(super) fn apply_store_dir_setting(&self) {
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
        // in preferences immediately retargets subsequent store ops.
        let store_dir = self.settings().string("store-dir");
        if !store_dir.trim().is_empty() {
            std::env::set_var("PASSWORD_STORE_DIR", store_dir.as_str());
        }
    }

    pub(super) fn apply_autopush_setting(&self) {
        let autopush = self.settings().boolean("autopush");
        self.controller().borrow_mut().set_autopush(autopush);
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
    }

    pub(super) fn setup_vault_setup_view(&self) {
        let imp = self.imp();
        let provider_model = gtk::StringList::new(&["No Sync", "GitHub", "GitLab", "Custom"]);
        imp.setup_provider_row.set_model(Some(&provider_model));
        imp.setup_provider_row.set_selected(SETUP_PROVIDER_NONE);
        imp.setup_remote_row.set_sensitive(false);
        self.setup_recipient_model();

        let configured_store_dir = if self.settings_has_key("store-dir") {
            self.settings().string("store-dir").to_string()
        } else {
            String::new()
        };
        let default_store_dir = if configured_store_dir.trim().is_empty() {
            store::password_store_dir()
        } else {
            PathBuf::from(&configured_store_dir)
        };
        imp.setup_path_row
            .set_text(&default_store_dir.to_string_lossy());
        self.update_create_vault_button();
    }

    pub(super) fn show_setup_view(&self) {
        let imp = self.imp();
        imp.main_stack.set_visible_child_name("setup");
        imp.new_entry_button.set_sensitive(false);
        imp.lock_vault_button.set_sensitive(false);
        imp.changes_button.set_sensitive(false);
        imp.window_title.set_subtitle("Setup");
    }

    pub(super) fn update_create_vault_button(&self) {
        let imp = self.imp();
        let has_path = !imp.setup_path_row.text().trim().is_empty();
        let has_recipient = imp
            .setup_recipients
            .borrow()
            .get(imp.setup_recipient_row.selected() as usize)
            .is_some();
        let needs_remote = imp.setup_provider_row.selected() != SETUP_PROVIDER_NONE;
        let has_remote = !imp.setup_remote_row.text().trim().is_empty();

        imp.create_vault_button
            .set_sensitive(has_path && has_recipient && (!needs_remote || has_remote));
    }

    pub(super) fn create_vault_from_setup(&self) {
        let imp = self.imp();
        let store_dir = PathBuf::from(imp.setup_path_row.text().trim());
        let Some(recipient) = imp
            .setup_recipients
            .borrow()
            .get(imp.setup_recipient_row.selected() as usize)
            .map(|recipient| recipient.id.clone())
        else {
            self.show_error_dialog("Select a GPG recipient before creating the vault");
            return;
        };
        let remote_url = (imp.setup_provider_row.selected() != SETUP_PROVIDER_NONE)
            .then(|| imp.setup_remote_row.text().trim().to_string())
            .filter(|url| !url.is_empty());

        let setup = VaultSetup {
            store_dir: store_dir.clone(),
            recipient,
            remote_url,
            autopush: self.controller().borrow().autopush(),
        };

        std::env::set_var("PASSWORD_STORE_DIR", &store_dir);

        let setup_result = {
            let controller = self.controller();
            let mut controller = controller.borrow_mut();
            controller.setup_vault(&setup)
        };

        match setup_result {
            Ok(()) => {
                if self.settings_has_key("store-dir") {
                    if let Err(err) = self
                        .settings()
                        .set_string("store-dir", &store_dir.to_string_lossy())
                    {
                        self.show_error_dialog(&err.to_string());
                        return;
                    }
                } else {
                    log::warn!(
                        "GSettings schema is missing store-dir; vault path was not persisted"
                    );
                }
                self.rebuild_vault_tree();
                if let Err(err) = self.reload_changes_view() {
                    self.show_error_dialog(&err.to_string());
                }
                imp.entry_view.display_empty();
                self.show_empty_content();
                self.set_edit_unlock_state(false, false, false);
                self.show_app_view();
                self.start_autoload_if_enabled();
            }
            Err(err) => self.show_error_dialog(&err.to_string()),
        }
    }

    pub(super) fn setup_recipient_model(&self) {
        let imp = self.imp();
        let recipients = match self.controller().borrow().available_recipients() {
            Ok(recipients) => recipients,
            Err(err) => {
                log::warn!("Failed to list GPG recipients: {err}");
                Vec::new()
            }
        };

        if recipients.is_empty() {
            let model = gtk::StringList::new(&[]);
            imp.setup_recipient_row.set_model(Some(&model));
            imp.setup_recipient_row
                .set_selected(gtk::INVALID_LIST_POSITION);
            imp.setup_recipient_row
                .set_subtitle("No usable local secret encryption key found");
            imp.setup_recipient_row.set_sensitive(false);
        } else {
            let labels = recipients
                .iter()
                .map(|recipient| recipient.label.as_str())
                .collect::<Vec<_>>();
            let model = gtk::StringList::new(&labels);
            imp.setup_recipient_row.set_model(Some(&model));
            imp.setup_recipient_row.set_selected(0);
            imp.setup_recipient_row.set_subtitle("");
            imp.setup_recipient_row.set_sensitive(true);
        }

        imp.setup_recipients.replace(recipients);
        self.update_create_vault_button();
    }
}
