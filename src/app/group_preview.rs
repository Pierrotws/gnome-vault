use std::sync::mpsc;

use crate::pass::model::{EntryData, PassNode};

use super::controller::AppController;

pub enum GroupPreviewMessage {
    Loaded {
        index: usize,
        node: PassNode,
        entry: EntryData,
    },
    Failed {
        node: PassNode,
        error: String,
    },
    Finished,
}

pub fn load_group_previews(nodes: Vec<PassNode>) -> mpsc::Receiver<GroupPreviewMessage> {
    let (sender, receiver) = mpsc::channel();

    std::thread::spawn(move || {
        for (index, node) in nodes.into_iter().enumerate() {
            let message = match AppController::load_entry_for_cache(&node) {
                Ok(entry) => GroupPreviewMessage::Loaded { index, node, entry },
                Err(err) => GroupPreviewMessage::Failed {
                    node,
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
