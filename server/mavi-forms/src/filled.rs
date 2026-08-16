//! What somebody sent, held against what the form asked for.
//!
//! This runs on a public endpoint. Whatever a page does before posting is a
//! courtesy — the form's address takes a body from anybody, and every rule
//! that matters is here.

use mavi_core::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use mavi_core::asked::Declared;

pub use mavi_core::asked::{
    AT_MOST_ALTOGETHER, IT_HAS_NO_SUCH_FIELD as THAT_FORM_HAS_NO_SUCH_FIELD,
    IT_WANTS_THAT_FIELD as THAT_FORM_WANTS_THAT_FIELD,
    THAT_IS_MORE_THAN_IT_TAKES as THAT_IS_MORE_THAN_A_FORM_TAKES,
    THAT_IS_NOT_WHAT_THAT_FIELD_HOLDS,
};

/// What somebody filled in.
///
/// Serialised as well as read, so the test beside the description can hold
/// what it says it takes against what it takes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Filled {
    pub answers: Map<String, Value>,
}

impl Filled {
    /// Whether this is what the form asked for.
    ///
    /// The order is not decoration. Size first, because it is the only check
    /// whose cost does not depend on how big the thing is; then the fields the
    /// form declared, so a refusal names what is missing; then anything sent
    /// that the form never asked for.
    pub fn fits(&self, declared: &Declared) -> Result<()> {
        mavi_core::asked::fits(&self.answers, declared)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mavi_core::asked::Field;
    use mavi_core::slug::Slug;
    use serde_json::json;

    fn asking(key: &str, kind: Kind, required: bool) -> Field {
        Field {
            key: Slug::parse(key).expect("an address"),
            label: "Something".to_owned(),
            required,
            kind,
            options: Vec::new(),
        }
    }

    fn filled(answers: &Value) -> Filled {
        Filled {
            answers: answers.as_object().expect("an object").clone(),
        }
    }

    fn refused(declared: &Declared, answers: &Value) -> &'static str {
        filled(answers)
            .fits(declared)
            .expect_err("a refusal")
            .said()
            .expect("a sentence")
            .key
    }

    fn a_contact_form() -> Declared {
        Declared::checked(vec![
            asking("name", Kind::Text, true),
            asking("email", Kind::Email, true),
            asking("message", Kind::Long, false),
        ])
        .expect("a form")
    }

    #[test]
    fn what_the_form_asked_for_is_what_it_takes() {
        let form = a_contact_form();

        assert!(
            filled(&json!({"name": "A Visitor", "email": "someone@example.test"}))
                .fits(&form)
                .is_ok()
        );

        assert_eq!(
            refused(&form, &json!({"email": "someone@example.test"})),
            THAT_FORM_WANTS_THAT_FIELD
        );
        assert_eq!(
            refused(&form, &json!({"name": "A Visitor", "email": "not one"})),
            THAT_IS_NOT_WHAT_THAT_FIELD_HOLDS
        );
    }

    #[test]
    fn a_form_that_asks_for_nothing_accepts_nothing() {
        // The hole this closes: the check for unasked keys used to be skipped
        // when a form declared no fields, so a form with nothing on it was the
        // one form that took whatever anybody sent — and it was stored, and
        // shown to whoever reads the submissions.
        let empty = Declared::checked(Vec::new()).expect("a form");

        assert_eq!(
            refused(&empty, &json!({"anything": "at all"})),
            THAT_FORM_HAS_NO_SUCH_FIELD
        );
        assert!(filled(&json!({})).fits(&empty).is_ok());
    }

    #[test]
    fn a_key_the_form_never_asked_for_is_not_kept() {
        let form = a_contact_form();

        assert_eq!(
            refused(
                &form,
                &json!({
                    "name": "A Visitor",
                    "email": "someone@example.test",
                    "role": "admin",
                })
            ),
            THAT_FORM_HAS_NO_SUCH_FIELD
        );
    }

    #[test]
    fn what_it_takes_is_bounded_altogether_and_not_only_one_at_a_time() {
        // Each answer under the limit and the whole thing far over it is the
        // shape a per-answer limit misses, and it is the one somebody sending
        // a hundred of these a minute will find.
        let form = Declared::checked(vec![asking("message", Kind::Long, false)]).expect("a form");

        let long = "x".repeat(AT_MOST_ALTOGETHER + 1);

        assert_eq!(
            refused(&form, &json!({ "message": long })),
            THAT_IS_MORE_THAN_A_FORM_TAKES
        );
    }

    #[test]
    fn something_left_blank_that_was_not_asked_for_is_fine() {
        let form = a_contact_form();

        assert!(
            filled(&json!({
                "name": "A Visitor",
                "email": "someone@example.test",
                "message": "   ",
            }))
            .fits(&form)
            .is_ok()
        );
    }

    #[test]
    fn a_choice_takes_only_what_it_offers() {
        let mut choice = asking("colour", Kind::Choice, true);
        choice.options = vec!["red".to_owned(), "blue".to_owned()];
        let form = Declared::checked(vec![choice]).expect("a form");

        assert!(filled(&json!({"colour": "red"})).fits(&form).is_ok());
        assert_eq!(
            refused(&form, &json!({"colour": "green"})),
            THAT_IS_NOT_WHAT_THAT_FIELD_HOLDS
        );
    }
}
