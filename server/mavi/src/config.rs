//! What this process was told, read once.
//!
//! The only file in the whole workspace that reads the environment. Everything
//! else is handed a value, which is what makes a test able to construct two
//! installations without either of them inheriting the machine.
//!
//! What is missing is refused here rather than at the moment it is first
//! needed. A process that starts without knowing where its database is will
//! fail on the first request instead — and the person watching the rollout
//! sees a healthy pod answering five hundreds.

use std::time::Duration;

use mavi_core::error::{Error, Result};

/// Everything this process needs to be told.
#[derive(Debug, Clone)]
pub struct Config {
    /// Where the rows are.
    pub database: String,
    /// How many connections it may hold. A default that fits a small machine:
    /// a pool larger than the database's own limit is a pool that fails at
    /// the busiest moment rather than the quietest.
    pub at_most_connections: u32,
    /// Where files somebody uploaded are kept.
    pub files: String,
    /// What to answer on.
    pub listen: String,
    pub worker: Worker,
}

/// How the worker beside the server behaves.
#[derive(Debug, Clone)]
pub struct Worker {
    /// What it calls itself when it takes a job. Two processes must not answer
    /// to one name, or each will think the other's work is its own.
    pub named: String,
    /// How long to wait when there was nothing to do. Long enough not to ask a
    /// database a hundred times a second about an empty table; short enough
    /// that a letter is not sitting there while somebody watches for it.
    pub when_there_is_nothing: Duration,
}

impl Config {
    /// Read once, at the edge.
    pub fn from_the_environment() -> Result<Self> {
        let database = told("DATABASE_URL")?;

        Ok(Self {
            database,
            at_most_connections: number("DATABASE_CONNECTIONS", 10)?,
            files: std::env::var("FILES").unwrap_or_else(|_| "./files".to_owned()),
            listen: std::env::var("LISTEN").unwrap_or_else(|_| "0.0.0.0:8080".to_owned()),
            worker: Worker {
                named: std::env::var("WORKER").unwrap_or_else(|_| named_after_the_machine()),
                when_there_is_nothing: Duration::from_millis(
                    number("WORKER_PAUSE_MS", 500)?.into(),
                ),
            },
        })
    }

    /// What it was told, as a person can read it — and **never the password**.
    ///
    /// A connection string in a log is a credential in a log, and the log is
    /// the one place everybody has read access to.
    #[must_use]
    pub fn as_said(&self) -> String {
        format!(
            "database {}, files {}, worker {}",
            without_the_password(&self.database),
            self.files,
            self.worker.named
        )
    }
}

fn told(name: &str) -> Result<String> {
    std::env::var(name).map_err(|_| {
        Error::internal(std::io::Error::other(format!(
            "{name} is not set, and this cannot be guessed"
        )))
    })
}

fn number(name: &str, unless: u32) -> Result<u32> {
    match std::env::var(name) {
        Err(_) => Ok(unless),
        Ok(said) => said
            .parse()
            .map_err(|_| Error::internal(std::io::Error::other(format!("{name} is not a number")))),
    }
}

/// A name for this worker that two machines will not share.
fn named_after_the_machine() -> String {
    let machine = std::env::var("HOSTNAME").unwrap_or_else(|_| "somewhere".to_owned());

    format!("{machine}-{}", std::process::id())
}

/// A connection string with whatever is between `:` and `@` taken out.
fn without_the_password(address: &str) -> String {
    let Some((front, back)) = address.split_once("://") else {
        return address.to_owned();
    };

    let Some((credentials, host)) = back.split_once('@') else {
        return address.to_owned();
    };

    let who = credentials.split(':').next().unwrap_or_default();

    format!("{front}://{who}@{host}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_password_is_not_something_a_log_says() {
        // The one that matters here: this line is printed at every start, and
        // a log is the place everybody has read access to.
        // The host is `localhost` because the check that keeps somebody's real
        // connection string out of this repository reads a diff rather than a
        // mind, and a placeholder that looks like a machine is a placeholder
        // it has to stop.
        let said = without_the_password("postgres://somebody:hunter2@localhost:5432/mavi");

        assert_eq!(said, "postgres://somebody@localhost:5432/mavi");
        assert!(!said.contains("hunter2"));
    }

    #[test]
    fn something_that_is_not_a_connection_string_comes_back_as_it_was() {
        // Rather than half of one: a value that cannot be taken apart is a
        // value nobody should be guessing at.
        assert_eq!(without_the_password("nonsense"), "nonsense");
        assert_eq!(
            without_the_password("postgres://localhost/mavi"),
            "postgres://localhost/mavi"
        );
    }

    #[test]
    fn two_workers_on_one_machine_are_two_names() {
        // The process id is in it because a machine runs more than one, and
        // two workers answering to one name each think the other's job is
        // theirs.
        assert!(named_after_the_machine().contains(&std::process::id().to_string()));
    }
}
