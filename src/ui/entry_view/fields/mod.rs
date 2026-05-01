mod multiline_field_row;
mod otp_field_row;
mod password_field_row;
mod plain_field_row;

use gtk::{glib, prelude::*};

use crate::pass::model::EntryField;
use crate::ui::entry_view::EntryView;

pub use multiline_field_row::MultilineFieldRow;
pub use otp_field_row::OtpFieldRow;
pub use password_field_row::PasswordFieldRow;
pub use plain_field_row::PlainFieldRow;

pub trait EntryFieldRow {
    fn key(&self) -> String;
    fn entry_field(&self) -> EntryField;
    fn set_entry_field(&self, field: &EntryField);

    fn named_entry_field(&self) -> (String, EntryField) {
        (self.key(), self.entry_field())
    }
}

/// Behavior shared by the three custom-field row variants the user
/// can append to an entry (`PlainFieldRow`, `OtpFieldRow`,
/// `MultilineFieldRow`). PasswordFieldRow is the entry's primary
/// field — it isn't reorderable and doesn't have an "empty" notion —
/// so it deliberately doesn't implement this trait.
///
/// Centralizing the trio makes [`with_custom_field_row`] a single
/// downcast cascade per call rather than three open-coded ladders in
/// the entry view; adding a new variant is one impl plus one downcast
/// case in the helper.
pub trait CustomFieldRow: EntryFieldRow {
    fn set_editable_mode(&self, editable: bool);
    fn is_empty(&self) -> bool;
    fn drag_handle(&self) -> gtk::Widget;
}

/// Downcasts `widget` to whichever custom-field row variant it is and
/// runs `f` against the typed reference; returns `None` if the widget
/// is not a custom-field row.
pub(super) fn with_custom_field_row<F, R>(widget: &gtk::Widget, f: F) -> Option<R>
where
    F: FnOnce(&dyn CustomFieldRow) -> R,
{
    if let Ok(row) = widget.clone().downcast::<PlainFieldRow>() {
        return Some(f(&row));
    }
    if let Ok(row) = widget.clone().downcast::<OtpFieldRow>() {
        return Some(f(&row));
    }
    if let Ok(row) = widget.clone().downcast::<MultilineFieldRow>() {
        return Some(f(&row));
    }
    None
}

/// Toggles the standard "this entry is read-only outside edit mode"
/// flags on a `gtk::Entry`. Plain and multiline rows both set the
/// same three properties when entering / leaving edit mode; the
/// helper centralizes the policy.
pub(super) fn set_entry_editable(entry: &gtk::Entry, editable: bool) {
    entry.set_editable(editable);
    entry.set_can_focus(editable);
    entry.set_has_frame(editable);
}

/// Wires a field row's delete button to the standard "remove from
/// parent gtk::Box and mark the EntryView dirty" behavior. Captures
/// `row` and `entry_view` weakly so the closure cannot keep them alive
/// past their lifetime, mirroring the per-row pattern that the
/// custom-field rows previously duplicated.
pub(super) fn wire_delete_button(
    button: &gtk::Button,
    row: &gtk::Widget,
    entry_view: &EntryView,
) {
    button.connect_clicked(glib::clone!(
        #[weak(rename_to = this)]
        row,
        #[weak]
        entry_view,
        move |_| {
            if let Some(list) = this.parent() {
                if let Ok(container) = list.downcast::<gtk::Box>() {
                    container.remove(&this);
                    entry_view.mark_changed();
                }
            }
        }
    ));
}
