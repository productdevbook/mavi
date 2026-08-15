//! Reaching an address somebody else configured.
//!
//! Every request out of here goes to a host a site's owner typed in, which
//! makes each one a way to ask this machine to fetch something on their behalf.
//! What guards that is here rather than in each caller, because a caller that
//! forgot is exactly the one that would forget to check.

use std::net::SocketAddr;
use std::time::Duration;

use super::error::{AppError, Result};
use super::webhook;
use crate::kernel::say;

#[derive(Debug)]
pub struct Reachable {
    pub client: reqwest::Client,
    pub url: reqwest::Url,
}

/// A client that will only talk to the address the name resolved to just now.
///
/// Resolved once, checked, and then pinned: a name that answers differently the
/// second time is how a check on the name alone is walked past.
pub async fn reach(url: &str, timeout: Duration, allow_private: bool) -> Result<Reachable> {
    let parsed =
        reqwest::Url::parse(url).map_err(|_| AppError::Invalid(say::NOT_ADDRESS.into()))?;

    if parsed.scheme() != "https" && !allow_private {
        return Err(AppError::Invalid(
            say::ADDRESS_ONLY_REACHED_OVER_HTTPS.into(),
        ));
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| AppError::Invalid(say::ADDRESS_NO_HOST.into()))?
        .to_owned();

    let port = parsed.port_or_known_default().unwrap_or(443);

    let addresses: Vec<SocketAddr> = tokio::net::lookup_host((host.clone(), port))
        .await
        .map_err(|_| AppError::Invalid(say::ADDRESS_NOT_RESOLVE.into()))?
        .collect();

    let first = *addresses
        .first()
        .ok_or_else(|| AppError::Invalid(say::ADDRESS_NOT_RESOLVE.into()))?;

    if !allow_private {
        for address in &addresses {
            webhook::allowed_destination(address.ip())?;
        }
    }

    let client = reqwest::Client::builder()
        .timeout(timeout)
        // A redirect is a second address, and it is not one anybody checked.
        .redirect(reqwest::redirect::Policy::none())
        .resolve(&host, first)
        .build()
        .map_err(|_| AppError::Bug("a client that cannot be built"))?;

    Ok(Reachable {
        client,
        url: parsed,
    })
}
