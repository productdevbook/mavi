//! Sending a letter, or writing down that one was sent.
//!
//! An enum rather than a trait object: what a machine can send with is decided
//! where it is configured, and a test uses the one that records instead of a
//! server that does not exist.
use serde::Serialize;

use lettre::AsyncTransport as _;

use super::error::{AppError, Result};
use super::secret::Secret;
use crate::kernel::say;

/// One message on its way out.
#[derive(Clone, Debug, Serialize)]
pub struct Letter {
    pub to: String,
    pub subject: String,
    pub body: String,
    /// Where a reply goes and what the recipient sees it came from. A site
    /// does not choose this: mail from an address a site invented is mail
    /// nobody delivers.
    pub from: String,
    /// What the receiver needs to stop hearing from this list. Present on
    /// anything that is not a message somebody asked for by acting.
    pub unsubscribe: Option<String>,
}

/// What happened to it, as far as the thing handing it over knows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Handed {
    /// Taken, with whatever the provider calls it.
    Over(String),
    /// Refused for a reason that will not change by trying again — a malformed
    /// address, a blocked recipient.
    Refused(String),
}

/// Where mail goes. One arm writes it down and hands it nowhere, which is what
/// a test and a machine with no provider both want; the other is SMTP.
#[derive(Clone, Debug)]
pub enum Mailer {
    Recorded(Recorder),
    Smtp(Smtp),
}

impl Mailer {
    /// A server to hand mail to, and the address it goes out as.
    #[must_use]
    pub fn new(url: Secret<String>, from: String) -> Self {
        Mailer::Smtp(Smtp::new(url, from))
    }

    /// Written down and handed nowhere. What a test wants, and what a machine
    /// with no provider configured has.
    #[must_use]
    pub fn recorded() -> Self {
        Mailer::Recorded(Recorder::default())
    }

    /// Reads the environment, and falls back to recording rather than to
    /// pretending: a machine with no provider configured should be obviously
    /// not sending, not quietly not sending.
    #[must_use]
    pub fn from_env() -> Self {
        let Ok(url) = std::env::var("SMTP_URL") else {
            return Mailer::recorded();
        };

        Mailer::new(
            Secret::new(url),
            std::env::var("MAIL_FROM").unwrap_or_else(|_| "no-reply@example.invalid".to_owned()),
        )
    }

    #[must_use]
    pub fn from(&self) -> String {
        match self {
            Mailer::Recorded(_) => "no-reply@example.invalid".to_owned(),
            Mailer::Smtp(smtp) => smtp.from.clone(),
        }
    }

    /// Whether it is there, for whatever can be asked of it. A recorder is
    /// always there, which is what a machine with nothing configured has.
    pub async fn reachable(&self) -> Result<()> {
        match self {
            Mailer::Recorded(_) => Ok(()),
            Mailer::Smtp(smtp) => smtp.reachable().await,
        }
    }

    pub async fn hand_over(&self, letter: &Letter) -> Result<Handed> {
        match self {
            Mailer::Recorded(recorder) => Ok(recorder.keep(letter)),
            Mailer::Smtp(smtp) => smtp.send(letter).await,
        }
    }
}

/// What was handed over, in memory, for a test to read back and for a machine
/// with nothing configured to leave a trail in the log.
#[derive(Clone, Debug, Default)]
pub struct Recorder {
    kept: std::sync::Arc<std::sync::Mutex<Vec<Letter>>>,
}

impl Recorder {
    fn keep(&self, letter: &Letter) -> Handed {
        tracing::info!(to = %letter.to, subject = %letter.subject, "no provider; not sent");

        if let Ok(mut kept) = self.kept.lock() {
            kept.push(letter.clone());
        }

        Handed::Over(format!("recorded-{}", uuid::Uuid::now_v7()))
    }

    /// Everything handed over so far. For a test; nothing in a request path
    /// reads this.
    #[must_use]
    pub fn all(&self) -> Vec<Letter> {
        self.kept
            .lock()
            .map(|kept| kept.clone())
            .unwrap_or_default()
    }
}

/// `List-Unsubscribe`, which lettre has no typed header for.
#[derive(Clone, Debug)]
struct Unsubscribe(String);

impl lettre::message::header::Header for Unsubscribe {
    fn name() -> lettre::message::header::HeaderName {
        lettre::message::header::HeaderName::new_from_ascii_str("List-Unsubscribe")
    }

    fn parse(value: &str) -> std::result::Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self(value.to_owned()))
    }

    fn display(&self) -> lettre::message::header::HeaderValue {
        lettre::message::header::HeaderValue::new(Self::name(), format!("<{}>", self.0))
    }
}

#[derive(Clone, Debug)]
pub struct Smtp {
    url: Secret<String>,
    from: String,
}

impl Smtp {
    #[must_use]
    pub fn new(url: Secret<String>, from: String) -> Self {
        Self { url, from }
    }

    /// Whether the server answers and takes the credentials in the address.
    /// Asked when somebody configures it, so that a site finds out here rather
    /// than from a customer who never got their receipt.
    pub async fn reachable(&self) -> Result<()> {
        let transport: lettre::AsyncSmtpTransport<lettre::Tokio1Executor> =
            lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::from_url(self.url.expose())
                .map_err(|_| AppError::Invalid(say::NOT_MAIL_SERVER_ADDRESS.into()))?
                .build();

        match transport.test_connection().await {
            Ok(true) => Ok(()),
            Ok(false) => Err(AppError::Refused(say::MAIL_SERVER_DID_NOT_ANSWER.into())),
            Err(_) => Err(AppError::Refused(say::MAIL_SERVER_COULD_NOT_REACHED.into())),
        }
    }

    /// Handing one message to a server that speaks SMTP.
    ///
    /// The address of the server, and any password in it, is a secret and does
    /// not appear in an error: what comes back says what went wrong, not where.
    async fn send(&self, letter: &Letter) -> Result<Handed> {
        let message = lettre::Message::builder()
            .from(
                self.from
                    .parse()
                    .map_err(|_| AppError::Bug("the address this machine sends from"))?,
            )
            .to(letter
                .to
                .parse()
                .map_err(|_| AppError::Invalid(say::NOT_ADDRESS_WRITE.into()))?)
            .subject(letter.subject.clone())
            .header(lettre::message::header::ContentType::TEXT_PLAIN);

        // The header a mail client turns into a one-click way out. Written as
        // a raw header because lettre has no typed one for it.
        let message = match &letter.unsubscribe {
            Some(link) => message.header(Unsubscribe(link.clone())),
            None => message,
        }
        .body(letter.body.clone())
        .map_err(|_| AppError::Invalid(say::MESSAGE_CANNOT_SENT.into()))?;

        let transport =
            lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::from_url(self.url.expose())
                .map_err(|_| AppError::Bug("the address of the mail server"))?
                .build();

        match transport.send(message).await {
            Ok(answer) => Ok(Handed::Over(answer.message().collect::<Vec<_>>().join(" "))),
            Err(why) if why.is_permanent() => Ok(Handed::Refused(why.to_string())),
            Err(_) => Err(AppError::Bug("the mail server could not be reached")),
        }
    }
}
