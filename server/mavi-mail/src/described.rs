//! What a site writes to people.

use mavi_api::{Field, Is, Of, Shape};

#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "Letter",
            "One of a site's own letters, in one language.",
            vec![
                Field::new("kind", Of::One(Is::Text), "Which letter this is."),
                Field::new("language", Of::One(Is::Text), "Which language it is in."),
                Field::new(
                    "subject",
                    Of::One(Is::Text),
                    "What the line at the top says.",
                ),
                Field::new("body", Of::One(Is::Text), "What it says."),
                Field::new(
                    "theirs",
                    Of::One(Is::Bool),
                    "Whether a site wrote this. False means it is what this \
                     software says, having been told nothing — which is why \
                     every kind is listed and not only the ones somebody has \
                     edited.",
                ),
                Field::new(
                    "names",
                    Of::Many(Is::Text),
                    "What this letter may name. Answered rather than written \
                     into a screen: a panel that has to know the list is a \
                     panel that goes out of date.",
                ),
            ],
        ),
        Shape::list_of(
            "LetterList",
            "Letter",
            "Every letter a site sends, in one language. Every kind, always.",
        ),
        Shape::new(
            "Wording",
            "What one of a site's letters should say. A name it has no value \
             for is refused rather than left in: an obvious hole says a great \
             deal to whoever wrote the letter and nothing at all to whoever \
             receives one.",
            vec![
                Field::new("language", Of::One(Is::Text), "Which language.").maybe(),
                Field::new("subject", Of::One(Is::Text), "The line at the top."),
                Field::new("body", Of::One(Is::Text), "What it says."),
            ],
        ),
        Shape::new(
            "Values",
            "What to put in a letter's names, to see what it would look like.",
            vec![Field::new(
                "values",
                Of::Whatever,
                "One value per name the letter uses.",
            )],
        ),
        Shape::new(
            "Pressed",
            "The letter with the values in it, sent to nobody.",
            vec![
                Field::new("subject", Of::One(Is::Text), "The line at the top."),
                Field::new("body", Of::One(Is::Text), "What it says."),
            ],
        ),
        Shape::new(
            "List",
            "One of a site's mailing lists.",
            vec![
                Field::new("id", Of::One(Is::Id), "Which one."),
                Field::new("name", Of::One(Is::Text), "What it is called."),
                Field::new(
                    "reading",
                    Of::One(Is::Number),
                    "How many are on it **and may still be written to**. Not \
                     how many rows there are: a list of nine hundred nobody may \
                     write to is a number that tells whoever reads it the wrong \
                     thing.",
                ),
                Field::new("created_at", Of::One(Is::Moment), "When it was made."),
            ],
        ),
        Shape::list_of("ListList", "List", "Every list, and how many are on each."),
        Shape::new(
            "NewList",
            "One to make.",
            vec![Field::new("name", Of::One(Is::Text), "What to call it.")],
        ),
        Shape::new(
            "Reader",
            "Somebody on a list.",
            vec![
                Field::new("id", Of::One(Is::Id), "Which one."),
                Field::new("email", Of::One(Is::Text), "Where they are reached."),
                Field::new("name", Of::One(Is::Text), "What they are called.").or_null(),
                Field::new(
                    "standing",
                    Of::One(Is::Text),
                    "Whether they may still be written to.",
                ),
                Field::new("created_at", Of::One(Is::Moment), "When they were added."),
            ],
        ),
        Shape::page_of("ReaderPage", "Reader", "Who is on one list."),
        Shape::new(
            "NewReader",
            "Somebody to put on a list.",
            vec![
                Field::new("email", Of::One(Is::Text), "Where to reach them."),
                Field::new("name", Of::One(Is::Text), "What to call them.")
                    .maybe()
                    .or_null(),
            ],
        ),
        Shape::new(
            "Sending",
            "Something to send to everybody on a list who may still be written \
             to. How many went is what comes back.",
            vec![
                Field::new("subject", Of::One(Is::Text), "The line at the top."),
                Field::new("body", Of::One(Is::Text), "What it says."),
                Field::new("letters", Of::One(Is::Number), "How many went.").maybe(),
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::letter::Pressed;
    use crate::store::{Letter, List, NewSending, Reader};
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
        let letter = Letter {
            kind: "somebody_was_invited".to_owned(),
            language: "en".to_owned(),
            subject: "You have been invited".to_owned(),
            body: "Follow the link: {link}".to_owned(),
            theirs: false,
            names: vec!["link".to_owned()],
        };

        assert_eq!(
            keys(&serde_json::to_value(&letter).expect("a letter")),
            fields_of("Letter")
        );

        let list = List {
            id: uuid::Uuid::nil(),
            name: "A List".to_owned(),
            reading: 0,
            created_at: chrono::Utc::now(),
        };

        assert_eq!(
            keys(&serde_json::to_value(&list).expect("a list")),
            fields_of("List")
        );

        let reader = Reader {
            id: uuid::Uuid::nil(),
            email: "somebody@example.test".to_owned(),
            name: None,
            standing: "subscribed".to_owned(),
            created_at: chrono::Utc::now(),
        };

        assert_eq!(
            keys(&serde_json::to_value(&reader).expect("a reader")),
            fields_of("Reader")
        );

        let pressed = Pressed {
            subject: "You have been invited".to_owned(),
            body: "Follow the link: here".to_owned(),
        };

        assert_eq!(
            keys(&serde_json::to_value(&pressed).expect("a pressed letter")),
            fields_of("Pressed")
        );
    }

    #[test]
    fn what_is_described_is_what_is_taken() {
        // What is sent takes two of `Sending`'s three fields — the third is
        // what comes back, and saying so is what stops a caller sending it.
        let sending = serde_json::to_value(NewSending {
            subject: "Something".to_owned(),
            body: "Something else.".to_owned(),
        })
        .expect("something to send");

        let taken = keys(&sending);
        let described = fields_of("Sending");

        assert!(taken.is_subset(&described));
        assert_eq!(
            described.difference(&taken).copied().collect::<Vec<_>>(),
            vec!["letters"]
        );
    }
}
