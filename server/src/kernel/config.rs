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
//! the address it listens on, how many workers it runs, how many proxies it
//! believes, which role it is running as — is not here, because two processes
//! of the same installation differ in it; that is read where the process is
//! set up.
use super::builder::Builder;
use super::crypto::Keyring;
use super::mailer::Mailer;
use super::payments::Payments;
use super::storage::Store;
use super::transcoder::Transcoder;

/// One installation, handed in.
#[derive(Clone, Debug)]
pub struct Config {
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
    /// An installation with nothing configured: a key nobody gave, mail
    /// written down and handed nowhere, no way to take money, no generator
    /// and no transcoder, uploads in a directory beside the process.
    ///
    /// What a test wants, and what somebody trying this out on a laptop has.
    /// Nothing sealed under an invented key survives the process that made
    /// it, which is why a machine holding anything says so through
    /// [`Config::from_env`] instead.
    #[must_use]
    pub fn nothing_configured() -> Self {
        Self {
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
    /// The keyring is given rather than read here: a machine with no key at
    /// all is a refusal, and whether to refuse or to invent one is a decision
    /// about how this process was started rather than about what this crate
    /// does. `start` makes it, in one place.
    #[must_use]
    pub fn from_env(keyring: Keyring) -> Self {
        Self {
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
    }
}
