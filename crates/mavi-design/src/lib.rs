//! How a site looks, and how it goes live.
//!
//! A site's look is a project of its own: pages, styles, pictures. Changing it
//! is not like changing a post, and the difference is the whole shape of this
//! crate.
//!
//! **Nothing written here reaches the live site.** It goes onto a change that
//! somebody looks at, and a person publishes it. That is not caution for its
//! own sake: a broken layout is every page of the site at once, and whoever
//! notices is a visitor.
//!
//! **Building is somebody else's machine and somebody else's minute.** So it
//! is work in the queue, and what a caller gets back is that it has been
//! asked for — never a page waiting on a build.

pub mod where_it_goes;

use mavi_api::{Answers, Endpoint, Is, Method, Parameter, Who};
use mavi_core::error::Code;
use mavi_core::grant::{Access, Needs};
use mavi_core::id;
use mavi_core::page::{Key, Keyset, Kind};
use mavi_work::Kind as Work;
use serde::{Deserialize, Serialize};

pub use where_it_goes::to_write;

id!(
    /// One set of changes to how a site looks.
    ChangeId
);

id!(
    /// One build.
    BuildId
);

pub const DESIGN: &str = "design";
pub const PUBLISH: &str = "publish";

#[must_use]
pub const fn to_read_design() -> Needs {
    Needs::new(DESIGN, Access::View)
}

#[must_use]
pub const fn to_write_design() -> Needs {
    Needs::new(DESIGN, Access::Write)
}

/// What publishing needs. A separate capability from writing a design, because
/// they are separate jobs: somebody may be trusted to lay out a page and not
/// to put it in front of everybody.
#[must_use]
pub const fn to_publish() -> Needs {
    Needs::new(PUBLISH, Access::Write)
}

/// Building a change, so somebody can look at it. Not worth trying for ever:
/// a build that fails fails the same way each time, and what somebody needs is
/// the error rather than another go.
pub const BUILD_A_LOOK: Work = Work::new("design.build", 2);

/// Building what is published and putting it where visitors reach it.
pub const PUT_IT_LIVE: Work = Work::new("design.publish", 3);

/// Where a set of changes has got to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Where {
    /// Being written. Not built, not looked at.
    Writing,
    /// Built, and somewhere a person can look at it.
    ToLookAt,
    /// It did not build. What went wrong is kept, because "it failed" is not
    /// something anybody can act on.
    Broken,
    /// Live.
    Published,
}

impl Where {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Where::Writing => "writing",
            Where::ToLookAt => "to_look_at",
            Where::Broken => "broken",
            Where::Published => "published",
        }
    }

    /// Whether this may go live.
    ///
    /// Only something that has been built and looked at. Publishing straight
    /// from what somebody typed is how a site goes down at the moment its
    /// author closes the laptop.
    #[must_use]
    pub const fn may_be_published(self) -> bool {
        matches!(self, Where::ToLookAt)
    }
}

pub const BY_RECENT: Keyset = Keyset(&[
    Key::newest("created_at", Kind::Moment),
    Key::newest("id", Kind::Id),
]);

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    let mut all = the_files();
    all.extend(the_changes());
    all
}

/// What is in the project.
fn the_files() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/design/files",
            named: "design.files",
            about: "Everything in the site's own project that the panel may change.",
            who: Who::AnAccount,
            parameters: vec![Parameter::query(
                "change",
                Is::Id,
                "As it stands in this set of changes. As published, unsaid.",
            )],
            takes: None,
            answers: Answers::With("FileList"),
            refuses: &[Code::NotFound],
            changes: false,
        },
        Endpoint {
            method: Method::Get,
            path: "/api/design/files/{path}",
            named: "design.read",
            about: "One file, as it stands.",
            who: Who::AnAccount,
            parameters: vec![
                Parameter::path("path", Is::Text, "Which file, under `src/` or `public/`."),
                Parameter::query("change", Is::Id, "As it stands in this set of changes."),
            ],
            takes: None,
            answers: Answers::With("File"),
            refuses: &[Code::NotFound],
            changes: false,
        },
        Endpoint {
            method: Method::Put,
            path: "/api/design/files/{path}",
            named: "design.write",
            about: "Writes one file into a set of changes. Never into the live site.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path(
                "path",
                Is::Text,
                "Which file, under `src/` or `public/`.",
            )],
            takes: Some("Contents"),
            answers: Answers::With("File"),
            // A path that climbs out of the project, or a file that decides
            // how the site is built rather than how it looks.
            refuses: &[Code::NotFound],
            changes: true,
        },
    ]
}

/// Sets of changes, and what happens to them.
fn the_changes() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/design/changes",
            named: "changes.list",
            about: "Sets of changes to how the site looks, newest first.",
            who: Who::AnAccount,
            parameters: vec![
                Parameter::query("after", Is::Text, "The cursor the last page ended with."),
                Parameter::query("limit", Is::Number, "How many, at most a hundred."),
            ],
            takes: None,
            answers: Answers::With("ChangePage"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/design/changes",
            named: "changes.start",
            about: "Starts a set of changes from what is published now.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: Some("NewChange"),
            answers: Answers::Made("Change"),
            refuses: &[],
            changes: true,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/design/changes/{id}/builds",
            named: "changes.build",
            about: "Asks for it to be built, so somebody can look at it.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which set of changes.")],
            takes: None,
            // The build is somebody else's minute. What comes back is that it
            // has been asked for, never a page held open waiting for it.
            answers: Answers::Later,
            refuses: &[Code::NotFound, Code::Conflict],
            changes: true,
        },
        Endpoint {
            method: Method::Get,
            path: "/api/design/changes/{id}",
            named: "changes.read",
            about: "Where a set of changes has got to, and what went wrong if it did.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which set of changes.")],
            takes: None,
            answers: Answers::With("Change"),
            refuses: &[Code::NotFound],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/design/changes/{id}/published",
            named: "changes.publish",
            about: "Puts a built set of changes in front of everybody.",
            // A capability of its own. Laying out a page and putting it in
            // front of everybody are different jobs, and somebody may be
            // trusted with the first and not the second.
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which set of changes.")],
            takes: None,
            answers: Answers::Later,
            // Never published straight from what somebody typed: only
            // something that has been built and looked at.
            refuses: &[Code::NotFound, Code::Conflict],
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
    fn no_two_of_these_are_the_same_route() {
        let clashes = Api::of(endpoints()).clashes();

        assert!(clashes.is_empty(), "{clashes:#?}");
    }

    #[test]
    fn nothing_written_here_goes_straight_to_the_live_site() {
        // Every way of writing takes a set of changes, and the only endpoint
        // that mentions being published is the one a person presses. A broken
        // layout is every page of the site at once.
        let straight_to_live = endpoints()
            .into_iter()
            .filter(|e| e.changes && e.named != "changes.publish")
            .any(|e| e.path.contains("published"));

        assert!(!straight_to_live);
    }

    #[test]
    fn only_something_built_and_looked_at_goes_live() {
        assert!(Where::ToLookAt.may_be_published());

        for not_yet in [Where::Writing, Where::Broken, Where::Published] {
            assert!(
                !not_yet.may_be_published(),
                "{} went live",
                not_yet.as_str()
            );
        }
    }

    #[test]
    fn both_of_what_this_domain_asks_for_are_capabilities_the_site_has() {
        assert!(mavi_people::is_a_capability(DESIGN));
        assert!(mavi_people::is_a_capability(PUBLISH));
    }
}
