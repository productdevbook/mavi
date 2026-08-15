//! The six digits an authenticator app shows.
//!
//! RFC 6238 over RFC 4226: HMAC-SHA1 of the number of thirty-second steps
//! since the epoch, truncated to six digits. SHA-1 rather than anything newer
//! is not a choice — it is what the apps people already have will compute.

use base64::Engine as _;
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use rand::TryRngCore;
use sha1::Sha1;

use super::error::{AppError, Result};
use crate::kernel::say;

/// How long one code lasts.
pub const STEP_SECONDS: i64 = 30;

/// How far out of step a clock may be and still be believed: one step either
/// way, which is a phone that is half a minute wrong rather than a window wide
/// enough to be worth guessing into.
const DRIFT_STEPS: i64 = 1;

const DIGITS: u32 = 6;

/// A shared secret, twenty bytes, as the apps expect.
#[must_use]
pub fn invent() -> [u8; 20] {
    let mut bytes = [0_u8; 20];
    rand::rngs::OsRng
        .try_fill_bytes(&mut bytes)
        .expect("the operating system has randomness");
    bytes
}

/// Base32 without padding, which is the only encoding an authenticator reads.
#[must_use]
pub fn to_base32(secret: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

    let mut out = String::with_capacity(secret.len().div_ceil(5) * 8);
    let mut buffer = 0_u32;
    let mut bits = 0_u32;

    for byte in secret {
        buffer = (buffer << 8) | u32::from(*byte);
        bits += 8;

        while bits >= 5 {
            bits -= 5;
            let index = ((buffer >> bits) & 0b1_1111) as usize;
            out.push(char::from(ALPHABET[index]));
        }
    }

    if bits > 0 {
        let index = ((buffer << (5 - bits)) & 0b1_1111) as usize;
        out.push(char::from(ALPHABET[index]));
    }

    out
}

/// The other way, for reading back a secret somebody typed in.
pub fn from_base32(text: &str) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 5 / 8);
    let mut buffer = 0_u32;
    let mut bits = 0_u32;

    for c in text.chars().filter(|c| *c != '=' && !c.is_whitespace()) {
        let value = match c.to_ascii_uppercase() {
            c @ 'A'..='Z' => u32::from(c as u8 - b'A'),
            c @ '2'..='7' => u32::from(c as u8 - b'2') + 26,
            _ => return Err(AppError::Invalid(say::NOT_BASE32.into())),
        };

        buffer = (buffer << 5) | value;
        bits += 5;

        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }

    Ok(out)
}

/// What is put in the QR code. The label says which account on which site, so
/// somebody with several does not have to guess which row is which.
#[must_use]
pub fn otpauth(secret: &[u8], site: &str, account: &str) -> String {
    format!(
        "otpauth://totp/{}:{}?secret={}&issuer={}&algorithm=SHA1&digits={DIGITS}&period={STEP_SECONDS}",
        escape(site),
        escape(account),
        to_base32(secret),
        escape(site),
    )
}

fn escape(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~') {
                vec![c]
            } else {
                let mut buffer = [0_u8; 4];
                c.encode_utf8(&mut buffer)
                    .as_bytes()
                    .iter()
                    .flat_map(|b| format!("%{b:02X}").chars().collect::<Vec<_>>())
                    .collect()
            }
        })
        .collect()
}

#[must_use]
pub fn step_at(moment: DateTime<Utc>) -> i64 {
    moment.timestamp().div_euclid(STEP_SECONDS)
}

#[must_use]
pub fn code_for(secret: &[u8], step: i64) -> String {
    let mut mac = Hmac::<Sha1>::new_from_slice(secret).expect("hmac takes a key of any length");
    mac.update(&step.to_be_bytes());
    let digest = mac.finalize().into_bytes();

    let offset = usize::from(digest[digest.len() - 1] & 0x0f);
    let truncated = u32::from_be_bytes([
        digest[offset] & 0x7f,
        digest[offset + 1],
        digest[offset + 2],
        digest[offset + 3],
    ]);

    format!(
        "{:0>width$}",
        truncated % 10_u32.pow(DIGITS),
        width = DIGITS as usize
    )
}

/// Whether the digits are right, and which step they were for.
///
/// The step comes back because a code that is right twice is a code somebody
/// read over a shoulder: the caller records the last step it accepted and
/// refuses anything not after it.
#[must_use]
pub fn check(secret: &[u8], code: &str, now: DateTime<Utc>, after: Option<i64>) -> Option<i64> {
    let code = code.trim().replace(' ', "");
    if code.len() != DIGITS as usize {
        return None;
    }

    let current = step_at(now);

    (current - DRIFT_STEPS..=current + DRIFT_STEPS)
        .filter(|step| after.is_none_or(|last| *step > last))
        .find(|step| super::secret::same(code_for(secret, *step).as_bytes(), code.as_bytes()))
}

/// The secret as it is stored and read back: base64, so that sealing it is the
/// same shape as sealing anything else.
pub fn from_base64(raw: &str) -> Result<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(raw)
        .map_err(|_| AppError::Bug("a stored second-factor secret that is not base64"))
}

#[must_use]
pub fn to_base64(secret: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(secret)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// RFC 6238's own vectors, for the twenty-byte SHA-1 secret it uses.
    #[test]
    fn the_digits_are_the_ones_the_standard_says() {
        let secret = b"12345678901234567890";

        for (seconds, expected) in [
            (59_i64, "287082"),
            (1_111_111_109, "081804"),
            (1_234_567_890, "005924"),
            (2_000_000_000, "279037"),
        ] {
            assert_eq!(code_for(secret, seconds / STEP_SECONDS), expected);
        }
    }

    #[test]
    fn base32_is_what_an_authenticator_reads() {
        assert_eq!(
            to_base32(b"12345678901234567890"),
            "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"
        );
    }

    #[test]
    fn a_secret_typed_in_by_hand_is_the_secret_that_was_shown() {
        let secret = invent();
        assert_eq!(from_base32(&to_base32(&secret)).unwrap(), secret);
        assert!(from_base32("not base32!").is_err());
    }

    #[test]
    fn a_code_is_taken_once_and_not_again() {
        let secret = invent();
        let now = Utc.with_ymd_and_hms(2024, 5, 1, 12, 0, 0).unwrap();
        let step = step_at(now);
        let code = code_for(&secret, step);

        assert_eq!(check(&secret, &code, now, None), Some(step));
        assert_eq!(
            check(&secret, &code, now, Some(step)),
            None,
            "a code that has been used is a code somebody could have read"
        );
    }

    #[test]
    fn a_clock_half_a_minute_out_is_still_believed() {
        let secret = invent();
        let now = Utc.with_ymd_and_hms(2024, 5, 1, 12, 0, 0).unwrap();

        for drift in [-1_i64, 0, 1] {
            let code = code_for(&secret, step_at(now) + drift);
            assert!(check(&secret, &code, now, None).is_some());
        }

        let far = code_for(&secret, step_at(now) + 5);
        assert!(check(&secret, &far, now, None).is_none());
    }

    #[test]
    fn what_is_not_six_digits_is_not_a_code() {
        let secret = invent();
        let now = Utc::now();

        assert!(check(&secret, "", now, None).is_none());
        assert!(check(&secret, "12345", now, None).is_none());
        assert!(check(&secret, "abcdef", now, None).is_none());
    }
}
