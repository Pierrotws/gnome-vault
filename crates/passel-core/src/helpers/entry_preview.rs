use crate::pass::model::{EntryData, EntryField};

/// Builds the one-line subtitle shown on group-view rows.
///
/// Returns the first `Plain` custom field as `key: value`. OTP,
/// Password, and Multiline variants are skipped: OTP/Password are
/// secret-shaped and shouldn't surface in a list row, and Multiline's
/// first line is rarely a meaningful summary. Returns `None` when the
/// entry has no Plain field, in which case the row is rendered with no
/// subtitle.
pub fn subtitle(entry: &EntryData) -> Option<String> {
    let (key, value) = entry
        .fields
        .iter()
        .find(|(_, value)| matches!(value, EntryField::Plain(_)))?;
    let value = value.display_value();
    let first_line = value.lines().next().unwrap_or("").trim();

    Some(if first_line.is_empty() {
        format!("{key}:")
    } else {
        format!("{key}: {first_line}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(fields: Vec<(&str, EntryField)>) -> EntryData {
        EntryData {
            password: EntryField::Password("secret".into()),
            fields: fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        }
    }

    #[test]
    fn subtitle_returns_first_plain_field() {
        let data = entry(vec![
            ("login", EntryField::Plain("alice@example.com".into())),
            ("note", EntryField::Multiline("ignored".into())),
        ]);
        assert_eq!(
            subtitle(&data),
            Some("login: alice@example.com".to_string())
        );
    }

    #[test]
    fn subtitle_skips_non_plain_variants_to_find_a_plain_field() {
        let data = entry(vec![
            ("totp", EntryField::Otp("otpauth://...".into())),
            ("notes", EntryField::Multiline("hidden\nlines".into())),
            ("user", EntryField::Plain("bob".into())),
        ]);
        assert_eq!(subtitle(&data), Some("user: bob".to_string()));
    }

    #[test]
    fn subtitle_is_none_when_no_plain_field_exists() {
        let data = entry(vec![
            ("totp", EntryField::Otp("otpauth://...".into())),
            ("notes", EntryField::Multiline("a\nb".into())),
        ]);
        assert_eq!(subtitle(&data), None);
    }

    #[test]
    fn subtitle_renders_empty_value_as_key_colon() {
        let data = entry(vec![("user", EntryField::Plain(String::new()))]);
        assert_eq!(subtitle(&data), Some("user:".to_string()));
    }
}
