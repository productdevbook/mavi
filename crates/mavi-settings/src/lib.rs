//! What this site is.
//!
//! Its name, where it answers, what it writes in, and what it wants a visitor
//! to be shown before anything else has loaded. One installation is one site,
//! so this is one row — said in the schema, where a second row is refused
//! rather than merely never written.

pub mod language;
pub mod store;

use mavi_api::{Answers, Endpoint, Is, Method, Parameter, Who};
use mavi_core::error::{Code, Error, Result};
use mavi_core::say::Say;
use serde::{Deserialize, Serialize};

pub use language::{Language, Tag, crowning, may_forget};

pub const SETTINGS: &str = "settings";

pub const A_NAME_IS_BETWEEN_ONE_AND_TWO_HUNDRED: &str = "a_name_is_between_one_and_two_hundred";
pub const THAT_IS_NOT_A_TIME_ZONE: &str = "that_is_not_a_time_zone";

use mavi_core::grant::{Access, Needs};

#[must_use]
pub const fn to_read() -> Needs {
    Needs::new(SETTINGS, Access::View)
}

#[must_use]
pub const fn to_write() -> Needs {
    Needs::new(SETTINGS, Access::Write)
}

/// What this site is.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub name: String,
    /// What a page says about itself where it has nothing of its own to say.
    pub about: Option<String>,
    /// Which zone a site's own hours are in — when "tomorrow at nine" is, and
    /// what day a report covers. Stored rather than guessed from the machine:
    /// a machine is moved and a site is not.
    pub time_zone: String,
}

impl Settings {
    pub fn checked(name: &str, about: Option<&str>, time_zone: &str) -> Result<Self> {
        if !(1..=200).contains(&name.trim().chars().count()) {
            return Err(Error::invalid(Say::of(
                A_NAME_IS_BETWEEN_ONE_AND_TWO_HUNDRED,
            )));
        }

        // The shape of a zone name and not a list of them: which zones exist
        // is a list kept by somebody else that changes twice a year, and a
        // copy of it here is a copy that goes stale. What is refused is what
        // cannot be one at all.
        let zone_is_shaped_right = (1..=64).contains(&time_zone.len())
            && time_zone
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '+' | '-'))
            && !time_zone.starts_with('/')
            && !time_zone.ends_with('/')
            && !time_zone.contains("..");

        if !zone_is_shaped_right {
            return Err(Error::invalid(Say::of(THAT_IS_NOT_A_TIME_ZONE)));
        }

        Ok(Self {
            name: name.trim().to_owned(),
            about: about.map(|about| about.trim().to_owned()),
            time_zone: time_zone.to_owned(),
        })
    }
}

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/settings",
            named: "settings.read",
            about: "What this site is.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: None,
            answers: Answers::With("Settings"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Patch,
            path: "/api/settings",
            named: "settings.change",
            about: "Changes what this site is called, or says about itself.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: Some("SettingsChanges"),
            answers: Answers::With("Settings"),
            refuses: &[],
            changes: true,
        },
        Endpoint {
            method: Method::Get,
            path: "/api/languages",
            named: "languages.list",
            about: "What this site writes in, and which of them is its own.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: None,
            answers: Answers::With("LanguageList"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/languages",
            named: "languages.add",
            about: "Adds one.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: Some("NewLanguage"),
            answers: Answers::Made("Language"),
            refuses: &[Code::Conflict],
            changes: true,
        },
        Endpoint {
            method: Method::Put,
            path: "/api/languages/{tag}/own",
            named: "languages.make-own",
            about: "Makes one the site's own, and every other one not.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("tag", Is::Text, "Which language.")],
            takes: None,
            answers: Answers::With("LanguageList"),
            refuses: &[Code::NotFound],
            changes: true,
        },
        Endpoint {
            method: Method::Delete,
            path: "/api/languages/{tag}",
            named: "languages.forget",
            about: "Stops writing in one. Never the last, and never the site's own.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("tag", Is::Text, "Which language.")],
            takes: None,
            answers: Answers::Nothing,
            refuses: &[Code::NotFound],
            changes: true,
        },
        Endpoint {
            method: Method::Get,
            path: "/api/open/site",
            named: "open.site",
            about: "What a page needs to draw itself: the site's name and what it writes in.",
            who: Who::Anybody,
            parameters: Vec::new(),
            takes: None,
            // Not `Settings`: what a visitor is shown and what an editor sees
            // are different shapes, and the way they stop being different is
            // somebody adding a field to the one they were both reading.
            answers: Answers::With("PublicSite"),
            refuses: &[],
            changes: false,
        },
    ]
}

/// What anybody at all is told about this site.
#[derive(Clone, Debug, Serialize)]
pub struct PublicSite {
    pub name: String,
    pub about: Option<String>,
    pub languages: Vec<Language>,
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
        assert!(Api::of(endpoints()).clashes().is_empty());
    }

    #[test]
    fn what_anybody_can_reach_says_so_in_its_path() {
        for endpoint in endpoints() {
            assert_eq!(
                endpoint.who == Who::Anybody,
                endpoint.path.starts_with("/api/open/"),
                "{} is one thing in its path and another in its audience",
                endpoint.named
            );
        }
    }

    #[test]
    fn what_a_visitor_is_shown_is_its_own_shape() {
        // Two types, so that adding somewhere to reach the site's owner to the
        // one an editor reads does not put it on every page of the site.
        let shown = serde_json::to_value(PublicSite {
            name: "A Site".to_owned(),
            about: None,
            languages: Vec::new(),
        })
        .expect("json");

        let mut keys: Vec<&str> = shown
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();

        assert_eq!(keys, ["about", "languages", "name"]);
    }

    #[test]
    fn a_time_zone_is_a_zone_and_not_a_path() {
        assert!(Settings::checked("A Site", None, "Europe/Istanbul").is_ok());
        assert!(Settings::checked("A Site", None, "UTC").is_ok());

        for wrong in ["", "../etc/passwd", "/Europe/Istanbul", "Europe/../secrets"] {
            assert!(
                Settings::checked("A Site", None, wrong).is_err(),
                "{wrong:?} was taken for a time zone"
            );
        }
    }

    #[test]
    fn a_site_has_a_name() {
        assert!(Settings::checked("   ", None, "UTC").is_err());
    }

    #[test]
    fn what_this_domain_asks_for_is_a_capability_the_site_has() {
        assert!(mavi_people::is_a_capability(SETTINGS));
    }
}
