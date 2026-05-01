use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

use crate::pass::model::{EntryData, PassNode};

use super::controller::AppController;

pub enum CacheWarmupMessage {
    Loaded(PassNode, EntryData),
    /// `index` is the position in the input `nodes` vec. It deliberately
    /// does NOT carry the failed entry's `PathBuf` — pass-store paths
    /// are themselves the secret labels, and the receive site logs to
    /// stderr/journalctl. The user can re-attempt by opening the entry
    /// directly, where the error surfaces in the UI.
    ///
    /// Named fields to match `GroupPreviewMessage::Failed`'s shape; both
    /// payloads carry the same (row index, redacted error) and the
    /// shared shape lets future code parameterize over them.
    Failed { index: usize, error: String },
    Finished,
}

/// Streams decrypted entries back to the main loop while honoring
/// `cancel`. The worker checks the flag before each entry so a
/// cancellation set by the UI stops the loop without finishing the
/// remaining nodes.
pub fn load_entries(
    nodes: Vec<PassNode>,
    cancel: Arc<AtomicBool>,
) -> mpsc::Receiver<CacheWarmupMessage> {
    let (sender, receiver) = mpsc::channel();

    std::thread::spawn(move || {
        for (index, node) in nodes.into_iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let message = match AppController::load_entry_for_cache(&node) {
                Ok(entry) => CacheWarmupMessage::Loaded(node, entry),
                Err(err) => CacheWarmupMessage::Failed {
                    index,
                    error: err.to_string(),
                },
            };
            if sender.send(message).is_err() {
                return;
            }
        }

        let _ = sender.send(CacheWarmupMessage::Finished);
    });

    receiver
}
