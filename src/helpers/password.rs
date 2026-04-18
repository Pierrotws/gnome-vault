use crate::helpers::macros::concat_bytes;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordMode {
    Numeric,
    Alphanumeric,
    LimitedSpecial,
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

pub fn get_charset_for_mode(mode: PasswordMode) -> &'static [u8] {
    match mode {
        PasswordMode::Numeric => NUMERIC,
        PasswordMode::Alphanumeric => ALPHANUMERIC,
        PasswordMode::LimitedSpecial => LIMITED_SPECIAL,
        PasswordMode::All => ALL,
    }
}

fn random_index(max: usize) -> usize {
    let max_u64 = max as u64;
    let zone = u64::MAX - (u64::MAX % max_u64);

    loop {
        let mut buf = [0u8; 8];
        getrandom::fill(&mut buf).unwrap();
        let value = u64::from_le_bytes(buf);

        if value < zone {
            return (value % max_u64) as usize;
        }
    }
}

//Fisher-Yates shuffle
fn shuffle(bytes: &mut [u8]) {
    for i in (1..bytes.len()).rev() {
        let j = random_index(i + 1);
        bytes.swap(i, j);
    }
}

pub fn generate_password(length: usize, mode: PasswordMode) -> String {
    assert!(length > 0, "length must be > 0");

    let charset = get_charset_for_mode(mode);

    let mut password: Vec<u8> = Vec::with_capacity(length);

    match mode {
        PasswordMode::Numeric => {
            //nothing to do, will be filled with digits only anyway
        }
        PasswordMode::Alphanumeric => {
            assert!(length >= 3, "length must be > 3");
            //at least one of each
            password.push(NUMERIC[random_index(NUMERIC.len())]);
            password.push(LOWER[random_index(LOWER.len())]);
            password.push(UPPER[random_index(UPPER.len())]);
        }
        PasswordMode::LimitedSpecial => {
            assert!(length >= 4, "length must be > 4");
            //at least one of each
            password.push(NUMERIC[random_index(NUMERIC.len())]);
            password.push(LOWER[random_index(LOWER.len())]);
            password.push(UPPER[random_index(UPPER.len())]);
            password.push(LIMITED[random_index(LIMITED.len())]);
        }
        PasswordMode::All => {
            assert!(length >= 4, "length must be > 4");
            //at least one of each
            password.push(NUMERIC[random_index(NUMERIC.len())]);
            password.push(LOWER[random_index(LOWER.len())]);
            password.push(UPPER[random_index(UPPER.len())]);
            password.push(ALL[random_index(ALL.len())]);
        }
    }

    // Fill remaining
    for _ in password.len()..length {
        let idx = random_index(charset.len());
        password.push(charset[idx]);
    }

    // 🔀 Shuffle to avoid predictable positions
    shuffle(&mut password);

    String::from_utf8(password).unwrap()
}
