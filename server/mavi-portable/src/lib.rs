//! A site, as a file.
//!
//! What a site wrote, written out and read back: its languages, what it files
//! things under, and everything it wrote. Enough to move a site somewhere
//! else, or to keep a copy that is not a database backup.
//!
//! **Deliberately not everything**, and what is missing is written down here
//! rather than left to be discovered:
//!
//! - **Uploaded files.** The rows come out; the bytes do not. A site read back
//!   somewhere else has writings that point at pictures which are not there.
//!   Putting megabytes of photographs inside a JSON file is a thing that works
//!   until somebody has a real site.
//! - **People, and how they get in.** An account is not a site's content, and
//!   a file that carried password hashes around would be a file nobody could
//!   safely email themselves.
//! - **What people sent, and what they bought.** Form submissions and orders
//!   are other people's, not the site's. A copy of a site should not be a copy
//!   of everybody who ever wrote to it.
//! - **How it looks.** A design is a project with its own history; it belongs
//!   in whatever holds projects.
//!
//! ## Reading one back never overwrites
//!
//! Anything already answering at the same address is left alone and counted.
//! That is the whole of the safety of this: reading a file into a site that
//! has things in it can only add, so there is no version of "I imported the
//! wrong file" that loses work.

pub mod bundle;
pub mod described;
pub mod store;

use mavi_api::{Answers, Endpoint, Method, Who};
use mavi_core::error::Code;
use mavi_core::grant::{Access, Needs};

pub use bundle::{Bundle, Read};

/// What holding `portable` is about: taking the whole site, or reading a whole
/// one in.
///
/// **Its own capability, not implied by being able to edit a post.** An export
/// is every writing a site has including its drafts, in one call, in a file
/// somebody can walk out with — and somebody trusted to fix a typo has not
/// thereby been trusted with that.
pub const PORTABLE: &str = "portable";

#[must_use]
pub const fn to_take() -> Needs {
    Needs::new(PORTABLE, Access::View)
}

#[must_use]
pub const fn to_read_one_in() -> Needs {
    Needs::new(PORTABLE, Access::Write)
}

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/portable",
            named: "portable.take",
            about: "The whole site as a file: its languages, what it files \
                    things under, and everything it wrote.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: None,
            answers: Answers::With("Bundle"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/portable",
            named: "portable.read-in",
            about: "Reads a file back in. Anything already answering at the \
                    same address is left alone, so this can only add.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: Some("Bundle"),
            answers: Answers::With("WhatWasRead"),
            refuses: &[Code::Invalid],
            changes: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use mavi_api::Api;

    #[test]
    fn everything_this_domain_answers_is_described_completely() {
        let holes = Api::of(endpoints()).holes();

        assert!(holes.is_empty(), "{holes:#?}");
    }

    #[test]
    fn taking_a_whole_site_is_not_something_editing_a_post_implies() {
        // The reason this has a capability of its own. An export is every
        // writing including the drafts, in one call, in a file somebody can
        // walk out with.
        assert_eq!(to_take().of, PORTABLE);
        assert_ne!(
            to_take().of,
            mavi_core::grant::Needs::new("content", Access::View).of
        );
    }
}
