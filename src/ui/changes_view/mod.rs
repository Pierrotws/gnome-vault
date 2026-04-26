mod imp;

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
        let this = self.clone();
        self.imp().push_button.connect_clicked(move |_| {
            this.emit_by_name::<()>("push-requested", &[]);
        });
    }

    pub fn set_changes(&self, changes: &[GitChange]) {
        let imp = self.imp();
        while let Some(child) = imp.changes_box.first_child() {
            imp.changes_box.remove(&child);
        }

        imp.empty_label.set_visible(changes.is_empty());
        imp.push_button
            .set_sensitive(changes.iter().any(|change| !change.is_pushed));
        for change in changes {
            imp.changes_box.append(&self.build_change_row(change));
        }
    }

    pub fn set_autopush_enabled(&self, autopush: bool) {
        self.imp().push_button.set_visible(!autopush);
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

        let meta = gtk::Label::new(Some(&format!("{} · {}", change.short_id, change.author)));
        meta.set_xalign(0.0);
        meta.add_css_class("dim-label");

        content.append(&title);
        content.append(&meta);
        row.append(&icon);
        row.append(&content);

        let commit_id = change.id.clone();
        let this = self.clone();
        let gesture = gtk::GestureClick::new();
        gesture.set_button(gdk::BUTTON_SECONDARY);
        gesture.connect_pressed(move |gesture, _, x, y| {
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

            let undo_button = gtk::Button::with_label("Undo Action");
            let rollback_button = gtk::Button::with_label("Rollback");
            rollback_button.add_css_class("destructive-action");

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

            actions.append(&undo_button);
            actions.append(&rollback_button);
            popover.set_child(Some(&actions));
            popover.popup();
        });
        row.add_controller(gesture);

        row
    }
}

impl Default for ChangesView {
    fn default() -> Self {
        Self::new()
    }
}
