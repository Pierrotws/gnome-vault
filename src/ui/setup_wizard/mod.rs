//! First-run wizard: collects a [`SetupPlan`] over a few pages, then runs
//! its steps on a worker thread with live progress on the review page.

mod imp;

use std::path::{Path, PathBuf};

use adw::{prelude::*, subclass::prelude::*};
use gtk::{gio, glib};

use crate::app::setup;
use crate::pass::store::{SetupPlan, SetupStep, StoreStatus};

/// Index of "No Sync" in the provider combo.
const PROVIDER_NONE: u32 = 0;

glib::wrapper! {
    pub struct SetupWizard(ObjectSubclass<imp::SetupWizard>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

/// The plan shown on the review page, its rows, and how many of its
/// steps already succeeded.
#[derive(Debug)]
pub struct SetupRun {
    plan: SetupPlan,
    rows: Vec<StepRow>,
    done: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StepState {
    Running,
    Done,
    Failed,
    Skipped,
}

#[derive(Debug)]
struct StepRow {
    step: SetupStep,
    icon: gtk::Image,
    spinner: gtk::Spinner,
}

impl StepRow {
    fn set_state(&self, state: StepState) {
        self.spinner.set_visible(state == StepState::Running);
        self.spinner.set_spinning(state == StepState::Running);
        let icon = match state {
            StepState::Running => None,
            StepState::Done => Some(("object-select-symbolic", "success")),
            StepState::Failed => Some(("dialog-error-symbolic", "error")),
            StepState::Skipped => Some(("action-unavailable-symbolic", "dim-label")),
        };
        for class in ["success", "error", "dim-label"] {
            self.icon.remove_css_class(class);
        }
        match icon {
            Some((name, class)) => {
                self.icon.set_icon_name(Some(name));
                self.icon.add_css_class(class);
                self.icon.set_visible(true);
            }
            None => self.icon.set_visible(false),
        }
    }
}

impl Default for SetupWizard {
    fn default() -> Self {
        Self::new()
    }
}

impl SetupWizard {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Returns the wizard to its welcome page with fresh defaults.
    pub fn reset(&self, store_dir: &Path, autopush: bool) {
        let imp = self.imp();
        imp.autopush.set(autopush);
        imp.run.replace(None);
        imp.path_row.set_text(&store_dir.to_string_lossy());
        imp.provider_row.set_selected(PROVIDER_NONE);
        imp.remote_row.set_text("");
        self.load_recipients();
        imp.navigation.pop_to_tag("welcome");
        self.update_vault_next();
        self.update_sync_next();
    }

    pub fn connect_vault_created<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, PathBuf) + 'static,
    {
        self.connect_local("vault-created", false, move |values| {
            let obj = values[0]
                .get::<SetupWizard>()
                .expect("vault-created: first arg should be SetupWizard");
            let store_dir = values[1]
                .get::<String>()
                .expect("vault-created: second arg should be String");
            f(&obj, PathBuf::from(store_dir));
            None
        })
    }

    fn setup(&self) {
        let imp = self.imp();
        let provider_model = gtk::StringList::new(&["No Sync", "GitHub", "GitLab", "Custom"]);
        imp.provider_row.set_model(Some(&provider_model));
        imp.provider_row.set_selected(PROVIDER_NONE);

        // Every handler captures the wizard weakly: its template children
        // own the closures, so a strong capture would be a ref cycle.
        imp.create_button.connect_clicked(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| wizard.imp().navigation.push_by_tag("vault")
        ));
        for back in [
            &*imp.vault_back_button,
            &*imp.sync_back_button,
            &*imp.review_back_button,
        ] {
            back.connect_clicked(glib::clone!(
                #[weak(rename_to = wizard)]
                self,
                move |_| {
                    wizard.imp().navigation.pop();
                }
            ));
        }

        imp.path_row.connect_changed(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| wizard.update_vault_next()
        ));
        imp.recipient_row.connect_selected_notify(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| wizard.update_vault_next()
        ));
        imp.vault_next_button.connect_clicked(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| wizard.imp().navigation.push_by_tag("sync")
        ));

        imp.provider_row.connect_selected_notify(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |row| {
                let sync_enabled = row.selected() != PROVIDER_NONE;
                wizard.imp().remote_row.set_sensitive(sync_enabled);
                wizard.update_sync_next();
            }
        ));
        imp.remote_row.connect_changed(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| wizard.update_sync_next()
        ));
        imp.sync_next_button.connect_clicked(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| {
                wizard.prepare_review();
                wizard.imp().navigation.push_by_tag("review");
            }
        ));

        imp.apply_button.connect_clicked(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| wizard.apply()
        ));
        imp.skip_button.connect_clicked(glib::clone!(
            #[weak(rename_to = wizard)]
            self,
            move |_| wizard.skip_remaining()
        ));
    }

    fn load_recipients(&self) {
        let imp = self.imp();
        let recipients = setup::available_recipients();
        let labels = recipients
            .iter()
            .map(|recipient| recipient.label.as_str())
            .collect::<Vec<_>>();
        imp.recipient_row
            .set_model(Some(&gtk::StringList::new(&labels)));
        if recipients.is_empty() {
            imp.recipient_row.set_selected(gtk::INVALID_LIST_POSITION);
            imp.recipient_row
                .set_subtitle("No usable local secret encryption key found");
            imp.recipient_row.set_sensitive(false);
        } else {
            imp.recipient_row.set_selected(0);
            imp.recipient_row.set_subtitle("");
            imp.recipient_row.set_sensitive(true);
        }
        imp.recipients.replace(recipients);
    }

    fn update_vault_next(&self) {
        let imp = self.imp();
        let path = imp.path_row.text();
        let path = path.trim();
        let folder_problem = if path.is_empty() {
            None
        } else {
            match setup::folder_status(Path::new(path)) {
                StoreStatus::Missing | StoreStatus::Empty | StoreStatus::Ready => None,
                StoreStatus::PassWithoutGit => {
                    Some("This folder already holds a password store. Choose an empty folder.")
                }
                StoreStatus::Unrelated => {
                    Some("This folder is not empty. Choose an empty or new folder.")
                }
            }
        };
        imp.vault_group.set_description(folder_problem);
        if folder_problem.is_some() {
            imp.path_row.add_css_class("error");
        } else {
            imp.path_row.remove_css_class("error");
        }
        let has_recipient = self.selected_recipient().is_some();
        imp.vault_next_button
            .set_sensitive(!path.is_empty() && folder_problem.is_none() && has_recipient);
    }

    fn update_sync_next(&self) {
        let imp = self.imp();
        let needs_remote = imp.provider_row.selected() != PROVIDER_NONE;
        let has_remote = !imp.remote_row.text().trim().is_empty();
        imp.sync_next_button
            .set_sensitive(!needs_remote || has_remote);
    }

    fn selected_recipient(&self) -> Option<(String, String)> {
        let imp = self.imp();
        imp.recipients
            .borrow()
            .get(imp.recipient_row.selected() as usize)
            .map(|recipient| (recipient.id.clone(), recipient.label.clone()))
    }

    fn build_plan(&self) -> SetupPlan {
        let imp = self.imp();
        let remote_url = (imp.provider_row.selected() != PROVIDER_NONE)
            .then(|| imp.remote_row.text().trim().to_string())
            .filter(|url| !url.is_empty());
        SetupPlan {
            store_dir: PathBuf::from(imp.path_row.text().trim()),
            recipient: self
                .selected_recipient()
                .map(|(id, _)| id)
                .unwrap_or_default(),
            remote_url,
            autopush: imp.autopush.get(),
        }
    }

    /// Lists the plan's steps on the review page. A run that already got
    /// some steps done is kept as is: the review page cannot be left once
    /// anything was written, so its plan cannot have changed.
    fn prepare_review(&self) {
        let imp = self.imp();
        if imp.run.borrow().as_ref().is_some_and(|run| run.done > 0) {
            return;
        }

        while let Some(row) = imp.steps_list.first_child() {
            imp.steps_list.remove(&row);
        }
        let plan = self.build_plan();
        let key_label = self.selected_recipient().map(|(_, label)| label);
        let rows = plan
            .steps()
            .into_iter()
            .map(|step| {
                let (title, subtitle) = step_text(step, &plan, key_label.as_deref());
                // Subtitles hold paths, URLs and key labels like
                // `Name <mail@host>`: turn markup off before setting them.
                let row = adw::ActionRow::new();
                row.set_use_markup(false);
                row.set_title(&title);
                row.set_subtitle(&subtitle);
                let icon = gtk::Image::builder().visible(false).build();
                let spinner = gtk::Spinner::builder().visible(false).build();
                row.add_suffix(&spinner);
                row.add_suffix(&icon);
                imp.steps_list.append(&row);
                StepRow {
                    step,
                    icon,
                    spinner,
                }
            })
            .collect();
        imp.run.replace(Some(SetupRun {
            plan,
            rows,
            done: 0,
        }));

        imp.error_label.set_visible(false);
        imp.skip_button.set_visible(false);
        imp.apply_button.set_label("Create Vault");
        imp.apply_button.set_sensitive(true);
        imp.review_back_button.set_sensitive(true);
        imp.review_page.set_can_pop(true);
    }

    fn apply(&self) {
        let imp = self.imp();
        if imp.running.get() {
            return;
        }
        let finished = imp
            .run
            .borrow()
            .as_ref()
            .is_some_and(|run| run.done == run.rows.len());
        if finished {
            // "Open Vault" after an earlier attempt to open it failed
            // (e.g. restart required): just ask the window again.
            self.emit_vault_created();
            return;
        }

        imp.running.set(true);
        imp.apply_button.set_sensitive(false);
        imp.review_back_button.set_sensitive(false);
        imp.skip_button.set_visible(false);
        imp.error_label.set_visible(false);
        let wizard = self.clone();
        glib::spawn_future_local(async move {
            wizard.run_steps().await;
            let imp = wizard.imp();
            imp.running.set(false);
            imp.apply_button.set_sensitive(true);
        });
    }

    async fn run_steps(&self) {
        let imp = self.imp();
        let Some((plan, steps, start)) = imp.run.borrow().as_ref().map(|run| {
            let steps = run.rows.iter().map(|row| row.step).collect::<Vec<_>>();
            (run.plan.clone(), steps, run.done)
        }) else {
            return;
        };

        if start == 0 {
            let checked = plan.clone();
            let result = gio::spawn_blocking(move || setup::check_plan(&checked)).await;
            if let Err(message) = flatten(result) {
                self.show_failure(&message, None);
                imp.review_back_button.set_sensitive(true);
                return;
            }
        }

        for (index, step) in steps.into_iter().enumerate().skip(start) {
            self.set_row_state(index, StepState::Running);
            let step_plan = plan.clone();
            let result = gio::spawn_blocking(move || setup::run_step(&step_plan, step)).await;
            if let Err(message) = flatten(result) {
                self.set_row_state(index, StepState::Failed);
                self.show_failure(&message, Some(step));
                imp.review_back_button.set_sensitive(self.done_count() == 0);
                return;
            }
            self.set_row_state(index, StepState::Done);
            if let Some(run) = imp.run.borrow_mut().as_mut() {
                run.done = index + 1;
            }
            // Something is on disk now: going back to change the plan
            // would leave it half applied.
            imp.review_page.set_can_pop(false);
        }

        imp.apply_button.set_label("Open Vault");
        self.emit_vault_created();
    }

    /// After the vault itself exists, a failing remote step can be
    /// skipped: the vault works locally and sync can be set up later.
    fn skip_remaining(&self) {
        let imp = self.imp();
        if imp.running.get() {
            return;
        }
        if let Some(run) = imp.run.borrow_mut().as_mut() {
            for row in &run.rows[run.done..] {
                row.set_state(StepState::Skipped);
            }
            run.done = run.rows.len();
        }
        imp.skip_button.set_visible(false);
        imp.error_label.set_visible(false);
        imp.apply_button.set_label("Open Vault");
        self.emit_vault_created();
    }

    fn show_failure(&self, message: &str, step: Option<SetupStep>) {
        let imp = self.imp();
        imp.error_label.set_label(message);
        imp.error_label.set_visible(true);
        imp.apply_button.set_label("Retry");
        let can_skip = matches!(step, Some(SetupStep::ConnectRemote | SetupStep::Upload));
        imp.skip_button.set_visible(can_skip);
    }

    fn set_row_state(&self, index: usize, state: StepState) {
        if let Some(row) = self
            .imp()
            .run
            .borrow()
            .as_ref()
            .and_then(|run| run.rows.get(index))
        {
            row.set_state(state);
        }
    }

    fn done_count(&self) -> usize {
        self.imp().run.borrow().as_ref().map_or(0, |run| run.done)
    }

    fn emit_vault_created(&self) {
        let store_dir = self
            .imp()
            .run
            .borrow()
            .as_ref()
            .map(|run| run.plan.store_dir.to_string_lossy().into_owned());
        if let Some(store_dir) = store_dir {
            self.emit_by_name::<()>("vault-created", &[&store_dir]);
        }
    }
}

/// Folds a worker panic into the same user-facing error path.
fn flatten(
    result: Result<Result<(), crate::app::app_error::AppError>, Box<dyn std::any::Any + Send>>,
) -> Result<(), String> {
    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(err)) => Err(err.to_string()),
        Err(_) => Err("Setup task panicked unexpectedly".to_string()),
    }
}

/// Review-page wording for a step. Avoids git vocabulary on purpose.
fn step_text(step: SetupStep, plan: &SetupPlan, key_label: Option<&str>) -> (String, String) {
    match step {
        SetupStep::CreateFolder => (
            "Create the vault folder".to_string(),
            plan.store_dir.to_string_lossy().into_owned(),
        ),
        SetupStep::StartHistory => (
            "Turn on history".to_string(),
            "Record every change so it can be undone".to_string(),
        ),
        SetupStep::SaveRecipient => (
            "Encrypt entries with your key".to_string(),
            key_label.unwrap_or(plan.recipient.as_str()).to_string(),
        ),
        SetupStep::ConnectRemote => (
            "Connect to the remote".to_string(),
            plan.remote_url().unwrap_or_default().to_string(),
        ),
        SetupStep::Upload => (
            "Upload the vault".to_string(),
            "Send the first version to the remote".to_string(),
        ),
    }
}
