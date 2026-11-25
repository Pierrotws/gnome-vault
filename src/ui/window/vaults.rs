//! Several vaults: the saved list (GSettings `vaults` / `active-vault`),
//! opening and switching, the header-bar switcher, each vault's settings
//! dialog, and adding an existing folder.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};

use crate::app::controller::AppController;
use crate::app::vaults::{self, VaultConfig, VaultList};
use crate::pass::store::StoreStatus;

use super::MainWindow;

impl MainWindow {
    /// Reads the vault list from settings. The first time (no saved
    /// list yet), the vault Passel used before multiple vaults existed
    /// becomes the first entry, with the global branch and auto-push
    /// settings.
    pub(super) fn load_vault_list(&self) {
        let settings = self.settings();
        let saved = self.settings_has_key("vaults") && settings.user_value("vaults").is_some();
        let list = if saved {
            let vaults = settings
                .value("vaults")
                .iter()
                .filter_map(|item| vault_from_variant(&item))
                .collect();
            let active = Some(self.setting_string("active-vault", ""))
                .filter(|path| !path.is_empty())
                .map(PathBuf::from);
            VaultList::new(vaults, active)
        } else {
            self.migrated_vault_list()
        };
        self.imp().vaults.replace(list);
        if !saved {
            self.save_vault_list();
        }
    }

    fn migrated_vault_list(&self) -> VaultList {
        let mut list = VaultList::default();
        let previous = match crate::pass::store::dev_store_dir() {
            Some(dev_dir) => dev_dir,
            None => Some(self.setting_string("store-dir", ""))
                .filter(|dir| !dir.trim().is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(AppController::default_store_dir),
        };
        if matches!(
            AppController::store_status(&previous),
            StoreStatus::Ready | StoreStatus::PassWithoutGit
        ) {
            let branch = self.setting_string("branch", "");
            let autopush = self.setting_boolean("autopush", true);
            list.add(&previous);
            if let Some(vault) = list.get_mut(&previous) {
                vault.branch = branch;
                vault.autopush = autopush;
            }
            list.set_active(&previous);
        }
        list
    }

    pub(super) fn save_vault_list(&self) {
        if !self.settings_has_key("vaults") {
            log::warn!("GSettings schema is missing vaults; the vault list was not saved");
            return;
        }
        let settings = self.settings();
        let list = self.imp().vaults.borrow().clone();
        let value = glib::Variant::array_from_iter_with_type(
            glib::VariantTy::VARDICT,
            list.vaults().iter().map(vault_to_variant),
        );
        let active = list
            .active()
            .map(|vault| vault.path.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut result = settings
            .set_value("vaults", &value)
            .and_then(|()| settings.set_string("active-vault", &active));
        // Keep the single-vault setting pointing at the active vault, so a
        // downgraded Passel still opens it.
        if result.is_ok() && !active.is_empty() && self.settings_has_key("store-dir") {
            result = settings.set_string("store-dir", &active);
        }
        if let Err(err) = result {
            self.show_error_dialog(&format!("Could not save the vault list: {err}"));
        }
    }

    /// Opens the active vault, or shows why it cannot be opened.
    pub(super) fn open_active_vault(&self) {
        let active = {
            let mut list = self.imp().vaults.borrow_mut();
            if list.active().is_none() {
                if let Some(first) = list.vaults().first().map(|vault| vault.path.clone()) {
                    list.set_active(&first);
                }
            }
            list.active().cloned()
        };
        self.update_vault_switcher();
        let Some(vault) = active else {
            self.close_open_vault();
            self.show_setup_view();
            return;
        };
        match AppController::store_status(&vault.path) {
            StoreStatus::Ready => self.open_vault(&vault),
            StoreStatus::PassWithoutGit if vault.history_declined => self.open_vault(&vault),
            StoreStatus::PassWithoutGit => {
                self.close_open_vault();
                self.show_enable_history_view(&vault.path);
            }
            StoreStatus::Missing | StoreStatus::Empty | StoreStatus::Unrelated => {
                self.close_open_vault();
                self.show_missing_vault_view(&vault);
            }
        }
    }

    fn open_vault(&self, vault: &VaultConfig) {
        self.stop_background_loads();
        let result = self.controller().borrow_mut().open_vault(&vault.path);
        if let Err(err) = result {
            self.close_open_vault();
            self.show_missing_vault_view(vault);
            self.show_error_dialog(&err.to_string());
            return;
        }
        self.apply_vault_settings(vault);
        self.rebuild_vault_tree();
        self.show_opened_vault();
        if let Err(err) = self.reload_changes_view() {
            log::debug!("history unavailable: {err}");
        }
        self.update_vault_switcher();
    }

    fn close_open_vault(&self) {
        self.stop_background_loads();
        self.controller().borrow_mut().close_vault();
    }

    /// Stops decrypting entries of the vault being left: their results
    /// would land in the next vault's cache.
    fn stop_background_loads(&self) {
        self.stop_autoload_cache();
        let imp = self.imp();
        imp.group_preview_generation
            .set(imp.group_preview_generation.get().wrapping_add(1));
        if let Some(previous) = imp.group_preview_cancel.borrow_mut().take() {
            previous.store(true, Ordering::Relaxed);
        }
    }

    fn apply_vault_settings(&self, vault: &VaultConfig) {
        let controller = self.controller();
        let mut controller = controller.borrow_mut();
        controller.set_autopush(vault.autopush);
        controller.set_branch(vault.branch());
    }

    /// Switches to another listed vault.
    pub(super) fn switch_to_vault(&self, path: &Path) {
        let path = path.to_path_buf();
        self.discard_changes_then(move |window| {
            if !window.imp().vaults.borrow_mut().set_active(&path) {
                return;
            }
            window.save_vault_list();
            window.open_active_vault();
        });
    }

    /// Adds the vault the setup wizard created and opens it. Returns
    /// whether it is now open.
    pub(super) fn add_created_vault(&self, path: &Path) -> bool {
        {
            let mut list = self.imp().vaults.borrow_mut();
            list.add(path);
            list.set_active(path);
        }
        self.save_vault_list();
        self.open_active_vault();
        self.controller()
            .borrow()
            .store_dir()
            .is_ok_and(|open| open == path)
    }

    /// Answer "Not Now" to turning on history: open without it, and
    /// remember the answer for this vault.
    pub(super) fn decline_history(&self) {
        let Some(path) = self.active_vault_path() else {
            return;
        };
        if let Some(vault) = self.imp().vaults.borrow_mut().get_mut(&path) {
            vault.history_declined = true;
        }
        self.save_vault_list();
        self.open_active_vault();
    }

    pub(super) fn enable_history_for_active_vault(&self) {
        let Some(path) = self.active_vault_path() else {
            return;
        };
        match AppController::enable_history_at(&path) {
            Ok(()) => self.open_active_vault(),
            Err(err) => self.show_error_dialog(&err.to_string()),
        }
    }

    fn active_vault_path(&self) -> Option<PathBuf> {
        self.imp()
            .vaults
            .borrow()
            .active()
            .map(|vault| vault.path.clone())
    }

    pub(super) fn remove_active_vault_from_list(&self) {
        if let Some(path) = self.active_vault_path() {
            self.remove_vault_from_list(&path);
        }
    }

    fn remove_vault_from_list(&self, path: &Path) {
        if self.active_vault_path().as_deref() == Some(path) {
            let path = path.to_path_buf();
            self.discard_changes_then(move |window| window.remove_listed_vault(&path));
        } else {
            self.remove_listed_vault(path);
        }
    }

    fn remove_listed_vault(&self, path: &Path) {
        let was_active = self.active_vault_path().as_deref() == Some(path);
        self.imp().vaults.borrow_mut().remove(path);
        self.save_vault_list();
        if was_active {
            self.open_active_vault();
        } else {
            self.update_vault_switcher();
        }
    }

    fn show_missing_vault_view(&self, vault: &VaultConfig) {
        let imp = self.imp();
        let description = format!(
            "<b>{}</b> was not found, or holds no vault. If it is on a drive, plug it in and \
             switch to this vault again.",
            glib::markup_escape_text(&vault.path.to_string_lossy())
        );
        imp.missing_vault_page.set_description(Some(&description));
        imp.main_stack.set_visible_child_name("missing");
        self.disable_vault_actions();
        imp.window_title.set_subtitle("Folder not found");
    }

    pub(super) fn disable_vault_actions(&self) {
        let imp = self.imp();
        imp.new_entry_button.set_sensitive(false);
        imp.changes_button.set_sensitive(false);
        imp.window_title.set_subtitle("");
    }

    /// Without history there is nothing to show in Changes: the area
    /// (button and separator) is hidden, and the changes page is left
    /// if it was showing.
    pub(super) fn update_history_controls(&self) {
        let has_history = self.controller().borrow().has_history();
        let imp = self.imp();
        imp.changes_area.set_visible(has_history);
        if !has_history {
            imp.changes_badge.set_visible(false);
            self.update_vault_subtitle(0);
            if imp.content_stack.visible_child_name().as_deref() == Some("changes") {
                imp.entry_view.display_empty();
                if self.show_group_view_enabled() {
                    self.show_root_group_content();
                } else {
                    self.show_empty_content();
                }
            }
        }
    }

    /// The header's subtitle shows the open vault's state only, and
    /// nothing when all is well. `unpushed` is the number of commits not
    /// pushed yet.
    pub(super) fn update_vault_subtitle(&self, unpushed: usize) {
        let subtitle = if !self.controller().borrow().has_history() {
            "No history".to_string()
        } else {
            match unpushed {
                0 => String::new(),
                1 => "1 change not pushed".to_string(),
                n => format!("{n} changes not pushed"),
            }
        };
        self.imp().window_title.set_subtitle(&subtitle);
    }

    /// Rebuilds the switcher's list and the window title.
    pub(super) fn update_vault_switcher(&self) {
        let imp = self.imp();
        while let Some(row) = imp.vault_list.first_child() {
            imp.vault_list.remove(&row);
        }
        let list = imp.vaults.borrow().clone();
        let active = list.active().map(|vault| vault.path.clone());
        imp.window_title
            .set_title(list.active().map_or("Passel", |vault| vault.name.as_str()));
        imp.vault_list.set_visible(!list.is_empty());
        imp.import_button
            .set_sensitive(self.controller().borrow().store_dir().is_ok());

        for vault in list.vaults() {
            let info = AppController::vault_info(&vault.path);
            let row = adw::ActionRow::new();
            row.set_use_markup(false);
            row.set_title(&vault.name);
            let state = if !matches!(
                info.status,
                StoreStatus::Ready | StoreStatus::PassWithoutGit
            ) {
                "not found"
            } else if !info.has_history {
                "no history"
            } else if info.remote_url.is_some() {
                "history, synced"
            } else {
                "history"
            };
            row.set_subtitle(&format!("{} · {state}", short_path(&vault.path)));
            row.set_subtitle_lines(1);
            row.set_tooltip_text(Some(&vault.path.to_string_lossy()));

            let check = gtk::Image::from_icon_name("object-select-symbolic");
            check.set_opacity(if active.as_ref() == Some(&vault.path) {
                1.0
            } else {
                0.0
            });
            row.add_prefix(&check);

            let settings_button = gtk::Button::from_icon_name("emblem-system-symbolic");
            settings_button.set_tooltip_text(Some("Vault Settings"));
            settings_button.set_valign(gtk::Align::Center);
            settings_button.add_css_class("flat");
            let path = vault.path.clone();
            settings_button.connect_clicked(glib::clone!(
                #[weak(rename_to = window)]
                self,
                move |_| {
                    window.imp().vault_popover.popdown();
                    window.show_vault_settings(&path);
                }
            ));
            row.add_suffix(&settings_button);

            row.set_activatable(true);
            let path = vault.path.clone();
            row.connect_activated(glib::clone!(
                #[weak(rename_to = window)]
                self,
                move |_| {
                    window.imp().vault_popover.popdown();
                    window.switch_to_vault(&path);
                }
            ));
            imp.vault_list.append(&row);
        }
    }

    /// Folder suggested for a new vault: pass's default location (the dev
    /// store in debug builds), or a numbered sibling when it is taken.
    pub(super) fn default_new_vault_dir(&self) -> PathBuf {
        let base =
            crate::pass::store::dev_store_dir().unwrap_or_else(AppController::default_store_dir);
        let list = self.imp().vaults.borrow();
        vaults::unused_vault_dir(&list, &base, |path| {
            !matches!(
                AppController::store_status(path),
                StoreStatus::Missing | StoreStatus::Empty
            )
        })
    }

    /// Lets the user pick a folder holding a vault and adds it. An empty
    /// folder opens the wizard on it instead.
    pub(super) fn add_existing_vault(&self) {
        let dialog = gtk::FileDialog::builder()
            .title("Open a Vault Folder")
            .modal(true)
            .build();
        dialog.select_folder(
            Some(self),
            gio::Cancellable::NONE,
            glib::clone!(
                #[weak(rename_to = window)]
                self,
                move |result| {
                    if let Some(path) = result.ok().and_then(|folder| folder.path()) {
                        window.add_vault_folder(&path);
                    }
                }
            ),
        );
    }

    fn add_vault_folder(&self, path: &Path) {
        match AppController::store_status(path) {
            StoreStatus::Ready | StoreStatus::PassWithoutGit => {
                let path = path.to_path_buf();
                self.discard_changes_then(move |window| {
                    {
                        let mut list = window.imp().vaults.borrow_mut();
                        list.add(&path);
                        list.set_active(&path);
                    }
                    window.save_vault_list();
                    window.open_active_vault();
                });
            }
            StoreStatus::Missing | StoreStatus::Empty => {
                let path = path.to_path_buf();
                self.discard_changes_then(move |window| window.present_setup_wizard_in(&path));
            }
            StoreStatus::Unrelated => self.show_error_dialog(
                "This folder does not hold a vault. Choose a folder created by Passel or pass, \
                 or an empty folder to create a new vault in it.",
            ),
        }
    }

    /// Settings of one vault: name, history, remote, branch, auto-push,
    /// and removing it from the list.
    pub(super) fn show_vault_settings(&self, path: &Path) {
        let Some(vault) = self.imp().vaults.borrow().get(path).cloned() else {
            return;
        };
        let info = AppController::vault_info(path);

        let dialog = adw::PreferencesDialog::builder()
            .title(format!("{} Settings", vault.name))
            .build();
        let page = adw::PreferencesPage::new();
        dialog.add(&page);

        // Vault
        let vault_group = adw::PreferencesGroup::new();
        let name_row = adw::EntryRow::builder()
            .title("Name")
            .text(vault.name.as_str())
            .show_apply_button(true)
            .build();
        let path_buf = path.to_path_buf();
        name_row.connect_apply(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[strong]
            path_buf,
            move |row| {
                let name = row.text().trim().to_string();
                if name.is_empty() {
                    return;
                }
                if let Some(vault) = window.imp().vaults.borrow_mut().get_mut(&path_buf) {
                    vault.name = name;
                }
                window.save_vault_list();
                window.update_vault_switcher();
            }
        ));
        vault_group.add(&name_row);
        let folder_row = adw::ActionRow::builder()
            .title("Folder")
            .subtitle(path.to_string_lossy().as_ref())
            .use_markup(false)
            .subtitle_selectable(true)
            .build();
        vault_group.add(&folder_row);
        page.add(&vault_group);

        // History
        let history_group = adw::PreferencesGroup::builder().title("History").build();
        let history_row = adw::ActionRow::builder().title("History").build();
        if info.has_history {
            history_row.set_subtitle("On: every change is recorded and can be undone.");
        } else {
            history_row.set_subtitle("Off: changes are saved but not recorded.");
            if info.status == StoreStatus::PassWithoutGit {
                let turn_on = gtk::Button::with_label("Turn On");
                turn_on.set_valign(gtk::Align::Center);
                turn_on.add_css_class("suggested-action");
                turn_on.connect_clicked(glib::clone!(
                    #[weak(rename_to = window)]
                    self,
                    #[weak]
                    dialog,
                    #[strong]
                    path_buf,
                    move |_| {
                        if let Err(err) = AppController::enable_history_at(&path_buf) {
                            window.show_error_dialog(&err.to_string());
                            return;
                        }
                        if let Some(vault) = window.imp().vaults.borrow_mut().get_mut(&path_buf) {
                            vault.history_declined = false;
                        }
                        window.save_vault_list();
                        if window.active_vault_path().as_deref() == Some(path_buf.as_path()) {
                            window.open_active_vault();
                        } else {
                            window.update_vault_switcher();
                        }
                        dialog.close();
                        window.show_vault_settings(&path_buf);
                    }
                ));
                history_row.add_suffix(&turn_on);
            }
        }
        history_group.add(&history_row);
        page.add(&history_group);

        // Sync (needs history)
        if info.has_history {
            let sync_group = adw::PreferencesGroup::builder()
                .title("Sync")
                .description("Leave the address empty to keep this vault on this computer only.")
                .build();
            let remote_row = adw::EntryRow::builder()
                .title("Remote address")
                .text(info.remote_url.clone().unwrap_or_default().as_str())
                .show_apply_button(true)
                .build();
            remote_row.connect_apply(glib::clone!(
                #[weak(rename_to = window)]
                self,
                #[strong]
                path_buf,
                move |row| {
                    let address = row.text().trim().to_string();
                    let address = (!address.is_empty()).then_some(address.as_str());
                    match AppController::set_vault_remote(&path_buf, address) {
                        Ok(()) => window.update_vault_switcher(),
                        Err(err) => window.show_error_dialog(&err.to_string()),
                    }
                }
            ));
            sync_group.add(&remote_row);

            let branch_row = adw::EntryRow::builder()
                .title("Branch (empty: current branch)")
                .text(vault.branch.as_str())
                .show_apply_button(true)
                .build();
            branch_row.connect_apply(glib::clone!(
                #[weak(rename_to = window)]
                self,
                #[strong]
                path_buf,
                move |row| {
                    let branch = row.text().trim().to_string();
                    window.update_vault_config(&path_buf, |vault| vault.branch = branch);
                }
            ));
            sync_group.add(&branch_row);

            let autopush_row = adw::SwitchRow::builder()
                .title("Push changes automatically")
                .subtitle("Upload after every saved change")
                .active(vault.autopush)
                .build();
            autopush_row.connect_active_notify(glib::clone!(
                #[weak(rename_to = window)]
                self,
                #[strong]
                path_buf,
                move |row| {
                    let autopush = row.is_active();
                    window.update_vault_config(&path_buf, |vault| vault.autopush = autopush);
                }
            ));
            sync_group.add(&autopush_row);
            page.add(&sync_group);
        }

        // Remove
        let remove_group = adw::PreferencesGroup::new();
        let remove_row = adw::ActionRow::builder()
            .title("Remove from List")
            .subtitle("The folder and its entries stay on disk.")
            .build();
        let remove_button = gtk::Button::with_label("Remove");
        remove_button.set_valign(gtk::Align::Center);
        remove_button.add_css_class("destructive-action");
        remove_button.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            #[weak]
            dialog,
            #[strong]
            path_buf,
            move |_| {
                dialog.close();
                window.remove_vault_from_list(&path_buf);
            }
        ));
        remove_row.add_suffix(&remove_button);
        remove_group.add(&remove_row);
        page.add(&remove_group);

        dialog.present(Some(self));
    }

    /// Changes a vault's settings, saves them, and applies them right
    /// away when it is the open vault.
    fn update_vault_config(&self, path: &Path, change: impl FnOnce(&mut VaultConfig)) {
        let updated = {
            let mut list = self.imp().vaults.borrow_mut();
            let Some(vault) = list.get_mut(path) else {
                return;
            };
            change(vault);
            vault.clone()
        };
        self.save_vault_list();
        if self.active_vault_path().as_deref() == Some(path) {
            self.apply_vault_settings(&updated);
            if let Err(err) = self.reload_changes_view() {
                log::debug!("history unavailable: {err}");
            }
        }
    }
}

/// Longest path shown in the switcher, in characters.
const SHORT_PATH_MAX: usize = 40;

/// A vault folder for the switcher: `~` for the home folder, and only the
/// end of a long path (the full path is in the row's tooltip).
fn short_path(path: &Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let shown = match home
        .as_deref()
        .and_then(|home| path.strip_prefix(home).ok())
    {
        Some(rest) => Path::new("~").join(rest).to_string_lossy().into_owned(),
        None => path.to_string_lossy().into_owned(),
    };
    let count = shown.chars().count();
    if count <= SHORT_PATH_MAX {
        return shown;
    }
    let tail: String = shown.chars().skip(count - (SHORT_PATH_MAX - 1)).collect();
    format!("…{tail}")
}

fn vault_to_variant(vault: &VaultConfig) -> glib::Variant {
    let dict = glib::VariantDict::new(None);
    dict.insert("name", vault.name.as_str());
    dict.insert("path", vault.path.to_string_lossy().as_ref());
    dict.insert("history-declined", vault.history_declined);
    dict.insert("branch", vault.branch.as_str());
    dict.insert("autopush", vault.autopush);
    dict.end()
}

fn vault_from_variant(value: &glib::Variant) -> Option<VaultConfig> {
    let dict = glib::VariantDict::new(Some(value));
    let path: String = dict.lookup("path").ok().flatten()?;
    if path.is_empty() {
        return None;
    }
    let path = PathBuf::from(path);
    let name: String = dict
        .lookup("name")
        .ok()
        .flatten()
        .unwrap_or_else(|| vaults::suggested_name(&path));
    let mut vault = VaultConfig::new(name, path);
    vault.history_declined = dict
        .lookup("history-declined")
        .ok()
        .flatten()
        .unwrap_or(false);
    vault.branch = dict.lookup("branch").ok().flatten().unwrap_or_default();
    vault.autopush = dict.lookup("autopush").ok().flatten().unwrap_or(true);
    Some(vault)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortens_long_vault_paths() {
        assert_eq!(short_path(Path::new("/srv/vault")), "/srv/vault");
        let long = Path::new("/media/usb/some/deeply/nested/folder/with/a/vault/inside");
        let shown = short_path(long);
        assert_eq!(shown.chars().count(), SHORT_PATH_MAX);
        assert!(shown.starts_with('…') && shown.ends_with("vault/inside"));
    }

    #[test]
    fn vault_settings_round_trip_through_a_variant() {
        let mut vault = VaultConfig::new("Work", "/home/a/work");
        vault.history_declined = true;
        vault.branch = "main".to_string();
        vault.autopush = false;
        assert_eq!(vault_from_variant(&vault_to_variant(&vault)), Some(vault));
    }

    #[test]
    fn reads_partial_saved_vaults_with_defaults() {
        let dict = glib::VariantDict::new(None);
        dict.insert("path", "/home/a/.password-store");
        let vault = vault_from_variant(&dict.end()).unwrap();
        assert_eq!(vault.name, "Passwords");
        assert!(vault.autopush);
        assert!(!vault.history_declined);

        let no_path = glib::VariantDict::new(None);
        no_path.insert("name", "Lost");
        assert_eq!(vault_from_variant(&no_path.end()), None);
    }
}
