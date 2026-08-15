//! Taking money, or not being able to.
//!
//! An enum of what a site can be configured with, including nothing at all: a
//! site with no provider gets an order and no way to pay for it, which is what
//! it has. Card details never reach this machine — what is kept is how to ask.
use serde::{Deserialize, Serialize};

use super::error::{AppError, Result};
use super::money::Money;
use super::secret::Secret;
use crate::kernel::say;

/// What a provider is asked for: take this much, for this order, and tell us
/// when it happens.
#[derive(Clone, Debug)]
pub struct Asking {
    pub order_id: uuid::Uuid,
    pub amount: Money,
    pub email: String,
    /// Where the person comes back to afterwards.
    pub back_to: String,
}

/// What it answered: its own name for the attempt, and where to send somebody.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Taking {
    pub provider_ref: String,
    pub pay_at: String,
}

/// Whoever takes the money. Never this machine: what a provider holds is what
/// this is not allowed to, which is the whole reason for the shape.
#[derive(Clone, Debug)]
pub enum Payments {
    /// Nothing configured. It answers with somewhere that does not exist, so a
    /// checkout is obviously not payable rather than looking payable.
    Absent,
    /// A hosted page: this machine hands over an amount and gets back a place
    /// to send somebody, and the card is typed on the provider's own page.
    Hosted(Hosted),
}

#[derive(Clone, Debug)]
pub struct Hosted {
    pub name: String,
    pub at: String,
    pub key: Secret<String>,
    /// What its callbacks are signed with.
    pub signing: Secret<String>,
}

impl Payments {
    #[must_use]
    pub fn from_env() -> Self {
        let (Ok(at), Ok(key), Ok(signing)) = (
            std::env::var("PAYMENTS_URL"),
            std::env::var("PAYMENTS_KEY"),
            std::env::var("PAYMENTS_SIGNING_KEY"),
        ) else {
            return Payments::Absent;
        };

        Payments::Hosted(Hosted {
            name: std::env::var("PAYMENTS_PROVIDER").unwrap_or_else(|_| "hosted".to_owned()),
            at,
            key: Secret::new(key),
            signing: Secret::new(signing),
        })
    }

    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Payments::Absent => "none",
            Payments::Hosted(hosted) => &hosted.name,
        }
    }

    pub async fn ask(&self, asking: &Asking) -> Result<Taking> {
        match self {
            Payments::Absent => Err(AppError::Refused(say::SITE_CANNOT_TAKE_MONEY_YET.into())),
            Payments::Hosted(hosted) => hosted.ask(asking).await,
        }
    }

    /// Whether a callback really came from the provider. An unsigned one is
    /// somebody telling this machine an order was paid for.
    #[must_use]
    pub fn signature_holds(&self, body: &str, signature: &str) -> bool {
        match self {
            Payments::Absent => false,
            Payments::Hosted(hosted) => {
                let expected = super::webhook::sign(
                    &Secret::new(hosted.signing.expose().as_bytes().to_vec()),
                    "payment",
                    0,
                    body,
                );

                // Compared to the end, not to the first difference: how long a
                // comparison takes is a thing somebody can measure.
                crate::kernel::secret::same(expected.as_bytes(), signature.as_bytes())
            }
        }
    }

    /// What the provider says it has taken, for the reconciliation pass.
    pub async fn taken_since(&self, since: chrono::DateTime<chrono::Utc>) -> Result<Vec<Settled>> {
        match self {
            Payments::Absent => Ok(Vec::new()),
            Payments::Hosted(hosted) => hosted.taken_since(since).await,
        }
    }
}

/// One payment as the provider sees it.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Settled {
    pub provider_ref: String,
    pub amount_minor: i64,
    pub state: String,
}

impl Hosted {
    async fn ask(&self, asking: &Asking) -> Result<Taking> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|_| AppError::Bug("a client that cannot be built"))?;

        let answer = client
            .post(format!("{}/payments", self.at.trim_end_matches('/')))
            .bearer_auth(self.key.expose())
            .json(&serde_json::json!({
                "reference": asking.order_id,
                "amount_minor": asking.amount.minor,
                "currency": asking.amount.currency,
                "email": asking.email,
                "return_to": asking.back_to,
            }))
            .send()
            .await
            .map_err(|_| AppError::Bug("the payment provider could not be reached"))?;

        if !answer.status().is_success() {
            return Err(AppError::Bug("the payment provider refused the request"));
        }

        answer
            .json::<Taking>()
            .await
            .map_err(|_| AppError::Bug("the payment provider answered with something else"))
    }

    async fn taken_since(&self, since: chrono::DateTime<chrono::Utc>) -> Result<Vec<Settled>> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|_| AppError::Bug("a client that cannot be built"))?;

        let answer = client
            .get(format!("{}/payments", self.at.trim_end_matches('/')))
            .bearer_auth(self.key.expose())
            .query(&[("since", since.to_rfc3339())])
            .send()
            .await
            .map_err(|_| AppError::Bug("the payment provider could not be reached"))?;

        answer
            .json::<Vec<Settled>>()
            .await
            .map_err(|_| AppError::Bug("the payment provider answered with something else"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_configured_refuses_rather_than_pretends() {
        assert_eq!(Payments::Absent.name(), "none");
        assert!(!Payments::Absent.signature_holds("anything", "anything"));
    }

    #[test]
    fn a_comparison_that_takes_the_same_time_either_way() {
        assert!(crate::kernel::secret::same(b"the same", b"the same"));
        assert!(!crate::kernel::secret::same(b"the same", b"the sane"));
        assert!(!crate::kernel::secret::same(b"short", b"longer than that"));
    }

    #[test]
    fn a_callback_nobody_signed_is_not_one() {
        let payments = Payments::Hosted(Hosted {
            name: "hosted".to_owned(),
            at: "https://payments.example".to_owned(),
            key: Secret::new("a key".to_owned()),
            signing: Secret::new("a signing key".to_owned()),
        });

        assert!(!payments.signature_holds("{\"paid\":true}", "v1,made up"));
    }

    #[test]
    fn the_signature_it_makes_is_the_signature_it_takes() {
        let payments = Payments::Hosted(Hosted {
            name: "hosted".to_owned(),
            at: "https://payments.example".to_owned(),
            key: Secret::new("a key".to_owned()),
            signing: Secret::new("a signing key".to_owned()),
        });

        let body = "{\"provider_ref\":\"pay_1\",\"state\":\"paid\"}";

        let signature = super::super::webhook::sign(
            &Secret::new("a signing key".as_bytes().to_vec()),
            "payment",
            0,
            body,
        );

        assert!(payments.signature_holds(body, &signature));
    }
}
