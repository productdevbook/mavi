//! Concrete provider composition for the self-host runtime.
//!
//! Mavi's domain only knows the `Mailer` port. The composition root uses a
//! small, provider-neutral HTTPS webhook so deployments can put SMTP, a
//! cloud provider, or an internal mail gateway behind one stable contract.

use std::{env, sync::Arc, time::Duration};

use mavi_core::{
    MaviError, Result, SiteContext,
    ports::{BoxFuture, MailDeliveryReceipt, MailDeliveryRequest, Mailer},
};
use serde::{Deserialize, Serialize};

const WEBHOOK_ENV: &str = "MAVI_MAIL_WEBHOOK_URL";
const TOKEN_ENV: &str = "MAVI_MAIL_WEBHOOK_TOKEN";
const MAX_ENDPOINT_CHARS: usize = 2_048;
const MAX_REFERENCE_CHARS: usize = 1_024;

/// Builds the mail adapter once at process startup.
pub fn from_env() -> Result<Arc<dyn Mailer>> {
    let Some(endpoint) = env::var_os(WEBHOOK_ENV) else {
        tracing::warn!(
            "{WEBHOOK_ENV} is not configured; mail deliveries will fail closed instead of being silently dropped"
        );
        return Ok(Arc::new(DisabledMailer));
    };
    let endpoint = endpoint
        .into_string()
        .map_err(|_| MaviError::validation("mail_webhook_url_not_unicode"))?;
    let endpoint = validate_endpoint(&endpoint)?;
    let token = env::var_os(TOKEN_ENV)
        .map(|value| {
            value
                .into_string()
                .map_err(|_| MaviError::validation("mail_webhook_token_not_unicode"))
        })
        .transpose()?
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| MaviError::Internal)?;
    Ok(Arc::new(WebhookMailer {
        client,
        endpoint,
        token,
    }))
}

#[derive(Clone, Debug)]
struct WebhookMailer {
    client: reqwest::Client,
    endpoint: String,
    token: Option<String>,
}

impl Mailer for WebhookMailer {
    fn send<'a>(
        &'a self,
        context: &'a SiteContext,
        request: MailDeliveryRequest,
    ) -> BoxFuture<'a, Result<MailDeliveryReceipt>> {
        let client = self.client.clone();
        let endpoint = self.endpoint.clone();
        let token = self.token.clone();
        let payload = WebhookRequest::from_request(context, request);
        Box::pin(async move {
            let mut request = client.post(endpoint).json(&payload);
            if let Some(token) = token {
                request = request.bearer_auth(token);
            }
            let response = request
                .send()
                .await
                .map_err(|_| MaviError::conflict("mail_provider_unavailable"))?;
            let status = response.status();
            if !status.is_success() {
                return Err(MaviError::conflict(format!(
                    "mail_provider_http_{}",
                    status.as_u16()
                )));
            }
            let response: WebhookResponse = response
                .json()
                .await
                .map_err(|_| MaviError::conflict("mail_provider_response_invalid"))?;
            let reference = validate_reference(&response.reference)?;
            Ok(MailDeliveryReceipt {
                provider: "https_webhook".to_owned(),
                reference,
            })
        })
    }
}

#[derive(Debug)]
struct DisabledMailer;

impl Mailer for DisabledMailer {
    fn send<'a>(
        &'a self,
        _context: &'a SiteContext,
        _request: MailDeliveryRequest,
    ) -> BoxFuture<'a, Result<MailDeliveryReceipt>> {
        Box::pin(async { Err(MaviError::conflict("mail_provider_not_configured")) })
    }
}

#[derive(Debug, Serialize)]
struct WebhookRequest {
    site_id: String,
    delivery_id: String,
    attempt_number: u16,
    idempotency_key: Option<String>,
    purpose: String,
    recipient: String,
    subject: String,
    body: String,
    content_type: &'static str,
    unsubscribe_url: Option<String>,
}

impl WebhookRequest {
    fn from_request(context: &SiteContext, request: MailDeliveryRequest) -> Self {
        let message = request.message;
        Self {
            site_id: context.site_id.to_string(),
            delivery_id: request.delivery_id.to_string(),
            attempt_number: request.attempt_number,
            idempotency_key: request.idempotency_key,
            purpose: request.purpose.as_str().to_owned(),
            recipient: message.recipient,
            subject: message.subject,
            body: message.body,
            content_type: message.content_type.as_str(),
            unsubscribe_url: message.unsubscribe_url,
        }
    }
}

#[derive(Debug, Deserialize)]
struct WebhookResponse {
    reference: String,
}

fn validate_endpoint(value: &str) -> Result<String> {
    let value = value.trim();
    let parsed = reqwest::Url::parse(value)
        .map_err(|_| MaviError::validation("mail_webhook_url_invalid"))?;
    if value.chars().count() > MAX_ENDPOINT_CHARS
        || !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
    {
        return Err(MaviError::validation("mail_webhook_url_invalid"));
    }
    Ok(value.to_owned())
}

fn validate_reference(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > MAX_REFERENCE_CHARS
        || value.chars().any(char::is_control)
    {
        return Err(MaviError::conflict("mail_provider_reference_invalid"));
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{validate_endpoint, validate_reference};

    #[test]
    fn webhook_endpoint_rejects_embedded_credentials_and_fragments() {
        assert!(validate_endpoint("https://mail.example.test/send").is_ok());
        assert!(validate_endpoint("ftp://mail.example.test/send").is_err());
        assert!(validate_endpoint("https://user:pass@127.0.0.1/send").is_err());
        assert!(validate_endpoint("https://mail.example.test/send#secret").is_err());
    }

    #[test]
    fn provider_reference_is_bounded_and_control_free() {
        assert_eq!(
            validate_reference(" provider-1 ").expect("reference"),
            "provider-1"
        );
        assert!(validate_reference("provider\n-1").is_err());
    }
}
