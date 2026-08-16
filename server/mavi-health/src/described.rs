//! What being well looks like, described.

use mavi_api::{Field, Is, Of, Shape};

#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "Alive",
            "Whether this installation can answer at all. Nothing else — what \
             asks this is a container runtime rather than a person, and a \
             detailed answer here would be a description of somebody's \
             installation handed to whoever asks.",
            vec![Field::new(
                "alive",
                Of::One(Is::Bool),
                "Always true. An installation that would answer otherwise does \
                 not answer.",
            )],
        ),
        Shape::new(
            "Check",
            "One thing that is either well or not.",
            vec![
                Field::new(
                    "what",
                    Of::One(Is::Text),
                    "Which check. A key rather than a sentence, so a panel \
                     words it in somebody's own language.",
                ),
                Field::new("well", Of::One(Is::Bool), "Whether it is."),
                Field::new(
                    "detail",
                    Of::Whatever,
                    "What was found, where a number is what makes it worth \
                     reading.",
                ),
            ],
        ),
        Shape::new(
            "Health",
            "What is wrong with this installation, where anything is.",
            vec![
                Field::new("well", Of::One(Is::Bool), "Whether every check was."),
                Field::new("checks", Of::ManyOf("Check"), "Each of them."),
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Check as One, Health};
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
        let check = One {
            what: "site.has_pages",
            well: true,
            detail: serde_json::json!({}),
        };

        assert_eq!(
            keys(&serde_json::to_value(&check).expect("a check")),
            fields_of("Check")
        );

        let health = Health {
            well: true,
            checks: vec![check],
        };

        assert_eq!(
            keys(&serde_json::to_value(&health).expect("health")),
            fields_of("Health")
        );
    }
}
