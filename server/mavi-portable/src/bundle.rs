//! What is in the file.
//!
//! Its own types rather than the domains' own, and that is the point: what a
//! site answers with today may gain a field tomorrow, and a file somebody
//! wrote out last year still has to read. So the shapes here move on their own
//! schedule, and `version` says which schedule.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Which shape this file is.
///
/// Read rather than assumed: a file from a later version is refused with a
/// sentence, because reading half of one and stopping is worse than not
/// starting.
pub const VERSION: u32 = 1;

/// A whole site.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub version: u32,
    #[serde(default)]
    pub languages: Vec<Language>,
    #[serde(default)]
    pub terms: Vec<Term>,
    #[serde(default)]
    pub writings: Vec<Writing>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Language {
    pub tag: String,
    pub name: String,
    pub is_the_sites_own: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Term {
    /// Kept so that what a writing is filed under can point at it. Its own id
    /// within this file — nothing outside the file means anything by it.
    pub id: Uuid,
    pub sort: String,
    pub language: String,
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub parent: Option<Uuid>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Writing {
    pub id: Uuid,
    pub kind: String,
    pub language: String,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub excerpt: Option<String>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub fields: serde_json::Value,
    pub state: String,
    #[serde(default)]
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
    /// What it is filed under, **by the ids in this file**.
    #[serde(default)]
    pub terms: Vec<Uuid>,
}

/// What reading one did.
///
/// Both halves, always. A number that only said what was added would let
/// somebody read a file into the wrong site, see "0 added", and conclude the
/// file was empty rather than that everything in it was already there.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Read {
    pub languages: u64,
    pub terms: u64,
    pub writings: u64,
    /// What was already answering at the same address, and was left alone.
    pub left_alone: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_says_which_shape_it_is() {
        let bundle = Bundle {
            version: VERSION,
            ..Bundle::default()
        };

        let written = serde_json::to_value(&bundle).expect("a file");

        assert_eq!(written["version"], VERSION);
    }

    #[test]
    fn a_file_with_something_in_it_nobody_recognises_is_refused() {
        // `deny_unknown_fields`, and it is deliberate in exactly one
        // direction: a file written by something newer has fields this build
        // would drop silently, and dropping somebody's content silently while
        // reporting success is the worst thing this could do.
        let wrong = serde_json::json!({ "version": 1, "surprises": [] });

        assert!(serde_json::from_value::<Bundle>(wrong).is_err());
    }

    #[test]
    fn what_reading_one_did_says_both_halves() {
        let read = Read {
            writings: 0,
            left_alone: 12,
            ..Read::default()
        };

        // Nothing added and twelve left alone is a file that was already here,
        // which is a different thing from an empty file — and a screen can
        // only say which if it is told both.
        assert_eq!(read.writings, 0);
        assert_eq!(read.left_alone, 12);
    }
}
