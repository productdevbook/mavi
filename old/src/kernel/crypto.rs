//! Sealing what a site keeps.
//!
//! A site's mail password and its payment keys are sealed with a key this
//! machine holds and never written down in the clear. The key arrives in the
//! environment and the process refuses to start without it: a key made up at
//! boot would mean everything sealed by yesterday's process is unreadable.
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::TryRngCore;
use rand::rngs::OsRng;

use super::error::{AppError, Result};
use super::secret::Secret;

/// What a site's stored credentials are sealed with.
///
/// One key for the machine, from the environment, and a version travelling with
/// every sealed value so that rotating it is adding a key rather than
/// rewriting a table.
#[derive(Clone)]
pub struct Keyring {
    keys: Vec<(u32, Secret<[u8; 32]>)>,
}

impl std::fmt::Debug for Keyring {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Keyring")
            .field(
                "versions",
                &self.keys.iter().map(|(v, _)| *v).collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl Keyring {
    /// The newest key seals; every key opens. A value sealed with a key that
    /// has been taken away cannot be read, which is the point of taking one
    /// away.
    pub fn new(keys: Vec<(u32, [u8; 32])>) -> Result<Self> {
        if keys.is_empty() {
            return Err(AppError::Bug("a keyring with no keys in it"));
        }

        let mut keys: Vec<(u32, Secret<[u8; 32]>)> = keys
            .into_iter()
            .map(|(version, key)| (version, Secret::new(key)))
            .collect();

        keys.sort_by_key(|(version, _)| std::cmp::Reverse(*version));

        Ok(Self { keys })
    }

    /// From `MAVI_KEYS`: `1:<base64>,2:<base64>`, newest last or first, it
    /// does not matter which.
    pub fn from_env(raw: &str) -> Result<Self> {
        let mut keys = Vec::new();

        for part in raw.split(',').filter(|part| !part.trim().is_empty()) {
            let (version, material) = part
                .trim()
                .split_once(':')
                .ok_or(AppError::Bug("a key without a version on it"))?;

            let version: u32 = version
                .parse()
                .map_err(|_| AppError::Bug("a key version that is not a number"))?;

            let bytes = BASE64
                .decode(material)
                .map_err(|_| AppError::Bug("a key that is not base64"))?;

            let key: [u8; 32] = bytes
                .try_into()
                .map_err(|_| AppError::Bug("a key that is not thirty-two bytes"))?;

            keys.push((version, key));
        }

        Self::new(keys)
    }

    /// What was given, or an invented key where nothing was — the distinction
    /// an `Option` keeps and a discarded error does not.
    ///
    /// Nothing given is a machine with nothing sealed on it yet, and inventing
    /// one there costs nobody anything. Something given that cannot be read is
    /// a refusal: sealing under an invented key instead is noticed weeks later
    /// as old data failing to open, which reads as the key being wrong rather
    /// than as the key having been ignored.
    pub fn given(raw: Option<&str>) -> Result<Self> {
        match raw {
            Some(raw) => Self::from_env(raw),
            None => Ok(Self::invented()),
        }
    }

    /// [`Keyring::given`], reading `MAVI_KEYS` itself.
    pub fn from_the_environment() -> Result<Self> {
        match std::env::var("MAVI_KEYS") {
            Ok(raw) => Self::given(Some(raw.as_str())),
            // Set to something that is not text is still set, and falling back
            // to an invented key here would be the silence this refuses.
            Err(std::env::VarError::NotUnicode(_)) => Err(AppError::Bug("a key that is not text")),
            Err(std::env::VarError::NotPresent) => Self::given(None),
        }
    }

    /// For a test, and for a machine that has not been given one yet — which
    /// is a machine that cannot read what the last one sealed, and says so by
    /// failing to open it rather than by pretending.
    #[must_use]
    pub fn invented() -> Self {
        let mut key = [0_u8; 32];
        OsRng
            .try_fill_bytes(&mut key)
            .expect("the operating system has randomness");

        Self {
            keys: vec![(1, Secret::new(key))],
        }
    }

    fn newest(&self) -> (u32, &Secret<[u8; 32]>) {
        let (version, key) = &self.keys[0];
        (*version, key)
    }

    fn of_version(&self, wanted: u32) -> Option<&Secret<[u8; 32]>> {
        self.keys
            .iter()
            .find(|(version, _)| *version == wanted)
            .map(|(_, key)| key)
    }
}

/// What goes in a column: the key's version, the nonce, and the sealed bytes.
/// Written as one string so that a row carries everything needed to open it
/// except the key.
pub fn seal(keyring: &Keyring, plain: &str) -> Result<String> {
    let (version, key) = keyring.newest();

    let cipher = XChaCha20Poly1305::new(key.expose().into());

    let mut nonce = [0_u8; 24];
    OsRng
        .try_fill_bytes(&mut nonce)
        .map_err(|_| AppError::Bug("the operating system has randomness"))?;

    let sealed = cipher
        .encrypt(XNonce::from_slice(&nonce), plain.as_bytes())
        .map_err(|_| AppError::Bug("sealing"))?;

    Ok(format!(
        "v{version}.{}.{}",
        BASE64.encode(nonce),
        BASE64.encode(sealed)
    ))
}

pub fn open(keyring: &Keyring, sealed: &str) -> Result<Secret<String>> {
    let mut parts = sealed.splitn(3, '.');

    let version = parts
        .next()
        .and_then(|part| part.strip_prefix('v'))
        .and_then(|part| part.parse::<u32>().ok())
        .ok_or(AppError::Bug("something sealed by nothing here"))?;

    let nonce = parts
        .next()
        .and_then(|part| BASE64.decode(part).ok())
        .ok_or(AppError::Bug("something sealed by nothing here"))?;

    let body = parts
        .next()
        .and_then(|part| BASE64.decode(part).ok())
        .ok_or(AppError::Bug("something sealed by nothing here"))?;

    let key = keyring.of_version(version).ok_or(AppError::Bug(
        "sealed with a key this machine does not have",
    ))?;

    let cipher = XChaCha20Poly1305::new(key.expose().into());

    let plain = cipher
        .decrypt(XNonce::from_slice(&nonce), body.as_ref())
        .map_err(|_| AppError::Bug("that will not open"))?;

    String::from_utf8(plain)
        .map(Secret::new)
        .map_err(|_| AppError::Bug("that opened into something that is not text"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_sealed_opens_again_and_looks_like_nothing_meanwhile() {
        let keyring = Keyring::invented();
        let sealed = seal(&keyring, "a provider's key").expect("sealed");

        assert!(!sealed.contains("provider"));
        assert_eq!(
            open(&keyring, &sealed).expect("opens").expose(),
            "a provider's key"
        );
    }

    #[test]
    fn the_same_thing_sealed_twice_looks_different() {
        let keyring = Keyring::invented();

        assert_ne!(
            seal(&keyring, "the same").expect("sealed"),
            seal(&keyring, "the same").expect("sealed")
        );
    }

    #[test]
    fn a_key_this_machine_does_not_have_will_not_open_it() {
        let sealed = seal(&Keyring::invented(), "something").expect("sealed");

        assert!(open(&Keyring::invented(), &sealed).is_err());
    }

    #[test]
    fn something_tampered_with_will_not_open() {
        let keyring = Keyring::invented();
        let sealed = seal(&keyring, "something").expect("sealed");
        let bent = format!("{}x", &sealed[..sealed.len() - 1]);

        assert!(open(&keyring, &bent).is_err());
    }

    #[test]
    fn an_older_key_still_opens_what_it_sealed() {
        let old = Keyring::new(vec![(1, [7_u8; 32])]).expect("a keyring");
        let both = Keyring::new(vec![(1, [7_u8; 32]), (2, [9_u8; 32])]).expect("a keyring");

        let sealed = seal(&old, "from before").expect("sealed");

        assert_eq!(open(&both, &sealed).expect("opens").expose(), "from before");

        // And what the newer one seals says so.
        assert!(seal(&both, "from now").expect("sealed").starts_with("v2."));
    }
}
