use serde::{Deserialize, Serialize};

use crate::{Email, MaviError, Result};

const MAX_SENDER_NAME_CHARS: usize = 200;
const MAX_DOMAIN_CHARS: usize = 253;

/// A validated provider-facing sender identity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct MailSender {
    pub address: Email,
    pub name: Option<String>,
}

impl MailSender {
    pub fn parse(address: &str, name: Option<&str>) -> Result<Self> {
        let address = Email::parse(address)
            .map_err(|_| MaviError::validation("mail_sender_address_invalid"))?;
        let name = name
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        if name.as_deref().is_some_and(|value| {
            value.chars().count() > MAX_SENDER_NAME_CHARS || value.chars().any(char::is_control)
        }) {
            return Err(MaviError::validation("mail_sender_name_invalid"));
        }
        Ok(Self { address, name })
    }

    #[must_use]
    pub fn domain(&self) -> &str {
        self.address.domain()
    }
}

/// Deployment policy for the sender identity used by a mail gateway.
///
/// Site-level sender settings are resolved before a delivery reaches this
/// policy. The deployment default is used for legacy or unconfigured sites,
/// while the explicit domain allowlist prevents a tenant value from turning
/// into an arbitrary provider `From` header.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailSenderPolicy {
    default: MailSender,
    allowed_domains: Vec<String>,
}

impl MailSenderPolicy {
    pub fn new(
        default_address: &str,
        default_name: Option<&str>,
        allowed_domains: impl IntoIterator<Item = String>,
    ) -> Result<Self> {
        let default = MailSender::parse(default_address, default_name)?;
        let mut allowed_domains = allowed_domains
            .into_iter()
            .map(|domain| normalize_domain(&domain))
            .collect::<Result<Vec<_>>>()?;
        if allowed_domains.is_empty() {
            allowed_domains.push(default.domain().to_owned());
        }
        if !allowed_domains
            .iter()
            .any(|domain| domain == default.domain())
        {
            return Err(MaviError::validation("mail_sender_domain_not_allowed"));
        }
        Ok(Self {
            default,
            allowed_domains,
        })
    }

    #[must_use]
    pub fn default_sender(&self) -> &MailSender {
        &self.default
    }

    #[must_use]
    pub fn allowed_domains(&self) -> &[String] {
        &self.allowed_domains
    }

    pub fn resolve(&self, sender: Option<&MailSender>) -> Result<MailSender> {
        let sender = sender.unwrap_or(&self.default);
        if !self
            .allowed_domains
            .iter()
            .any(|domain| domain == sender.domain())
        {
            return Err(MaviError::conflict("mail_sender_domain_not_allowed"));
        }
        Ok(sender.clone())
    }
}

fn normalize_domain(value: &str) -> Result<String> {
    let value = value.trim().to_ascii_lowercase();
    let valid = !value.is_empty()
        && value.len() <= MAX_DOMAIN_CHARS
        && value.contains('.')
        && value.split('.').all(|part| {
            !part.is_empty()
                && part.len() <= 63
                && !part.starts_with('-')
                && !part.ends_with('-')
                && part
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
        });
    if valid {
        Ok(value)
    } else {
        Err(MaviError::validation("mail_sender_domain_invalid"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sender_policy_defaults_to_the_sender_domain() {
        let policy = MailSenderPolicy::new("NoReply@Example.Test", None, []).expect("policy");

        assert_eq!(
            policy.default_sender().address.as_str(),
            "noreply@example.test"
        );
        assert_eq!(policy.allowed_domains(), &["example.test"]);
        assert_eq!(
            policy.resolve(None).expect("default").address.as_str(),
            "noreply@example.test"
        );
    }

    #[test]
    fn sender_policy_rejects_unlisted_domains_and_bad_names() {
        assert!(matches!(
            MailSenderPolicy::new("noreply@example.test", None, ["other.test".to_owned()]),
            Err(MaviError::Validation { code, .. }) if code == "mail_sender_domain_not_allowed"
        ));
        assert!(MailSender::parse("noreply@example.test", Some("Bad\nName")).is_err());
        assert!(
            MailSenderPolicy::new("noreply@example.test", None, ["*.example.test".to_owned()])
                .is_err()
        );
    }

    #[test]
    fn sender_policy_allows_a_configured_site_sender_in_the_allowlist() {
        let policy = MailSenderPolicy::new(
            "noreply@example.test",
            Some("Mavi"),
            ["example.test".to_owned(), "tenant.test".to_owned()],
        )
        .expect("policy");
        let sender = MailSender::parse("alerts@tenant.test", Some("Tenant")).expect("sender");

        assert_eq!(policy.resolve(Some(&sender)).expect("resolved"), sender);
    }
}
