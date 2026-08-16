//! The six digits an authenticator app shows.
//!
//! RFC 6238 over RFC 4226: HMAC-SHA1 of the number of thirty-second steps
//! since the epoch, truncated to six digits.
//!
//! **SHA-1 is not a choice.** It is what the apps people already have will
//! compute, and an authenticator that agrees with nothing is not a second
//! factor. Its weaknesses are about finding collisions in signed documents,
//! which is not what is happening here.

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use sha1::Sha1;
use subtle::ConstantTimeEq;

pub const THAT_IS_NOT_BASE32: &str = "that_is_not_base32";

/// How long one code lasts.
pub const STEP_SECONDS: i64 = 30;

/// How far out of step a clock may be and still be believed.
///
/// One step either way: a phone half a minute wrong, rather than a window wide
/// enough to be worth guessing into.
const DRIFT_STEPS: i64 = 1;

const DIGITS: u32 = 6;

/// How long a shared secret is. Twenty bytes, as the apps expect.
pub const HOW_LONG: usize = 20;

/// A shared secret nobody has seen.
#[must_use]
pub fn invent() -> Vec<u8> {
    use rand::RngCore;

    let mut secret = vec![0_u8; HOW_LONG];
    rand::rng().fill_bytes(&mut secret);

    secret
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
            out.push(char::from(ALPHABET[((buffer >> bits) & 0b1_1111) as usize]));
        }
    }

    if bits > 0 {
        out.push(char::from(
            ALPHABET[((buffer << (5 - bits)) & 0b1_1111) as usize],
        ));
    }

    out
}

/// The other way, for reading back a secret somebody typed in.
pub fn from_base32(text: &str) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 5 / 8);
    let mut buffer = 0_u32;
    let mut bits = 0_u32;

    for letter in text.chars().filter(|c| *c != '=' && !c.is_whitespace()) {
        let value = match letter.to_ascii_uppercase() {
            letter @ 'A'..='Z' => u32::from(letter as u8 - b'A'),
            letter @ '2'..='7' => u32::from(letter as u8 - b'2') + 26,
            _ => return Err(Error::invalid(Say::of(THAT_IS_NOT_BASE32))),
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

/// What goes in the picture an app reads.
///
/// The label says which account on which site, so somebody with several does
/// not have to guess which row is which.
#[must_use]
pub fn what_an_app_reads(secret: &[u8], site: &str, account: &str) -> String {
    format!(
        "otpauth://totp/{}:{}?secret={}&issuer={}&algorithm=SHA1&digits={DIGITS}&period={STEP_SECONDS}",
        escaped(site),
        escaped(account),
        to_base32(secret),
        escaped(site),
    )
}

fn escaped(text: &str) -> String {
    use std::fmt::Write;

    let mut out = String::with_capacity(text.len());

    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            // Writing into a string cannot fail.
            _ => {
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }

    out
}

/// Which thirty-second step a moment is in.
#[must_use]
pub fn step_at(moment: DateTime<Utc>) -> i64 {
    moment.timestamp().div_euclid(STEP_SECONDS)
}

/// The digits for one step.
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
/// The step comes back because **a code that works twice is a code somebody
/// read over a shoulder**: whoever calls this writes down the last step it
/// took and refuses anything not after it.
///
/// Compared in constant time. Six digits is a small space and a timing
/// difference is a way to walk it.
#[must_use]
pub fn check(secret: &[u8], code: &str, now: DateTime<Utc>, after: Option<i64>) -> Option<i64> {
    let code = code.trim().replace(' ', "");

    if code.len() != DIGITS as usize {
        return None;
    }

    let current = step_at(now);

    (current - DRIFT_STEPS..=current + DRIFT_STEPS)
        .filter(|step| after.is_none_or(|last| *step > last))
        .find(|step| bool::from(code_for(secret, *step).as_bytes().ct_eq(code.as_bytes())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// The secret RFC 4226 tests with, so these numbers are the specification's
    /// rather than this implementation's own opinion of itself.
    const RFC: &[u8] = b"12345678901234567890";

    #[test]
    fn the_digits_are_the_specifications() {
        // RFC 6238's own table, at the times it gives. A test that only checks
        // this against itself would pass with the arithmetic inside out.
        for (seconds, expected) in [
            (59_i64, "287082"),
            (1_111_111_109, "081804"),
            (1_234_567_890, "005924"),
            (2_000_000_000, "279037"),
        ] {
            let step = seconds.div_euclid(STEP_SECONDS);

            assert_eq!(code_for(RFC, step), expected, "at {seconds}");
        }
    }

    #[test]
    fn a_code_is_taken_once() {
        let now = Utc
            .timestamp_opt(1_234_567_890, 0)
            .single()
            .expect("a moment");
        let step = step_at(now);
        let code = code_for(RFC, step);

        assert_eq!(check(RFC, &code, now, None), Some(step));

        // The same digits again, after that step has been used. A code that
        // works twice is a code somebody read over a shoulder.
        assert_eq!(check(RFC, &code, now, Some(step)), None);
    }

    #[test]
    fn a_phone_half_a_minute_wrong_still_works() {
        let now = Utc
            .timestamp_opt(1_234_567_890, 0)
            .single()
            .expect("a moment");
        let step = step_at(now);

        assert!(check(RFC, &code_for(RFC, step - 1), now, None).is_some());
        assert!(check(RFC, &code_for(RFC, step + 1), now, None).is_some());

        // And two steps out is not believed, or the window is wide enough to
        // be worth guessing into.
        assert!(check(RFC, &code_for(RFC, step - 2), now, None).is_none());
        assert!(check(RFC, &code_for(RFC, step + 2), now, None).is_none());
    }

    #[test]
    fn base32_goes_both_ways() {
        let secret = invent();

        assert_eq!(from_base32(&to_base32(&secret)).expect("back"), secret);

        // What an app shows is upper case and grouped; what somebody types in
        // is whatever they typed.
        assert_eq!(
            from_base32(" jbsw y3dp ").expect("read"),
            from_base32("JBSWY3DP").expect("read")
        );

        assert!(from_base32("nope!").is_err());
    }

    #[test]
    fn what_an_app_reads_names_the_account_and_the_site() {
        let link = what_an_app_reads(RFC, "A Site", "somebody@example.test");

        assert!(link.starts_with("otpauth://totp/A%20Site:somebody%40example.test?"));
        assert!(link.contains("algorithm=SHA1"));
        assert!(link.contains("period=30"));
    }
}
