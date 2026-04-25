use crate::pass::model::{EntryData, EntryField};

/// Parses decrypted pass entry text into an [`EntryData`] model.
///
/// The first line is always the secret/password. Remaining fields use the
/// pass-compatible YAML subset emitted by [`format_entry`]:
///
/// - `key: value` for plain scalar fields.
/// - `key: |` / `key: >` block scalars for multiline fields.
/// - `key:` followed by indented `- value` rows for array fields.
///
/// Malformed field lines are ignored, but an empty file returns `None`.
pub fn parse_entry(plaintext: &str) -> Option<EntryData> {
    let mut lines = plaintext.lines();
    let password = lines.next()?.to_string();
    let field_lines: Vec<&str> = lines.collect();

    Some(EntryData {
        password: EntryField::Password(password),
        fields: parse_fields(&field_lines),
    })
}

/// Formats an [`EntryData`] model back into pass entry plaintext.
///
/// This writes a conservative YAML-like subset rather than arbitrary YAML.
/// Scalars that could be misread by a YAML parser are double-quoted and escaped.
/// Multiline fields are always emitted as literal block scalars with an explicit
/// chomp indicator so trailing newlines roundtrip.
pub fn format_entry(entry: &EntryData) -> String {
    let mut out = String::new();
    let password_str = &entry.password.display_value();
    out.push_str(password_str);
    out.push('\n');

    for (key, value) in &entry.fields {
        if should_skip_field(key, value) {
            continue;
        }

        match value {
            EntryField::Array(values) => {
                out.push_str(&format_yaml_scalar(key));
                out.push_str(":\n");
                for item in values {
                    out.push_str("  - ");
                    out.push_str(&format_yaml_scalar(item));
                    out.push('\n');
                }
            }
            EntryField::Multiline(value) => {
                out.push_str(&format_yaml_scalar(key));
                let (indicator, body) = multiline_block_parts(value);
                out.push_str(": ");
                out.push_str(indicator);
                out.push('\n');
                for line in body.split('\n') {
                    out.push_str("  ");
                    out.push_str(line);
                    out.push('\n');
                }
            }
            EntryField::Password(value) | EntryField::Plain(value) | EntryField::OTP(value) => {
                out.push_str(&format_yaml_scalar(key));
                out.push_str(": ");
                out.push_str(&format_yaml_scalar(value));
                out.push('\n');
            }
        }
    }

    out
}

fn parse_fields(lines: &[&str]) -> Vec<(String, EntryField)> {
    let mut fields = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim();

        if trimmed.is_empty() || is_indented(line) {
            index += 1;
            continue;
        }

        let Some((key, value)) = split_mapping_line(trimmed) else {
            index += 1;
            continue;
        };

        let key = parse_yaml_scalar(key.trim());
        if key.is_empty() {
            index += 1;
            continue;
        }

        let value = value.trim();
        if let Some(block) = parse_block_indicator(value) {
            index += 1;
            let mut block_lines = Vec::new();

            // A block or sequence belongs to the preceding top-level key until
            // the next non-indented mapping line starts.
            while index < lines.len() && !is_top_level_field(lines[index]) {
                block_lines.push(strip_yaml_indent(lines[index]).to_string());
                index += 1;
            }

            let value = parse_block_scalar(block, &block_lines);

            fields.push((key, EntryField::Multiline(value)));
            continue;
        }

        if value.is_empty() && next_significant_line(lines, index + 1).is_some_and(is_array_item) {
            index += 1;
            let mut values = Vec::new();

            while index < lines.len() && !is_top_level_field(lines[index]) {
                if let Some(item) = parse_array_item(lines[index]) {
                    values.push(parse_yaml_scalar(item));
                }
                index += 1;
            }

            fields.push((key, EntryField::Array(values)));
            continue;
        }

        fields.push((key, EntryField::Plain(parse_yaml_scalar(value))));
        index += 1;
    }

    fields
}

fn should_skip_field(key: &str, value: &EntryField) -> bool {
    let key_is_empty = key.trim().is_empty();
    match value {
        EntryField::Array(values) => {
            key_is_empty && values.iter().all(|value| value.trim().is_empty())
        }
        EntryField::Password(value)
        | EntryField::Plain(value)
        | EntryField::OTP(value)
        | EntryField::Multiline(value) => key_is_empty && value.trim().is_empty(),
    }
}

fn is_top_level_field(line: &str) -> bool {
    if line.trim().is_empty() || is_indented(line) {
        return false;
    }

    split_mapping_line(line).is_some_and(|(key, _)| !key.trim().is_empty())
}

fn is_indented(line: &str) -> bool {
    line.starts_with(' ') || line.starts_with('\t')
}

fn strip_yaml_indent(line: &str) -> &str {
    line.strip_prefix("  ")
        .or_else(|| line.strip_prefix(' '))
        .or_else(|| line.strip_prefix('\t'))
        .unwrap_or(line)
}

fn next_significant_line<'a>(lines: &'a [&str], start: usize) -> Option<&'a str> {
    lines
        .iter()
        .skip(start)
        .copied()
        .find(|line| !line.trim().is_empty())
}

fn is_array_item(line: &str) -> bool {
    parse_array_item(line).is_some()
}

fn parse_array_item(line: &str) -> Option<&str> {
    if !is_indented(line) {
        return None;
    }

    let item = line.trim_start().strip_prefix('-')?;
    if item.is_empty() {
        return Some("");
    }

    item.strip_prefix(' ').map(str::trim)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockStyle {
    Literal,
    Folded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Chomp {
    Clip,
    Strip,
    Keep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BlockIndicator {
    style: BlockStyle,
    chomp: Chomp,
}

fn multiline_block_parts(value: &str) -> (&'static str, &str) {
    if let Some(body) = value.strip_suffix('\n') {
        ("|+", body)
    } else {
        ("|-", value)
    }
}

fn parse_block_indicator(value: &str) -> Option<BlockIndicator> {
    let mut chars = value.chars();
    let style = match chars.next()? {
        '|' => BlockStyle::Literal,
        '>' => BlockStyle::Folded,
        _ => return None,
    };

    let chomp = match chars.next() {
        Some('-') => Chomp::Strip,
        Some('+') => Chomp::Keep,
        Some(_) => return None,
        None => Chomp::Clip,
    };

    if chars.next().is_some() {
        return None;
    }

    Some(BlockIndicator { style, chomp })
}

fn parse_block_scalar(block: BlockIndicator, lines: &[String]) -> String {
    let mut value = match block.style {
        BlockStyle::Literal => literal_block_lines(lines),
        BlockStyle::Folded => fold_block_lines(lines),
    };

    match block.chomp {
        Chomp::Strip => {
            while value.ends_with('\n') {
                value.pop();
            }
        }
        Chomp::Clip => {
            while value.ends_with('\n') {
                value.pop();
            }
            if !value.is_empty() || !lines.is_empty() {
                value.push('\n');
            }
        }
        Chomp::Keep => {}
    }

    value
}

fn literal_block_lines(lines: &[String]) -> String {
    if lines.is_empty() {
        return String::new();
    }

    let mut out = lines.join("\n");
    out.push('\n');
    out
}

fn fold_block_lines(lines: &[String]) -> String {
    if lines.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    let mut previous_blank = false;

    for line in lines {
        if line.is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            previous_blank = true;
            continue;
        }

        if !out.is_empty() && !previous_blank {
            out.push(' ');
        }
        out.push_str(line);
        previous_blank = false;
    }

    out.push('\n');
    out
}

/// Splits a top-level mapping at the first colon outside double quotes.
fn split_mapping_line(line: &str) -> Option<(&str, &str)> {
    let mut escaped = false;
    let mut in_quotes = false;

    for (index, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }

        match ch {
            '\\' if in_quotes => escaped = true,
            '"' => in_quotes = !in_quotes,
            ':' if !in_quotes => return Some((&line[..index], &line[index + 1..])),
            _ => {}
        }
    }

    None
}

/// Formats a scalar for the local YAML subset.
///
/// Only double-quoted scalars are emitted when quoting is needed. The matching
/// parser supports the same escape set.
fn format_yaml_scalar(value: &str) -> String {
    if !needs_quotes(value) {
        return value.to_string();
    }

    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn needs_quotes(value: &str) -> bool {
    value.is_empty()
        || value.trim() != value
        || value.contains(':')
        || value.contains('#')
        || value.contains('\n')
        || value.contains('\r')
        || value.contains('\t')
        || is_yaml_reserved_scalar(value)
        || value.chars().next().is_some_and(|ch| {
            ch.is_ascii_digit()
                || matches!(
                    ch,
                    '-' | '?' | '!' | '&' | '*' | '[' | ']' | '{' | '}' | '|' | '>' | '@' | '`'
                )
        })
}

fn is_yaml_reserved_scalar(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "true" | "false" | "null" | "~"
    )
}

/// Parses a scalar emitted by [`format_yaml_scalar`].
fn parse_yaml_scalar(value: &str) -> String {
    let Some(inner) = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    else {
        return value.to_string();
    };

    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }

        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_array_and_literal_multiline_fields() {
        let entry = parse_entry(
            "secret\nusername: test\nnotes: |\n  Test1\n  test2\n  Multiline\ntags:\n  - elem1\n  - elem2\n",
        )
        .unwrap();

        assert_eq!(entry.password, EntryField::Password("secret".into()));
        assert_eq!(
            entry.fields,
            vec![
                ("username".into(), EntryField::Plain("test".into())),
                (
                    "notes".into(),
                    EntryField::Multiline("Test1\ntest2\nMultiline\n".into())
                ),
                (
                    "tags".into(),
                    EntryField::Array(vec!["elem1".into(), "elem2".into()])
                ),
            ]
        );
    }

    #[test]
    fn parses_folded_multiline_fields() {
        let entry = parse_entry("secret\nnotes: >\n  Test1\n  test2\n  Multiline\n").unwrap();

        assert_eq!(
            entry.fields,
            vec![(
                "notes".into(),
                EntryField::Multiline("Test1 test2 Multiline\n".into())
            )]
        );
    }

    #[test]
    fn writes_array_and_multiline_fields_as_yaml() {
        let entry = EntryData {
            password: EntryField::Password("secret".into()),
            fields: vec![
                ("username".into(), EntryField::Plain("test".into())),
                (
                    "notes".into(),
                    EntryField::Multiline("Test1\ntest2\nMultiline".into()),
                ),
                (
                    "tags".into(),
                    EntryField::Array(vec!["elem1".into(), "elem2".into()]),
                ),
            ],
        };

        assert_eq!(
            format_entry(&entry),
            "secret\nusername: test\nnotes: |-\n  Test1\n  test2\n  Multiline\ntags:\n  - elem1\n  - elem2\n"
        );
    }

    #[test]
    fn quotes_and_parses_yaml_sensitive_scalars() {
        let entry = EntryData {
            password: EntryField::Password("secret".into()),
            fields: vec![
                (
                    "url".into(),
                    EntryField::Plain("https://example.invalid/a: b".into()),
                ),
                (
                    "tags".into(),
                    EntryField::Array(vec![
                        "hash # value".into(),
                        "- leading dash".into(),
                        "quote \" slash \\".into(),
                    ]),
                ),
            ],
        };

        let formatted = format_entry(&entry);

        assert!(formatted.contains("url: \"https://example.invalid/a: b\""));
        assert!(formatted.contains("  - \"hash # value\""));
        assert!(formatted.contains("  - \"- leading dash\""));
        assert_eq!(parse_entry(&formatted), Some(entry));
    }

    #[test]
    fn preserves_trailing_multiline_newlines() {
        let entry = EntryData {
            password: EntryField::Password("secret".into()),
            fields: vec![("notes".into(), EntryField::Multiline("a\nb\n\n".into()))],
        };

        let formatted = format_entry(&entry);

        assert!(formatted.contains("notes: |+"));
        assert_eq!(parse_entry(&formatted), Some(entry));
    }
}
