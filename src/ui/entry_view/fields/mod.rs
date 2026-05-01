mod array_field_row;
mod multiline_field_row;
mod otp_field_row;
mod password_field_row;
mod plain_field_row;

use gtk::{glib, prelude::*};

use crate::pass::model::EntryField;
use crate::ui::entry_view::EntryView;

pub use array_field_row::ArrayFieldRow;
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

/// Toggles the standard "this entry is read-only outside edit mode"
/// flags on a `gtk::Entry`. Plain, multiline, and array rows all set
/// the same three properties when entering / leaving edit mode; the
/// helper centralizes the policy.
pub(super) fn set_entry_editable(entry: &gtk::Entry, editable: bool) {
    entry.set_editable(editable);
    entry.set_can_focus(editable);
    entry.set_has_frame(editable);
}

/// Wires a field row's delete button to the standard "remove from
/// parent gtk::Box and mark the EntryView dirty" behavior. Captures
/// `row` and `entry_view` weakly so the closure cannot keep them alive
/// past their lifetime, mirroring the per-row pattern that the four
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
