use std::borrow::Cow;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Typed value stored in a password-store entry.
///
/// Field contents are wiped from memory on drop via `ZeroizeOnDrop`.
#[derive(Clone, PartialEq, Zeroize, ZeroizeOnDrop)]
pub enum EntryField {
    /// The first line secret of a pass entry.
    Password(String),
    /// A single-line `key: value` field.
    Plain(String),
    /// One-time password URI or secret.
    Otp(String),
    /// A YAML-style block scalar field. The parser also folds legacy
    /// YAML sequences (`key:\n  - elem1\n  - elem2`) into this variant
    /// by joining items with newlines, so on-disk arrays still
    /// round-trip when the user opens an existing vault.
    Multiline(String),
}

#[cfg(not(test))]
impl std::fmt::Debug for EntryField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            EntryField::Password(_) => "Password",
            EntryField::Plain(_) => "Plain",
            EntryField::Otp(_) => "OTP",
            EntryField::Multiline(_) => "Multiline",
        };
        write!(f, "{name}(<redacted>)")
    }
}

#[cfg(test)]
impl std::fmt::Debug for EntryField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntryField::Password(v) => f.debug_tuple("Password").field(v).finish(),
            EntryField::Plain(v) => f.debug_tuple("Plain").field(v).finish(),
            EntryField::Otp(v) => f.debug_tuple("OTP").field(v).finish(),
            EntryField::Multiline(v) => f.debug_tuple("Multiline").field(v).finish(),
        }
    }
}

impl EntryField {
    /// Returns a display-oriented string for the field value.
    ///
    /// Storage formatting is handled separately by `helpers::parser`.
    pub fn display_value(&self) -> Cow<'_, str> {
        match self {
            EntryField::Password(s)
            | EntryField::Plain(s)
            | EntryField::Otp(s)
            | EntryField::Multiline(s) => Cow::Borrowed(s),
        }
    }
}
