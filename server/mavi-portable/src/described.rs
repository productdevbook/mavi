//! What a site as a file looks like, described.

use mavi_api::{Field, Is, Of, Shape};

#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![
        a_bundle(),
        Shape::new(
            "BundledLanguage",
            "One language, in a file. Read back in it is never made the site's \
             own — which language a site writes in is a decision it has already \
             made, and a file does not get to change that from underneath \
             whoever made it.",
            vec![
                Field::new("tag", Of::One(Is::Text), "`en`, `tr`, `pt-BR`."),
                Field::new("name", Of::One(Is::Text), "What it is called, in itself."),
                Field::new(
                    "is_the_sites_own",
                    Of::One(Is::Bool),
                    "What it was in the site this came from. Written out and \
                     not acted on.",
                ),
            ],
        ),
        a_term(),
        a_writing(),
        Shape::new(
            "WhatWasRead",
            "What reading a file in did. Both halves, always — a number that \
             only said what was added would let somebody read a file into the \
             wrong site, see nothing added, and conclude the file was empty \
             rather than that everything in it was already there.",
            vec![
                Field::new("languages", Of::One(Is::Number), "How many were added."),
                Field::new("terms", Of::One(Is::Number), "How many were added."),
                Field::new("writings", Of::One(Is::Number), "How many were added."),
                Field::new(
                    "left_alone",
                    Of::One(Is::Number),
                    "How many were already answering at the same address. \
                     Nothing is ever overwritten, so reading a file in can only \
                     add.",
                ),
            ],
        ),
    ]
}

fn a_bundle() -> Shape {
    Shape::new(
        "Bundle",
        "A whole site as a file. Its own shapes rather than the ones the API \
         answers with elsewhere, on purpose: what a listing answers may gain a \
         field tomorrow, and a file somebody wrote out last year still has to \
         read. Uploaded files, accounts, what people sent, what they bought, \
         and how the site looks are all deliberately not in it.",
        vec![
            Field::new(
                "version",
                Of::One(Is::Number),
                "Which shape this file is. One from a later version is refused \
                 rather than half read.",
            ),
            Field::new(
                "languages",
                Of::ManyOf("BundledLanguage"),
                "What it writes in.",
            )
            .maybe(),
            Field::new(
                "terms",
                Of::ManyOf("BundledTerm"),
                "What it files things under.",
            )
            .maybe(),
            Field::new(
                "writings",
                Of::ManyOf("BundledWriting"),
                "Everything it wrote.",
            )
            .maybe(),
        ],
    )
}

fn a_term() -> Shape {
    Shape::new(
        "BundledTerm",
        "One term, in a file.",
        vec![
            Field::new(
                "id",
                Of::One(Is::Id),
                "Its own id **within this file** — what a writing here points \
                 at. Nothing outside the file means anything by it, and reading \
                 it in gives it a new one.",
            ),
            Field::new("sort", Of::One(Is::Text), "A category or a tag."),
            Field::new("language", Of::One(Is::Text), "Which language."),
            Field::new("slug", Of::One(Is::Text), "Where it answers."),
            Field::new("name", Of::One(Is::Text), "What it is called."),
            Field::new(
                "parent",
                Of::One(Is::Id),
                "Which category it is under, by the ids in this file.",
            )
            .maybe()
            .or_null(),
        ],
    )
}

fn a_writing() -> Shape {
    Shape::new(
        "BundledWriting",
        "One writing, in a file.",
        vec![
            Field::new("id", Of::One(Is::Id), "Its own id within this file."),
            Field::new("kind", Of::One(Is::Text), "What the site decided it is."),
            Field::new("language", Of::One(Is::Text), "Which language."),
            Field::new("slug", Of::One(Is::Text), "Where it answers."),
            Field::new("title", Of::One(Is::Text), "What it is called."),
            Field::new("excerpt", Of::One(Is::Text), "A line about it.")
                .maybe()
                .or_null(),
            Field::new("body", Of::One(Is::Text), "What it says.").maybe(),
            Field::new("fields", Of::Whatever, "Whatever the site kept beside it.").maybe(),
            Field::new("state", Of::One(Is::Text), "Whether it was out."),
            Field::new("published_at", Of::One(Is::Moment), "When it went out.")
                .maybe()
                .or_null(),
            Field::new(
                "terms",
                Of::Many(Is::Id),
                "What it is filed under, by the ids **in this file**.",
            )
            .maybe(),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::{Bundle, Language, Read, Term, VERSION, Writing};
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
    fn what_is_described_is_what_is_in_the_file() {
        let bundle = Bundle {
            version: VERSION,
            ..Bundle::default()
        };

        assert_eq!(
            keys(&serde_json::to_value(&bundle).expect("a file")),
            fields_of("Bundle")
        );

        let language = Language {
            tag: "en".to_owned(),
            name: "English".to_owned(),
            is_the_sites_own: true,
        };

        assert_eq!(
            keys(&serde_json::to_value(&language).expect("a language")),
            fields_of("BundledLanguage")
        );

        let term = Term {
            id: uuid::Uuid::nil(),
            sort: "tag".to_owned(),
            language: "en".to_owned(),
            slug: "news".to_owned(),
            name: "News".to_owned(),
            parent: None,
        };

        assert_eq!(
            keys(&serde_json::to_value(&term).expect("a term")),
            fields_of("BundledTerm")
        );

        let writing = Writing {
            id: uuid::Uuid::nil(),
            kind: "post".to_owned(),
            language: "en".to_owned(),
            slug: "hello".to_owned(),
            title: "A Title".to_owned(),
            excerpt: None,
            body: String::new(),
            fields: serde_json::Value::Null,
            state: "draft".to_owned(),
            published_at: None,
            terms: Vec::new(),
        };

        assert_eq!(
            keys(&serde_json::to_value(&writing).expect("a writing")),
            fields_of("BundledWriting")
        );

        assert_eq!(
            keys(&serde_json::to_value(Read::default()).expect("what it did")),
            fields_of("WhatWasRead")
        );
    }

    #[test]
    fn nothing_about_a_person_is_in_the_file() {
        // What this deliberately does not carry, held to. A file somebody
        // emails themselves must not be a copy of everybody who ever wrote to
        // the site, and must not be one nobody could safely email.
        for shape in shapes() {
            for field in shape.fields() {
                assert!(
                    !["email", "password", "answers", "who", "said_by", "address"]
                        .contains(&field.name),
                    "{} carries {}",
                    shape.named,
                    field.name
                );
            }
        }
    }
}
