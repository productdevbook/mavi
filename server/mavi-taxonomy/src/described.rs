//! What a term looks like going in and coming out.

use mavi_api::{Field, Is, Of, Shape};

/// What a sort is, said once. Two, and closed: a category may be under
/// another and a tag is flat, and that difference is this crate's rather than
/// a site's — so unlike a writing's kind, it really is a choice between two.
const A_SORT: &[&str] = &["category", "tag"];

#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![
        a_term(),
        Shape::page_of("TermPage", "Term", "What a site files things under."),
        Shape::list_of(
            "TermList",
            "Term",
            "All of them, with nothing to page through — what one writing is \
             filed under is a handful, not a listing.",
        ),
        something_new(),
        what_may_change(),
        a_filing(),
    ]
}

fn a_term() -> Shape {
    Shape::new(
        "Term",
        "Somewhere a site files things, or something it says they are about.",
        vec![
            Field::new("id", Of::One(Is::Id), "Which one."),
            Field::new(
                "sort",
                Of::OneOf(A_SORT),
                "A category is somewhere it lives and may be under another. A \
                 tag is something it is about, and is flat.",
            ),
            Field::new(
                "language",
                Of::One(Is::Text),
                "Which language it is written in.",
            ),
            Field::new("slug", Of::One(Is::Text), "Where it answers."),
            Field::new("name", Of::One(Is::Text), "What it is called."),
            Field::new(
                "parent",
                Of::One(Is::Id),
                "Which category it is under. Always nothing for a tag.",
            )
            .or_null(),
            Field::new("created_at", Of::One(Is::Moment), "When it was made."),
            Field::new("updated_at", Of::One(Is::Moment), "When it last changed."),
        ],
    )
}

fn something_new() -> Shape {
    Shape::new(
        "NewTerm",
        "One to make.",
        vec![
            Field::new("sort", Of::OneOf(A_SORT), "Which of the two."),
            Field::new(
                "language",
                Of::One(Is::Text),
                "Which language it is written in.",
            ),
            Field::new("slug", Of::One(Is::Text), "Where it should answer."),
            Field::new("name", Of::One(Is::Text), "What it is called."),
            Field::new(
                "parent",
                Of::One(Is::Id),
                "Which category to put it under. Refused for a tag.",
            )
            .maybe()
            .or_null(),
        ],
    )
}

fn what_may_change() -> Shape {
    Shape::new(
        "TermChanges",
        "What may be changed about one. Its sort and its address are not among \
         them: those are what everything filed under it points at.",
        vec![
            Field::new("name", Of::One(Is::Text), "What it is called.").maybe(),
            Field::new(
                "parent",
                Of::One(Is::Id),
                "Which category to move it under. Null moves it out from under \
                 anything; left out leaves it where it is.",
            )
            .maybe()
            .or_null(),
        ],
    )
}

fn a_filing() -> Shape {
    Shape::new(
        "Filing",
        "What one writing is filed under. Replaces whatever it was, so what is \
         sent is the whole of it rather than what to add.",
        vec![Field::new(
            "terms",
            Of::Many(Is::Id),
            "Every term it is filed under now.",
        )],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{NewTerm, TermChanges};
    use crate::term::{Sort, Term, TermId};
    use mavi_core::slug::Slug;
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
        let term = Term {
            id: TermId(uuid::Uuid::nil()),
            sort: Sort::Category,
            language: "en".to_owned(),
            slug: Slug::parse("news").expect("a slug"),
            name: "News".to_owned(),
            parent: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let sent = serde_json::to_value(&term).expect("a term");

        assert_eq!(keys(&sent), fields_of("Term"));
    }

    #[test]
    fn what_is_described_is_what_is_taken() {
        let new = serde_json::to_value(NewTerm {
            sort: "tag".to_owned(),
            language: "en".to_owned(),
            slug: "news".to_owned(),
            name: "News".to_owned(),
            parent: None,
        })
        .expect("a new term");

        assert_eq!(keys(&new), fields_of("NewTerm"));

        let changes = serde_json::to_value(TermChanges {
            name: None,
            parent: None,
        })
        .expect("changes");

        assert_eq!(keys(&changes), fields_of("TermChanges"));
    }
}
