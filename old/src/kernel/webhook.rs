//! Telling somebody else's software what happened.
//!
//! Signed the way the specification says, retried with a delay that grows, and
//! given up on into the dead letter. What is sent is what happened rather than
//! what a receiver might want, because a receiver that wants more can ask.
use std::net::IpAddr;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use super::error::{AppError, Result};
use super::secret::Secret;
use crate::kernel::say;

/// The Standard Webhooks signature: `v1,<base64 of hmac-sha256>` over
/// `id.timestamp.payload`.
#[must_use]
pub fn sign(secret: &Secret<Vec<u8>>, id: &str, timestamp: i64, payload: &str) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.expose()).expect("hmac takes a key of any length");

    mac.update(format!("{id}.{timestamp}.{payload}").as_bytes());

    format!("v1,{}", BASE64.encode(mac.finalize().into_bytes()))
}

/// The form a receiver is given: `whsec_` and then the key in base64.
pub fn parse_secret(text: &str) -> Result<Secret<Vec<u8>>> {
    let body = text.strip_prefix("whsec_").unwrap_or(text);

    BASE64
        .decode(body)
        .map(Secret::new)
        .map_err(|_| AppError::Invalid(say::NOT_SIGNING_SECRET.into()))
}

/// Refuses an address that would reach this machine or the network it is on.
///
/// Checked against the address the name resolved to, not the name: a name that
/// resolves differently the second time is how the first check is walked past.
pub fn allowed_destination(ip: IpAddr) -> Result<()> {
    let refused = match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.octets()[0] == 0
                // Carrier-grade NAT, and the address a cloud instance asks for
                // its own credentials on.
                || (v4.octets()[0] == 100 && (64..128).contains(&v4.octets()[1]))
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                // Unique local, and link local.
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    };

    if refused {
        return Err(AppError::Invalid(
            say::ADDRESS_NOT_ONE_MACHINE_WILL_SEND.into(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The vector from the Standard Webhooks specification.
    #[test]
    fn the_signature_matches_the_specification() {
        let secret = parse_secret("whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw").expect("a secret");

        assert_eq!(
            sign(
                &secret,
                "msg_p5jXN8AQM9LWM0D4loKWxJek",
                1_614_265_330,
                r#"{"test": 2432232314}"#
            ),
            "v1,g0hM9SsE+OTPJTGt/tmIKtSyZlE3uFJELVlNIOLJ1OE="
        );
    }

    #[test]
    fn it_will_not_send_to_itself_or_to_its_own_network() {
        for refused in [
            "127.0.0.1",
            "10.0.0.5",
            "192.168.1.1",
            "172.16.0.1",
            "169.254.169.254",
            "0.0.0.0",
            "100.64.0.1",
            "::1",
            "fd00::1",
            "fe80::1",
        ] {
            assert!(
                allowed_destination(refused.parse().expect("an address")).is_err(),
                "{refused} was allowed"
            );
        }

        for allowed in ["93.184.216.34", "2606:2800:220:1:248:1893:25c8:1946"] {
            assert!(
                allowed_destination(allowed.parse().expect("an address")).is_ok(),
                "{allowed} was refused"
            );
        }
    }
}
