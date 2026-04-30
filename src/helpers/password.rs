use zeroize::Zeroizing;

use crate::helpers::macros::concat_bytes;

/// Failure modes for [`generate_password`].
#[derive(Debug)]
pub enum PasswordError {
    /// `length` was below the minimum required by the selected mode.
    TooShort { min: usize, got: usize },
    /// The OS RNG (`getrandom`) refused to fill our buffer.
    Rng(getrandom::Error),
}

impl std::fmt::Display for PasswordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PasswordError::TooShort { min, got } => write!(
                f,
                "password length {got} is below the minimum {min} required for the selected mode"
            ),
            PasswordError::Rng(err) => write!(f, "RNG failure: {err}"),
        }
    }
}

impl std::error::Error for PasswordError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PasswordError::Rng(err) => Some(err),
            PasswordError::TooShort { .. } => None,
        }
    }
}

impl From<getrandom::Error> for PasswordError {
    fn from(err: getrandom::Error) -> Self {
        PasswordError::Rng(err)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordMode {
    /// Digits only.
    Numeric,
    /// Lowercase, uppercase, and digits.
    Alphanumeric,
    /// Alphanumeric plus a small set of easier-to-type symbols.
    LimitedSpecial,
    /// Alphanumeric plus the full symbol set supported by the app.
    All,
}

const NUMERIC: &[u8] = b"0123456789";
const LOWER: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
//remove O, because too close to 0
const UPPER: &[u8] = b"ABCDEFGHIJKLMNPQRSTUVWXYZ";
const LIMITED: &[u8] = b".!?+-_:&*%@";
const SPECIAL: &[u8] = b".!?+-_:&*%@#$^()=[]{}<>~,;";

const ALPHANUMERIC: &[u8] = &concat_bytes!(LOWER, UPPER, NUMERIC);
const LIMITED_SPECIAL: &[u8] = &concat_bytes!(LOWER, UPPER, NUMERIC, LIMITED);
const ALL: &[u8] = &concat_bytes!(LOWER, UPPER, NUMERIC, SPECIAL);

/// Returns the byte charset used to generate passwords for a mode.
pub fn get_charset_for_mode(mode: PasswordMode) -> &'static [u8] {
    match mode {
        PasswordMode::Numeric => NUMERIC,
        PasswordMode::Alphanumeric => ALPHANUMERIC,
        PasswordMode::LimitedSpecial => LIMITED_SPECIAL,
        PasswordMode::All => ALL,
    }
}

/// Generates an unbiased random index in `0..max`.
fn random_index(max: usize) -> Result<usize, getrandom::Error> {
    let max_u64 = max as u64;
    let zone = u64::MAX - (u64::MAX % max_u64);
    loop {
        let mut buf = [0u8; 8];
        getrandom::fill(&mut buf)?;
        let value = u64::from_le_bytes(buf);
        if value < zone {
            return Ok((value % max_u64) as usize);
        }
    }
}

/// Shuffles bytes in place with Fisher-Yates.
fn shuffle(bytes: &mut [u8]) -> Result<(), getrandom::Error> {
    for i in (1..bytes.len()).rev() {
        let j = random_index(i + 1)?;
        bytes.swap(i, j);
    }
    Ok(())
}

/// Generates a random password of `length` bytes for the requested mode.
///
/// Non-numeric modes guarantee at least one character from each required
/// group; `Err(PasswordError::TooShort)` is returned when `length` is below
/// the minimum required for the mode. RNG failures from `getrandom` are
/// propagated as [`PasswordError::Rng`].
///
/// The returned [`Zeroizing<String>`] wipes its backing memory on drop so a
/// password that has been moved into a GTK widget (or otherwise consumed)
/// does not linger in the local stack frame's heap allocation.
pub fn generate_password(
    length: usize,
    mode: PasswordMode,
) -> Result<Zeroizing<String>, PasswordError> {
    if length == 0 {
        return Err(PasswordError::TooShort { min: 1, got: length });
    }
    let charset = get_charset_for_mode(mode);
    let mut password: Vec<u8> = Vec::with_capacity(length);
    let mut must_shuffle = true;
    match mode {
        PasswordMode::Numeric => {
            must_shuffle = false;
        }
        PasswordMode::Alphanumeric => {
            if length < 3 {
                return Err(PasswordError::TooShort { min: 3, got: length });
            }
            password.push(NUMERIC[random_index(NUMERIC.len())?]);
            password.push(LOWER[random_index(LOWER.len())?]);
            password.push(UPPER[random_index(UPPER.len())?]);
        }
        PasswordMode::LimitedSpecial => {
            if length < 4 {
                return Err(PasswordError::TooShort { min: 4, got: length });
            }
            password.push(NUMERIC[random_index(NUMERIC.len())?]);
            password.push(LOWER[random_index(LOWER.len())?]);
            password.push(UPPER[random_index(UPPER.len())?]);
            password.push(LIMITED[random_index(LIMITED.len())?]);
        }
        PasswordMode::All => {
            if length < 4 {
                return Err(PasswordError::TooShort { min: 4, got: length });
            }
            password.push(NUMERIC[random_index(NUMERIC.len())?]);
            password.push(LOWER[random_index(LOWER.len())?]);
            password.push(UPPER[random_index(UPPER.len())?]);
            password.push(SPECIAL[random_index(SPECIAL.len())?]);
        }
    }
    for _ in password.len()..length {
        let idx = random_index(charset.len())?;
        password.push(charset[idx]);
    }
    // Avoid predictable required-character positions.
    if must_shuffle {
        shuffle(&mut password)?;
    }
    Ok(Zeroizing::new(
        // Charsets are ASCII-only, so the bytes are guaranteed valid UTF-8.
        String::from_utf8(password).expect("password charset is ASCII"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let password = generate_password(32, PasswordMode::Numeric).unwrap();

        assert_eq!(password.len(), 32);
        assert!(only_uses_charset(
            &password,
            get_charset_for_mode(PasswordMode::Numeric)
        ));
    }

    #[test]
    fn generated_limited_special_password_contains_required_groups() {
        let password = generate_password(64, PasswordMode::LimitedSpecial).unwrap();
        let charset = get_charset_for_mode(PasswordMode::LimitedSpecial);

        assert_eq!(password.len(), 64);
        assert!(only_uses_charset(&password, charset));
        assert!(contains_any(&password, b"0123456789"));
        assert!(contains_any(&password, b"abcdefghijklmnopqrstuvwxyz"));
        assert!(contains_any(&password, b"ABCDEFGHIJKLMNPQRSTUVWXYZ"));
        assert!(contains_any(&password, b".!?+-_:&*%@"));
    }

    #[test]
    fn generated_all_password_contains_required_groups() {
        let password = generate_password(64, PasswordMode::All).unwrap();
        let charset = get_charset_for_mode(PasswordMode::All);

        assert_eq!(password.len(), 64);
        assert!(only_uses_charset(&password, charset));
        assert!(contains_any(&password, b"0123456789"));
        assert!(contains_any(&password, b"abcdefghijklmnopqrstuvwxyz"));
        assert!(contains_any(&password, b"ABCDEFGHIJKLMNPQRSTUVWXYZ"));
        assert!(contains_any(&password, b".!?+-_:&*%@#$^()=[]{}<>~,;"));
    }

    #[test]
    fn rejects_zero_length_passwords() {
        let err = generate_password(0, PasswordMode::Numeric).unwrap_err();
        assert!(matches!(
            err,
            PasswordError::TooShort {
                min: 1,
                got: 0,
            }
        ));
    }

    #[test]
    fn rejects_too_short_for_alphanumeric() {
        let err = generate_password(2, PasswordMode::Alphanumeric).unwrap_err();
        assert!(matches!(
            err,
            PasswordError::TooShort {
                min: 3,
                got: 2,
            }
        ));
    }
}
