//! The wizard's Google Drive page: connects without asking when this
//! computer is already signed in ("Sign In with Google" only when Google
//! needs the user), then lists the vaults to download, or notes the ones
//! already there when creating a new one.

use std::sync::atomic::Ordering;

use adw::{prelude::*, subclass::prelude::*};
use gtk::{gio, glib};

use super::SetupWizard;
use crate::app::drive::{DriveApi, DriveError, DriveFile};
use crate::helpers::google;
use crate::i18n::{gettext, gettext_f, ngettext_f};

impl SetupWizard {
    pub(super) fn setup_drive(&self) {
        let imp = self.imp();
        imp.drive_status_label.set_use_markup(false);
        if !google::is_available() {
            imp.drive_row.set_sensitive(false);
            imp.drive_row
                .set_subtitle(&gettext("Google Drive is not available in this build"));
        }
        imp.drive_sign_in_button.connect_clicked(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| wizard.drive_sign_in()
        ));
        imp.drive_next_button.connect_clicked(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| wizard.go_to_review()
        ));
        imp.drive_back_button.connect_clicked(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| {
                wizard.cancel_drive_sign_in();
                wizard.imp().navigation.pop();
            }
        ));
        imp.drive_switch_button.connect_clicked(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| {
                let download = !wizard.imp().download.get();
                wizard.set_mode(download);
                wizard.update_drive_page();
            }
        ));
    }

    pub(super) fn reset_drive(&self) {
        let imp = self.imp();
        self.cancel_drive_sign_in();
        imp.drive_file.replace(None);
        imp.drive_error.replace(None);
    }

    /// Shows the Drive page, connecting first when not connected yet.
    pub(super) fn enter_drive_page(&self) {
        let imp = self.imp();
        imp.navigation.push_by_tag("drive");
        if imp.drive_vaults.borrow().is_none() && !imp.drive_busy.get() {
            self.connect_drive();
        }
        self.update_drive_page();
    }

    /// Connects with the sign-in this computer keeps, without asking.
    fn connect_drive(&self) {
        if !google::is_available() {
            return;
        }
        self.drive_task(|| google::connect()?.vaults());
    }

    /// Signs in through the browser, then lists the vaults.
    fn drive_sign_in(&self) {
        let imp = self.imp();
        let sign_in = match google::begin_sign_in() {
            Ok(sign_in) => sign_in,
            Err(err) => {
                imp.drive_error.replace(Some(err.to_string()));
                self.update_drive_page();
                return;
            }
        };
        imp.drive_cancel.replace(Some(sign_in.cancel_flag()));
        self.open_link(&sign_in.url);
        self.drive_task(move || {
            sign_in.finish()?;
            google::connect()?.vaults()
        });
    }

    fn cancel_drive_sign_in(&self) {
        if let Some(cancel) = self.imp().drive_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Runs `task` (it blocks) off the main loop, then shows its vaults.
    fn drive_task(
        &self,
        task: impl FnOnce() -> Result<Vec<DriveFile>, DriveError> + Send + 'static,
    ) {
        let imp = self.imp();
        imp.drive_busy.set(true);
        imp.drive_error.replace(None);
        self.update_drive_page();
        let wizard = self.clone();
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(task)
                .await
                .unwrap_or(Err(DriveError::Unreachable));
            let imp = wizard.imp();
            imp.drive_busy.set(false);
            imp.drive_cancel.replace(None);
            match result {
                Ok(vaults) => {
                    imp.drive_vaults.replace(Some(vaults));
                }
                // Not signed in yet: the button says what to do.
                Err(DriveError::SignIn) => {}
                Err(err) => {
                    imp.drive_error.replace(Some(err.to_string()));
                }
            }
            wizard.update_drive_page();
        });
    }

    pub(super) fn update_drive_page(&self) {
        let imp = self.imp();
        let download = imp.download.get();
        let busy = imp.drive_busy.get();
        let waiting_browser = imp.drive_cancel.borrow().is_some();
        let vaults = imp.drive_vaults.borrow().clone();
        let error = imp.drive_error.borrow().clone();

        imp.drive_spinner.set_visible(busy);
        imp.drive_spinner.set_spinning(busy);
        for class in ["success", "error"] {
            imp.drive_icon.remove_css_class(class);
        }
        let (status, icon) = if !google::is_available() {
            (
                gettext("Google Drive is not available in this build"),
                Some(("dialog-error-symbolic", "error")),
            )
        } else if waiting_browser {
            (
                gettext("Continue in your browser, then come back here."),
                None,
            )
        } else if busy {
            (gettext("Connecting to Google Drive…"), None)
        } else if let Some(error) = &error {
            (error.clone(), Some(("dialog-error-symbolic", "error")))
        } else if vaults.is_some() {
            (
                gettext("Connected to Google Drive"),
                Some(("object-select-symbolic", "success")),
            )
        } else {
            (gettext("Sign in to use your Google Drive."), None)
        };
        imp.drive_status_label.set_label(&status);
        imp.drive_icon.set_visible(!busy && icon.is_some());
        if let Some((name, class)) = icon {
            imp.drive_icon.set_icon_name(Some(name));
            imp.drive_icon.add_css_class(class);
        }
        imp.drive_sign_in_button
            .set_visible(google::is_available() && !busy && vaults.is_none());

        for row in imp.drive_rows.take() {
            imp.drive_vaults_group.remove(&row);
        }
        let vaults = vaults.unwrap_or_default();
        let connected = imp.drive_vaults.borrow().is_some();
        let (hint, switch) = match (connected, download, vaults.len()) {
            (false, _, _) => (None, None),
            (true, false, 0) => (None, None),
            (true, false, n) => (
                Some(ngettext_f(
                    "This Google Drive already holds {n} vault.",
                    "This Google Drive already holds {n} vaults.",
                    n as u64,
                    &[("n", &n.to_string())],
                )),
                Some(gettext("Download One Instead")),
            ),
            (true, true, 0) => (
                Some(gettext("No Passel vault on this Google Drive")),
                Some(gettext("Create a New Vault There")),
            ),
            (true, true, _) => (None, None),
        };
        imp.drive_hint_box.set_visible(hint.is_some());
        imp.drive_hint_label
            .set_label(hint.as_deref().unwrap_or_default());
        if let Some(switch) = switch {
            imp.drive_switch_button.set_label(&switch);
        }

        let show_list = connected && download && !vaults.is_empty();
        imp.drive_vaults_group.set_visible(show_list);
        if show_list {
            let chosen = imp.drive_file.borrow().clone();
            if !vaults.iter().any(|file| Some(&file.id) == chosen.as_ref()) {
                imp.drive_file
                    .replace(vaults.first().map(|file| file.id.clone()));
            }
            let chosen = imp.drive_file.borrow().clone();
            let mut group: Option<gtk::CheckButton> = None;
            let mut rows = Vec::new();
            for file in &vaults {
                let row = adw::ActionRow::new();
                row.set_use_markup(false);
                row.set_title(file.vault_name());
                if let Some(changed) = file.modified_time.as_deref().and_then(changed_on) {
                    row.set_subtitle(&changed);
                }
                let check = gtk::CheckButton::new();
                check.set_valign(gtk::Align::Center);
                check.set_group(group.as_ref());
                check.set_active(chosen.as_ref() == Some(&file.id));
                let id = file.id.clone();
                check.connect_toggled(glib::clone!(
                    #[weak(rename_to = wizard)]
                    self,
                    move |check| {
                        if check.is_active() {
                            wizard.imp().drive_file.replace(Some(id.clone()));
                            wizard.update_drive_next();
                        }
                    }
                ));
                group.get_or_insert_with(|| check.clone());
                row.add_prefix(&check);
                row.set_activatable_widget(Some(&check));
                imp.drive_vaults_group.add(&row);
                rows.push(row.upcast());
            }
            imp.drive_rows.replace(rows);
        }
        self.update_drive_next();
    }

    fn update_drive_next(&self) {
        let imp = self.imp();
        let connected = imp.drive_vaults.borrow().is_some();
        let ready = if imp.download.get() {
            imp.drive_file.borrow().is_some()
                && imp
                    .drive_vaults
                    .borrow()
                    .as_ref()
                    .is_some_and(|vaults| !vaults.is_empty())
        } else {
            true
        };
        imp.drive_next_button
            .set_sensitive(connected && ready && !imp.drive_busy.get());
    }

    /// The name of the Drive vault chosen for download.
    pub(super) fn chosen_drive_name(&self) -> Option<String> {
        let imp = self.imp();
        let id = imp.drive_file.borrow().clone()?;
        imp.drive_vaults
            .borrow()
            .as_ref()?
            .iter()
            .find(|file| file.id == id)
            .map(|file| file.vault_name().to_string())
    }
}

/// `2026-11-27T10:58:18.000Z` → "Changed on 27/11/2026" (in the
/// locale's style).
fn changed_on(time: &str) -> Option<String> {
    let date = glib::DateTime::from_iso8601(time, None)
        .ok()?
        .to_local()
        .ok()?
        .format("%x")
        .ok()?;
    Some(gettext_f("Changed on {date}", &[("date", &date)]))
}
