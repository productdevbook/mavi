//! What a site writes in.
//!
//! One of them is the site's own, and there is always exactly one. Both halves
//! of that are rules: two defaults is a site that cannot say what a visitor
//! with no preference gets, and none is the same thing said differently.

use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use serde::{Deserialize, Serialize};

pub const THAT_IS_NOT_A_LANGUAGE_TAG: &str = "that_is_not_a_language_tag";
pub const A_NAME_IS_BETWEEN_ONE_AND_A_HUNDRED: &str = "a_name_is_between_one_and_a_hundred";
pub const A_SITE_WRITES_IN_SOMETHING: &str = "a_site_writes_in_something";
pub const THE_SITES_OWN_LANGUAGE_IS_PASSED_ON_RATHER_THAN_DROPPED: &str =
    "the_sites_own_language_is_passed_on_rather_than_dropped";
pub const THIS_SITE_DOES_NOT_WRITE_IN_THAT: &str = "this_site_does_not_write_in_that";

/// A language tag: `en`, `tr`, `pt-BR`, `sr-Latn-RS`.
///
/// Checked for shape and not for existence. Which tags are real is a list that
/// changes, kept by somebody else, and a site writing in a language this
/// machine has not heard of is the site's business rather than an error.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Tag(String);

impl Tag {
    pub fn parse(text: &str) -> Result<Self> {
        let mut parts = text.split('-');

        let language = parts.next().unwrap_or_default();
        let right = (2..=3).contains(&language.len())
            && language.chars().all(|c| c.is_ascii_lowercase())
            && parts.all(|part| {
                (2..=8).contains(&part.len()) && part.chars().all(|c| c.is_ascii_alphanumeric())
            });

        if !right {
            return Err(Error::invalid(Say::of(THAT_IS_NOT_A_LANGUAGE_TAG)));
        }

        Ok(Self(text.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Tag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One language a site writes in.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Language {
    pub tag: Tag,
    /// What it is called, in itself: `Türkçe` rather than `Turkish`. Whoever
    /// is choosing it reads that one.
    pub name: String,
    /// Whether this is the site's own. Exactly one is.
    pub is_the_sites_own: bool,
}

impl Language {
    pub fn checked(tag: &str, name: &str, is_the_sites_own: bool) -> Result<Self> {
        let tag = Tag::parse(tag)?;

        if !(1..=100).contains(&name.trim().chars().count()) {
            return Err(Error::invalid(Say::of(A_NAME_IS_BETWEEN_ONE_AND_A_HUNDRED)));
        }

        Ok(Self {
            tag,
            name: name.trim().to_owned(),
            is_the_sites_own,
        })
    }
}

/// Whether this language may be taken away.
///
/// Two refusals, and they are the same shape as the one that let the last
/// owner of a site be deleted: a rule about *the rest of the rows*, which no
/// constraint on a single row can see, and which reads as obviously fine at
/// every individual call site.
///
/// The site's own language is passed on rather than dropped — somebody has to
/// choose which is the new one, because this machine choosing produces a site
/// whose default is whatever sorted first.
pub fn may_forget(languages: &[Language], tag: &Tag) -> Result<()> {
    let Some(going) = languages.iter().find(|one| &one.tag == tag) else {
        return Err(Error::not_found(
            Say::of(THIS_SITE_DOES_NOT_WRITE_IN_THAT).with("language", &tag.as_str()),
        ));
    };

    if languages.len() == 1 {
        return Err(Error::invalid(Say::of(A_SITE_WRITES_IN_SOMETHING)));
    }

    if going.is_the_sites_own {
        return Err(Error::invalid(Say::of(
            THE_SITES_OWN_LANGUAGE_IS_PASSED_ON_RATHER_THAN_DROPPED,
        )));
    }

    Ok(())
}

/// What the site writes in after one language is made its own.
///
/// Every other one stops being it, in the same breath. Two writes — one to
/// take the crown off and one to put it on — is a moment with two defaults or
/// none, and the moment is exactly when something else reads the list.
#[must_use]
pub fn crowning(languages: &[Language], tag: &Tag) -> Vec<Language> {
    languages
        .iter()
        .map(|one| Language {
            is_the_sites_own: &one.tag == tag,
            ..one.clone()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn writing_in(tags: &[(&str, bool)]) -> Vec<Language> {
        tags.iter()
            .map(|(tag, own)| Language::checked(tag, "A Language", *own).expect("a language"))
            .collect()
    }

    fn refused(languages: &[Language], tag: &str) -> &'static str {
        may_forget(languages, &Tag::parse(tag).expect("a tag"))
            .expect_err("a refusal")
            .said()
            .expect("a sentence")
            .key
    }

    #[test]
    fn a_language_tag_is_a_tag() {
        for right in ["en", "tr", "pt-BR", "sr-Latn-RS", "zh-Hant-TW"] {
            assert!(Tag::parse(right).is_ok(), "{right} was refused");
        }

        for wrong in ["", "e", "EN", "english-language", "en_GB", "en-"] {
            assert!(Tag::parse(wrong).is_err(), "{wrong:?} was taken");
        }
    }

    #[test]
    fn a_site_writes_in_something() {
        // The same shape as the bug that let the last owner of a site be
        // deleted: nothing about the single row is wrong, and the site is
        // left unable to answer anybody.
        let only = writing_in(&[("en", true)]);

        assert_eq!(refused(&only, "en"), A_SITE_WRITES_IN_SOMETHING);
    }

    #[test]
    fn the_sites_own_language_is_passed_on_rather_than_dropped() {
        let both = writing_in(&[("en", true), ("tr", false)]);

        assert_eq!(
            refused(&both, "en"),
            THE_SITES_OWN_LANGUAGE_IS_PASSED_ON_RATHER_THAN_DROPPED
        );

        // And the other one goes without any trouble.
        assert!(may_forget(&both, &Tag::parse("tr").expect("a tag")).is_ok());
    }

    #[test]
    fn crowning_one_uncrowns_the_rest_in_the_same_breath() {
        // Written as one list rather than as two writes. Two writes is a
        // moment with two defaults or none, and that moment is exactly when
        // something else reads the list.
        let before = writing_in(&[("en", true), ("tr", false), ("de", false)]);

        let after = crowning(&before, &Tag::parse("tr").expect("a tag"));

        let own: Vec<&str> = after
            .iter()
            .filter(|one| one.is_the_sites_own)
            .map(|one| one.tag.as_str())
            .collect();

        assert_eq!(own, ["tr"]);
    }

    #[test]
    fn forgetting_something_the_site_does_not_write_in_says_so() {
        let both = writing_in(&[("en", true), ("tr", false)]);

        assert_eq!(refused(&both, "de"), THIS_SITE_DOES_NOT_WRITE_IN_THAT);
    }
}
