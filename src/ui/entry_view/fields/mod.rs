mod array_field_row;
mod multiline_field_row;
mod password_field_row;
mod plain_field_row;

use crate::pass::model::EntryField;

pub use array_field_row::ArrayFieldRow;
pub use multiline_field_row::MultilineFieldRow;
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
