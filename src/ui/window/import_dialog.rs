//! "Import Passwords" dialog: read a KeePassXC or Buttercup vault with its
//! master password, preview what was found, then write the entries into
//! the open vault in one commit.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};
use zeroize::Zeroizing;

use crate::app::app_error::AppError;
use crate::app::import;
use crate::pass::import::{ImportSource, ImportedItem};

use super::MainWindow;

const SOURCES: [ImportSource; 2] = [ImportSource::KeePassXc, ImportSource::Buttercup];

#[derive(Default)]
struct ImportState {
    file: Option<PathBuf>,
    /// Entries read from the file, once it was opened.
    items: Option<Vec<ImportedItem>>,
    busy: bool,
}

/// The dialog's widgets, shared by its handlers.
#[derive(Clone)]
struct ImportDialog {
    dialog: adw::Dialog,
    source_row: adw::ComboRow,
    file_row: adw::ActionRow,
    choose_button: gtk::Button,
    password_row: adw::PasswordEntryRow,
    destination_row: adw::EntryRow,
    status_box: gtk::Box,
    spinner: gtk::Spinner,
    status_icon: gtk::Image,
    status_label: gtk::Label,
    action_button: gtk::Button,
    state: Rc<RefCell<ImportState>>,
}

impl MainWindow {
    pub(super) fn show_import_dialog(&self) {
        if self.open_store_dir().is_none() {
            return;
        }
        let ui = ImportDialog::build();
        ui.connect(self);
        ui.update();
        ui.dialog.present(Some(self));
    }
}

impl ImportDialog {
    fn build() -> Self {
        let source_row = adw::ComboRow::builder()
            .title("From")
            .model(&gtk::StringList::new(&[
                "KeePassXC (.kdbx)",
                "Buttercup (.bcup)",
            ]))
            .build();
        let choose_button = gtk::Button::builder()
            .label("Choose…")
            .valign(gtk::Align::Center)
            .build();
        let file_row = adw::ActionRow::builder()
            .title("File")
            .subtitle("No file chosen")
            .use_markup(false)
            .build();
        file_row.add_suffix(&choose_button);
        let password_row = adw::PasswordEntryRow::builder()
            .title("Master password")
            .build();
        let from_group = adw::PreferencesGroup::builder()
            .description("The master password only opens the file here; Passel does not keep it.")
            .build();
        from_group.add(&source_row);
        from_group.add(&file_row);
        from_group.add(&password_row);

        let destination_row = adw::EntryRow::builder()
            .title("Folder in this vault")
            .text(default_destination(ImportSource::KeePassXc).as_str())
            .build();
        let into_group = adw::PreferencesGroup::builder()
            .title("Into")
            .description(
                "Groups become folders inside it. A name already in use gets a number, \
                 e.g. “Bank (2)”.",
            )
            .build();
        into_group.add(&destination_row);

        let spinner = gtk::Spinner::new();
        let status_icon = gtk::Image::new();
        let status_label = gtk::Label::builder()
            .wrap(true)
            .xalign(0.0)
            .hexpand(true)
            .build();
        let status_box = gtk::Box::builder().spacing(8).visible(false).build();
        status_box.append(&spinner);
        status_box.append(&status_icon);
        status_box.append(&status_label);

        let action_button = gtk::Button::builder()
            .label("Open File")
            .halign(gtk::Align::End)
            .css_classes(["suggested-action"])
            .build();

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(18)
            .margin_top(12)
            .margin_bottom(24)
            .margin_start(24)
            .margin_end(24)
            .build();
        content.append(&from_group);
        content.append(&into_group);
        content.append(&status_box);
        content.append(&action_button);

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&adw::HeaderBar::new());
        toolbar.set_content(Some(&content));

        let dialog = adw::Dialog::builder()
            .title("Import Passwords")
            .content_width(520)
            .child(&toolbar)
            .build();

        Self {
            dialog,
            source_row,
            file_row,
            choose_button,
            password_row,
            destination_row,
            status_box,
            spinner,
            status_icon,
            status_label,
            action_button,
            state: Rc::default(),
        }
    }

    fn source(&self) -> ImportSource {
        SOURCES
            .get(self.source_row.selected() as usize)
            .copied()
            .unwrap_or(ImportSource::KeePassXc)
    }

    fn connect(&self, window: &MainWindow) {
        let ui = self.clone();
        let previous_source = Rc::new(RefCell::new(self.source()));
        self.source_row.connect_selected_notify(move |_| {
            // Follow the source in the destination, unless it was edited.
            let source = ui.source();
            let old = previous_source.replace(source);
            if ui.destination_row.text() == default_destination(old) {
                ui.destination_row.set_text(&default_destination(source));
            }
            ui.reset();
        });
        let ui = self.clone();
        self.password_row.connect_changed(move |_| ui.reset());
        let ui = self.clone();
        self.destination_row.connect_changed(move |_| ui.update());

        let ui = self.clone();
        self.choose_button.connect_clicked(glib::clone!(
            #[weak]
            window,
            move |_| ui.choose_file(&window)
        ));
        let ui = self.clone();
        self.password_row.connect_entry_activated(glib::clone!(
            #[weak]
            window,
            move |_| ui.act(&window)
        ));
        let ui = self.clone();
        self.action_button.connect_clicked(glib::clone!(
            #[weak]
            window,
            move |_| ui.act(&window)
        ));
    }

    /// Forgets what was read: the file, password or source changed.
    fn reset(&self) {
        self.state.borrow_mut().items = None;
        self.set_status(None);
        self.update();
    }

    fn update(&self) {
        let state = self.state.borrow();
        let (label, ready) = match &state.items {
            None => (
                "Open File".to_string(),
                state.file.is_some() && !self.password_row.text().is_empty(),
            ),
            Some(items) if items.len() == 1 => ("Import 1 Entry".to_string(), true),
            Some(items) => (format!("Import {} Entries", items.len()), !items.is_empty()),
        };
        self.action_button.set_label(&label);
        self.action_button.set_sensitive(ready && !state.busy);
        for widget in [
            self.source_row.upcast_ref::<gtk::Widget>(),
            self.choose_button.upcast_ref(),
            self.password_row.upcast_ref(),
            self.destination_row.upcast_ref(),
        ] {
            widget.set_sensitive(!state.busy);
        }
        self.dialog.set_can_close(!state.busy);
    }

    /// `None` hides the status; `Some((done, text))` shows a ✓/✗ line;
    /// a spinner is shown while busy.
    fn set_status(&self, status: Option<(Option<bool>, &str)>) {
        let Some((outcome, text)) = status else {
            self.status_box.set_visible(false);
            self.spinner.set_spinning(false);
            return;
        };
        self.status_box.set_visible(true);
        self.spinner.set_visible(outcome.is_none());
        self.spinner.set_spinning(outcome.is_none());
        self.status_icon.set_visible(outcome.is_some());
        for class in ["success", "error"] {
            self.status_icon.remove_css_class(class);
        }
        if let Some(ok) = outcome {
            let (icon, class) = if ok {
                ("object-select-symbolic", "success")
            } else {
                ("dialog-error-symbolic", "error")
            };
            self.status_icon.set_icon_name(Some(icon));
            self.status_icon.add_css_class(class);
        }
        self.status_label.set_label(text);
    }

    fn choose_file(&self, window: &MainWindow) {
        let filter = gtk::FileFilter::new();
        let (name, pattern) = match self.source() {
            ImportSource::KeePassXc => ("KeePassXC databases", "*.kdbx"),
            ImportSource::Buttercup => ("Buttercup vaults", "*.bcup"),
        };
        filter.set_name(Some(name));
        filter.add_pattern(pattern);
        let any = gtk::FileFilter::new();
        any.set_name(Some("All files"));
        any.add_pattern("*");
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        filters.append(&any);
        let dialog = gtk::FileDialog::builder()
            .title("Choose a File to Import")
            .modal(true)
            .filters(&filters)
            .build();
        let ui = self.clone();
        dialog.open(Some(window), gio::Cancellable::NONE, move |result| {
            if let Some(path) = result.ok().and_then(|file| file.path()) {
                ui.file_row.set_subtitle(&path.to_string_lossy());
                ui.state.borrow_mut().file = Some(path);
                ui.reset();
            }
        });
    }

    fn act(&self, window: &MainWindow) {
        if !self.action_button.is_sensitive() {
            return;
        }
        if self.state.borrow().items.is_some() {
            self.write(window);
        } else {
            self.read();
        }
    }

    fn read(&self) {
        let Some(path) = self.state.borrow().file.clone() else {
            return;
        };
        let password = Zeroizing::new(self.password_row.text().to_string());
        let source = self.source();
        self.state.borrow_mut().busy = true;
        self.set_status(Some((None, "Opening the file…")));
        self.update();

        let ui = self.clone();
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || import::read_file(source, &path, &password))
                .await
                .unwrap_or_else(|_| Err("The import stopped unexpectedly".to_string()));
            ui.state.borrow_mut().busy = false;
            match result {
                Ok(items) => {
                    let message = if items.is_empty() {
                        "This file has no entries to import.".to_string()
                    } else {
                        format!(
                            "Found {} in {} (the trash is left out).",
                            count(items.len(), "entry", "entries"),
                            count(import::group_count(&items), "group", "groups")
                        )
                    };
                    ui.set_status(Some((Some(!items.is_empty()), &message)));
                    ui.state.borrow_mut().items = Some(items);
                }
                Err(message) => ui.set_status(Some((Some(false), &message))),
            }
            ui.update();
        });
    }

    fn write(&self, window: &MainWindow) {
        let Some(store_dir) = window.open_store_dir() else {
            return;
        };
        let Some(items) = self.state.borrow().items.clone() else {
            return;
        };
        let destination = PathBuf::from(self.destination_row.text().trim().trim_matches('/'));
        let source = self.source();
        let autopush = window.controller().borrow().autopush();
        self.state.borrow_mut().busy = true;
        self.set_status(Some((None, "Encrypting and saving the entries…")));
        self.update();

        let ui = self.clone();
        let window = window.clone();
        glib::spawn_future_local(async move {
            let shown_destination = destination.clone();
            let result = gio::spawn_blocking(move || {
                import::write(&store_dir, &destination, &items, source, autopush)
            })
            .await
            .unwrap_or_else(|_| {
                Err(AppError::Io(std::io::Error::other(
                    "the import stopped unexpectedly",
                )))
            });
            ui.state.borrow_mut().busy = false;
            ui.update();
            let summary = match result {
                Ok(summary) => Some(summary),
                Err(AppError::PushFailed(_)) => None,
                Err(err) => {
                    ui.set_status(Some((Some(false), &err.to_string())));
                    return;
                }
            };
            ui.password_row.set_text("");
            ui.dialog.close();
            window.finish_import(&shown_destination, summary);
        });
    }
}

impl MainWindow {
    /// Shows the imported entries and says what happened.
    fn finish_import(
        &self,
        destination: &Path,
        summary: Option<crate::pass::store::ImportSummary>,
    ) {
        if let Err(err) = self.reload_vault_tree() {
            self.show_error_dialog(&err.to_string());
        }
        if let Err(err) = self.reload_changes_view() {
            log::debug!("history unavailable: {err}");
        }
        let place = if destination.as_os_str().is_empty() {
            "the vault".to_string()
        } else {
            format!("“{}”", destination.display())
        };
        let body = match summary {
            Some(summary) if summary.renamed > 0 => format!(
                "Added {} to {place}. {} a number because the name was already in use.",
                count(summary.written, "entry", "entries"),
                count(summary.renamed, "entry got", "entries got")
            ),
            Some(summary) => format!(
                "Added {} to {place}.",
                count(summary.written, "entry", "entries")
            ),
            None => format!(
                "The entries were added to {place} and saved, but uploading them failed. \
                 Use Changes to try again."
            ),
        };
        let dialog = adw::AlertDialog::builder()
            .heading("Import Finished")
            .body(body)
            .build();
        dialog.add_responses(&[("ok", "OK")]);
        dialog.set_default_response(Some("ok"));
        dialog.set_close_response("ok");
        dialog.present(Some(self));
    }
}

fn default_destination(source: ImportSource) -> String {
    format!("Imported/{}", source.name())
}

/// "1 entry", "2 entries".
fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}
