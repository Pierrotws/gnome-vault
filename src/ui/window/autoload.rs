//! Background entry-cache warmup.
//!
//! On startup (when the GSettings flag is on) every uncached entry is
//! decrypted by the app layer; results are streamed back to the main loop
//! and inserted into the controller's entry cache.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;

use crate::app::cache_warmup::{self, CacheWarmupMessage};

use super::MainWindow;

/// Cadence at which the autoload tick drains the worker channel. Tuned
/// loose enough to keep main-loop work negligible during full-vault
/// warmups; the worker thread does the decryption regardless.
const AUTOLOAD_TICK: Duration = Duration::from_millis(100);

/// Maximum cached-entry installs per tick. Caps the worst-case time
/// spent inside one polling fire so a fast worker can't starve other
/// timers; remaining messages drain on the next tick.
const AUTOLOAD_MESSAGES_PER_TICK: usize = 32;

impl MainWindow {
    pub(super) fn start_autoload_if_enabled(&self) {
        if self.setting_boolean("autoload", false) {
            self.start_autoload_cache();
        }
    }

    pub(super) fn start_autoload_cache(&self) {
        let imp = self.imp();

        // Signal any in-flight worker to stop and immediately clear the
        // imp's running/cancel state so a fresh run can claim it. The
        // old timer still owns its own Arc<AtomicBool>; the ptr_eq
        // guard in its cleanup branches keeps it from clobbering the
        // state we install below when it observes the cancel.
        if let Some(prev) = imp.autoload_cancel.borrow_mut().take() {
            prev.store(true, Ordering::Relaxed);
        }
        imp.autoload_running.set(false);

        let nodes = self.controller().borrow().uncached_entry_nodes();
        if nodes.is_empty() {
            return;
        }

        imp.autoload_running.set(true);
        let cancel = Arc::new(AtomicBool::new(false));
        *imp.autoload_cancel.borrow_mut() = Some(Arc::clone(&cancel));
        let receiver = cache_warmup::load_entries(nodes, Arc::clone(&cancel));

        glib::timeout_add_local(
            AUTOLOAD_TICK,
            glib::clone!(
                #[weak(rename_to = window)]
                self,
                #[upgrade_or]
                glib::ControlFlow::Break,
                move || {
                    // Honor cancellation set by stop_autoload_cache or by a
                    // start_autoload_cache that replaced us; the worker thread
                    // checks the same flag, so we usually see Finished shortly
                    // after, but break early to free the timer.
                    if cancel.load(Ordering::Relaxed) {
                        clear_if_still_ours(&window, &cancel);
                        return glib::ControlFlow::Break;
                    }
                    let mut finished = false;

                    for _ in 0..AUTOLOAD_MESSAGES_PER_TICK {
                        match receiver.try_recv() {
                            Ok(CacheWarmupMessage::Loaded(node, entry)) => {
                                window
                                    .controller()
                                    .borrow_mut()
                                    .cache_loaded_entry(&node, entry);
                            }
                            Ok(CacheWarmupMessage::Failed { index, error }) => {
                                // The path is deliberately not threaded through:
                                // pass-store paths *are* the secret labels and
                                // this log can land in journalctl. The opaque
                                // index is enough to count failures.
                                log::warn!("Failed to autoload entry #{index}: {error}");
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
                        if clear_if_still_ours(&window, &cancel)
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
                        glib::ControlFlow::Break
                    } else {
                        glib::ControlFlow::Continue
                    }
                }
            ),
        );
    }

    /// Signals the in-flight autoload worker (if any) to stop. The
    /// worker breaks on its next loop iteration and the polling tick
    /// breaks on its next firing.
    pub(super) fn stop_autoload_cache(&self) {
        if let Some(flag) = self.imp().autoload_cancel.borrow().as_ref() {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

/// Clears the imp's autoload state only when the cell still points at
/// the timer's own cancel flag. Returns `true` if the clear happened.
/// Without this guard a stale timer that observes its own cancel could
/// erase a fresh run that `start_autoload_cache` installed in the same
/// 100 ms tick.
fn clear_if_still_ours(window: &MainWindow, cancel: &Arc<AtomicBool>) -> bool {
    let imp = window.imp();
    let still_ours = imp
        .autoload_cancel
        .borrow()
        .as_ref()
        .is_some_and(|c| Arc::ptr_eq(c, cancel));
    if still_ours {
        imp.autoload_running.set(false);
        imp.autoload_cancel.borrow_mut().take();
    }
    still_ours
}
