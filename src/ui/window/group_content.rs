use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use adw::{prelude::*, subclass::prelude::*};
use gtk::glib;

use crate::app::group_preview::{self, GroupPreviewMessage};
use crate::helpers::entry_preview;
use crate::pass::model::{PassNode, PassNodeKind};
use crate::ui::GroupEntry;

use super::MainWindow;

impl MainWindow {
    pub(super) fn show_group_content(&self, node: &PassNode) {
        let imp = self.imp();
        let entries = self.group_entries_for(node);
        imp.group_view.set_group(node, &entries);
        imp.content_stack.set_visible_child_name("empty");
        imp.lock_vault_button.set_sensitive(false);
        imp.window_title.set_subtitle("Group");
        self.start_group_subtitle_load(node);
    }

    pub(super) fn show_root_group_content(&self) {
        let root = self.root_group_node();
        self.show_group_content(&root);
    }

    fn root_group_node(&self) -> PassNode {
        let search_text = self.imp().vault_view.search_entry().text().to_string();
        let children = self.controller().borrow().filtered_tree(&search_text);
        PassNode {
            name: "Vault".to_string(),
            path: PathBuf::new(),
            kind: PassNodeKind::Group,
            children,
        }
    }

    fn group_entries_for(&self, node: &PassNode) -> Vec<GroupEntry> {
        node.children
            .iter()
            .filter(|child| child.is_entry())
            .map(|child| GroupEntry {
                node: child.clone(),
                subtitle: None,
            })
            .collect()
    }

    fn start_group_subtitle_load(&self, group: &PassNode) {
        let generation = self.imp().group_preview_generation.get().wrapping_add(1);
        self.imp().group_preview_generation.set(generation);

        let nodes = group
            .children
            .iter()
            .filter(|child| child.is_entry())
            .cloned()
            .collect::<Vec<_>>();
        if nodes.is_empty() {
            return;
        }

        let receiver = group_preview::load_group_previews(nodes);

        let window = self.clone();
        glib::timeout_add_local(Duration::from_millis(50), move || {
            if window.imp().group_preview_generation.get() != generation {
                return glib::ControlFlow::Break;
            }

            for _ in 0..16 {
                match receiver.try_recv() {
                    Ok(GroupPreviewMessage::Loaded { index, node, entry }) => {
                        let subtitle = entry_preview::subtitle(&entry);
                        window
                            .controller()
                            .borrow_mut()
                            .cache_loaded_entry(&node, entry);
                        window.imp().group_view.update_entry_subtitle(
                            index,
                            &node,
                            subtitle.as_deref(),
                        );
                    }
                    Ok(GroupPreviewMessage::Failed { node, error }) => {
                        log::warn!(
                            "Failed to load entry preview for {}: {error}",
                            node.path.display()
                        );
                    }
                    Ok(GroupPreviewMessage::Finished) => return glib::ControlFlow::Break,
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => return glib::ControlFlow::Break,
                }
            }

            glib::ControlFlow::Continue
        });
    }
}
