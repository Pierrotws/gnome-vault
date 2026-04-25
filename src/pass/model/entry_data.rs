///Represents entry in pass
use crate::pass::model::EntryField;

#[derive(Debug, Clone, PartialEq)]
pub struct EntryData {
    pub password: EntryField,
    pub fields: Vec<(String, EntryField)>,
}
