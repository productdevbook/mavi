//! What one installation is, as a value rather than as the environment.
//!
//! Everything [`AppState`](super::http::AppState) used to read for itself:
//! where uploads go, what seals a site's secrets, where mail goes, who takes
//! the money, who turns files into pages, who turns a video into something a
//! browser can play. Read at the edge — `start`, for the binary — and handed
//! in from there.
//!
//! Read in the middle it is a global, and every consumer inherits it: a test
//! that wants mail to go nowhere has to set a variable the whole process
//! sees, and two tests in one process cannot want different things.
//!
//! What is here belongs to an *installation*. What belongs to a *process* —
//! the socket it binds, how many workers it runs, how many proxies it
//! believes, which role it is running as — is not here, because two processes
//! of the same installation differ in it; that is read where the process is
//! set up. [`Address`] is the installation's, not the process's: two
//! processes of one installation bind different sockets and are still reached
//! at the same address, and it is the one they both have to put in a letter.
use super::builder::Builder;
use super::crypto::Keyring;
use super::error::{AppError, Result};
use super::mailer::Mailer;
use super::payments::Payments;
use super::storage::Store;
use super::transcoder::Transcoder;

/// Where this installation answers, as somebody outside it would type it: a
/// scheme, a host, and whatever path it is served under.
///
/// Held without a trailing slash, so that a whole URL is this and a path
/// joined and nothing has to decide which half of the join carries the
/// separator. Normalised once, here, rather than at each place that builds
/// one.
///
/// Configured rather than worked out, because what needs it most has no
/// request to take it from. Resolving a site from `Host` made an address feel
/// unnecessary — every request carried one — but a scheduled job sending a
/// letter carries nothing, and a bare path in a plain-text mail is not a link
/// anybody can click.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Address(String);

impl Address {
    /// What was given, refused where it is not something anybody could type
    /// into a browser.
    ///
    /// Strict on purpose. A scheme, because a link without one is the whole
    /// reason this exists; `http` or `https`, because a mail client will not
    /// offer to follow anything else; and no query or fragment, because what
    /// is joined onto this brings its own.
    pub fn read(raw: &str) -> Result<Self> {
        let raw = raw.trim();

        let rest = raw
            .strip_prefix("https://")
            .or_else(|| raw.strip_prefix("http://"))
            .ok_or(AppError::Bug(
                "an address with no http:// or https:// on it",
            ))?;

        if rest.contains(['?', '#']) {
            return Err(AppError::Bug("an address carrying a query or a fragment"));
        }

        let host = rest.split('/').next().unwrap_or_default();

        if host.is_empty() || host.contains(char::is_whitespace) {
            return Err(AppError::Bug("an address with no host in it"));
        }

        Ok(Self(raw.trim_end_matches('/').to_owned()))
    }

    /// [`Address::read`], reading `MAVI_URL` itself.
    ///
    /// Absent is its own case rather than an error — the same distinction
    /// [`Keyring::given`] keeps. Whether a process that does not know its own
    /// address may run at all is a decision about how it was started, and
    /// `start` makes it; whether what it was given is an address at all is
    /// answered here, where it is read, rather than at the letter that turns
    /// out not to have a link in it.
    pub fn from_the_environment() -> Result<Option<Self>> {
        match std::env::var("MAVI_URL") {
            Ok(raw) => Self::read(&raw).map(Some),
            // Set to something that is not text is still set, and falling back
            // to an invented address here would be the silence this refuses.
            Err(std::env::VarError::NotUnicode(_)) => {
                Err(AppError::Bug("an address that is not text"))
            }
            Err(std::env::VarError::NotPresent) => Ok(None),
        }
    }

    /// An obviously invented one, on a name nothing can resolve.
    ///
    /// What a test wants — a whole URL to read back out of a letter — and what
    /// [`Config::nothing_configured`] has. A machine anybody can reach says
    /// its own through `MAVI_URL`, and `start` will not run without it.
    #[must_use]
    pub fn invented() -> Self {
        Self("https://example.invalid".to_owned())
    }

    /// The host on its own, for the one thing that asks the network about it
    /// rather than putting it in a link.
    #[must_use]
    pub fn host(&self) -> &str {
        self.0
            .split_once("://")
            .map_or(self.0.as_str(), |(_, rest)| rest)
            .split('/')
            .next()
            .unwrap_or_default()
    }

    /// A whole URL for one path within this installation — the path the panel
    /// routes on, leading slash and all.
    #[must_use]
    pub fn link(&self, path: &str) -> String {
        debug_assert!(path.starts_with('/'), "a path within a site starts at /");

        format!("{}{path}", self.0)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One installation, handed in.
#[derive(Clone, Debug)]
pub struct Config {
    /// Where this installation answers, for anything building a link with no
    /// request in front of it.
    pub address: Address,
    /// Where what anybody uploaded is kept.
    pub store: Store,
    /// What a site's stored credentials are sealed with.
    pub keyring: Keyring,
    /// Where mail goes.
    pub mailer: Mailer,
    /// Whoever takes the money.
    pub payments: Payments,
    /// Who turns a site's files into pages.
    pub builder: Builder,
    /// Who turns an uploaded video into something a browser can play.
    pub transcoder: Transcoder,
}

impl Config {
    /// An installation with nothing configured: an address nothing resolves,
    /// a key nobody gave, mail written down and handed nowhere, no way to take
    /// money, no generator and no transcoder, uploads in a directory beside
    /// the process.
    ///
    /// What a test wants, and what somebody trying this out on a laptop has.
    /// Nothing sealed under an invented key survives the process that made
    /// it, which is why a machine holding anything says so through
    /// [`Config::from_env`] instead.
    #[must_use]
    pub fn nothing_configured() -> Self {
        Self {
            address: Address::invented(),
            store: Store::beside_the_process(),
            keyring: Keyring::invented(),
            mailer: Mailer::recorded(),
            payments: Payments::Absent,
            builder: Builder::Direct,
            transcoder: Transcoder::AsUploaded,
        }
    }

    /// The same, read from the environment — one line per thing, each through
    /// the `from_env` that already belonged to it.
    ///
    /// The keyring and the address are given rather than read here, for the
    /// same reason: a machine with neither is a refusal, and whether to refuse
    /// or to carry on is a decision about how this process was started rather
    /// than about what this crate does. `start` makes both, in one place.
    #[must_use]
    pub fn from_env(keyring: Keyring, address: Address) -> Self {
        Self {
            address,
            store: Store::from_env(),
            keyring,
            mailer: Mailer::from_env(),
            payments: Payments::from_env(),
            builder: Builder::from_env(),
            transcoder: Transcoder::from_env(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Not a tautology: what is being asked is that the arms an installation
    /// with nothing configured gets are the ones that are obviously doing
    /// nothing — recorded, absent, direct — rather than ones that look like
    /// a working machine and are not.
    #[test]
    fn nothing_configured_is_obviously_doing_nothing() {
        let config = Config::nothing_configured();

        assert!(matches!(config.mailer, Mailer::Recorded(_)));
        assert!(matches!(config.payments, Payments::Absent));
        assert!(matches!(config.builder, Builder::Direct));
        assert!(matches!(config.transcoder, Transcoder::AsUploaded));
        assert!(config.address.as_str().ends_with(".invalid"));
    }

    /// Refused where it is read, rather than at the letter it would have been
    /// a link in: by then nobody can tell an address that was mistyped from
    /// one that was never set.
    #[test]
    fn something_that_is_not_an_address_is_refused_where_it_is_read() {
        for said in [
            "example.invalid",
            "//example.invalid",
            "/forgotten",
            "",
            "https://",
            "ftp://example.invalid",
            "https://example .invalid",
            "https://example.invalid/?utm=1",
        ] {
            assert!(
                Address::read(said).is_err(),
                "{said:?} was taken for an address"
            );
        }
    }

    #[test]
    fn an_address_is_normalised_once_so_that_no_link_is_joined_twice() {
        let plain = Address::read("  https://example.invalid/  ").expect("an address");

        assert_eq!(
            plain.link("/forgotten?token=abc"),
            "https://example.invalid/forgotten?token=abc"
        );

        let under = Address::read("http://example.invalid/cms").expect("an address");

        assert_eq!(
            under.link("/forgotten?token=abc"),
            "http://example.invalid/cms/forgotten?token=abc"
        );
    }
}
