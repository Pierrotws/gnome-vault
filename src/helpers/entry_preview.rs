use crate::pass::model::EntryData;

pub fn subtitle(entry: &EntryData) -> Option<String> {
    let (key, value) = entry.fields.first()?;
    let value = value.display_value();
    let first_line = value.lines().next().unwrap_or("").trim();

    Some(if first_line.is_empty() {
        format!("{key}:")
    } else {
        format!("{key}: {first_line}")
    })
}
