//! What is in the bin, described.

use mavi_api::{Field, Is, Of, Shape};

#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "Thrown",
            "One thing somebody threw away.",
            vec![
                Field::new(
                    "kind",
                    Of::OneOf(WHAT_SORTS),
                    "What sort of thing. The same word the address takes.",
                ),
                Field::new("id", Of::One(Is::Id), "Which one."),
                Field::new(
                    "called",
                    Of::One(Is::Text),
                    "Enough to know which one it is. A bin where nine rows say \
                     the same thing is one nobody can restore from.",
                ),
                Field::new("thrown_away_at", Of::One(Is::Moment), "When it went in."),
            ],
        ),
        Shape::list_of(
            "ThrownList",
            "Thrown",
            "What a site threw away, newest first, across every sort at once.",
        ),
    ]
}

/// The sorts, said to a client as the closed list they are.
const WHAT_SORTS: &[&str] = &[
    "writings", "files", "terms", "forms", "products", "courses", "boards", "cards", "flows",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::EVERY;
    use crate::store::Thrown;
    use std::collections::BTreeSet;

    #[test]
    fn what_is_described_is_what_is_sent() {
        let thrown = Thrown {
            kind: "writings",
            id: uuid::Uuid::nil(),
            called: "A Title".to_owned(),
            thrown_away_at: chrono::Utc::now(),
        };

        let sent = serde_json::to_value(&thrown).expect("something thrown away");
        let sent: BTreeSet<&str> = sent
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();

        let described: BTreeSet<&str> = shapes()
            .iter()
            .find(|shape| shape.named == "Thrown")
            .expect("a shape")
            .fields()
            .iter()
            .map(|field| field.name)
            .collect();

        assert_eq!(sent, described);
    }

    #[test]
    fn the_sorts_a_client_is_told_about_are_the_sorts_there_are() {
        // Written out for the description and compared against the code, so a
        // tenth sort added to one and not the other is a client offering a
        // button that refuses.
        let described: Vec<&str> = WHAT_SORTS.to_vec();
        let there: Vec<&str> = EVERY.iter().map(|kind| kind.as_str()).collect();

        assert_eq!(described, there);
    }
}
