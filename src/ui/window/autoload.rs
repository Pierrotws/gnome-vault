//! Background entry-cache warmup.
//!
//! On startup (when the GSettings flag is on) every uncached entry is
//! decrypted by the app layer; results are streamed back to the main loop
//! and inserted into the controller's entry cache.

use std::sync::mpsc;
use std::time::Duration;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;

use crate::app::cache_warmup::{self, CacheWarmupMessage};

use super::MainWindow;

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
        let receiver = cache_warmup::load_entries(nodes);

        let window = self.clone();
        glib::timeout_add_local(Duration::from_millis(100), move || {
            let mut finished = false;

            for _ in 0..32 {
                match receiver.try_recv() {
                    Ok(CacheWarmupMessage::Loaded(node, entry)) => {
                        window
                            .controller()
                            .borrow_mut()
                            .cache_loaded_entry(&node, entry);
                    }
                    Ok(CacheWarmupMessage::Failed(path, err)) => {
                        log::warn!("Failed to autoload {}: {err}", path.display());
                    }
                    Ok(CacheWarmupMessage::Finished) => {
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

            // Tree/right-pane refresh is deferred to autoload completion.
            // Refreshing every tick (100 ms) tore down the search-results
            // rows mid-click; the user could not reliably select an entry.
            // While autoload runs, the search the user typed reflects
            // whatever is in cache so far; new field-content matches will
            // appear when they keep typing (each keystroke rebuilds) or
            // when autoload finishes.
            if finished {
                window.imp().autoload_running.set(false);
                if !window
                    .imp()
                    .vault_view
                    .search_entry()
                    .text()
                    .trim()
                    .is_empty()
                {
                    window.rebuild_vault_tree();
                }
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
    }
}
