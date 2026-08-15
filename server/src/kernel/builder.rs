//! Whoever turns a site's design into pages.
//!
//! This process where a generator is configured, something on the network
//! where one is, and nobody at all — in which case the files are the site.
//! Which of the three is a deployment's decision rather than a site's.
use serde::{Deserialize, Serialize};

use super::error::{AppError, Result};
use super::secret::Secret;

/// What a builder is given: a site, a branch, and the files as they are.
/// Both sides of the wire read this from here rather than each writing their
/// own: two copies of a shape are two things to keep in step, and the one
/// that drifts is found by a build that quietly does nothing.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Building {
    pub tenant: uuid::Uuid,
    pub branch: String,
    pub publish: uuid::Uuid,
}

/// What it says afterwards.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Built {
    pub ok: bool,
    /// What it said while it worked, which is what somebody reads when it did
    /// not work.
    #[serde(default)]
    pub log: String,
}

/// Who turns a site's files into pages.
///
/// Not this process: a site generator is a container with a language runtime in
/// it, and this machine serves other people's sites while it runs. What is here
/// is everything around that — one at a time, recorded, counted — and the arm
/// that actually builds.
#[derive(Clone, Debug)]
pub enum Builder {
    /// Nothing to build with. A publish becomes live immediately, which is
    /// what a site with no generator is: its files are its pages.
    Direct,
    /// Run here, in a workspace of its own, one at a time. What that costs and
    /// what it guards against is in `building`.
    Here(crate::building::Generator),
    /// Something on the network that takes a site and answers when it is done.
    Elsewhere(Elsewhere),
}

#[derive(Clone, Debug)]
pub struct Elsewhere {
    pub at: String,
    pub key: Secret<String>,
}

impl Builder {
    #[must_use]
    pub fn from_env() -> Self {
        if let Some(generator) = crate::building::Generator::from_env() {
            return Builder::Here(generator);
        }

        let (Ok(at), Ok(key)) = (std::env::var("BUILDER_URL"), std::env::var("BUILDER_KEY")) else {
            return Builder::Direct;
        };

        Builder::Elsewhere(Elsewhere {
            at,
            key: Secret::new(key),
        })
    }

    /// Whether something else writes what is served. With nothing to build
    /// with, a site's own files are its pages and this process is what puts
    /// them where they are read from.
    #[must_use]
    pub fn builds_elsewhere(&self) -> bool {
        matches!(self, Builder::Elsewhere(_) | Builder::Here(_))
    }

    pub async fn build(&self, building: &Building) -> Result<Built> {
        match self {
            Builder::Direct => Ok(Built {
                ok: true,
                log: "no builder configured; the files are the site".to_owned(),
            }),
            Builder::Here(_) => Err(AppError::Bug(
                "a generator that runs here is run by publishing, not by this",
            )),
            Builder::Elsewhere(elsewhere) => elsewhere.build(building).await,
        }
    }
}

impl Elsewhere {
    async fn build(&self, building: &Building) -> Result<Built> {
        let client = reqwest::Client::builder()
            // A build is minutes, not seconds, and a builder that has stopped
            // answering must not hold a worker for the afternoon.
            .timeout(std::time::Duration::from_mins(10))
            .build()
            .map_err(|_| AppError::Bug("a client that cannot be built"))?;

        let answer = client
            .post(format!("{}/builds", self.at.trim_end_matches('/')))
            .bearer_auth(self.key.expose())
            .json(building)
            .send()
            .await
            .map_err(|_| AppError::Bug("the builder could not be reached"))?;

        if !answer.status().is_success() {
            return Ok(Built {
                ok: false,
                log: format!("the builder answered {}", answer.status()),
            });
        }

        answer
            .json::<Built>()
            .await
            .map_err(|_| AppError::Bug("the builder answered with something else"))
    }
}
