//! The wizard's sync pages: where to keep a copy of the vault (nowhere,
//! online, or a folder/drive), with step-by-step help for users who have
//! never used git hosting.

use std::path::PathBuf;

use adw::{prelude::*, subclass::prelude::*};
use gtk::{gio, glib};

use super::hosts::{self, Host, HOSTS};
use super::SetupWizard;
use crate::app::setup::{self, CopyDirStatus};
use crate::i18n::{gettext, gettext_f};
use crate::pass::store::{RemoteCheck, SyncTarget};

/// Index of "I already have one" in the service combo. The services of
/// [`HOSTS`] follow, in order.
const HOST_OWN: u32 = 0;

/// Name of the copy Passel creates inside the chosen folder.
const COPY_NAME: &str = "passwords.git";
const BUNDLE_NAME: &str = "passwords.bundle";

impl SetupWizard {
    pub(super) fn setup_sync(&self) {
        let imp = self.imp();
        let mut names = vec![gettext("I already have one")];
        names.extend(HOSTS.iter().map(|host| host.name.to_string()));
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        imp.host_row.set_model(Some(&gtk::StringList::new(&names)));
        imp.host_row.set_selected(HOST_OWN);
        imp.copy_row.set_use_markup(false);

        imp.sync_next_button.connect_clicked(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| wizard.sync_next()
        ));
        imp.host_row.connect_selected_notify(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| wizard.update_host_steps()
        ));
        imp.address_row.connect_changed(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| wizard.update_check_status()
        ));
        imp.address_row.connect_entry_activated(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| wizard.online_next()
        ));
        imp.check_button.connect_clicked(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| wizard.check_address(false)
        ));
        imp.online_next_button.connect_clicked(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| wizard.online_next()
        ));
        imp.choose_copy_button.connect_clicked(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| wizard.choose_copy_folder()
        ));
        imp.folder_next_button.connect_clicked(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| wizard.go_to_review()
        ));
        imp.bundle_file_check.connect_toggled(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| wizard.update_copy_row()
        ));
    }

    pub(super) fn reset_sync(&self) {
        let imp = self.imp();
        imp.online_check.set_active(true);
        imp.git_folder_check.set_active(true);
        imp.host_row.set_selected(HOST_OWN);
        imp.address_row.set_text("");
        imp.remote_check.replace(None);
        imp.copy_parent.replace(None);
        self.update_host_steps();
        self.update_check_status();
        self.update_copy_row();
    }

    /// The copy target of the current choices, for the plan.
    pub(super) fn sync_target(&self) -> Option<SyncTarget> {
        let imp = self.imp();
        if imp.online_check.is_active() {
            Some(SyncTarget::Remote(
                imp.address_row.text().trim().to_string(),
            ))
        } else if imp.folder_check.is_active() {
            let copy = self.copy_dir()?;
            Some(if imp.bundle_file_check.is_active() {
                SyncTarget::Bundle(copy)
            } else {
                SyncTarget::Folder(copy)
            })
        } else {
            None
        }
    }

    fn sync_next(&self) {
        let imp = self.imp();
        if imp.online_check.is_active() {
            imp.navigation.push_by_tag("online");
        } else if imp.folder_check.is_active() {
            // The vault folder may have changed since the copy was chosen.
            self.update_copy_row();
            imp.navigation.push_by_tag("folder");
        } else {
            self.go_to_review();
        }
    }

    fn selected_host(&self) -> Option<&'static Host> {
        let selected = self.imp().host_row.selected();
        if selected == HOST_OWN {
            return None;
        }
        HOSTS.get(selected.checked_sub(1)? as usize)
    }

    /// Rebuilds the "create a repository" checklist for the chosen service.
    fn update_host_steps(&self) {
        let imp = self.imp();
        for row in imp.host_step_rows.take() {
            imp.host_steps_group.remove(&row);
        }

        let host = self.selected_host();
        imp.host_row
            .set_subtitle(&host.map_or_else(String::new, |host| (host.note)()));
        imp.address_row.set_title(&match host {
            Some(host) => host.address_example.to_string(),
            None => gettext("SSH address, like git@server:you/passwords.git"),
        });
        imp.host_steps_group.set_visible(host.is_some());
        let Some(host) = host else {
            return;
        };

        let mut rows: Vec<gtk::Widget> = Vec::new();
        let open_row = adw::ActionRow::new();
        open_row.set_use_markup(false);
        open_row.set_title(&gettext_f(
            // Translators: {service} is GitHub, GitLab…
            "Open {service}",
            &[("service", host.name)],
        ));
        open_row.set_subtitle(&gettext(
            "Sign in or create a free account, then start a new repository",
        ));
        open_row.set_activatable(true);
        open_row.add_suffix(&gtk::Image::from_icon_name("web-browser-symbolic"));
        let url = host.new_repo_url;
        open_row.connect_activated(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| wizard.open_link(url)
        ));
        rows.push(open_row.upcast());

        for (index, item) in (host.checklist)().iter().enumerate() {
            let row = adw::ActionRow::new();
            row.set_use_markup(false);
            row.set_title(item);
            let number = gtk::Label::new(Some(&(index + 1).to_string()));
            number.add_css_class("dim-label");
            number.add_css_class("numeric");
            row.add_prefix(&number);
            rows.push(row.upcast());
        }

        rows.push(self.ssh_key_row(host).upcast());

        for row in &rows {
            imp.host_steps_group.add(row);
        }
        imp.host_step_rows.replace(rows);
    }

    /// "Add your SSH key" row: copy the local public key and open the
    /// service's key page, or explain how to create a key.
    fn ssh_key_row(&self, host: &'static Host) -> adw::ActionRow {
        let keys = setup::ssh_public_keys();
        let key = if host.rsa_only {
            keys.iter().find(|key| key.is_rsa())
        } else {
            keys.first()
        };

        let row = adw::ActionRow::new();
        row.set_use_markup(false);
        row.set_title(&gettext_f(
            // Translators: {service} is GitHub, GitLab…
            "Add your SSH key to {service}",
            &[("service", host.name)],
        ));
        let button = |icon: &str, tooltip: &str| {
            let button = gtk::Button::from_icon_name(icon);
            button.set_tooltip_text(Some(tooltip));
            button.set_valign(gtk::Align::Center);
            button.add_css_class("flat");
            button
        };

        match key {
            Some(key) => {
                let file_name = key
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                row.set_subtitle(&gettext_f(
                    "This lets Passel upload. Copy your key ({file}), then add it on the SSH keys page.",
                    &[("file", &file_name)],
                ));
                let copy = button("edit-copy-symbolic", &gettext("Copy Key"));
                let text = key.text.clone();
                copy.connect_clicked(move |button| {
                    // A public key: no need for the auto-clearing secret
                    // clipboard helper.
                    button.clipboard().set_text(&text);
                    button.set_icon_name("object-select-symbolic");
                });
                let open = button("web-browser-symbolic", &gettext("Open the SSH Keys Page"));
                let url = host.ssh_keys_url;
                open.connect_clicked(glib::clone!(
                    #[weak(rename_to = wizard)]
                    self,
                    move |_| wizard.open_link(url)
                ));
                row.add_suffix(&copy);
                row.add_suffix(&open);
            }
            None => {
                row.set_subtitle(&if host.rsa_only && !keys.is_empty() {
                    gettext("This service only accepts RSA keys, and this computer has none yet.")
                } else {
                    gettext(
                        "This computer has no SSH key yet. Create one, then add it to your account.",
                    )
                });
                let help = gtk::Button::with_label(&gettext("How to Create One"));
                help.set_valign(gtk::Align::Center);
                let url = host.ssh_help_url;
                help.connect_clicked(glib::clone!(
                    #[weak(rename_to = wizard)]
                    self,
                    move |_| wizard.open_link(url)
                ));
                row.add_suffix(&help);
            }
        }
        row
    }

    fn open_link(&self, url: &str) {
        let window = self.root().and_downcast::<gtk::Window>();
        gtk::UriLauncher::new(url).launch(window.as_ref(), gio::Cancellable::NONE, |result| {
            if let Err(err) = result {
                log::warn!("Failed to open a link: {err}");
            }
        });
    }

    fn current_address(&self) -> String {
        self.imp().address_row.text().trim().to_string()
    }

    /// The last check result, if it was for the address now typed.
    fn current_check(&self) -> Option<RemoteCheck> {
        let address = self.current_address();
        self.imp()
            .remote_check
            .borrow()
            .as_ref()
            .filter(|(checked, _)| *checked == address)
            .map(|(_, result)| *result)
    }

    fn update_check_status(&self) {
        let imp = self.imp();
        let address = self.current_address();
        let checking = imp.checking_remote.get();
        let result = self.current_check();

        imp.check_box.set_visible(checking || result.is_some());
        imp.check_spinner.set_visible(checking);
        imp.check_spinner.set_spinning(checking);
        imp.check_icon.set_visible(!checking && result.is_some());
        for class in ["success", "error"] {
            imp.check_icon.remove_css_class(class);
        }
        if checking {
            imp.check_label.set_label(&gettext("Checking the address…"));
        } else if let Some(result) = result {
            let (ok, message) = check_message(result, &address);
            let (icon, class) = if ok {
                ("object-select-symbolic", "success")
            } else {
                ("dialog-error-symbolic", "error")
            };
            imp.check_icon.set_icon_name(Some(icon));
            imp.check_icon.add_css_class(class);
            imp.check_label.set_label(&message);
        }

        let failed = result.is_some_and(|result| result != RemoteCheck::Empty);
        imp.check_button
            .set_sensitive(!address.is_empty() && !checking);
        imp.online_next_button.set_sensitive(
            !address.is_empty() && !checking && result != Some(RemoteCheck::NotEmpty),
        );
        imp.online_next_button.set_label(&if failed {
            gettext("Continue Anyway")
        } else {
            gettext("Next")
        });
    }

    /// Next checks the address first. A failed check can be overridden
    /// (e.g. offline right now), except a repository that already holds
    /// something: uploading there would fail.
    fn online_next(&self) {
        match self.current_check() {
            None => self.check_address(true),
            Some(RemoteCheck::NotEmpty) => {}
            Some(_) => self.go_to_review(),
        }
    }

    fn check_address(&self, then_continue: bool) {
        let imp = self.imp();
        let address = self.current_address();
        if address.is_empty() || imp.checking_remote.get() {
            return;
        }
        imp.checking_remote.set(true);
        self.update_check_status();

        let wizard = self.clone();
        glib::spawn_future_local(async move {
            let checked = address.clone();
            let result = gio::spawn_blocking(move || setup::check_remote(&checked))
                .await
                .unwrap_or(RemoteCheck::Unreachable);
            let imp = wizard.imp();
            imp.checking_remote.set(false);
            imp.remote_check.replace(Some((address.clone(), result)));
            wizard.update_check_status();

            let still_here = imp
                .navigation
                .visible_page()
                .and_then(|page| page.tag())
                .is_some_and(|tag| tag == "online");
            if then_continue
                && result == RemoteCheck::Empty
                && still_here
                && wizard.current_address() == address
            {
                wizard.go_to_review();
            }
        });
    }

    /// The copy's path: a git folder or a bundle file in the chosen folder.
    fn copy_dir(&self) -> Option<PathBuf> {
        let imp = self.imp();
        let name = if imp.bundle_file_check.is_active() {
            BUNDLE_NAME
        } else {
            COPY_NAME
        };
        imp.copy_parent
            .borrow()
            .as_ref()
            .map(|parent| parent.join(name))
    }

    fn choose_copy_folder(&self) {
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Choose a Folder for the Copy"))
            .modal(true)
            .build();
        if let Some(parent) = self.imp().copy_parent.borrow().as_ref() {
            dialog.set_initial_folder(Some(&gio::File::for_path(parent)));
        }
        let window = self.root().and_downcast::<gtk::Window>();
        dialog.select_folder(
            window.as_ref(),
            gio::Cancellable::NONE,
            glib::clone!(
                #[weak(rename_to = wizard)]
                self,
                move |result| {
                    // An error here is usually just "dismissed".
                    if let Some(path) = result.ok().and_then(|folder| folder.path()) {
                        wizard.imp().copy_parent.replace(Some(path));
                        wizard.update_copy_row();
                    }
                }
            ),
        );
    }

    fn update_copy_row(&self) {
        let imp = self.imp();
        let problem = match self.copy_dir() {
            None => {
                imp.copy_row.set_subtitle(&gettext("No folder chosen"));
                imp.copy_group.set_description(None);
                imp.copy_row.remove_css_class("error");
                imp.folder_next_button.set_sensitive(false);
                return;
            }
            Some(copy_dir) => {
                imp.copy_row.set_subtitle(&copy_dir.to_string_lossy());
                let store_dir = PathBuf::from(imp.path_row.text().trim());
                if copy_dir.to_str().is_none() {
                    Some(gettext(
                        "This folder’s name has unsupported characters. Choose another folder.",
                    ))
                } else {
                    match setup::copy_dir_status(&store_dir, &copy_dir) {
                        CopyDirStatus::Usable => None,
                        CopyDirStatus::InsideVault => Some(gettext(
                            "This folder is inside the vault. Choose a folder outside it.",
                        )),
                        CopyDirStatus::Taken if imp.bundle_file_check.is_active() => Some(gettext(
                            "This folder already has a “passwords.bundle”. Choose another folder.",
                        )),
                        CopyDirStatus::Taken => Some(gettext(
                            "This folder already has a “passwords.git”. Choose another folder.",
                        )),
                    }
                }
            }
        };
        imp.copy_group.set_description(problem.as_deref());
        if problem.is_some() {
            imp.copy_row.add_css_class("error");
        } else {
            imp.copy_row.remove_css_class("error");
        }
        imp.folder_next_button.set_sensitive(problem.is_none());
    }
}

/// Whether a check result allows uploading, and how to say it.
fn check_message(result: RemoteCheck, address: &str) -> (bool, String) {
    let message = match result {
        RemoteCheck::Empty => {
            return (
                true,
                gettext("Ready: the repository exists and is empty."),
            )
        }
        RemoteCheck::NotEmpty => gettext(
            "This repository already has content. Create a new, empty one for the vault.",
        ),
        RemoteCheck::AccessDenied if address.starts_with("http") => gettext(
            "Access denied. HTTPS addresses need a saved password or token: use the SSH address instead.",
        ),
        RemoteCheck::AccessDenied => match hosts::detect(address) {
            Some(host) => gettext_f(
                // Translators: {service} is GitHub, GitLab…
                "Access denied. Add your SSH key to {service}, then check again.",
                &[("service", host.name)],
            ),
            None => gettext("Access denied. Add your SSH key to the service, then check again."),
        },
        RemoteCheck::UnknownHost => gettext(
            "This server isn’t in your known SSH hosts yet. Connect to it once with ssh in a terminal, then check again.",
        ),
        RemoteCheck::Unreachable => gettext(
            "No repository found at this address. Check the address and your internet connection.",
        ),
    };
    (false, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explains_check_results() {
        let ssh = "git@github.com:you/passwords.git";
        assert!(check_message(RemoteCheck::Empty, ssh).0);
        assert!(!check_message(RemoteCheck::NotEmpty, ssh).0);
        assert!(check_message(RemoteCheck::AccessDenied, ssh)
            .1
            .contains("SSH key to GitHub"));
        assert!(
            check_message(RemoteCheck::AccessDenied, "https://github.com/you/p.git")
                .1
                .contains("use the SSH address")
        );
        assert!(
            check_message(RemoteCheck::AccessDenied, "git@example.org:p.git")
                .1
                .contains("the service")
        );
    }
}
