//! A secret somebody is given, and what is kept instead of it.
//!
//! Two rules, and the second is the one that was broken:
//!
//! **What is stored is the hash.** A stolen database is not a stolen set of
//! sessions.
//!
//! **What is handed out is the token.** The crate this replaces generated one,
//! hashed it, and dropped the raw value inside the argument list — so nothing
//! ever held it again. The link that was supposed to carry it carried the
//! *hash* instead, the handler compared a hash of that hash against the hash,
//! and nobody could ever unsubscribe from anything. Nothing reported a failure
//! at any point.
//!
//! So [`mint`] hands back both halves at once, named, and there is no way to
//! get one without the other.

use rand::TryRngCore;
use sha2::{Digest, Sha256};

/// A secret and the hash to store beside it. The pair, because handing back
/// only one is how the other gets lost.
#[derive(Debug)]
pub struct Minted {
    /// What goes in the link. Never stored.
    pub token: String,
    /// What is stored. Never sent.
    pub hash: [u8; 32],
}

/// Two hundred and fifty-six bits from the operating system, and its hash.
///
/// # Panics
///
/// If the operating system cannot produce randomness. There is no sensible
/// weaker answer: a token that is not random is not a token, and carrying on
/// with a predictable one is worse than stopping.
#[must_use]
pub fn mint() -> Minted {
    let mut bytes = [0_u8; 32];
    rand::rngs::OsRng
        .try_fill_bytes(&mut bytes)
        .expect("the operating system has randomness");

    let token = base32(&bytes);
    let hash = hash(&token);

    Minted { token, hash }
}

/// The hash of a token somebody sent back.
#[must_use]
pub fn hash(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

/// Crockford's alphabet, without the letters somebody would mistype: no `I`,
/// no `L`, no `O`, no `U`. A token is read off a screen and typed into a phone
/// often enough to be worth it.
fn base32(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

    let mut out = String::with_capacity(bytes.len() * 8 / 5 + 1);
    let mut held: u16 = 0;
    let mut bits = 0_u32;

    for byte in bytes {
        held = (held << 8) | u16::from(*byte);
        bits += 8;

        while bits >= 5 {
            bits -= 5;
            let index = ((held >> bits) & 0b0001_1111) as usize;
            out.push(ALPHABET[index] as char);
        }
    }

    if bits > 0 {
        let index = ((held << (5 - bits)) & 0b0001_1111) as usize;
        out.push(ALPHABET[index] as char);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_handed_out_and_what_is_kept_come_together() {
        // The bug this shape exists to prevent: a token generated, hashed, and
        // dropped, so the link carried the hash and nothing could ever be
        // redeemed. Here there is no way to get one half without the other.
        let minted = mint();

        assert_eq!(hash(&minted.token), minted.hash);
        assert!(!minted.token.is_empty());
    }

    #[test]
    fn two_are_not_the_same() {
        assert_ne!(mint().token, mint().token);
    }

    #[test]
    fn a_token_has_nothing_in_it_somebody_would_mistype() {
        let token = mint().token;

        for wrong in ['I', 'L', 'O', 'U'] {
            assert!(!token.contains(wrong), "{token} has a {wrong} in it");
        }

        assert!(
            token
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()),
            "{token}"
        );
    }

    #[test]
    fn the_hash_is_of_the_token_and_not_of_the_hash() {
        // Stated because the crate this replaces got exactly this wrong: it
        // hashed the hex of a hash and compared it to the hash, which can
        // never match.
        let minted = mint();

        assert_ne!(hash(&hex(&minted.hash)), minted.hash);
        assert_eq!(hash(&minted.token), minted.hash);
    }

    fn hex(bytes: &[u8; 32]) -> String {
        use std::fmt::Write as _;

        bytes.iter().fold(String::new(), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
    }
}
