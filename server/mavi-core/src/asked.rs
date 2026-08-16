//! A list of things somebody is asked for, declared.
//!
//! Two things in this software declare what they want and then take whatever
//! arrives: a **form**, which anybody fills in, and a **kind of writing**,
//! whose own fields a site decides. They are the same idea, so they are one
//! vocabulary rather than two that drift — and it lives here, because a domain
//! reaching into another domain for a type is the thing crate boundaries are
//! for.
//!
//! The list is checked **when it is declared**, not when it is filled in. That
//! order is the whole design: a declaration is made once by somebody signed
//! in, and answered any number of times by anybody. Every question settled at
//! declaration is a question not asked on a public endpoint.

use crate::error::{Error, Result};
use crate::say::Say;
use crate::slug::Slug;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const IT_ASKS_AT_MOST_SO_MANY_THINGS: &str = "a_form_asks_at_most_so_many_things";
pub const IT_WANTS_THAT_FIELD: &str = "that_form_wants_that_field";
pub const THAT_IS_NOT_WHAT_THAT_FIELD_HOLDS: &str = "that_is_not_what_that_field_holds";
pub const IT_HAS_NO_SUCH_FIELD: &str = "that_form_has_no_such_field";
pub const THAT_IS_MORE_THAN_IT_TAKES: &str = "that_is_more_than_a_form_takes";

/// How much one answer may weigh, all of it together.
pub const AT_MOST_ALTOGETHER: usize = 64 * 1024;
pub const IT_ASKS_EACH_THING_ONCE: &str = "a_form_asks_each_thing_once";
pub const A_LABEL_IS_BETWEEN_ONE_AND_TWO_HUNDRED: &str = "a_label_is_between_one_and_two_hundred";
pub const A_CHOICE_NEEDS_SOMETHING_TO_CHOOSE_FROM: &str = "a_choice_needs_something_to_choose_from";
pub const ONLY_A_CHOICE_HAS_OPTIONS: &str = "only_a_choice_has_options";
pub const A_CHOICE_OFFERS_AT_MOST_SO_MANY: &str = "a_choice_offers_at_most_so_many";

/// How many things one declaration may ask for.
///
/// A limit exists because one with none is a `jsonb` column whose size
/// whoever declared it decides, checked in a loop on every answer.
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
                Say::of(IT_ASKS_AT_MOST_SO_MANY_THINGS).with("at_most", &AT_MOST_FIELDS),
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
                    Say::of(IT_ASKS_EACH_THING_ONCE).with("field", &field.key.as_str()),
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

/// What arrived, held against what was asked for.
///
/// The order is not decoration. Size first, because it is the only check whose
/// cost does not depend on how big the thing is; then the fields that were
/// declared, so a refusal names what is missing; then anything sent that was
/// never asked for.
///
/// Written here rather than beside a form, because the second thing that
/// declares a list and then takes whatever arrives — a site's own kind of
/// writing — has to check it the same way. Two copies of this would be two
/// answers to whether an email is an email.
pub fn fits(answers: &Map<String, Value>, declared: &Declared) -> Result<()> {
    weighs(answers)?;

    for field in declared.fields() {
        let given = answers.get(field.key.as_str());

        let empty = match given {
            None | Some(Value::Null) => true,
            Some(Value::String(text)) => text.trim().is_empty(),
            Some(_) => false,
        };

        if empty {
            if field.required {
                return Err(Error::invalid(
                    Say::of(IT_WANTS_THAT_FIELD).with("field", &field.key.as_str()),
                ));
            }

            continue;
        }

        let Some(value) = given else { continue };

        let holds = match field.kind {
            Kind::Text | Kind::Long => value.is_string(),
            Kind::Email => value
                .as_str()
                .is_some_and(|written| crate::email::Email::parse(written).is_ok()),
            Kind::Number => value.is_number(),
            Kind::Boolean => value.is_boolean(),
            Kind::Choice => value
                .as_str()
                .is_some_and(|written| field.options.iter().any(|one| one == written)),
        };

        if !holds {
            return Err(Error::invalid(
                Say::of(THAT_IS_NOT_WHAT_THAT_FIELD_HOLDS).with("field", &field.key.as_str()),
            ));
        }
    }

    // Anything never asked for. In the crate this replaces the whole check was
    // skipped when nothing had been declared — so the one shape where nothing
    // at all was declared was the one shape that accepted anything, and what
    // it accepted went in front of whoever reads the submissions.
    if let Some(unasked) = answers
        .keys()
        .find(|key| !declared.fields().iter().any(|f| f.key.as_str() == *key))
    {
        return Err(Error::invalid(
            Say::of(IT_HAS_NO_SUCH_FIELD).with("field", unasked),
        ));
    }

    Ok(())
}

/// How much of it there is, counted the way it will be stored.
///
/// A limit on each answer and none on the whole is not a limit: a hundred
/// answers of ten thousand characters each is a megabyte, and the endpoint
/// that takes a form takes it from anybody.
pub fn weighs(answers: &Map<String, Value>) -> Result<()> {
    let mut weight = 0_usize;

    for (key, value) in answers {
        weight += key.len();
        weight += match value {
            Value::String(text) => text.len(),
            other => other.to_string().len(),
        };

        if weight > AT_MOST_ALTOGETHER {
            return Err(Error::invalid(
                Say::of(THAT_IS_MORE_THAN_IT_TAKES).with("at_most", &AT_MOST_ALTOGETHER),
            ));
        }
    }

    Ok(())
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

        assert_eq!(refused(twice), IT_ASKS_EACH_THING_ONCE);
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

        assert_eq!(refused(many), IT_ASKS_AT_MOST_SO_MANY_THINGS);

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
