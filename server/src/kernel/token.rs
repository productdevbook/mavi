//! Opaque tokens: session cookies, invitations, anything held by somebody who
//! must not be able to guess another one.

use rand::TryRngCore;
use sha2::{Digest, Sha256};

/// 256 bits from the operating system, hex. Not a uuid: a uuid v7 carries the
/// time it was made and is not claimed to be unguessable.
#[must_use]
pub fn generate() -> String {
    let mut bytes = [0_u8; 32];
    rand::rngs::OsRng
        .try_fill_bytes(&mut bytes)
        .expect("the operating system has randomness");

    bytes.iter().fold(String::with_capacity(64), |mut out, b| {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// What is stored. The token itself is shown once and never written down, so a
/// copy of the table is not a drawer of working sessions.
#[must_use]
pub fn hash(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_tokens_are_not_the_same_token() {
        assert_ne!(generate(), generate());
        assert_eq!(generate().len(), 64);
    }

    #[test]
    fn the_hash_is_the_same_every_time() {
        assert_eq!(hash("a"), hash("a"));
        assert_ne!(hash("a"), hash("b"));
    }
}
