//! What a second step looks like, described.

use mavi_api::{Field, Is, Of, Shape};

#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "SecondStanding",
            "Whether whoever is asking has a second step.",
            vec![
                Field::new("set_up", Of::One(Is::Bool), "Whether one has been started."),
                Field::new(
                    "confirmed",
                    Of::One(Is::Bool),
                    "Whether the digits have been shown to work. **An \
                     unconfirmed one stands between nobody and their account** \
                     — somebody who scanned a picture and closed the tab has \
                     not locked themselves out.",
                ),
                Field::new(
                    "ways_back_in",
                    Of::One(Is::Number),
                    "How many are left unused. Somebody down to their last one \
                     should be told before the phone goes, not after.",
                ),
            ],
        ),
        Shape::new(
            "SecondToSetUp",
            "What to put in front of somebody setting one up. Shown once.",
            vec![
                Field::new(
                    "what_an_app_reads",
                    Of::One(Is::Text),
                    "An `otpauth://` address. What a picture is made out of.",
                ),
                Field::new(
                    "typed_in",
                    Of::One(Is::Text),
                    "The same secret written out, for somebody whose machine \
                     cannot show a picture or whose phone cannot read one.",
                ),
            ],
        ),
        Shape::new(
            "SomeDigits",
            "The six digits an app is showing.",
            vec![Field::new("code", Of::One(Is::Text), "What it says.")],
        ),
        Shape::new(
            "WaysBackIn",
            "What gets somebody back in when the phone is gone. **Shown once** \
             — what is kept is their hashes, so nothing can answer them again.",
            vec![Field::new(
                "codes",
                Of::Many(Is::Text),
                "Ten of them. No letters a handwritten note confuses, because \
                 the moment these are read is the moment somebody is already \
                 locked out and typing off paper.",
            )],
        ),
        Shape::new(
            "Finishing",
            "Finishing signing in.",
            vec![
                Field::new(
                    "moment",
                    Of::One(Is::Text),
                    "What signing in answered with. Short-lived, and it says \
                     nothing about who it is for.",
                ),
                Field::new(
                    "code",
                    Of::One(Is::Text),
                    "The six digits, or one of the ways back in. Either is \
                     taken here, because somebody without their phone is \
                     somebody who has to get in.",
                ),
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Standing, ToSetUp, WaysBackIn};
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
        let standing = Standing {
            set_up: true,
            confirmed: false,
            ways_back_in: 0,
        };

        assert_eq!(
            keys(&serde_json::to_value(standing).expect("standing")),
            fields_of("SecondStanding")
        );

        let to_set_up = ToSetUp {
            what_an_app_reads: "otpauth://totp/x".to_owned(),
            typed_in: "AAAA".to_owned(),
        };

        assert_eq!(
            keys(&serde_json::to_value(&to_set_up).expect("to set up")),
            fields_of("SecondToSetUp")
        );

        let ways = WaysBackIn {
            codes: vec!["ABCDE-FGHJK".to_owned()],
        };

        assert_eq!(
            keys(&serde_json::to_value(&ways).expect("ways back in")),
            fields_of("WaysBackIn")
        );
    }

    #[test]
    fn nothing_here_answers_the_secret_after_it_was_shown() {
        // Set up answers it once. Nothing else does — a standing that carried
        // the secret would be an endpoint that hands it to whoever holds a
        // session, which is exactly what a second step exists to be separate
        // from.
        for shape in shapes()
            .iter()
            .filter(|shape| shape.named != "SecondToSetUp")
        {
            for field in shape.fields() {
                assert!(
                    !["secret", "sealed", "typed_in", "what_an_app_reads"].contains(&field.name),
                    "{} answers with {}",
                    shape.named,
                    field.name
                );
            }
        }
    }
}
