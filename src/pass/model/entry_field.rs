use std::borrow::Cow;

/// Typed value stored in a password-store entry.
#[derive(Debug, Clone, PartialEq)]
pub enum EntryField {
    /// The first line secret of a pass entry.
    Password(String),
    /// A single-line `key: value` field.
    Plain(String),
    /// One-time password URI or secret.
    OTP(String),
    /// A YAML-style sequence field.
    Array(Vec<String>),
    /// A YAML-style block scalar field.
    Multiline(String),
}

impl<'a> EntryField {
    /// Returns a display-oriented string for the field value.
    ///
    /// Arrays are joined with newlines for clipboard/UI display. Storage
    /// formatting is handled separately by `helpers::parser`.
    pub fn display_value(&'a self) -> Cow<'a, str> {
        match self {
            EntryField::Password(s)
            | EntryField::Plain(s)
            | EntryField::OTP(s)
            | EntryField::Multiline(s) => Cow::Borrowed(s),
            EntryField::Array(arr) => Cow::Owned(arr.join("\n")),
        }
    }
}
