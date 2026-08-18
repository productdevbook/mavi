//! Authenticated encryption for site-scoped credential material.
//!
//! The domain only depends on [`mavi_core::ports::Seals`]. This crate is the
//! self-host/cloud adapter: it keeps an ordered keyring, encrypts with the
//! active key, and can still read older keys during rotation. The site ID is
//! authenticated data, so a ciphertext copied between sites cannot be opened
//! under the wrong [`mavi_core::SiteContext`].

use std::{collections::BTreeSet, fmt};

use aes_gcm::{
    Aes256Gcm,
    aead::{Aead, Generate, Key, KeyInit, Nonce, Payload},
};
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use mavi_core::{
    MaviError, Result, SiteContext,
    ports::{BoxFuture, Seals},
};

const ENVELOPE_MAGIC: &[u8] = b"MAVI-SEAL-V1";
const KEY_ID_BYTES: usize = 4;
const NONCE_BYTES: usize = 12;
const MIN_ENVELOPE_BYTES: usize = ENVELOPE_MAGIC.len() + KEY_ID_BYTES + NONCE_BYTES + 16;

#[derive(Clone)]
struct SealingKey {
    id: u32,
    cipher: Aes256Gcm,
}

/// A versioned AES-256-GCM keyring.
///
/// The first key in the specification is active for new values. Older keys
/// remain readable until every stored value has been re-sealed and the old
/// key is intentionally removed from the deployment configuration.
#[derive(Clone)]
pub struct KeyringSealer {
    keys: Vec<SealingKey>,
}

impl fmt::Debug for KeyringSealer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KeyringSealer")
            .field(
                "key_ids",
                &self.keys.iter().map(|key| key.id).collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl KeyringSealer {
    /// Parses `active_id:base64_key,older_id:base64_key`.
    pub fn from_spec(spec: &str) -> Result<Self> {
        let mut keys = Vec::new();
        let mut ids = BTreeSet::new();

        for entry in spec.split(',') {
            let entry = entry.trim();
            let (id, encoded) = entry
                .split_once(':')
                .ok_or_else(|| MaviError::validation("sealing_key_invalid"))?;
            let id = id
                .parse::<u32>()
                .map_err(|_| MaviError::validation("sealing_key_invalid"))?;
            if id == 0 || !ids.insert(id) {
                return Err(MaviError::validation("sealing_key_invalid"));
            }

            let bytes = STANDARD
                .decode(encoded.trim())
                .or_else(|_| URL_SAFE_NO_PAD.decode(encoded.trim()))
                .map_err(|_| MaviError::validation("sealing_key_invalid"))?;
            if bytes.len() != 32 {
                return Err(MaviError::validation("sealing_key_invalid"));
            }
            let key = Key::<Aes256Gcm>::try_from(bytes.as_slice())
                .map_err(|_| MaviError::validation("sealing_key_invalid"))?;
            keys.push(SealingKey {
                id,
                cipher: Aes256Gcm::new(&key),
            });
        }

        if keys.is_empty() {
            return Err(MaviError::validation("sealing_key_required"));
        }
        Ok(Self { keys })
    }

    /// Creates a deterministic one-key adapter for composition roots and
    /// tests that already own key material.
    #[must_use]
    pub fn from_key(key: [u8; 32]) -> Self {
        let key = Key::<Aes256Gcm>::try_from(key.as_slice()).expect("fixed-size key");
        Self {
            keys: vec![SealingKey {
                id: 1,
                cipher: Aes256Gcm::new(&key),
            }],
        }
    }

    fn seal_now(&self, context: &SiteContext, value: &[u8]) -> Result<Vec<u8>> {
        let active = self.keys.first().ok_or(MaviError::Internal)?;
        let nonce = Nonce::<Aes256Gcm>::generate();
        let ciphertext = active
            .cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: value,
                    aad: &associated_data(context),
                },
            )
            .map_err(|_| MaviError::Internal)?;

        let mut envelope = Vec::with_capacity(MIN_ENVELOPE_BYTES + value.len());
        envelope.extend_from_slice(ENVELOPE_MAGIC);
        envelope.extend_from_slice(&active.id.to_be_bytes());
        envelope.extend_from_slice(nonce.as_slice());
        envelope.extend_from_slice(&ciphertext);
        Ok(envelope)
    }

    fn unseal_now(&self, context: &SiteContext, envelope: &[u8]) -> Result<Vec<u8>> {
        if envelope.len() < MIN_ENVELOPE_BYTES || !envelope.starts_with(ENVELOPE_MAGIC) {
            return Err(MaviError::Internal);
        }

        let key_start = ENVELOPE_MAGIC.len();
        let key_end = key_start + KEY_ID_BYTES;
        let key_id = u32::from_be_bytes(
            envelope[key_start..key_end]
                .try_into()
                .map_err(|_| MaviError::Internal)?,
        );
        let key = self
            .keys
            .iter()
            .find(|key| key.id == key_id)
            .ok_or(MaviError::Internal)?;
        let nonce_start = key_end;
        let nonce_end = nonce_start + NONCE_BYTES;
        let nonce = Nonce::<Aes256Gcm>::try_from(&envelope[nonce_start..nonce_end])
            .map_err(|_| MaviError::Internal)?;
        key.cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: &envelope[nonce_end..],
                    aad: &associated_data(context),
                },
            )
            .map_err(|_| MaviError::Internal)
    }
}

impl Seals for KeyringSealer {
    fn seal<'a>(
        &'a self,
        context: &'a SiteContext,
        value: &'a [u8],
    ) -> BoxFuture<'a, Result<Vec<u8>>> {
        let result = self.seal_now(context, value);
        Box::pin(async move { result })
    }

    fn unseal<'a>(
        &'a self,
        context: &'a SiteContext,
        value: &'a [u8],
    ) -> BoxFuture<'a, Result<Vec<u8>>> {
        let result = self.unseal_now(context, value);
        Box::pin(async move { result })
    }
}

fn associated_data(context: &SiteContext) -> Vec<u8> {
    format!("mavi.site.v1:{}", context.site_id).into_bytes()
}

#[cfg(test)]
mod tests {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use mavi_core::{SiteContext, SiteId, ports::Seals};

    use super::KeyringSealer;

    #[tokio::test]
    async fn ciphertext_is_authenticated_to_the_site() {
        let sealer = KeyringSealer::from_key([7; 32]);
        let first = SiteContext::public(SiteId::new());
        let second = SiteContext::public(SiteId::new());
        let envelope = sealer.seal(&first, b"provider-secret").await.expect("seal");

        assert_ne!(envelope, b"provider-secret");
        assert_eq!(
            sealer.unseal(&first, &envelope).await.expect("unseal"),
            b"provider-secret"
        );
        assert!(sealer.unseal(&second, &envelope).await.is_err());
    }

    #[test]
    fn key_spec_supports_rotation_and_redacts_key_material() {
        let first = STANDARD.encode([1; 32]);
        let second = STANDARD.encode([2; 32]);
        let sealer = KeyringSealer::from_spec(&format!("7:{first},8:{second}")).expect("keyring");
        let debug = format!("{sealer:?}");

        assert!(debug.contains('7'));
        assert!(debug.contains('8'));
        assert!(!debug.contains(&first));
        assert!(!debug.contains(&second));
    }
}
