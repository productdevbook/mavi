//! What a writing looks like going in and coming out.
//!
//! Beside the types rather than in a file of its own far away, because the two
//! have to agree and the way that happens is that somebody changing one sees
//! the other. What holds it to the type is the test at the bottom: a real
//! value is serialised and its fields are compared with what is declared here.

use mavi_api::{Field, Is, Of, Shape};

/// A kind, said the same way everywhere it appears.
///
/// **Not a list of two.** A kind is whatever a site decided a thing is — the
/// type's own words are that a CMS whose kinds are fixed at compile time is a
/// CMS for one site. Describing it as a choice between `post` and `page` would
/// be a description that a generated client turns into a type refusing every
/// other site's kinds.
const A_KIND: &str = "What a site decided this is. Lowercase, at most \
                      thirty-one characters. `post` and `page` are what an \
                      installation starts with — a page is one that is not in \
                      the feed — and a site may have as many others as it \
                      likes.";

/// Everything a writing is, going in and coming out.
#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![
        a_writing(),
        Shape::page_of("WritingPage", "Writing", "What a site has written."),
        something_to_write(),
        what_may_change(),
    ]
}

fn a_writing() -> Shape {
    Shape::new(
        "Writing",
        "Something a site wrote.",
        vec![
            Field::new("id", Of::One(Is::Id), "Which one."),
            Field::new("kind", Of::One(Is::Text), A_KIND),
            Field::new(
                "language",
                Of::One(Is::Text),
                "Which language it is written in.",
            ),
            Field::new("slug", Of::One(Is::Text), "Where it answers."),
            Field::new("title", Of::One(Is::Text), "What it is called."),
            Field::new("excerpt", Of::One(Is::Text), "A line about it.").or_null(),
            Field::new("body", Of::One(Is::Text), "What it says."),
            Field::new(
                "fields",
                Of::Whatever,
                "Whatever this site decided to keep beside it.",
            ),
            Field::new(
                "state",
                Of::OneOf(&["draft", "published"]),
                "Whether it is out.",
            ),
            Field::new(
                "published_at",
                Of::One(Is::Moment),
                "When it went out, or is going to. A date in the future is a \
                 thing that goes out on it.",
            )
            .or_null(),
            Field::new("created_at", Of::One(Is::Moment), "When it was written."),
            Field::new("updated_at", Of::One(Is::Moment), "When it last changed."),
        ],
    )
}

fn something_to_write() -> Shape {
    Shape::new(
        "NewWriting",
        "Something to write.",
        vec![
            Field::new("kind", Of::One(Is::Text), A_KIND),
            Field::new(
                "language",
                Of::One(Is::Text),
                "Which language it is written in.",
            ),
            Field::new(
                "slug",
                Of::One(Is::Text),
                "Where it should answer. Taken by the database rather than \
                 checked first, so two people writing at one address is one of \
                 them told so.",
            ),
            Field::new(
                "title",
                Of::One(Is::Text),
                "What it is called. Between one and two hundred characters.",
            ),
            Field::new("excerpt", Of::One(Is::Text), "A line about it.")
                .maybe()
                .or_null(),
            Field::new("body", Of::One(Is::Text), "What it says.").maybe(),
            Field::new(
                "fields",
                Of::Whatever,
                "Whatever this site decided to keep beside it.",
            )
            .maybe(),
            Field::new(
                "publish_at",
                Of::One(Is::Moment),
                "Left out means a draft. A date in the future is a thing that \
                 goes out on it.",
            )
            .maybe()
            .or_null(),
        ],
    )
}

fn what_may_change() -> Shape {
    Shape::new(
        "WritingChanges",
        "What may be changed about one. Only what is sent is changed — a change \
         that wrote every field would write back over whatever somebody else \
         changed a second ago.",
        vec![
            Field::new(
                "slug",
                Of::One(Is::Text),
                "Where it answers. The old address keeps working.",
            )
            .maybe(),
            Field::new("title", Of::One(Is::Text), "What it is called.").maybe(),
            Field::new("excerpt", Of::One(Is::Text), "A line about it.").maybe(),
            Field::new("body", Of::One(Is::Text), "What it says.").maybe(),
            Field::new(
                "fields",
                Of::Whatever,
                "Whatever this site decided to keep beside it.",
            )
            .maybe(),
            Field::new(
                "publish_at",
                Of::One(Is::Moment),
                "Left out leaves it where it is. Null takes it back off the \
                 site; a date sends it out then.",
            )
            .maybe()
            .or_null(),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writing::{Kind, State, Writing, WritingId};
    use mavi_core::slug::Slug;
    use std::collections::BTreeSet;

    fn fields_of(named: &str) -> BTreeSet<&'static str> {
        shapes()
            .into_iter()
            .find(|shape| shape.named == named)
            .expect("a shape")
            .fields
            .into_iter()
            .map(|field| field.name)
            .collect()
    }

    #[test]
    fn what_is_described_is_what_is_sent() {
        // The one thing a hand-written description cannot be trusted about. A
        // real value goes out through the same serialiser a caller reads, and
        // its keys are compared with what is declared above — so a field
        // added to the type and not here fails here rather than in somebody's
        // client.
        let writing = Writing {
            id: WritingId(uuid::Uuid::nil()),
            kind: Kind::parse("post").expect("a kind"),
            language: "en".to_owned(),
            slug: Slug::parse("hello").expect("a slug"),
            title: "A Title".to_owned(),
            excerpt: None,
            body: String::new(),
            fields: serde_json::Value::Null,
            state: State::Draft,
            published_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let sent: serde_json::Value = serde_json::to_value(&writing).expect("a writing");
        let sent: BTreeSet<&str> = sent
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();

        let described = fields_of("Writing");

        assert_eq!(
            sent,
            described.iter().copied().collect::<BTreeSet<&str>>(),
            "what a Writing is and what it says it is have come apart"
        );
    }

    #[test]
    fn what_is_described_is_what_is_taken() {
        // The other direction. A field a caller may send that is described
        // nowhere is a field nothing tells them about; one described and not
        // read is one they will send and wonder about.
        let taken: serde_json::Value = serde_json::to_value(crate::writing::New {
            kind: "post".to_owned(),
            language: "en".to_owned(),
            slug: "hello".to_owned(),
            title: "A Title".to_owned(),
            excerpt: None,
            body: String::new(),
            fields: serde_json::Value::Null,
            publish_at: None,
        })
        .expect("something to write");

        let taken: BTreeSet<&str> = taken
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();

        assert_eq!(taken, fields_of("NewWriting"));

        let changes: serde_json::Value =
            serde_json::to_value(crate::store::Changes::default()).expect("changes");
        let changes: BTreeSet<&str> = changes
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();

        assert_eq!(changes, fields_of("WritingChanges"));
    }
}
