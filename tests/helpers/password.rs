use gnome_vault::helpers::password::{generate_password, get_charset_for_mode, PasswordMode};

fn contains_any(value: &str, charset: &[u8]) -> bool {
    value.bytes().any(|byte| charset.contains(&byte))
}

fn only_uses_charset(value: &str, charset: &[u8]) -> bool {
    value.bytes().all(|byte| charset.contains(&byte))
}

#[test]
fn returns_expected_numeric_charset() {
    assert_eq!(get_charset_for_mode(PasswordMode::Numeric), b"0123456789");
}

#[test]
fn alphanumeric_charset_excludes_ambiguous_uppercase_o() {
    let charset = get_charset_for_mode(PasswordMode::Alphanumeric);

    assert!(charset.contains(&b'0'));
    assert!(!charset.contains(&b'O'));
    assert!(charset.contains(&b'N'));
    assert!(charset.contains(&b'P'));
}

#[test]
fn generated_numeric_password_has_requested_length_and_digits_only() {
    let password = generate_password(32, PasswordMode::Numeric);

    assert_eq!(password.len(), 32);
    assert!(only_uses_charset(
        &password,
        get_charset_for_mode(PasswordMode::Numeric)
    ));
}

#[test]
fn generated_limited_special_password_contains_required_groups() {
    let password = generate_password(64, PasswordMode::LimitedSpecial);
    let charset = get_charset_for_mode(PasswordMode::LimitedSpecial);

    assert_eq!(password.len(), 64);
    assert!(only_uses_charset(&password, charset));
    assert!(contains_any(&password, b"0123456789"));
    assert!(contains_any(&password, b"abcdefghijklmnopqrstuvwxyz"));
    assert!(contains_any(&password, b"ABCDEFGHIJKLMNPQRSTUVWXYZ"));
    assert!(contains_any(&password, b".!?+-_:&*%@"));
}

#[test]
#[should_panic(expected = "length must be > 0")]
fn rejects_zero_length_passwords() {
    generate_password(0, PasswordMode::Numeric);
}
