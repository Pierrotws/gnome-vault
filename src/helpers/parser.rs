use crate::pass::model::{EntryData, EntryField};

pub fn parse_entry(plaintext: &str) -> Option<EntryData> {
    let mut lines = plaintext.lines();
    let password = lines.next()?.to_string();
    let field_lines: Vec<&str> = lines.collect();

    Some(EntryData {
        password: EntryField::Password(password),
        fields: parse_fields(&field_lines),
    })
}

pub fn format_entry(entry: &EntryData) -> String {
    let mut out = String::new();
    let password_str = &entry.password.to_str();
    out.push_str(password_str);
    out.push('\n');

    for (key, value) in &entry.fields {
        if should_skip_field(key, value) {
            continue;
        }

        match value {
            EntryField::Array(values) => {
                out.push_str(key);
                out.push_str(":\n");
                for item in values {
                    out.push_str("  - ");
                    out.push_str(item);
                    out.push('\n');
                }
            }
            EntryField::Multiline(value) => {
                out.push_str(key);
                out.push_str(": |\n");
                for line in value.lines() {
                    out.push_str("  ");
                    out.push_str(line);
                    out.push('\n');
                }
            }
            EntryField::Password(value) | EntryField::Plain(value) | EntryField::OTP(value) => {
                out.push_str(key);
                out.push_str(": ");
                out.push_str(value);
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

        let Some((key, value)) = trimmed.split_once(':') else {
            index += 1;
            continue;
        };

        let key = key.trim();
        if key.is_empty() {
            index += 1;
            continue;
        }

        let value = value.trim();
        if value == "|" || value == ">" {
            index += 1;
            let mut block_lines = Vec::new();

            while index < lines.len() && !is_top_level_field(lines[index]) {
                block_lines.push(strip_yaml_indent(lines[index]).to_string());
                index += 1;
            }

            trim_trailing_empty_lines(&mut block_lines);
            let value = if value == ">" {
                fold_block_lines(&block_lines)
            } else {
                block_lines.join("\n")
            };

            fields.push((key.to_string(), EntryField::Multiline(value)));
            continue;
        }

        if value.is_empty() && next_significant_line(lines, index + 1).is_some_and(is_array_item) {
            index += 1;
            let mut values = Vec::new();

            while index < lines.len() && !is_top_level_field(lines[index]) {
                if let Some(item) = parse_array_item(lines[index]) {
                    values.push(item.to_string());
                }
                index += 1;
            }

            fields.push((key.to_string(), EntryField::Array(values)));
            continue;
        }

        fields.push((key.to_string(), EntryField::Plain(value.to_string())));
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

    line.split_once(':')
        .is_some_and(|(key, _)| !key.trim().is_empty())
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

fn trim_trailing_empty_lines(lines: &mut Vec<String>) {
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
}

fn fold_block_lines(lines: &[String]) -> String {
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
                    EntryField::Multiline("Test1\ntest2\nMultiline".into())
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
                EntryField::Multiline("Test1 test2 Multiline".into())
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
            "secret\nusername: test\nnotes: |\n  Test1\n  test2\n  Multiline\ntags:\n  - elem1\n  - elem2\n"
        );
    }
}
