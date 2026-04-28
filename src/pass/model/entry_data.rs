use crate::pass::model::EntryField;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Represents a decrypted password-store entry.
///
/// Field contents are wiped from memory on drop via `ZeroizeOnDrop`.
#[derive(Clone, PartialEq, Zeroize, ZeroizeOnDrop)]
pub struct EntryData {
    pub password: EntryField,
    pub fields: Vec<(String, EntryField)>,
}

#[cfg(not(test))]
impl std::fmt::Debug for EntryData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntryData")
            .field("password", &self.password)
            .field("fields", &format_args!("[{} field(s)]", self.fields.len()))
            .finish()
    }
}

#[cfg(test)]
impl std::fmt::Debug for EntryData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntryData")
            .field("password", &self.password)
            .field("fields", &self.fields)
            .finish()
    }
}
