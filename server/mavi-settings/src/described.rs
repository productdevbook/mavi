//! What a site says it is, described.

use mavi_api::{Field, Is, Of, Shape};

const A_TAG: &str = "A language tag: `en`, `tr`, `pt-BR`. The shape is checked \
                     rather than the value looked up in a list — which \
                     languages exist is somebody else's list and a copy of it \
                     here goes stale.";

#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "Settings",
            "What a site says it is.",
            vec![
                Field::new("name", Of::One(Is::Text), "What the site is called."),
                Field::new(
                    "about",
                    Of::One(Is::Text),
                    "What a page says about itself where it has nothing of its \
                     own to say.",
                )
                .or_null(),
                Field::new(
                    "time_zone",
                    Of::One(Is::Text),
                    "Which zone a site's own hours are in — when \"tomorrow at \
                     nine\" is, and what day a report covers. Kept rather than \
                     guessed from the machine: a machine is moved and a site \
                     is not.",
                ),
            ],
        ),
        Shape::new(
            "SettingsChanges",
            "What may be changed. Whatever is sent is held against the whole of \
             what the settings would become, so a site cannot end up with a \
             name and no time zone.",
            vec![
                Field::new("name", Of::One(Is::Text), "What the site is called.").maybe(),
                Field::new("about", Of::One(Is::Text), "What a page says about itself.").maybe(),
                Field::new(
                    "time_zone",
                    Of::One(Is::Text),
                    "Which zone a site's own hours are in.",
                )
                .maybe(),
            ],
        ),
        Shape::new(
            "Language",
            "One language a site writes in.",
            vec![
                Field::new("tag", Of::One(Is::Text), A_TAG),
                Field::new(
                    "name",
                    Of::One(Is::Text),
                    "What it is called, in itself: `Türkçe` rather than \
                     `Turkish`. Whoever is choosing it reads that one.",
                ),
                Field::new(
                    "is_the_sites_own",
                    Of::One(Is::Bool),
                    "Whether this is the site's own. Exactly one is.",
                ),
            ],
        ),
        Shape::list_of(
            "LanguageList",
            "Language",
            "Every language a site writes in. A handful, with nothing to page \
             through.",
        ),
        Shape::new(
            "NewLanguage",
            "One to start writing in.",
            vec![
                Field::new("tag", Of::One(Is::Text), A_TAG),
                Field::new("name", Of::One(Is::Text), "What it is called, in itself."),
            ],
        ),
        Shape::new(
            "PublicSite",
            "What anybody at all is told about this site. Its own shape rather \
             than the settings an editor reads, so that adding somewhere to \
             reach a site's owner does not put it on every page of the site.",
            vec![
                Field::new("name", Of::One(Is::Text), "What the site is called."),
                Field::new("about", Of::One(Is::Text), "What it says about itself.").or_null(),
                Field::new(
                    "languages",
                    Of::ManyOf("Language"),
                    "What it is written in.",
                ),
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::Language;
    use crate::store::SettingsChanges;
    use crate::{PublicSite, Settings};
    use std::collections::BTreeSet;

    fn fields_of(named: &str) -> BTreeSet<&'static str> {
        shapes()
            .iter()
            .find(|shape| shape.named == named)
            .expect("a shape")
            .fields()
            .iter()
            .map(|field| field.name)
            .collect()
    }

    fn keys(what: &serde_json::Value) -> BTreeSet<&str> {
        what.as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect()
    }

    #[test]
    fn what_is_described_is_what_is_sent() {
        let settings = Settings::checked("A Site", None, "Europe/Istanbul").expect("settings");
        assert_eq!(
            keys(&serde_json::to_value(&settings).expect("settings")),
            fields_of("Settings")
        );

        let language = Language::checked("en", "English", true).expect("a language");
        assert_eq!(
            keys(&serde_json::to_value(&language).expect("a language")),
            fields_of("Language")
        );

        // The two that must not become one. What a visitor is shown and what
        // an editor sees are different shapes, and the way they stop being
        // different is somebody adding a field to the one they both read.
        let shown = PublicSite {
            name: "A Site".to_owned(),
            about: None,
            languages: vec![language],
        };

        assert_eq!(
            keys(&serde_json::to_value(&shown).expect("a site")),
            fields_of("PublicSite")
        );
    }

    #[test]
    fn what_is_described_is_what_is_taken() {
        let changes = serde_json::to_value(SettingsChanges {
            name: None,
            about: None,
            time_zone: None,
        })
        .expect("changes");

        assert_eq!(keys(&changes), fields_of("SettingsChanges"));
    }
}
