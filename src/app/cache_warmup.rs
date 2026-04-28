use std::path::PathBuf;
use std::sync::mpsc;

use crate::pass::model::{EntryData, PassNode};

use super::controller::AppController;

pub enum CacheWarmupMessage {
    Loaded(PassNode, EntryData),
    Failed(PathBuf, String),
    Finished,
}

pub fn load_entries(nodes: Vec<PassNode>) -> mpsc::Receiver<CacheWarmupMessage> {
    let (sender, receiver) = mpsc::channel();

    std::thread::spawn(move || {
        for node in nodes {
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
