//! Background entry-cache warmup.
//!
//! On startup (when the GSettings flag is on) every uncached entry is
//! decrypted on a worker thread; results are streamed back to the main loop
//! and inserted into the controller's entry cache.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;

use crate::app::controller::AppController;
use crate::pass::model::{EntryData, PassNode};

use super::MainWindow;

pub(super) enum AutoloadMessage {
    Loaded(PassNode, EntryData),
    Failed(PathBuf, String),
    Finished,
}

impl MainWindow {
    pub(super) fn start_autoload_if_enabled(&self) {
        if self.setting_boolean("autoload", false) {
            self.start_autoload_cache();
        }
    }

    pub(super) fn start_autoload_cache(&self) {
        let imp = self.imp();
        if imp.autoload_running.get() {
            return;
        }

        let nodes = self.controller().borrow().uncached_entry_nodes();
        if nodes.is_empty() {
            return;
        }

        imp.autoload_running.set(true);
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            for node in nodes {
                let message = match AppController::load_entry_for_cache(&node) {
                    Ok(entry) => AutoloadMessage::Loaded(node, entry),
                    Err(err) => AutoloadMessage::Failed(node.path.clone(), err.to_string()),
                };
                if sender.send(message).is_err() {
                    return;
                }
            }
            let _ = sender.send(AutoloadMessage::Finished);
        });

        let window = self.clone();
        glib::timeout_add_local(Duration::from_millis(100), move || {
            let mut finished = false;
            let mut loaded_any = false;

            for _ in 0..32 {
                match receiver.try_recv() {
                    Ok(AutoloadMessage::Loaded(node, entry)) => {
                        window
                            .controller()
                            .borrow_mut()
                            .cache_loaded_entry(&node, entry);
                        loaded_any = true;
                    }
                    Ok(AutoloadMessage::Failed(path, err)) => {
                        log::warn!("Failed to autoload {}: {err}", path.display());
                    }
                    Ok(AutoloadMessage::Finished) => {
                        finished = true;
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        finished = true;
                        break;
                    }
                }
            }

            if loaded_any
                && !window
                    .imp()
                    .vault_view
                    .search_entry()
                    .text()
                    .trim()
                    .is_empty()
            {
                window.rebuild_vault_tree();
            }

            if finished {
                window.imp().autoload_running.set(false);
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }
}
