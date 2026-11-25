//! The wizard's encryption key page: use a key from this computer,
//! create one, or import a backup, and save a backup of the chosen key.
//!
//! Passphrases never pass through Passel: gpg-agent asks for them with
//! its own pinentry window while a key action runs on a worker thread.

use std::path::PathBuf;

use adw::{prelude::*, subclass::prelude::*};
use gtk::{gio, glib};

use super::SetupWizard;
use crate::app::setup::{self, KeyActionError};
use crate::i18n::{gettext, gettext_f};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyMode {
    Use,
    Create,
    Import,
}

enum KeyStatus<'a> {
    Hidden,
    Busy(&'a str),
    Done(&'a str),
    Failed(&'a str),
}

impl SetupWizard {
    pub(super) fn setup_key_page(&self) {
        let imp = self.imp();
        imp.key_file_row.set_use_markup(false);

        for check in [
            &*imp.use_key_check,
            &*imp.create_key_check,
            &*imp.import_key_check,
        ] {
            check.connect_toggled(glib::clone!(
                #[weak(rename_to = wizard)]
                self,
                move |check| {
                    if check.is_active() {
                        wizard.set_key_status(KeyStatus::Hidden);
                        wizard.update_key_page();
                    }
                }
            ));
        }
        imp.recipient_row.connect_selected_notify(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| wizard.update_key_page()
        ));
        for row in [&*imp.key_name_row, &*imp.key_email_row] {
            row.connect_changed(glib::clone!(
                #[weak(rename_to = wizard)]
                self,
                move |_| wizard.update_key_page()
            ));
            row.connect_entry_activated(glib::clone!(
                #[weak(rename_to = wizard)]
                self,
                move |_| wizard.key_next()
            ));
        }
        imp.choose_key_file_button.connect_clicked(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| wizard.choose_key_file()
        ));
        imp.backup_button.connect_clicked(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| wizard.save_backup()
        ));
        imp.key_next_button.connect_clicked(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| wizard.key_next()
        ));
    }

    pub(super) fn reset_key_page(&self) {
        let imp = self.imp();
        imp.key_name_row.set_text("");
        imp.key_email_row.set_text("");
        imp.key_file.replace(None);
        imp.key_file_row.set_subtitle(&gettext("No file chosen"));
        imp.use_key_check.set_active(true);
        self.set_key_status(KeyStatus::Hidden);
        self.load_recipients(None);
    }

    /// Fills the key picker from gpg, selecting the key with fingerprint
    /// `select` (and the "use" option) when given. Without any key, the
    /// "use" option is hidden and "create" is chosen.
    fn load_recipients(&self, select: Option<&str>) {
        let imp = self.imp();
        let recipients = setup::available_recipients();
        let labels = recipients
            .iter()
            .map(|recipient| recipient.label.as_str())
            .collect::<Vec<_>>();
        imp.recipient_row
            .set_model(Some(&gtk::StringList::new(&labels)));
        let index = select
            .and_then(|fingerprint| {
                recipients
                    .iter()
                    .position(|recipient| recipient.id.eq_ignore_ascii_case(fingerprint))
            })
            .unwrap_or(0);
        let has_keys = !recipients.is_empty();
        imp.recipients.replace(recipients);
        imp.recipient_row.set_selected(if has_keys {
            index as u32
        } else {
            gtk::INVALID_LIST_POSITION
        });

        imp.use_key_row.set_visible(has_keys);
        if has_keys && select.is_some() {
            imp.use_key_check.set_active(true);
        } else if !has_keys && imp.use_key_check.is_active() {
            imp.create_key_check.set_active(true);
        }
        self.update_key_page();
    }

    fn key_mode(&self) -> KeyMode {
        let imp = self.imp();
        if imp.create_key_check.is_active() {
            KeyMode::Create
        } else if imp.import_key_check.is_active() {
            KeyMode::Import
        } else {
            KeyMode::Use
        }
    }

    fn update_key_page(&self) {
        let imp = self.imp();
        let mode = self.key_mode();
        let busy = imp.key_busy.get();
        let has_key = self.selected_recipient().is_some();

        imp.existing_key_group.set_visible(mode == KeyMode::Use);
        imp.create_key_group.set_visible(mode == KeyMode::Create);
        imp.import_key_group.set_visible(mode == KeyMode::Import);
        imp.backup_group
            .set_visible(mode == KeyMode::Use && has_key);

        let (label, ready) = match mode {
            KeyMode::Use => (gettext("Next"), has_key),
            KeyMode::Create => (
                gettext("Create Key"),
                setup::is_valid_key_identity(&imp.key_name_row.text(), &imp.key_email_row.text()),
            ),
            KeyMode::Import => (gettext("Import Key"), imp.key_file.borrow().is_some()),
        };
        imp.key_next_button.set_label(&label);
        imp.key_next_button.set_sensitive(ready && !busy);

        for widget in [
            imp.use_key_check.upcast_ref::<gtk::Widget>(),
            imp.create_key_check.upcast_ref(),
            imp.import_key_check.upcast_ref(),
            imp.existing_key_group.upcast_ref(),
            imp.create_key_group.upcast_ref(),
            imp.import_key_group.upcast_ref(),
            imp.backup_group.upcast_ref(),
            imp.key_back_button.upcast_ref(),
        ] {
            widget.set_sensitive(!busy);
        }
    }

    fn set_key_status(&self, status: KeyStatus<'_>) {
        let imp = self.imp();
        let (text, icon) = match status {
            KeyStatus::Hidden => {
                imp.key_status_box.set_visible(false);
                imp.key_spinner.set_spinning(false);
                return;
            }
            KeyStatus::Busy(text) => (text, None),
            KeyStatus::Done(text) => (text, Some(("object-select-symbolic", "success"))),
            KeyStatus::Failed(text) => (text, Some(("dialog-error-symbolic", "error"))),
        };
        imp.key_status_box.set_visible(true);
        imp.key_spinner.set_visible(icon.is_none());
        imp.key_spinner.set_spinning(icon.is_none());
        imp.key_status_icon.set_visible(icon.is_some());
        for class in ["success", "error"] {
            imp.key_status_icon.remove_css_class(class);
        }
        if let Some((name, class)) = icon {
            imp.key_status_icon.set_icon_name(Some(name));
            imp.key_status_icon.add_css_class(class);
        }
        imp.key_status_label.set_label(text);
    }

    fn key_next(&self) {
        let imp = self.imp();
        if imp.key_busy.get() || !imp.key_next_button.is_sensitive() {
            return;
        }
        match self.key_mode() {
            KeyMode::Use => imp.navigation.push_by_tag("sync"),
            KeyMode::Create => {
                let name = imp.key_name_row.text().to_string();
                let email = imp.key_email_row.text().to_string();
                self.run_key_task(
                    &gettext("Creating your key… GPG will ask you to choose a passphrase."),
                    move || setup::create_key(&name, &email),
                    |wizard, fingerprint| {
                        wizard.load_recipients(Some(&fingerprint));
                        wizard.set_key_status(KeyStatus::Done(&gettext(
                            "Key created. Save a backup of it before you continue.",
                        )));
                    },
                );
            }
            KeyMode::Import => {
                let Some(path) = imp.key_file.borrow().clone() else {
                    return;
                };
                self.run_key_task(
                    &gettext("Importing the key… GPG may ask for its passphrase."),
                    move || setup::import_key(&path),
                    |wizard, fingerprints| {
                        wizard.load_recipients(fingerprints.first().map(String::as_str));
                        wizard.set_key_status(KeyStatus::Done(&gettext("Key imported.")));
                    },
                );
            }
        }
    }

    /// Runs a gpg operation on a worker (pinentry may show up meanwhile),
    /// with the page locked and a spinner.
    fn run_key_task<T, F, D>(&self, progress: &str, task: F, on_done: D)
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, KeyActionError> + Send + 'static,
        D: FnOnce(&Self, T) + 'static,
    {
        let imp = self.imp();
        if imp.key_busy.get() {
            return;
        }
        imp.key_busy.set(true);
        self.set_key_status(KeyStatus::Busy(progress));
        self.update_key_page();

        let wizard = self.clone();
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(task).await.unwrap_or_else(|_| {
                Err(KeyActionError::Failed(gettext(
                    "The key task stopped unexpectedly",
                )))
            });
            wizard.imp().key_busy.set(false);
            match result {
                Ok(value) => on_done(&wizard, value),
                Err(KeyActionError::Cancelled) => wizard.set_key_status(KeyStatus::Hidden),
                Err(KeyActionError::Failed(message)) => {
                    wizard.set_key_status(KeyStatus::Failed(&message));
                }
            }
            wizard.update_key_page();
        });
    }

    fn choose_key_file(&self) {
        let filter = gtk::FileFilter::new();
        filter.set_name(Some(&gettext("Key files")));
        for pattern in ["*.asc", "*.gpg", "*.pgp", "*.key"] {
            filter.add_pattern(pattern);
        }
        let any = gtk::FileFilter::new();
        any.set_name(Some(&gettext("All files")));
        any.add_pattern("*");
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        filters.append(&any);

        let dialog = gtk::FileDialog::builder()
            .title(gettext("Choose a Key Backup"))
            .modal(true)
            .filters(&filters)
            .build();
        let window = self.root().and_downcast::<gtk::Window>();
        dialog.open(
            window.as_ref(),
            gio::Cancellable::NONE,
            glib::clone!(
                #[weak(rename_to = wizard)]
                self,
                move |result| {
                    if let Some(path) = result.ok().and_then(|file| file.path()) {
                        let imp = wizard.imp();
                        imp.key_file_row.set_subtitle(&path.to_string_lossy());
                        imp.key_file.replace(Some(path));
                        wizard.set_key_status(KeyStatus::Hidden);
                        wizard.update_key_page();
                    }
                }
            ),
        );
    }

    fn save_backup(&self) {
        let Some((fingerprint, _)) = self.selected_recipient() else {
            return;
        };
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Save Key Backup"))
            .initial_name("passel-key-backup.asc")
            .modal(true)
            .build();
        let window = self.root().and_downcast::<gtk::Window>();
        dialog.save(
            window.as_ref(),
            gio::Cancellable::NONE,
            glib::clone!(
                #[weak(rename_to = wizard)]
                self,
                move |result| {
                    let Some(path) = result.ok().and_then(|file| file.path()) else {
                        return;
                    };
                    wizard.export_backup(fingerprint, path);
                }
            ),
        );
    }

    fn export_backup(&self, fingerprint: String, path: PathBuf) {
        let shown = path.to_string_lossy().into_owned();
        self.run_key_task(
            &gettext("Saving the backup… GPG will ask for the key’s passphrase."),
            move || setup::export_key(&fingerprint, &path),
            move |wizard, ()| {
                wizard.set_key_status(KeyStatus::Done(&gettext_f(
                    "Backup saved to {file}. Move it somewhere safe, away from this computer.",
                    &[("file", &shown)],
                )));
            },
        );
    }
}
