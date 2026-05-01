use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

use crate::pass::model::{EntryData, PassNode};

use super::controller::AppController;

pub enum CacheWarmupMessage {
    Loaded(PassNode, EntryData),
    Failed(PathBuf, String),
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
        for node in nodes {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let message = match AppController::load_entry_for_cache(&node) {
                Ok(entry) => CacheWarmupMessage::Loaded(node, entry),
                Err(err) => CacheWarmupMessage::Failed(node.path.clone(), err.to_string()),
            };
            if sender.send(message).is_err() {
                return;
            }
        }

        let _ = sender.send(CacheWarmupMessage::Finished);
    });

    receiver
}
