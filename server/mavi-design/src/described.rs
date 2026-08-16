//! How a site looks, described.

use mavi_api::{Field, Is, Of, Shape};

#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "Change",
            "One set of changes to how a site looks. Everything written goes \
             into one of these — there is no way to write a file into what is \
             published, which is the whole shape of this said once.",
            vec![
                Field::new("id", Of::One(Is::Id), "Which one."),
                Field::new("name", Of::One(Is::Text), "What somebody called it."),
                Field::new(
                    "at",
                    Of::OneOf(&["writing", "to_look_at", "broken", "published"]),
                    "Where it has got to. Only something built and looked at \
                     may be published.",
                ),
                Field::new(
                    "look_at",
                    Of::One(Is::Text),
                    "Where to look at it, once it has been built. Under the \
                     build's own id, which nothing links to.",
                )
                .or_null(),
                Field::new(
                    "went_wrong",
                    Of::One(Is::Text),
                    "What the build said, where it did not build. Kept, because \
                     \"it failed\" is not something anybody can act on.",
                )
                .or_null(),
                Field::new("created_at", Of::One(Is::Moment), "When it was started."),
            ],
        ),
        Shape::page_of(
            "ChangePage",
            "Change",
            "Every set of changes, newest first.",
        ),
        Shape::new(
            "NewChange",
            "A set of changes to start. It starts from what is published, so \
             the files that are live are copied in rather than left to be \
             worked out later.",
            vec![Field::new("name", Of::One(Is::Text), "What to call it.").maybe()],
        ),
        Shape::new(
            "File",
            "One file in a site's own project.",
            vec![
                Field::new(
                    "path",
                    Of::One(Is::Text),
                    "Where it is, under `src/` or `public/`. Whatever decides \
                     how a site is built is refused on purpose: that is a way \
                     to run anything on the machine that does the building.",
                ),
                Field::new("contents", Of::One(Is::Text), "What is in it."),
                Field::new(
                    "removed",
                    Of::One(Is::Bool),
                    "Whether this set of changes takes it away.",
                ),
            ],
        ),
        Shape::list_of(
            "FileList",
            "File",
            "Every file in a project. The paths, without what is in them.",
        ),
        Shape::new(
            "Contents",
            "What to write into a file. Never into what is published: this is \
             always a set of changes, which somebody looks at and publishes \
             themselves.",
            vec![
                Field::new("contents", Of::One(Is::Text), "What to put in it."),
                Field::new(
                    "change",
                    Of::One(Is::Id),
                    "Which set of changes to write it into.",
                ),
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Where;
    use crate::store::{Change, File};
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
        let change = Change {
            id: uuid::Uuid::nil(),
            name: "A change".to_owned(),
            at: Where::Writing,
            look_at: None,
            went_wrong: None,
            created_at: chrono::Utc::now(),
        };

        assert_eq!(
            keys(&serde_json::to_value(&change).expect("a change")),
            fields_of("Change")
        );

        let file = File {
            path: "public/index.html".to_owned(),
            contents: "<h1>Hello</h1>".to_owned(),
            removed: false,
        };

        assert_eq!(
            keys(&serde_json::to_value(&file).expect("a file")),
            fields_of("File")
        );
    }
}
