//! What an uploaded file looks like coming back.

use mavi_api::{Field, Is, Of, Shape};

#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "File",
            "Something somebody uploaded.",
            vec![
                Field::new("id", Of::One(Is::Id), "Which one."),
                Field::new(
                    "kind",
                    Of::OneOf(&["image", "video", "audio", "document"]),
                    "What sort of thing it is. Decided by reading the bytes — \
                     what a file was called is what somebody typed, and what \
                     they typed is not evidence.",
                ),
                Field::new(
                    "mime",
                    Of::One(Is::Text),
                    "What it is, exactly. From the bytes, and from a list — \
                     never from the name.",
                ),
                Field::new(
                    "name",
                    Of::One(Is::Text),
                    "What it was called when it arrived. Shown to people and \
                     used for nothing else.",
                ),
                Field::new(
                    "kept_at",
                    Of::One(Is::Text),
                    "Where it is kept. Opaque: whatever is holding it decides \
                     what this means, and nothing reading it may take it apart.",
                ),
                Field::new("bytes", Of::One(Is::Number), "How big it is."),
                Field::new("created_at", Of::One(Is::Moment), "When it arrived."),
            ],
        ),
        Shape::page_of("FilePage", "File", "What a site has uploaded."),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kept::{File, FileId, Kind};
    use std::collections::BTreeSet;

    #[test]
    fn what_is_described_is_what_is_sent() {
        let file = File {
            id: FileId(uuid::Uuid::nil()),
            kind: Kind::Image,
            mime: "image/png".to_owned(),
            name: "a-picture.png".to_owned(),
            kept_at: "ab/cdef.png".to_owned(),
            bytes: 12,
            created_at: chrono::Utc::now(),
        };

        let sent = serde_json::to_value(&file).expect("a file");
        let sent: BTreeSet<&str> = sent
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();

        let described: BTreeSet<&str> = shapes()
            .iter()
            .find(|shape| shape.named == "File")
            .expect("a shape")
            .fields()
            .iter()
            .map(|field| field.name)
            .collect();

        assert_eq!(sent, described);
    }
}
