//! What a form is, and what somebody sending one is.

use mavi_api::{Field as Says, Is, Of, Shape};

/// What a form may ask for. Closed, and this software's: a screen has to know
/// which box to draw, so a kind nothing can draw is not a kind.
const A_KIND: &[&str] = &["text", "long", "email", "number", "choice", "boolean"];

#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![
        a_field(),
        a_form(),
        Shape::page_of("FormPage", "Form", "What a site asks people."),
        an_open_form(),
        something_new(),
        what_may_change(),
        what_was_sent(),
        Shape::page_of("FilledPage", "Sent", "What people sent."),
        what_somebody_sends(),
        Shape::new(
            "Received",
            "It arrived. Nothing about the site comes back here — whoever filled \
             the form in is not somebody this tells what else exists.",
            vec![Says::new("id", Of::One(Is::Id), "What arrived.")],
        ),
        Shape::new(
            "Seen",
            "How many were marked as read.",
            vec![Says::new(
                "seen",
                Of::One(Is::Number),
                "How many had not been read and now have been.",
            )],
        ),
    ]
}

fn a_field() -> Shape {
    Shape::new(
        "FormField",
        "One thing a form asks for.",
        vec![
            Says::new(
                "key",
                Of::One(Is::Text),
                "What the answer comes back under. Also what a refusal names, \
                 so somebody filling the form in is told which box was wrong.",
            ),
            Says::new("label", Of::One(Is::Text), "What it says on the screen."),
            Says::new(
                "required",
                Of::One(Is::Bool),
                "Whether it may be left empty.",
            ),
            Says::new("kind", Of::OneOf(A_KIND), "Which box to draw."),
            Says::new(
                "options",
                Of::Many(Is::Text),
                "What a `choice` may be. Empty for every other kind, and \
                 refused if it is not.",
            ),
        ],
    )
}

fn a_form() -> Shape {
    Shape::new(
        "Form",
        "Something a site asks people, as whoever made it sees it.",
        vec![
            Says::new("id", Of::One(Is::Id), "Which one."),
            Says::new("slug", Of::One(Is::Text), "Where it answers."),
            Says::new("name", Of::One(Is::Text), "What it is called."),
            Says::new("fields", Of::ManyOf("FormField"), "What it asks for."),
            Says::new(
                "open",
                Of::One(Is::Bool),
                "Whether anybody may send it. A closed one answers the same \
                 way as one that was never made, so the refusal is not a way \
                 to ask what this site has.",
            ),
            Says::new(
                "kept_days",
                Of::One(Is::Number),
                "How long what people send is kept.",
            ),
            Says::new("created_at", Of::One(Is::Moment), "When it was made."),
            Says::new("updated_at", Of::One(Is::Moment), "When it last changed."),
        ],
    )
}

fn an_open_form() -> Shape {
    Shape::new(
        "OpenForm",
        "The same form as a page about to draw it sees it. Its own shape rather \
         than the whole one with fields left out — leaving things out is \
         something somebody has to keep doing, and what is missing here is \
         everything about the site rather than about the form.",
        vec![
            Says::new("slug", Of::One(Is::Text), "Where it answers."),
            Says::new("name", Of::One(Is::Text), "What it is called."),
            Says::new("fields", Of::ManyOf("FormField"), "What it asks for."),
        ],
    )
}

fn something_new() -> Shape {
    Shape::new(
        "NewForm",
        "One to make.",
        vec![
            Says::new("slug", Of::One(Is::Text), "Where it should answer."),
            Says::new("name", Of::One(Is::Text), "What it is called."),
            Says::new("fields", Of::ManyOf("FormField"), "What it asks for.").maybe(),
            Says::new(
                "kept_days",
                Of::One(Is::Number),
                "How long what people send is kept.",
            )
            .maybe()
            .or_null(),
        ],
    )
}

fn what_may_change() -> Shape {
    Shape::new(
        "FormChanges",
        "What may be changed about one. Its address is not among them: it is \
         what every page carrying the form points at.",
        vec![
            Says::new("name", Of::One(Is::Text), "What it is called.").maybe(),
            Says::new(
                "fields",
                Of::ManyOf("FormField"),
                "What it asks for. Replaces whatever it asked for before.",
            )
            .maybe()
            .or_null(),
            Says::new("open", Of::One(Is::Bool), "Whether anybody may send it.")
                .maybe()
                .or_null(),
            Says::new(
                "kept_days",
                Of::One(Is::Number),
                "How long what people send is kept.",
            )
            .maybe()
            .or_null(),
        ],
    )
}

fn what_was_sent() -> Shape {
    Shape::new(
        "Sent",
        "One thing somebody sent. Where it came from is written down and is not \
         here — an address is about whoever filled the form in rather than \
         about what they said.",
        vec![
            Says::new("id", Of::One(Is::Id), "Which one."),
            Says::new("form_id", Of::One(Is::Id), "Which form."),
            Says::new(
                "answers",
                Of::Whatever,
                "What they said, by the key each field declared.",
            ),
            Says::new("seen_at", Of::One(Is::Moment), "When somebody read it.").or_null(),
            Says::new("created_at", Of::One(Is::Moment), "When it arrived."),
        ],
    )
}

fn what_somebody_sends() -> Shape {
    Shape::new(
        "Filled",
        "Sending a form. Every rule is on this side, because anybody may.",
        vec![Says::new(
            "answers",
            Of::Whatever,
            "One value per field the form declared, by its key. A field it \
             never asked for is refused rather than kept.",
        )],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{FormChanges, NewForm};
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
        let field = mavi_core::asked::Field {
            key: mavi_core::slug::Slug::parse("name").expect("a key"),
            label: "Your name".to_owned(),
            required: true,
            kind: mavi_core::asked::Kind::Text,
            options: Vec::new(),
        };

        assert_eq!(
            keys(&serde_json::to_value(&field).expect("a field")),
            fields_of("FormField")
        );

        let sent = crate::Sent {
            id: crate::FilledId(uuid::Uuid::nil()),
            form_id: crate::FormId(uuid::Uuid::nil()),
            answers: serde_json::Map::new(),
            seen_at: None,
            created_at: chrono::Utc::now(),
        };

        assert_eq!(
            keys(&serde_json::to_value(&sent).expect("what was sent")),
            fields_of("Sent")
        );
    }

    #[test]
    fn what_is_described_is_what_is_taken() {
        let new = serde_json::to_value(NewForm {
            slug: "contact".to_owned(),
            name: "Contact".to_owned(),
            fields: Vec::new(),
            kept_days: None,
        })
        .expect("a new form");

        assert_eq!(keys(&new), fields_of("NewForm"));

        let changes = serde_json::to_value(FormChanges {
            name: None,
            fields: None,
            open: None,
            kept_days: None,
        })
        .expect("changes");

        assert_eq!(keys(&changes), fields_of("FormChanges"));

        let filling = serde_json::to_value(crate::Filled {
            answers: serde_json::Map::new(),
        })
        .expect("a filled form");

        assert_eq!(keys(&filling), fields_of("Filled"));
    }
}
