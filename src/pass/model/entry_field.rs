///Describe Type of entry
use std::borrow::Cow;

#[derive(Debug, Clone, PartialEq)]
pub enum EntryField {
    Password(String),
    Plain(String),
    OTP(String),
    Array(Vec<String>),
    Multiline(String),
}

impl<'a> EntryField {
    pub fn display_value(&'a self) -> Cow<'a, str> {
        match self {
            //Except for Array, return a borrowed value
            EntryField::Password(s)
            | EntryField::Plain(s)
            | EntryField::OTP(s)
            | EntryField::Multiline(s) => Cow::Borrowed(s),
            EntryField::Array(arr) => Cow::Owned(arr.join("\n")),
        }
    }
}
