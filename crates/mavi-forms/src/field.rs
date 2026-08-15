//! What a form asks for.
//!
//! A form is a list of fields, and the list is checked when it is declared
//! rather than when somebody fills it in. That order is the whole design: a
//! form is declared once by somebody signed in, and filled in by anybody, any
//! number of times. Every question answered at declaration is a question not
//! asked on a public endpoint.

use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use mavi_core::slug::Slug;
use serde::{Deserialize, Serialize};

pub const A_FORM_ASKS_AT_MOST_SO_MANY_THINGS: &str = "a_form_asks_at_most_so_many_things";
pub const A_FORM_ASKS_EACH_THING_ONCE: &str = "a_form_asks_each_thing_once";
pub const A_LABEL_IS_BETWEEN_ONE_AND_TWO_HUNDRED: &str = "a_label_is_between_one_and_two_hundred";
pub const A_CHOICE_NEEDS_SOMETHING_TO_CHOOSE_FROM: &str = "a_choice_needs_something_to_choose_from";
pub const ONLY_A_CHOICE_HAS_OPTIONS: &str = "only_a_choice_has_options";
pub const A_CHOICE_OFFERS_AT_MOST_SO_MANY: &str = "a_choice_offers_at_most_so_many";

/// How many things one form may ask for.
///
/// A limit exists because a form with none is a `jsonb` column whose size
/// whoever declares the form decides, checked in a loop on every submission.
pub const AT_MOST_FIELDS: usize = 50;

/// How many things one choice may offer.
pub const AT_MOST_OPTIONS: usize = 100;

/// What may be written in one field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    #[default]
    Text,
    /// More than a line of it. The same rule as text, said so that a screen
    /// can draw the right box.
    Long,
    Email,
    Number,
    Choice,
    Boolean,
}

impl Kind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Long => "long",
            Kind::Email => "email",
            Kind::Number => "number",
            Kind::Choice => "choice",
            Kind::Boolean => "boolean",
        }
    }

    #[must_use]
    pub const fn chooses(self) -> bool {
        matches!(self, Kind::Choice)
    }
}

/// One thing a form asks for.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Field {
    pub key: Slug,
    pub label: String,
    pub required: bool,
    #[serde(default)]
    pub kind: Kind,
    /// What a `choice` may be. Empty for every other kind, and checked.
    #[serde(default)]
    pub options: Vec<String>,
}

/// A list of fields that has been checked.
///
/// The type is what carries that. Nothing constructs one except [`Declared::checked`],
/// so a `Declared` in hand has already been through every rule below — which
/// is what keeps the public endpoint from having to ask again.
#[derive(Clone, Debug, Serialize)]
pub struct Declared(Vec<Field>);

impl Declared {
    /// Every rule about the shape of a form, in one place.
    pub fn checked(fields: Vec<Field>) -> Result<Self> {
        if fields.len() > AT_MOST_FIELDS {
            return Err(Error::invalid(
                Say::of(A_FORM_ASKS_AT_MOST_SO_MANY_THINGS).with("at_most", &AT_MOST_FIELDS),
            ));
        }

        for (at, field) in fields.iter().enumerate() {
            let length = field.label.trim().chars().count();
            if !(1..=200).contains(&length) {
                return Err(Error::invalid(
                    Say::of(A_LABEL_IS_BETWEEN_ONE_AND_TWO_HUNDRED)
                        .with("field", &field.key.as_str()),
                ));
            }

            if fields[..at].iter().any(|before| before.key == field.key) {
                return Err(Error::invalid(
                    Say::of(A_FORM_ASKS_EACH_THING_ONCE).with("field", &field.key.as_str()),
                ));
            }

            if field.kind.chooses() {
                if field.options.is_empty() {
                    // A choice with nothing to choose from refuses every
                    // answer, including the right one, and reads as a form
                    // that is simply broken to whoever tries to fill it in.
                    return Err(Error::invalid(
                        Say::of(A_CHOICE_NEEDS_SOMETHING_TO_CHOOSE_FROM)
                            .with("field", &field.key.as_str()),
                    ));
                }

                if field.options.len() > AT_MOST_OPTIONS {
                    return Err(Error::invalid(
                        Say::of(A_CHOICE_OFFERS_AT_MOST_SO_MANY)
                            .with("field", &field.key.as_str())
                            .with("at_most", &AT_MOST_OPTIONS),
                    ));
                }
            } else if !field.options.is_empty() {
                // Options on something that is not a choice are ignored when
                // the form is filled in, so whoever wrote them is looking at a
                // list nothing will ever offer.
                return Err(Error::invalid(
                    Say::of(ONLY_A_CHOICE_HAS_OPTIONS).with("field", &field.key.as_str()),
                ));
            }
        }

        Ok(Self(fields))
    }

    #[must_use]
    pub fn fields(&self) -> &[Field] {
        &self.0
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asking(key: &str, kind: Kind) -> Field {
        Field {
            key: Slug::parse(key).expect("an address"),
            label: "Something".to_owned(),
            required: false,
            kind,
            options: Vec::new(),
        }
    }

    fn refused(fields: Vec<Field>) -> &'static str {
        Declared::checked(fields)
            .expect_err("a refusal")
            .said()
            .expect("a sentence")
            .key
    }

    #[test]
    fn a_form_asks_each_thing_once() {
        // Two fields with one key is one column in the answers, and whichever
        // of the two is checked second is a rule nothing enforces.
        let twice = vec![asking("email", Kind::Email), asking("email", Kind::Text)];

        assert_eq!(refused(twice), A_FORM_ASKS_EACH_THING_ONCE);
    }

    #[test]
    fn a_choice_with_nothing_to_choose_from_is_refused_when_it_is_written() {
        // It would otherwise be refused every time somebody fills the form in,
        // which is a hundred refusals nobody can act on instead of one that
        // whoever made the form can.
        assert_eq!(
            refused(vec![asking("colour", Kind::Choice)]),
            A_CHOICE_NEEDS_SOMETHING_TO_CHOOSE_FROM
        );
    }

    #[test]
    fn options_on_something_that_is_not_a_choice_are_a_mistake_worth_saying() {
        let mut text = asking("name", Kind::Text);
        text.options = vec!["one".to_owned()];

        assert_eq!(refused(vec![text]), ONLY_A_CHOICE_HAS_OPTIONS);
    }

    #[test]
    fn a_form_is_bounded_at_both_ends() {
        let many: Vec<Field> = (0..=AT_MOST_FIELDS)
            .map(|n| asking(&format!("field-{n}"), Kind::Text))
            .collect();

        assert_eq!(refused(many), A_FORM_ASKS_AT_MOST_SO_MANY_THINGS);

        let mut choice = asking("colour", Kind::Choice);
        choice.options = (0..=AT_MOST_OPTIONS).map(|n| n.to_string()).collect();

        assert_eq!(refused(vec![choice]), A_CHOICE_OFFERS_AT_MOST_SO_MANY);
    }

    #[test]
    fn a_label_nobody_can_read_is_not_a_label() {
        let mut blank = asking("name", Kind::Text);
        blank.label = "   ".to_owned();

        assert_eq!(refused(vec![blank]), A_LABEL_IS_BETWEEN_ONE_AND_TWO_HUNDRED);
    }

    #[test]
    fn a_form_that_asks_for_nothing_is_a_form() {
        // Declaring nothing is allowed. What it accepts is the interesting
        // half, and that is decided where a submission is checked rather than
        // here: it accepts nothing, which is not what it used to do.
        assert!(Declared::checked(Vec::new()).expect("a form").is_empty());
    }
}
