//! What a site works through in stages.

use mavi_api::{Field, Is, Of, Shape};

#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "Stage",
            "One column on a board.",
            vec![
                Field::new("id", Of::One(Is::Id), "Which one."),
                Field::new("name", Of::One(Is::Text), "What it is called."),
                Field::new(
                    "place",
                    Of::One(Is::Number),
                    "Where it comes, left to right.",
                ),
            ],
        ),
        Shape::new(
            "Board",
            "Something a site works through in stages.",
            vec![
                Field::new("id", Of::One(Is::Id), "Which one."),
                Field::new("name", Of::One(Is::Text), "What it is called."),
                Field::new("stages", Of::ManyOf("Stage"), "Its columns, left to right."),
                Field::new("created_at", Of::One(Is::Moment), "When it was made."),
            ],
        ),
        Shape::list_of(
            "BoardList",
            "Board",
            "Every board. A handful, with nothing to page through.",
        ),
        Shape::new(
            "NewBoard",
            "One to make.",
            vec![
                Field::new("name", Of::One(Is::Text), "What it is called."),
                Field::new(
                    "stages",
                    Of::Many(Is::Text),
                    "The columns it starts with, left to right. At least one: a \
                     board with none is a board nothing can be put on, so it is \
                     refused here rather than made and then wondered about.",
                ),
            ],
        ),
        a_card(),
        Shape::page_of("CardPage", "Card", "What is on one board."),
        Shape::new(
            "NewCard",
            "One to put on a board. It goes at the bottom of its column.",
            vec![
                Field::new("stage", Of::One(Is::Id), "Which column."),
                Field::new("title", Of::One(Is::Text), "What it says."),
                Field::new("detail", Of::One(Is::Text), "The rest of it.")
                    .maybe()
                    .or_null(),
                Field::new("owner", Of::One(Is::Text), "Whose it is.")
                    .maybe()
                    .or_null(),
            ],
        ),
        Shape::new(
            "CardChanges",
            "What may be changed about one. Where it is is not among them: \
             moving it is `Between`.",
            vec![
                Field::new("title", Of::One(Is::Text), "What it says.").maybe(),
                Field::new("detail", Of::One(Is::Text), "The rest of it.").maybe(),
                Field::new("owner", Of::One(Is::Text), "Whose it is.").maybe(),
            ],
        ),
        Shape::new(
            "Between",
            "Where a card was dropped: which column, and between which two \
             cards. Its neighbours rather than a number, because what a person \
             did is drop it between two cards — the number is this software's \
             business. Both neighbours absent means an empty column.",
            vec![
                Field::new("stage", Of::One(Is::Id), "Which column it was dropped in."),
                Field::new("after", Of::One(Is::Id), "The card above it.")
                    .maybe()
                    .or_null(),
                Field::new("before", Of::One(Is::Id), "The card below it.")
                    .maybe()
                    .or_null(),
            ],
        ),
    ]
}

fn a_card() -> Shape {
    Shape::new(
        "Card",
        "One thing on a board.",
        vec![
            Field::new("id", Of::One(Is::Id), "Which one."),
            Field::new("board_id", Of::One(Is::Id), "Which board."),
            Field::new("stage_id", Of::One(Is::Id), "Which column it is in."),
            Field::new("title", Of::One(Is::Text), "What it says."),
            Field::new("detail", Of::One(Is::Text), "The rest of it.").or_null(),
            Field::new("owner", Of::One(Is::Text), "Whose it is.").or_null(),
            Field::new(
                "place",
                Of::One(Is::Number),
                "Where it sits in its column. A fraction, so dropping one \
                 between two others moves one row rather than every row below \
                 it.",
            ),
            Field::new("created_at", Of::One(Is::Moment), "When it was made."),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Between, Board, Card, CardChanges, NewBoard, NewCard, Stage};
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
        let stage = Stage {
            id: uuid::Uuid::nil(),
            name: "To do".to_owned(),
            place: 1,
        };

        assert_eq!(
            keys(&serde_json::to_value(&stage).expect("a stage")),
            fields_of("Stage")
        );

        let board = Board {
            id: uuid::Uuid::nil(),
            name: "A Board".to_owned(),
            stages: vec![stage],
            created_at: chrono::Utc::now(),
        };

        assert_eq!(
            keys(&serde_json::to_value(&board).expect("a board")),
            fields_of("Board")
        );

        let card = Card {
            id: uuid::Uuid::nil(),
            board_id: uuid::Uuid::nil(),
            stage_id: uuid::Uuid::nil(),
            title: "A Card".to_owned(),
            detail: None,
            owner: None,
            place: 1.0,
            created_at: chrono::Utc::now(),
        };

        assert_eq!(
            keys(&serde_json::to_value(&card).expect("a card")),
            fields_of("Card")
        );
    }

    #[test]
    fn what_is_described_is_what_is_taken() {
        let board = serde_json::to_value(NewBoard {
            name: "A Board".to_owned(),
            stages: vec!["To do".to_owned()],
        })
        .expect("a new board");

        assert_eq!(keys(&board), fields_of("NewBoard"));

        let card = serde_json::to_value(NewCard {
            stage: uuid::Uuid::nil(),
            title: "A Card".to_owned(),
            detail: None,
            owner: None,
        })
        .expect("a new card");

        assert_eq!(keys(&card), fields_of("NewCard"));

        assert_eq!(
            keys(&serde_json::to_value(CardChanges::default()).expect("changes")),
            fields_of("CardChanges")
        );

        let dropped = serde_json::to_value(Between {
            stage: uuid::Uuid::nil(),
            after: None,
            before: None,
        })
        .expect("where it went");

        assert_eq!(keys(&dropped), fields_of("Between"));
    }
}
