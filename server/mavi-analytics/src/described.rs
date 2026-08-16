//! What counting looks like, described.

use mavi_api::{Field, Is, Of, Shape};

#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "SomethingRead",
            "A page was read. Sent by a reader's own browser — so what is here \
             is what a browser knows, and it is the whole of what is kept. \
             Nothing about where it arrived from is looked at, held, or \
             written down.",
            vec![
                Field::new(
                    "path",
                    Of::One(Is::Text),
                    "Which page, as the reader's browser has it. Cut at five \
                     hundred characters rather than refused.",
                ),
                Field::new(
                    "felt",
                    Of::OneOf(crate::WHAT_A_BROWSER_MEASURES),
                    "What was measured, where anything was. The web's own \
                     names: when the biggest thing appeared, how long the page \
                     took to answer a tap, how much moved, how long the server \
                     took.",
                )
                .maybe()
                .or_null(),
                Field::new(
                    "value",
                    Of::One(Is::Number),
                    "Milliseconds, or a hundredth where the measurement is a \
                     ratio. Only read where `felt` says what it is.",
                )
                .maybe()
                .or_null(),
            ],
        ),
        Shape::new(
            "Read",
            "One day of one page.",
            vec![
                Field::new("on_day", Of::One(Is::Text), "Which day."),
                Field::new("path", Of::One(Is::Text), "Which page."),
                Field::new(
                    "views",
                    Of::One(Is::Number),
                    "How many times it was read. Times, not people — telling \
                     those apart means knowing where a request came from, and \
                     this software is not told that.",
                ),
            ],
        ),
        Shape::list_of(
            "ReadList",
            "Read",
            "Every day of every page that was read, newest and busiest first.",
        ),
        Shape::new(
            "Felt",
            "How one page felt, by one measurement.",
            vec![
                Field::new(
                    "kind",
                    Of::OneOf(crate::WHAT_A_BROWSER_MEASURES),
                    "Which measurement.",
                ),
                Field::new("path", Of::One(Is::Text), "Which page."),
                Field::new(
                    "middle",
                    Of::One(Is::Number),
                    "The middle one — what the site is usually like.",
                ),
                Field::new(
                    "bad_end",
                    Of::One(Is::Number),
                    "What a twentieth of readers had worse than. The thing an \
                     average hides, and the reason there is no average here.",
                ),
                Field::new("how_many", Of::One(Is::Number), "How many were measured."),
            ],
        ),
        Shape::list_of("FeltList", "Felt", "How the site felt, by page."),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Felt, Read};
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
        let read = Read {
            on_day: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).expect("a day"),
            path: "/".to_owned(),
            views: 1,
        };

        assert_eq!(
            keys(&serde_json::to_value(&read).expect("a day")),
            fields_of("Read")
        );

        let felt = Felt {
            kind: "lcp".to_owned(),
            path: "/".to_owned(),
            middle: 1,
            bad_end: 2,
            how_many: 3,
        };

        assert_eq!(
            keys(&serde_json::to_value(&felt).expect("how it felt")),
            fields_of("Felt")
        );
    }

    #[test]
    fn nothing_here_takes_or_answers_anything_about_a_person() {
        // The claim the whole crate rests on, held against what it actually
        // says. Both directions: a field named for a person or a machine, in
        // something a reader sends **or** in something a screen is answered
        // with, is one this must not have grown.
        for shape in shapes() {
            for field in shape.fields() {
                assert!(
                    ![
                        "address", "ip", "who", "reader", "visitor", "visitors", "agent", "mark",
                        "salt", "session", "person"
                    ]
                    .contains(&field.name),
                    "{} has a {}",
                    shape.named,
                    field.name
                );
            }
        }
    }
}
