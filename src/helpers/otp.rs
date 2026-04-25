use std::time::{SystemTime, UNIX_EPOCH};

use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, Mac};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

const DEFAULT_PERIOD: u64 = 30;
const DEFAULT_DIGITS: u32 = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtpCode {
    pub code: String,
    pub period: u64,
    pub remaining: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OtpError {
    InvalidScheme,
    MissingSecret,
    InvalidSecret,
    InvalidDigits,
    InvalidPeriod,
    UnsupportedAlgorithm(String),
    UnsupportedKind(String),
    SystemTime,
}

/// Returns whether a field value looks like an OTP auth URI.
pub fn is_otp_url(value: &str) -> bool {
    value
        .trim_start()
        .get(..10)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("otpauth://"))
}

/// Generates the current TOTP code for an `otpauth://totp/...` URI.
pub fn generate_totp_from_url(url: &str) -> Result<OtpCode, OtpError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| OtpError::SystemTime)?
        .as_secs();

    generate_totp_at(url, now)
}

fn generate_totp_at(url: &str, timestamp: u64) -> Result<OtpCode, OtpError> {
    let config = TotpConfig::parse(url)?;
    let counter = timestamp / config.period;
    let code = generate_hotp(&config.secret, counter, config.digits);
    let remaining = config.period - (timestamp % config.period);

    Ok(OtpCode {
        code,
        period: config.period,
        remaining,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TotpConfig {
    secret: Vec<u8>,
    period: u64,
    digits: u32,
}

impl TotpConfig {
    fn parse(url: &str) -> Result<Self, OtpError> {
        let url = url.trim();
        if !is_otp_url(url) {
            return Err(OtpError::InvalidScheme);
        }

        let rest = &url["otpauth://".len()..];
        let (kind, remainder) = rest
            .split_once('/')
            .ok_or_else(|| OtpError::UnsupportedKind(String::new()))?;
        if !kind.eq_ignore_ascii_case("totp") {
            return Err(OtpError::UnsupportedKind(kind.to_string()));
        }

        let query = remainder
            .split_once('?')
            .map(|(_, query)| query)
            .unwrap_or_default();
        let secret = query_value(query, "secret").ok_or(OtpError::MissingSecret)?;
        let secret = decode_base32_secret(&secret)?;

        let algorithm = query_value(query, "algorithm").unwrap_or_else(|| "SHA1".into());
        if !algorithm.eq_ignore_ascii_case("SHA1") {
            return Err(OtpError::UnsupportedAlgorithm(algorithm));
        }

        let period = query_value(query, "period")
            .map(|value| value.parse().map_err(|_| OtpError::InvalidPeriod))
            .transpose()?
            .unwrap_or(DEFAULT_PERIOD);
        if period == 0 {
            return Err(OtpError::InvalidPeriod);
        }

        let digits = query_value(query, "digits")
            .map(|value| value.parse().map_err(|_| OtpError::InvalidDigits))
            .transpose()?
            .unwrap_or(DEFAULT_DIGITS);
        if !(6..=8).contains(&digits) {
            return Err(OtpError::InvalidDigits);
        }

        Ok(Self {
            secret,
            period,
            digits,
        })
    }
}

fn generate_hotp(secret: &[u8], counter: u64, digits: u32) -> String {
    let mut mac = HmacSha1::new_from_slice(secret).expect("HMAC accepts keys of any size");
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = usize::from(digest[19] & 0x0f);
    let binary = (u32::from(digest[offset] & 0x7f) << 24)
        | (u32::from(digest[offset + 1]) << 16)
        | (u32::from(digest[offset + 2]) << 8)
        | u32::from(digest[offset + 3]);
    let modulo = 10_u32.pow(digits);

    format!("{:0width$}", binary % modulo, width = digits as usize)
}

fn decode_base32_secret(secret: &str) -> Result<Vec<u8>, OtpError> {
    let normalized: String = secret
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace() && *ch != '=')
        .map(|ch| ch.to_ascii_uppercase())
        .collect();

    BASE32_NOPAD
        .decode(normalized.as_bytes())
        .map_err(|_| OtpError::InvalidSecret)
}

fn query_value(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (pair_key, value) = pair.split_once('=')?;
        pair_key
            .eq_ignore_ascii_case(key)
            .then(|| percent_decode(value))
    })
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = &value[index + 1..index + 3];
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    index += 3;
                } else {
                    out.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }

    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RFC_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    #[test]
    fn generates_rfc_6238_sha1_totp() {
        let url = format!("otpauth://totp/example?secret={RFC_SECRET}&period=30&digits=8");

        assert_eq!(generate_totp_at(&url, 59).unwrap().code, "94287082");
        assert_eq!(
            generate_totp_at(&url, 1_111_111_109).unwrap().code,
            "07081804"
        );
        assert_eq!(
            generate_totp_at(&url, 2_000_000_000).unwrap().code,
            "69279037"
        );
    }

    #[test]
    fn detects_otp_urls_case_insensitively() {
        assert!(is_otp_url("otpauth://totp/example?secret=ABC"));
        assert!(is_otp_url("OTPAUTH://totp/example?secret=ABC"));
        assert!(!is_otp_url("https://example.invalid"));
    }

    #[test]
    fn reports_invalid_totp_url() {
        assert_eq!(
            generate_totp_at("otpauth://hotp/example?secret=ABC", 0).unwrap_err(),
            OtpError::UnsupportedKind("hotp".into())
        );
    }
}
