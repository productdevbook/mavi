//! Who turns an uploaded video into something a browser can play.
//!
//! Not this process. Transcoding is minutes of every core the machine has, and
//! this machine serves other people's sites while it runs. What is here is
//! everything around that — handed over, recorded, answered for — and the arm
//! that hands it to whatever actually does it.

use serde::{Deserialize, Serialize};

use super::error::{AppError, Result};
use super::secret::Secret;

/// What a transcoder is given: which video, and where the file it was made from
/// can be fetched.
#[derive(Clone, Debug, Serialize)]
pub struct Handing {
    pub tenant: uuid::Uuid,
    pub video: uuid::Uuid,
    pub source: String,
}

/// What it says when it has taken it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Taken {
    /// Its own name for the work, kept so that what comes back later can be
    /// matched to what went out.
    #[serde(default)]
    pub reference: String,
}

#[derive(Clone, Debug)]
pub enum Transcoder {
    /// Nothing to transcode with. The file that was uploaded is what plays,
    /// which is what a site with no transcoder has — an MP4 a browser can
    /// already read, served as it was.
    AsUploaded,
    Elsewhere(Elsewhere),
}

#[derive(Clone, Debug)]
pub struct Elsewhere {
    pub at: String,
    pub key: Secret<String>,
}

impl Transcoder {
    /// Something on the network that takes a video, and what to sign what is
    /// said to it with.
    #[must_use]
    pub fn elsewhere(at: String, key: Secret<String>) -> Self {
        Transcoder::Elsewhere(Elsewhere { at, key })
    }

    #[must_use]
    pub fn from_env() -> Self {
        let (Ok(at), Ok(key)) = (
            std::env::var("TRANSCODER_URL"),
            std::env::var("TRANSCODER_KEY"),
        ) else {
            return Transcoder::AsUploaded;
        };

        Transcoder::elsewhere(at, Secret::new(key))
    }

    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Transcoder::AsUploaded => "as-uploaded",
            Transcoder::Elsewhere(_) => "elsewhere",
        }
    }

    /// Whether anything has to be waited for. A machine with no transcoder
    /// answers "it is ready" rather than leaving a video working for ever.
    #[must_use]
    pub fn works_on_it(&self) -> bool {
        matches!(self, Transcoder::Elsewhere(_))
    }

    pub async fn hand_over(&self, handing: &Handing) -> Result<Taken> {
        match self {
            Transcoder::AsUploaded => Ok(Taken {
                reference: String::new(),
            }),
            Transcoder::Elsewhere(elsewhere) => elsewhere.hand_over(handing).await,
        }
    }
}

impl Elsewhere {
    async fn hand_over(&self, handing: &Handing) -> Result<Taken> {
        let client = reqwest::Client::builder()
            // Handing it over is a request; doing it is not. What is waited for
            // here is only "yes, I have it".
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|_| AppError::Bug("a client that cannot be built"))?;

        let answer = client
            .post(format!("{}/videos", self.at.trim_end_matches('/')))
            .bearer_auth(self.key.expose())
            .json(handing)
            .send()
            .await
            .map_err(|_| AppError::Bug("the transcoder could not be reached"))?;

        if !answer.status().is_success() {
            return Err(AppError::Bug("the transcoder would not take it"));
        }

        answer
            .json()
            .await
            .map_err(|_| AppError::Bug("the transcoder answered with nothing usable"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_machine_with_no_transcoder_does_not_leave_a_video_working_for_ever() {
        let nothing = Transcoder::AsUploaded;

        assert!(!nothing.works_on_it());
        assert_eq!(
            nothing
                .hand_over(&Handing {
                    tenant: uuid::Uuid::nil(),
                    video: uuid::Uuid::nil(),
                    source: "/uploads/something".to_owned(),
                })
                .await
                .expect("nothing to hand it to is not a failure")
                .reference,
            ""
        );
    }
}
