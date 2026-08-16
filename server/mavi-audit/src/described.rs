//! What has been done here, described.

use mavi_api::{Field, Is, Of, Shape};

#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "Receipt",
            "One thing somebody did. Written in the same transaction as the \
             change itself, so a change that answered and left no receipt is \
             not something that can have happened.",
            vec![
                Field::new("id", Of::One(Is::Id), "Which one."),
                Field::new(
                    "who",
                    Of::OneOf(&["an_account", "a_student", "the_machine"]),
                    "What sort of caller. `the_machine` is the site itself — a \
                     scheduled publish, a sweep, a letter going out — because \
                     \"nobody did this\" is an answer somebody will need one \
                     day.",
                ),
                Field::new("who_id", Of::One(Is::Text), "Which one of them.").or_null(),
                Field::new(
                    "did",
                    Of::One(Is::Text),
                    "The endpoint's own name — `writings.publish` — rather than \
                     a verb chosen at the call site. Two names for one action \
                     is two answers to \"what happened to this\".",
                ),
                Field::new(
                    "about",
                    Of::One(Is::Text),
                    "What sort of thing it was about.",
                ),
                Field::new("about_id", Of::One(Is::Text), "Which one.").or_null(),
                Field::new(
                    "what",
                    Of::Whatever,
                    "Whatever somebody reading this in a year needs in order to \
                     understand it **without** the row it describes — which may \
                     since have been deleted, and often has been.",
                ),
                Field::new("request", Of::One(Is::Text), "Which request it came in on."),
                Field::new("created_at", Of::One(Is::Moment), "When."),
            ],
        ),
        Shape::page_of("ReceiptPage", "Receipt", "What has been done here."),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ReceiptId, Who, Written};
    use std::collections::BTreeSet;

    #[test]
    fn what_is_described_is_what_is_sent() {
        let written = Written {
            id: ReceiptId(uuid::Uuid::nil()),
            who: Who::TheMachine,
            who_id: None,
            did: "writings.publish".to_owned(),
            about: "writing".to_owned(),
            about_id: None,
            what: serde_json::json!({}),
            request: "whatever".to_owned(),
            created_at: chrono::Utc::now(),
        };

        let sent = serde_json::to_value(&written).expect("a receipt");
        let sent: BTreeSet<&str> = sent
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();

        let described: BTreeSet<&str> = shapes()
            .iter()
            .find(|shape| shape.named == "Receipt")
            .expect("a shape")
            .fields()
            .iter()
            .map(|field| field.name)
            .collect();

        assert_eq!(sent, described);
    }
}
