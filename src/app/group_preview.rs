use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

use crate::pass::model::{EntryData, PassNode};

use super::controller::AppController;

pub enum GroupPreviewMessage {
    Loaded {
        index: usize,
        node: PassNode,
        entry: EntryData,
    },
    /// `index` is the original group-view row position. The failed
    /// node's `PassNode` (which carries the entry path / name) is
    /// deliberately NOT threaded through — pass-store paths are the
    /// secret labels and the receive site logs to stderr/journalctl.
    Failed {
        index: usize,
        error: String,
    },
    Finished,
}

/// Decrypts each entry in `items` on a worker thread and streams the
/// results back via the returned channel. The caller supplies the row
/// index alongside each node so cache-hit / cache-miss splits can
/// preserve the original positions in the group view. The worker
/// checks `cancel` before each entry so navigating away (or starting a
/// new preview load) stops the loop instead of running to completion.
pub fn load_group_previews(
    items: Vec<(usize, PassNode)>,
    cancel: Arc<AtomicBool>,
) -> mpsc::Receiver<GroupPreviewMessage> {
    let (sender, receiver) = mpsc::channel();

    std::thread::spawn(move || {
        for (index, node) in items {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let message = match AppController::load_entry_for_cache(&node) {
                Ok(entry) => GroupPreviewMessage::Loaded { index, node, entry },
                Err(err) => GroupPreviewMessage::Failed {
                    index,
                    error: err.to_string(),
                },
            };
            if sender.send(message).is_err() {
                return;
            }
        }

        let _ = sender.send(GroupPreviewMessage::Finished);
    });

    receiver
}
