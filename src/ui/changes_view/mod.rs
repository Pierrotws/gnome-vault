mod imp;

use std::time::{SystemTime, UNIX_EPOCH};

use adw::subclass::prelude::*;
use gtk::{gdk, glib, prelude::*};

use crate::helpers::git::GitChange;

glib::wrapper! {
    pub struct ChangesView(ObjectSubclass<imp::ChangesView>)
        @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl ChangesView {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn setup_callbacks(&self) {
        let adjustment = self.imp().changes_scrolled_window.vadjustment();
        adjustment.connect_value_changed(glib::clone!(
            #[weak(rename_to = this)]
            self,
            move |adjustment| {
                if !this.should_request_more(adjustment) {
                    return;
                }

                this.imp().is_loading_more.set(true);
                this.emit_by_name::<()>("load-more-requested", &[]);
            }
        ));
    }

    pub fn set_changes(&self, changes: &[GitChange]) {
        let imp = self.imp();
        while let Some(child) = imp.changes_box.first_child() {
            imp.changes_box.remove(&child);
        }

        imp.empty_label.set_visible(changes.is_empty());
        imp.has_more_changes.set(!changes.is_empty());
        imp.is_loading_more.set(false);
        for change in changes {
            imp.changes_box.append(&self.build_change_row(change));
        }
    }

    pub fn append_changes(&self, changes: &[GitChange]) {
        let imp = self.imp();
        let has_existing_changes = imp.changes_box.first_child().is_some();

        imp.empty_label
            .set_visible(!has_existing_changes && changes.is_empty());
        imp.has_more_changes.set(!changes.is_empty());
        imp.is_loading_more.set(false);
        for change in changes {
            imp.changes_box.append(&self.build_change_row(change));
        }
    }

    pub fn set_has_more_changes(&self, has_more_changes: bool) {
        self.imp().has_more_changes.set(has_more_changes);
    }

    pub fn set_loading_more(&self, is_loading_more: bool) {
        self.imp().is_loading_more.set(is_loading_more);
    }

    pub fn connect_push_requested<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_local("push-requested", false, move |values| {
            let obj = values[0]
                .get::<ChangesView>()
                .expect("push-requested: first arg should be ChangesView");
            f(&obj);
            None
        })
    }

    pub fn connect_load_more_requested<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self) + 'static,
    {
        self.connect_local("load-more-requested", false, move |values| {
            let obj = values[0]
                .get::<ChangesView>()
                .expect("load-more-requested: first arg should be ChangesView");
            f(&obj);
            None
        })
    }

    pub fn connect_revert_change_requested<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, String) + 'static,
    {
        self.connect_local("revert-change-requested", false, move |values| {
            let obj = values[0]
                .get::<ChangesView>()
                .expect("revert-change-requested: first arg should be ChangesView");
            let commit_id = values[1]
                .get::<String>()
                .expect("revert-change-requested: second arg should be String");
            f(&obj, commit_id);
            None
        })
    }

    pub fn connect_rollback_change_requested<F>(&self, f: F) -> glib::SignalHandlerId
    where
        F: Fn(&Self, String) + 'static,
    {
        self.connect_local("rollback-change-requested", false, move |values| {
            let obj = values[0]
                .get::<ChangesView>()
                .expect("rollback-change-requested: first arg should be ChangesView");
            let commit_id = values[1]
                .get::<String>()
                .expect("rollback-change-requested: second arg should be String");
            f(&obj, commit_id);
            None
        })
    }

    fn build_change_row(&self, change: &GitChange) -> gtk::Box {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.set_margin_top(6);
        row.set_margin_bottom(6);
        row.set_margin_start(6);
        row.set_margin_end(6);

        let icon = gtk::Image::from_icon_name("view-list-symbolic");
        icon.set_valign(gtk::Align::Start);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 2);
        content.set_hexpand(true);

        let title = gtk::Label::new(None);
        title.set_xalign(0.0);
        title.set_wrap(true);
        if change.is_pushed {
            title.set_label(&change.summary);
        } else {
            title.set_use_markup(true);
            title.set_markup(&format!(
                "<i>{}</i>",
                glib::markup_escape_text(&change.summary)
            ));
        }

        let meta = gtk::Label::new(Some(&format!(
            "{} · {}",
            change.author,
            format_author_time(change)
        )));
        meta.set_xalign(0.0);
        meta.add_css_class("dim-label");
        // Surface the underlying commit id only as a tooltip so users who
        // want it (debugging, scripting) can still find it without it
        // cluttering the row.
        meta.set_tooltip_text(Some(&change.short_id));

        content.append(&title);
        content.append(&meta);
        row.append(&icon);
        row.append(&content);

        let commit_id = change.id.clone();
        let is_pushed = change.is_pushed;
        let this_weak = self.downgrade();
        let gesture = gtk::GestureClick::new();
        gesture.set_button(gdk::BUTTON_SECONDARY);
        gesture.connect_pressed(move |gesture, _, x, y| {
            let Some(this) = this_weak.upgrade() else {
                return;
            };
            let Some(widget) = gesture.widget() else {
                return;
            };

            let popover = gtk::Popover::new();
            popover.set_parent(&widget);
            popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));

            let actions = gtk::Box::new(gtk::Orientation::Vertical, 6);
            actions.set_margin_top(6);
            actions.set_margin_bottom(6);
            actions.set_margin_start(6);
            actions.set_margin_end(6);

            let undo_button = gtk::Button::with_label("Undo this change");
            let rollback_button = gtk::Button::with_label("Discard later changes");
            rollback_button.add_css_class("destructive-action");
            let push_button = (!is_pushed).then(|| gtk::Button::with_label("Sync to remote"));

            let undo_commit_id = commit_id.clone();
            let rollback_commit_id = commit_id.clone();
            undo_button.connect_clicked(glib::clone!(
                #[weak]
                popover,
                #[weak(rename_to = obj)]
                this,
                move |_| {
                    popover.popdown();
                    obj.emit_by_name::<()>("revert-change-requested", &[&undo_commit_id]);
                }
            ));
            rollback_button.connect_clicked(glib::clone!(
                #[weak]
                popover,
                #[weak(rename_to = obj)]
                this,
                move |_| {
                    popover.popdown();
                    obj.emit_by_name::<()>("rollback-change-requested", &[&rollback_commit_id]);
                }
            ));
            if let Some(push_button) = push_button {
                push_button.connect_clicked(glib::clone!(
                    #[weak]
                    popover,
                    #[weak(rename_to = obj)]
                    this,
                    move |_| {
                        popover.popdown();
                        obj.emit_by_name::<()>("push-requested", &[]);
                    }
                ));
                actions.append(&push_button);
            }

            actions.append(&undo_button);
            actions.append(&rollback_button);
            popover.set_child(Some(&actions));
            popover.popup();
        });
        row.add_controller(gesture);

        row
    }

    fn should_request_more(&self, adjustment: &gtk::Adjustment) -> bool {
        let imp = self.imp();
        if imp.is_loading_more.get() || !imp.has_more_changes.get() {
            return false;
        }

        adjustment.value() + adjustment.page_size() >= adjustment.upper() - 24.0
    }
}

fn format_author_time(change: &GitChange) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(change.author_time_seconds);

    format_relative_time(change.author_time_seconds, now)
}

fn format_relative_time(timestamp: i64, now: i64) -> String {
    let (seconds, is_future) = if timestamp > now {
        (timestamp - now, true)
    } else {
        (now - timestamp, false)
    };

    if seconds < 5 {
        return "just now".to_string();
    }

    let (value, unit) = if seconds < 60 {
        (seconds, "second")
    } else if seconds < 60 * 60 {
        (seconds / 60, "minute")
    } else if seconds < 60 * 60 * 24 {
        (seconds / (60 * 60), "hour")
    } else if seconds < 60 * 60 * 24 * 30 {
        (seconds / (60 * 60 * 24), "day")
    } else if seconds < 60 * 60 * 24 * 365 {
        (seconds / (60 * 60 * 24 * 30), "month")
    } else {
        (seconds / (60 * 60 * 24 * 365), "year")
    };

    let unit = if value == 1 {
        unit.to_string()
    } else {
        format!("{unit}s")
    };

    if is_future {
        format!("in {value} {unit}")
    } else {
        format!("{value} {unit} ago")
    }
}

impl Default for ChangesView {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::format_relative_time;

    #[test]
    fn formats_recent_times_as_just_now() {
        assert_eq!(format_relative_time(98, 100), "just now");
    }

    #[test]
    fn formats_past_relative_times() {
        assert_eq!(format_relative_time(40, 100), "1 minute ago");
        assert_eq!(format_relative_time(100, 100 + 60 * 3), "3 minutes ago");
        assert_eq!(format_relative_time(100, 100 + 60 * 60 * 2), "2 hours ago");
        assert_eq!(
            format_relative_time(100, 100 + 60 * 60 * 24 * 2),
            "2 days ago"
        );
    }

    #[test]
    fn formats_future_relative_times() {
        assert_eq!(format_relative_time(100 + 60 * 2, 100), "in 2 minutes");
    }
}
