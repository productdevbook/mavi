//! Sealing with a key this machine holds.
//!
//! One implementation of [`mavi_core::ports::Seals`], and the smallest one
//! that is honest: a key handed to the process, and XChaCha20-Poly1305 over
//! it.
//!
//! **The nonce is random, and that is why the algorithm is this one.** A
//! ninety-six-bit nonce reused once with the same key is the whole secret; a
//! hundred-and-ninety-two-bit one can be drawn at random for the life of a
//! site and never repeat. Every other arrangement — a counter in a column, a
//! nonce derived from the row — is a thing that has to stay true through
//! restores, copies and clock changes.
//!
//! The nonce goes in front of what it sealed, because it is not a secret and
//! whoever opens it needs it.

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use mavi_core::error::{Error, Result};
use mavi_core::ports::{Answering, Seals};

/// How long the key is.
pub const HOW_LONG: usize = 32;

/// How long a nonce is.
const NONCE: usize = 24;

/// A key this machine was handed.
#[derive(Clone)]
pub struct WithAKey {
    sealing: XChaCha20Poly1305,
}

impl std::fmt::Debug for WithAKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the key, not even in a panic message. A `Debug` that prints a
        // key is a key in every log that ever printed a struct holding one.
        f.write_str("WithAKey")
    }
}

impl WithAKey {
    /// Reads a key, as thirty-two bytes written in hexadecimal.
    ///
    /// Refused at the edge where whoever set it can still see the message,
    /// rather than at the first sign-in that needed it.
    pub fn read(written: &str) -> Result<Self> {
        let written = written.trim();

        let refuse = || {
            Error::internal(std::io::Error::other(
                "a sealing key is thirty-two bytes written as sixty-four hexadecimal characters",
            ))
        };

        if written.len() != HOW_LONG * 2 {
            return Err(refuse());
        }

        let mut key = [0_u8; HOW_LONG];

        for (at, byte) in key.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&written[at * 2..at * 2 + 2], 16).map_err(|_| refuse())?;
        }

        Ok(Self {
            sealing: XChaCha20Poly1305::new((&key).into()),
        })
    }
}

impl Seals for WithAKey {
    fn seal<'a>(&'a self, what: &'a [u8]) -> Answering<'a, Vec<u8>> {
        Box::pin(async move {
            use rand::RngCore;

            let mut nonce = [0_u8; NONCE];
            rand::rng().fill_bytes(&mut nonce);

            let sealed = self
                .sealing
                .encrypt(XNonce::from_slice(&nonce), what)
                .map_err(|_| Error::internal(std::io::Error::other("nothing could be sealed")))?;

            Ok(nonce.into_iter().chain(sealed).collect())
        })
    }

    fn open<'a>(&'a self, sealed: &'a [u8]) -> Answering<'a, Vec<u8>> {
        Box::pin(async move {
            let wrong = || Error::internal(std::io::Error::other("that was not sealed with this"));

            if sealed.len() <= NONCE {
                return Err(wrong());
            }

            let (nonce, rest) = sealed.split_at(NONCE);

            self.sealing
                .decrypt(XNonce::from_slice(nonce), rest)
                .map_err(|_| wrong())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_key() -> WithAKey {
        WithAKey::read(&"ab".repeat(HOW_LONG)).expect("a key")
    }

    #[tokio::test]
    async fn what_is_sealed_comes_back() {
        let sealing = a_key();
        let secret = b"a second factor's own secret".to_vec();

        let sealed = sealing.seal(&secret).await.expect("sealed");

        assert_ne!(sealed, secret, "that is not sealed");
        assert_eq!(sealing.open(&sealed).await.expect("opened"), secret);
    }

    #[tokio::test]
    async fn the_same_thing_twice_does_not_look_the_same() {
        // What a random nonce is for. Two rows holding the same secret that
        // look identical is a table somebody can read facts out of without
        // opening anything.
        let sealing = a_key();

        let once = sealing.seal(b"the same").await.expect("sealed");
        let twice = sealing.seal(b"the same").await.expect("sealed");

        assert_ne!(once, twice);
    }

    #[tokio::test]
    async fn another_key_opens_nothing() {
        let sealed = a_key().seal(b"a secret").await.expect("sealed");

        let other = WithAKey::read(&"cd".repeat(HOW_LONG)).expect("a key");

        assert!(other.open(&sealed).await.is_err());
    }

    #[tokio::test]
    async fn something_somebody_changed_opens_nothing() {
        // What the Poly1305 half is for. A sealed value a byte of which was
        // altered is refused rather than opened into something else.
        let sealing = a_key();
        let mut sealed = sealing.seal(b"a secret").await.expect("sealed");

        let last = sealed.len() - 1;
        sealed[last] ^= 0xff;

        assert!(sealing.open(&sealed).await.is_err());
    }

    #[test]
    fn a_key_is_refused_where_somebody_can_still_see_the_message() {
        for wrong in [
            "",
            "abc",
            &"zz".repeat(HOW_LONG),
            &"ab".repeat(HOW_LONG - 1),
        ] {
            assert!(
                WithAKey::read(wrong).is_err(),
                "{wrong} was taken for a key"
            );
        }
    }

    #[test]
    fn a_key_is_never_printed() {
        assert_eq!(format!("{:?}", a_key()), "WithAKey");
    }
}
