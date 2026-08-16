//! Whoever turns a site's design into pages.
//!
//! This process where a generator is configured, something on the network
//! where one is, and nobody at all — in which case the files are the site.
//! Which of the three is a deployment's decision rather than a site's.
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;

use super::error::{AppError, Result};
use super::secret::Secret;

/// What a builder is given: a site, a branch, and the files as they are.
/// Both sides of the wire read this from here rather than each writing their
/// own: two copies of a shape are two things to keep in step, and the one
/// that drifts is found by a build that quietly does nothing.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Building {
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
    /// Run here, in a workspace of its own, one at a time. What is here is what
    /// to run; running it — the workspace, the limits, what a build may read —
    /// belongs to whoever does the building.
    Here(Generator),
    /// Something on the network that takes a site and answers when it is done.
    Elsewhere(Elsewhere),
}

#[derive(Clone, Debug)]
pub struct Elsewhere {
    pub at: String,
    pub key: Secret<String>,
}

/// What is run to turn a site's files into pages, where, and how many at once.
///
/// Configuration of an outside program, the same kind of thing as the address
/// and key of one on the network — so it is read here, beside the other arm,
/// rather than by whoever runs it.
#[derive(Clone, Debug)]
pub struct Generator {
    /// The command, as a program and its arguments — never a shell line. A
    /// shell line is how a site's own name ends up being executed.
    pub program: String,
    pub arguments: Vec<String>,
    /// What the command leaves behind, relative to the workspace.
    pub output: String,
    /// Where workspaces are made. Each build gets one of its own inside it.
    pub workspaces: PathBuf,
    /// How many builds may run at once on this machine. One, because a build
    /// takes every core it can and this machine is also serving.
    pub at_once: Arc<Semaphore>,
}

impl Generator {
    /// From the environment, or nothing — a machine with no generator is a
    /// machine whose sites are their own files, which is what most are.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let line = std::env::var("GENERATOR").ok()?;
        let mut words = line.split_whitespace().map(str::to_owned);
        let program = words.next()?;

        let at_once = std::env::var("BUILDS_AT_ONCE")
            .ok()
            .and_then(|how_many| how_many.parse().ok())
            .unwrap_or(1_usize)
            .clamp(1, 8);

        Some(Self {
            program,
            arguments: words.collect(),
            output: std::env::var("GENERATOR_OUTPUT").unwrap_or_else(|_| "dist".to_owned()),
            workspaces: std::env::var("WORKSPACES")
                .unwrap_or_else(|_| "workspaces".to_owned())
                .into(),
            at_once: Arc::new(Semaphore::new(at_once)),
        })
    }
}

impl Builder {
    /// Something on the network that takes a site, and what to sign what is
    /// said to it with.
    #[must_use]
    pub fn elsewhere(at: String, key: Secret<String>) -> Self {
        Builder::Elsewhere(Elsewhere { at, key })
    }

    #[must_use]
    pub fn from_env() -> Self {
        if let Some(generator) = Generator::from_env() {
            return Builder::Here(generator);
        }

        let (Ok(at), Ok(key)) = (std::env::var("BUILDER_URL"), std::env::var("BUILDER_KEY")) else {
            return Builder::Direct;
        };

        Builder::elsewhere(at, Secret::new(key))
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
                "a generator that runs here is run by whoever asked for the build, not by this",
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
