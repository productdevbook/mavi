//! What somebody is allowed to do.
//!
//! A grant is a string: `content:write`, `shop:view`, `content:write:own`.
//! **This crate does not know what `content` is**, and that is deliberate.
//!
//! In the crate this replaces, the kernel held an enum with one variant per
//! thing a site does, and a policy file naming the same list again in a second
//! language. It compiled — a string is a string — so nothing would ever have
//! failed on it. It simply meant the foundation had to be edited every time a
//! site learned to do something new, which is the thing a foundation exists
//! not to do.
//!
//! Worse, those names are **in the database**: every role of every
//! installation stores them. So moving them is a data migration wearing a
//! refactor's clothes, and the moment to not have that problem is now.
//!
//! So: the shape is here, the list is wherever the endpoints are declared.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::say::Say;

pub const NOBODY_GAVE_YOU_THAT: &str = "nobody_gave_you_that";

/// How much of a thing somebody may do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    View,
    Write,
    Delete,
}

impl Access {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Access::View => "view",
            Access::Write => "write",
            Access::Delete => "delete",
        }
    }
}

/// What is being asked for: a thing, and how much of it.
///
/// The thing is a string this crate never interprets. A domain names its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Needs {
    pub of: &'static str,
    pub access: Access,
}

impl Needs {
    #[must_use]
    pub const fn new(of: &'static str, access: Access) -> Self {
        Self { of, access }
    }

    /// `content:write`.
    #[must_use]
    pub fn whole(&self) -> String {
        format!("{}:{}", self.of, self.access.as_str())
    }

    /// `content:write:own` — the same, over what somebody made themselves.
    #[must_use]
    pub fn own(&self) -> String {
        format!("{}:{}:own", self.of, self.access.as_str())
    }
}

/// What somebody holds.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Grants(BTreeSet<String>);

impl Grants {
    #[must_use]
    pub fn of(grants: impl IntoIterator<Item = String>) -> Self {
        Self(grants.into_iter().collect())
    }

    #[must_use]
    pub fn holds(&self, grant: &str) -> bool {
        self.0.contains(grant)
    }

    #[must_use]
    pub fn all(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }
}

/// Whether somebody may do this, to this.
///
/// `owner` is who made the thing being reached, where that is known — `None`
/// for something nobody owns, or for a question asked before a row is read.
///
/// The two-step is the whole of it: a whole grant answers for anything, and an
/// `:own` grant answers only where the asker is the owner. Asking with no
/// owner and only an `:own` grant is refused, which is the case that matters —
/// it is what stops a narrow grant opening a listing of everybody's.
pub fn may(grants: &Grants, needs: Needs, asker: Option<&str>, owner: Option<&str>) -> Result<()> {
    if grants.holds(&needs.whole()) {
        return Ok(());
    }

    let their_own = matches!((asker, owner), (Some(asker), Some(owner)) if asker == owner);

    if their_own && grants.holds(&needs.own()) {
        return Ok(());
    }

    Err(Error::forbidden(
        Say::of(NOBODY_GAVE_YOU_THAT)
            .with("of", &needs.of)
            .with("access", &needs.access.as_str()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holding(what: &[&str]) -> Grants {
        Grants::of(what.iter().map(ToString::to_string))
    }

    const WRITING: Needs = Needs::new("content", Access::Write);

    #[test]
    fn holding_nothing_reaches_nothing() {
        assert!(may(&holding(&[]), WRITING, Some("me"), Some("me")).is_err());
    }

    #[test]
    fn a_whole_grant_answers_for_anything() {
        let grants = holding(&["content:write"]);

        assert!(may(&grants, WRITING, Some("me"), Some("somebody else")).is_ok());
        assert!(may(&grants, WRITING, Some("me"), None).is_ok());
    }

    #[test]
    fn an_own_grant_answers_only_for_their_own() {
        let grants = holding(&["content:write:own"]);

        assert!(may(&grants, WRITING, Some("me"), Some("me")).is_ok());
        assert!(may(&grants, WRITING, Some("me"), Some("somebody else")).is_err());
    }

    #[test]
    fn an_own_grant_does_not_open_a_question_with_no_owner() {
        // The case that matters. A listing asks before it has rows, so there
        // is nobody to be the owner — and an `:own` grant answering that
        // question is an `:own` grant opening a listing of everybody's.
        let grants = holding(&["content:view:own"]);

        assert!(
            may(
                &grants,
                Needs::new("content", Access::View),
                Some("me"),
                None
            )
            .is_err(),
            "a narrow grant answered a question about nobody in particular"
        );
    }

    #[test]
    fn a_grant_for_one_thing_is_not_a_grant_for_another() {
        let grants = holding(&["shop:write", "content:view"]);

        assert!(may(&grants, WRITING, Some("me"), Some("me")).is_err());
        assert!(may(&grants, Needs::new("shop", Access::Write), Some("me"), None).is_ok());
    }

    #[test]
    fn a_refusal_says_what_was_wanted() {
        let refused = may(&holding(&[]), WRITING, Some("me"), None).expect_err("a refusal");
        let said = refused.said().expect("a sentence");

        assert_eq!(said.key, NOBODY_GAVE_YOU_THAT);
        assert_eq!(said.named.get("of"), Some(&"content".to_owned()));
        assert_eq!(said.named.get("access"), Some(&"write".to_owned()));
    }
}
