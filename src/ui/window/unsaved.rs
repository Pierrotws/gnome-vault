use std::cell::Cell;

use adw::{prelude::*, subclass::prelude::*};

use super::MainWindow;

type Continuation = Box<dyn FnOnce(&MainWindow)>;

impl MainWindow {
    /// Runs `then` once the open entry has no unsaved changes: right
    /// away when it has none, otherwise after the user agrees to discard
    /// them. Nothing happens if they keep editing.
    pub(super) fn discard_changes_then(&self, then: impl FnOnce(&Self) + 'static) {
        self.ask_discard_changes(Box::new(then), Box::new(|_| {}));
    }

    /// Like [`Self::discard_changes_then`], for a click in the vault tree
    /// that already moved its selection: keeping the changes puts the
    /// selection back on what the window shows.
    pub(super) fn discard_changes_then_or_restore_selection(
        &self,
        then: impl FnOnce(&Self) + 'static,
    ) {
        self.ask_discard_changes(
            Box::new(then),
            Box::new(|window| window.restore_tree_selection()),
        );
    }

    fn ask_discard_changes(&self, then: Continuation, kept: Continuation) {
        if !self.controller().borrow().has_unsaved_changes() {
            then(self);
            return;
        }

        let dialog = adw::AlertDialog::builder()
            .heading("Discard changes?")
            .body("The changes to this entry have not been saved.")
            .build();
        dialog.add_responses(&[("cancel", "_Cancel"), ("discard", "_Discard")]);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);

        let window = self.clone();
        let continuations = Cell::new(Some((then, kept)));
        dialog.connect_response(None, move |_, response| {
            let Some((then, kept)) = continuations.take() else {
                return;
            };
            if response == "discard" {
                window.discard_current_changes();
                then(&window);
            } else {
                kept(&window);
            }
        });
        dialog.present(Some(self));
    }

    /// Reverts the open entry to its saved state and leaves edit mode.
    fn discard_current_changes(&self) {
        let result = self.controller().borrow_mut().revert_current_entry();
        match result {
            Ok(data) => self.imp().entry_view.set_entry_data(&data),
            Err(err) => log::debug!("nothing to discard: {err}"),
        }
    }

    /// Selects in the tree what the window shows: the open entry in the
    /// tree view, the shown folder in the group view (none for the
    /// vault's top level).
    fn restore_tree_selection(&self) {
        let imp = self.imp();
        let target = if self.show_group_view_enabled() {
            imp.shown_group.borrow().clone()
        } else {
            self.controller()
                .borrow()
                .current_session()
                .map(|session| session.node().path.clone())
        };
        imp.vault_view.select_quietly(target.as_deref());
    }
}
