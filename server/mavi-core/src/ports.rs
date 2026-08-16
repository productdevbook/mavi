//! What this software asks a host for.
//!
//! **Ports ask, they do not answer.** Everything here is something a host
//! already has, or already has an opinion about, and building it here would be
//! building a worse copy of it.
//!
//! This is the inverse of the seam it replaces. That one let a host hand in
//! endpoints and job kinds — it could *extend* this software. It had one user,
//! and what that user actually wanted was the other direction: something it
//! could construct and configure, not a host it could add to.
//!
//! Adding a trait here is a real decision. It becomes work for everybody
//! embedding this, and a port nobody implements differently is a parameter
//! wearing a costume. Prefer a value.
//!
//! Nothing here reads the environment. Configuration is read at the edge —
//! once, by whatever owns the process — and handed in. A library that reads
//! `std::env` in a constructor makes every consumer inherit a global, and the
//! same process constructing two of anything gets one of them wrong.

use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::Result;

/// What an implementation hands back.
pub type Answering<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// What time it is.
///
/// A port rather than `Utc::now()` for one reason: a test that cannot move
/// time cannot ask what happens after ninety days, and so nobody writes that
/// test.
pub trait Clock: Debug + Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// The system clock, for everything that is not a test.
#[derive(Debug, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Where a file somebody uploaded is kept.
///
/// A path is opaque: whoever implements this decides what it means, and
/// nothing above may take one apart. A path that arrives from outside is
/// checked by the implementation, not by its caller — the traversal is the
/// implementation's own business and every caller getting it right is a rule
/// that holds until it does not.
pub trait Files: Debug + Send + Sync {
    fn put<'a>(&'a self, at: &'a str, bytes: Vec<u8>) -> Answering<'a, ()>;
    fn get<'a>(&'a self, at: &'a str) -> Answering<'a, Vec<u8>>;
    fn remove<'a>(&'a self, at: &'a str) -> Answering<'a, ()>;
}

/// Turning what a design is into what a visitor is served.
///
/// The one port that runs somebody else's code. A site's look is a project —
/// it may be plain files, and it may be a generator with its own dependencies
/// and its own command. The second is a machine running whatever a customer
/// wrote, which is a sandbox, a scheduler and a quota rather than a function,
/// and none of those belong in a library that anybody installs.
///
/// So this software says what it needs and does not say how. What ships with
/// it serves what a design put under `public/`, which is a whole site when a
/// site is plain files. A host that builds each site's own project implements
/// this instead, and nothing above here knows which one it got.
pub trait Builds: Debug + Send + Sync {
    /// `everything` is a set of changes, whole: every path in the project and
    /// its bytes. Read whole rather than a file at a time because a build
    /// reads all of them, and files read one after another while somebody is
    /// writing are a build of two different things.
    fn build<'a>(
        &'a self,
        change: Uuid,
        everything: &'a [(String, Vec<u8>)],
    ) -> Answering<'a, Built>;
}

/// What a build came back with.
///
/// Not a `Result`. A design that does not compile is an ordinary thing for
/// somebody to go and fix, and what they need is **the message** — so it comes
/// back as an answer rather than as an error, which is the only way it survives
/// to be shown. An `Err` from [`Builds::build`] means the builder itself could
/// not be reached, and that is this end's problem rather than theirs.
#[derive(Clone, Debug)]
pub enum Built {
    /// What to serve, by the path a visitor asks for it at.
    Serve(Vec<(String, Vec<u8>)>),
    /// It did not build, and this is what it said.
    WentWrong(String),
}

/// Sealing something this software has to be able to read back.
///
/// A password is hashed, because nothing ever needs it again — what is checked
/// is whether a new hash matches. A **second factor's secret is not like that**:
/// the six digits have to be computed from it on every sign-in, so it has to
/// come back out. Hashing it is not an option, and keeping it plainly makes a
/// copy of the database a drawer of working authenticators.
///
/// So it is sealed, and the key belongs to whoever runs this rather than to
/// this. A host with a key manager uses it; a host with a file on disk uses
/// that. What this software knows is that the secret must not be readable from
/// the rows alone.
///
/// **An installation that provides none of this simply has no second factors.**
/// That is said where somebody asks for one, rather than sealed with a key
/// baked into the source — which would be the appearance of the thing without
/// the thing.
pub trait Seals: Debug + Send + Sync {
    fn seal<'a>(&'a self, what: &'a [u8]) -> Answering<'a, Vec<u8>>;
    fn open<'a>(&'a self, sealed: &'a [u8]) -> Answering<'a, Vec<u8>>;
}

/// Where a letter goes.
///
/// This software decides that a letter should be sent and what it says. It
/// does not decide how mail leaves a machine, and a host that already sends
/// mail should not gain a second way to.
pub trait Post: Debug + Send + Sync {
    fn send<'a>(&'a self, letter: Letter<'a>) -> Answering<'a, ()>;
}

/// One letter, ready to go.
#[derive(Debug)]
pub struct Letter<'a> {
    pub to: &'a str,
    pub subject: &'a str,
    pub body: &'a str,
    /// Where somebody stops receiving these. Absent for a letter that is not
    /// one anybody subscribes to — a password reset is not a mailing.
    pub unsubscribe: Option<&'a str>,
}

/// What happened, said outward.
///
/// This software writes the fact down in its own transaction and hands it
/// here. Delivering it — a queue, a webhook, a log — is the host's, and a
/// host that already has a message bus should not gain a second one.
pub trait Told: Debug + Send + Sync {
    fn tell<'a>(&'a self, what: &'a str, about: &'a serde_json::Value) -> Answering<'a, ()>;
}

/// This installation's own public address.
///
/// Not a lookup and not a header: one configured value. A scheduled job
/// sending a letter has no request to take an address from, which is how every
/// password-reset link in the crate this replaces came to be a bare path that
/// no mail client could turn into a link.
#[derive(Clone, Debug)]
pub struct Address(String);

impl Address {
    /// Refuses anything that is not an absolute `http` or `https` address, at
    /// the edge, where the person who set it can still see the message.
    pub fn parse(text: &str) -> Result<Self> {
        let text = text.trim_end_matches('/');

        let looks_right = (text.starts_with("https://") || text.starts_with("http://"))
            && text
                .split("://")
                .nth(1)
                .is_some_and(|rest| !rest.is_empty());

        if !looks_right {
            return Err(crate::error::Error::internal(std::io::Error::other(
                "an address is http:// or https:// and a host",
            )));
        }

        Ok(Self(text.to_owned()))
    }

    /// This address with a path on the end. The only way to build a link, so
    /// that no caller can build half of one.
    #[must_use]
    pub fn to(&self, path: &str) -> String {
        format!("{}/{}", self.0, path.trim_start_matches('/'))
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_is_a_link_rather_than_a_path() {
        let address = Address::parse("https://example.test/").expect("an address");

        assert_eq!(
            address.to("/forgotten?token=abc"),
            "https://example.test/forgotten?token=abc"
        );
        assert_eq!(address.to("forgotten"), "https://example.test/forgotten");
    }

    #[test]
    fn half_an_address_is_refused_where_it_is_read() {
        for wrong in [
            "",
            "example.test",
            "https://",
            "ftp://example.test",
            "/forgotten",
        ] {
            assert!(
                Address::parse(wrong).is_err(),
                "{wrong:?} was taken for an address"
            );
        }
    }
}
