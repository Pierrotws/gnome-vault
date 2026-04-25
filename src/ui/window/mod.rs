mod imp;

use std::cell::RefCell;
use std::rc::Rc;

use adw::{prelude::*, subclass::prelude::*};
use gtk::{gio, glib};

use crate::app::controller::AppController;
use crate::ui::vault_view::{build_selection_from_nodes, build_tree_factory};
use crate::ui::EntryView;

glib::wrapper! {
    pub struct MainWindow(ObjectSubclass<imp::MainWindow>)
        @extends adw::ApplicationWindow, gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk::Accessible, gtk::Buildable,
                    gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl MainWindow {
    pub fn new(app: &adw::Application, controller: Rc<RefCell<AppController>>) -> Self {
        let obj: Self = glib::Object::builder().property("application", app).build();

        let _ = obj.imp().controller.set(controller);

        obj.setup_views();
        obj.setup_callbacks();
        obj
    }

    fn controller(&self) -> Rc<RefCell<AppController>> {
        self.imp()
            .controller
            .get()
            .expect("MainWindow controller must be set")
            .clone()
    }

    fn setup_views(&self) {
        let imp = self.imp();

        let controller = self.controller();

        let nodes = {
            let mut controller = controller.borrow_mut();
            if let Err(err) = controller.reload_tree() {
                imp.entry_view.show_error(&err.to_string());
                Vec::new()
            } else {
                controller.state().tree().to_vec()
            }
        };

        let selection = build_selection_from_nodes(&nodes);
        imp.vault_view.set_selection_model(&selection);
        imp.vault_view.set_factory(Some(&build_tree_factory()));

        imp.entry_view.display_empty();
        self.set_edit_unlock_state(false, false, false);
    }

    pub fn setup_callbacks(&self) {
        let imp = self.imp();

        {
            let controller = self.controller();
            let entry_view = imp.entry_view.clone();
            let window = self.clone();

            imp.vault_view.connect_entry_selected(move |nav| {
                let Some(node) = nav.selected_node() else {
                    return;
                };

                let result = {
                    let mut controller = controller.borrow_mut();
                    controller.open_node(node)
                };

                match result {
                    Ok(data) => {
                        entry_view.set_entry_data(&data);
                        window.set_edit_unlock_state(true, false, false);
                        let is_dirty = controller.borrow().has_unsaved_changes();
                        entry_view.set_cancellable(is_dirty);
                        let is_valid = controller.borrow().has_valid_changes();
                        entry_view.set_saveable(is_valid);
                    }
                    Err(err) => entry_view.show_error(&err.to_string()),
                }
            });
        }

        {
            let controller = self.controller();
            let window = self.clone();

            imp.entry_view.connect_entry_changed(move |view| {
                let updated = view.to_entry_view_data();

                let result = {
                    let mut controller = controller.borrow_mut();
                    controller.update_current_entry(updated)
                };

                match result {
                    Ok(()) => {
                        let is_dirty = controller.borrow().has_unsaved_changes();
                        view.set_cancellable(is_dirty);
                        let is_valid = controller.borrow().has_valid_changes();
                        view.set_saveable(is_valid);
                        window.set_edit_unlock_state(true, view.is_editable_mode(), is_dirty);
                    }
                    Err(err) => view.show_error(&err.to_string()),
                }
            });
        }

        {
            let controller = self.controller();
            let window = self.clone();

            imp.entry_view.connect_save_requested(move |view| {
                let result = controller.borrow_mut().save_current_entry();

                match result {
                    Ok(()) => {
                        view.set_editable_mode(false);
                        window.set_edit_unlock_state(true, false, false);
                        let is_dirty = controller.borrow().has_unsaved_changes();
                        view.set_cancellable(is_dirty);
                        let is_valid = controller.borrow().has_valid_changes();
                        view.set_saveable(is_valid);
                    }
                    Err(err) => view.show_error(&err.to_string()),
                }
            });
        }

        {
            let controller = self.controller();
            let entry_view = imp.entry_view.clone();
            let window = self.clone();

            imp.entry_view.connect_revert_requested(move |_| {
                let result = controller.borrow_mut().revert_current_entry();

                match result {
                    Ok(entry_data) => {
                        entry_view.set_entry_data(&entry_data);
                        window.set_edit_unlock_state(true, false, false);
                        let is_dirty = controller.borrow().has_unsaved_changes();
                        entry_view.set_cancellable(is_dirty);
                        let is_valid = controller.borrow().has_valid_changes();
                        entry_view.set_saveable(is_valid);
                    }
                    Err(err) => entry_view.show_error(&err.to_string()),
                }
            });
        }

        {
            let entry_view = imp.entry_view.clone();
            let window = self.clone();

            imp.lock_vault_button.connect_clicked(move |_| {
                let editing = !entry_view.is_editable_mode();
                entry_view.set_editable_mode(editing);
                window.set_edit_unlock_state(true, editing, false);
            });
        }

        imp.vault_view.connect_search_changed(move |nav| {
            nav.handle_search_changed();
        });
    }

    pub fn get_entry_view(&self) -> EntryView {
        self.imp().entry_view.clone()
    }

    fn set_edit_unlock_state(&self, has_entry: bool, editing: bool, is_dirty: bool) {
        let imp = self.imp();
        imp.lock_vault_button.set_sensitive(has_entry && !is_dirty);
        imp.lock_vault_button.set_icon_name(if editing {
            "changes-allow-symbolic"
        } else {
            "system-lock-screen-symbolic"
        });
        imp.lock_vault_button.set_tooltip_text(Some(if editing {
            "Editing unlocked"
        } else {
            "Unlock editing"
        }));
        imp.window_title
            .set_subtitle(if editing { "Editing" } else { "Read-only" });
    }
}
