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
        let search_text = self.imp().vault_view.search_entry().text().to_string();
        if search_text.trim().is_empty() {
            let root = self.root_group_node();
            self.show_group_content(&root);
        } else {
            self.show_search_results(&search_text);
        }
    }

    /// Renders the right pane as a flat list of every leaf entry that
    /// `controller.filtered_tree(search_text)` keeps. Each row's subtitle is
    /// the parent group's path so the user can tell which group a hit lives
    /// in. No async preview load is started here — the parent path is the
    /// useful context, not the first decrypted field.
    fn show_search_results(&self, search_text: &str) {
        let imp = self.imp();
        let nodes = self.controller().borrow().filtered_tree(search_text);
        let mut leaves: Vec<(PassNode, Option<PassNode>)> = Vec::new();
        collect_matching_leaves(&nodes, None, &mut leaves);

        let pseudo_group = PassNode {
            name: format!("Results for \u{201C}{search_text}\u{201D}"),
            path: PathBuf::new(),
            kind: PassNodeKind::Group,
            children: Vec::new(),
        };
        let entries: Vec<GroupEntry> = leaves
            .into_iter()
            .map(|(node, parent)| GroupEntry {
                node,
                subtitle: parent.map(|p| {
                    let path = p.path.to_string_lossy();
                    if path.is_empty() {
                        "Vault".to_string()
                    } else {
                        path.into_owned()
                    }
                }),
            })
            .collect();

        imp.group_view.set_group(&pseudo_group, &entries);
        imp.content_stack.set_visible_child_name("empty");
        imp.lock_vault_button.set_sensitive(false);
        imp.window_title.set_subtitle("Search");
        // Bump the preview generation so any in-flight subtitle load from a
        // previous (non-search) group view doesn't overwrite parent-path
        // subtitles when its results trickle in.
        let generation = self.imp().group_preview_generation.get().wrapping_add(1);
        self.imp().group_preview_generation.set(generation);
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

        let entries: Vec<PassNode> = group
            .children
            .iter()
            .filter(|child| child.is_entry())
            .cloned()
            .collect();
        if entries.is_empty() {
            return;
        }

        // Cache hits are filled in synchronously on the main thread so the
        // user never sees a blank-subtitle flicker for entries autoload (or
        // a recent save) has already populated. Only the cache misses are
        // dispatched to the decrypt worker.
        let mut to_load: Vec<(usize, PassNode)> = Vec::new();
        {
            let controller = self.controller();
            let controller = controller.borrow();
            for (index, node) in entries.iter().enumerate() {
                if let Some(cached) = controller.state().cached_entry(&node.path) {
                    let subtitle = entry_preview::subtitle(cached);
                    self.imp()
                        .group_view
                        .update_entry_subtitle(index, node, subtitle.as_deref());
                } else {
                    to_load.push((index, node.clone()));
                }
            }
        }
        if to_load.is_empty() {
            return;
        }

        let receiver = group_preview::load_group_previews(to_load);

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

/// Walks `nodes` in tree order and pushes every entry leaf into `out`,
/// pairing it with its immediate parent group (if any). Used to flatten a
/// `filtered_tree` result into the per-row data the search-results pane
/// needs.
fn collect_matching_leaves(
    nodes: &[PassNode],
    parent: Option<&PassNode>,
    out: &mut Vec<(PassNode, Option<PassNode>)>,
) {
    for node in nodes {
        if node.is_entry() {
            out.push((node.clone(), parent.cloned()));
        } else {
            collect_matching_leaves(&node.children, Some(node), out);
        }
    }
}
