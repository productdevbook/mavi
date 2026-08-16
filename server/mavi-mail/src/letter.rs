//! What a site's own letters say.
//!
//! The invitation, the password link, the receipt. Written by this machine
//! until a site says otherwise, and in the site's own words when it does — a
//! receipt that says somebody else's name, in a language the reader does not
//! have, is a receipt that looks like a mistake.

use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use serde::{Deserialize, Serialize};

pub const NOTHING_SENDS_A_LETTER_LIKE_THAT: &str = "nothing_sends_a_letter_like_that";
pub const THAT_LETTER_CANNOT_SAY_THAT: &str = "that_letter_cannot_say_that";
pub const A_SUBJECT_IS_BETWEEN_ONE_AND_THREE_HUNDRED: &str =
    "a_subject_is_between_one_and_three_hundred";
pub const A_LETTER_IS_BETWEEN_ONE_AND_TWENTY_THOUSAND: &str =
    "a_letter_is_between_one_and_twenty_thousand";
pub const THAT_LETTER_HAS_A_HOLE_IN_IT: &str = "that_letter_has_a_hole_in_it";

/// Every letter this machine sends one person.
///
/// A closed list, and that is the point: a wording can be written for a kind
/// that is here and for nothing else, so the table cannot fill up with wordings
/// for letters nothing sends — which is what happens when the kind is a free
/// string and somebody renames the thing that sent it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Invitation,
    ForgottenPassword,
    AnAddressToProve,
    OrderPaid,
    OrderOnItsWay,
    PutOnACourse,
}

pub const KINDS: &[Kind] = &[
    Kind::Invitation,
    Kind::ForgottenPassword,
    Kind::AnAddressToProve,
    Kind::OrderPaid,
    Kind::OrderOnItsWay,
    Kind::PutOnACourse,
];

impl Kind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Kind::Invitation => "invitation",
            Kind::ForgottenPassword => "forgotten_password",
            Kind::AnAddressToProve => "an_address_to_prove",
            Kind::OrderPaid => "order_paid",
            Kind::OrderOnItsWay => "order_on_its_way",
            Kind::PutOnACourse => "put_on_a_course",
        }
    }

    pub fn parse(name: &str) -> Result<Self> {
        KINDS
            .iter()
            .copied()
            .find(|kind| kind.as_str() == name)
            .ok_or_else(|| Error::invalid(Say::of(NOTHING_SENDS_A_LETTER_LIKE_THAT)))
    }

    /// What this letter may name.
    ///
    /// Anything else in a wording is a hole in somebody's receipt, so it is
    /// refused when the wording is written rather than printed as `{total}` to
    /// a customer.
    #[must_use]
    pub const fn names(self) -> &'static [&'static str] {
        match self {
            Kind::Invitation
            | Kind::ForgottenPassword
            | Kind::AnAddressToProve
            | Kind::OrderOnItsWay
            | Kind::PutOnACourse => &["name", "site", "link"],
            // Every one of these is written out rather than left to a
            // wildcard: a kind added later gets a list somebody chose for it,
            // instead of whatever the last arm happened to say.
            Kind::OrderPaid => &["name", "site", "link", "total"],
        }
    }

    /// What it says when nobody has said otherwise.
    #[must_use]
    pub const fn subject(self) -> &'static str {
        match self {
            Kind::Invitation => "You have been invited",
            Kind::ForgottenPassword => "A new password",
            Kind::AnAddressToProve => "Confirm your address",
            Kind::OrderPaid => "Your order",
            Kind::OrderOnItsWay => "Your order is on its way",
            Kind::PutOnACourse => "Your course",
        }
    }

    #[must_use]
    pub const fn body(self) -> &'static str {
        match self {
            Kind::Invitation => {
                "Hello {name},\n\nSomebody has invited you to {site}. Choose a password \
                 here within three days:\n\n{link}\n"
            }
            Kind::ForgottenPassword => {
                "Hello {name},\n\nFollow the link below to choose a new password for \
                 {site}. If you did not ask for this, you can ignore this letter.\n\n{link}\n"
            }
            Kind::AnAddressToProve => {
                "Hello {name},\n\nConfirm this is your address for {site} by following \
                 the link below within three days:\n\n{link}\n"
            }
            Kind::OrderPaid => {
                "Hello {name},\n\nThank you — your order is paid for, {total} in all. \
                 You can see it here:\n\n{link}\n\n{site}\n"
            }
            Kind::OrderOnItsWay => {
                "Hello {name},\n\nYour order is on its way.\n\n{link}\n\n{site}\n"
            }
            Kind::PutOnACourse => {
                "Hello {name},\n\nYou have been put on a course at {site}. Sign in \
                 here:\n\n{link}\n"
            }
        }
    }
}

/// What a site says instead, for one kind in one language.
///
/// One row per language rather than one per kind, so a site writing Turkish
/// does not lose its English.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Wording {
    pub kind: Kind,
    pub language: String,
    pub subject: String,
    pub body: String,
}

impl Wording {
    /// Everything checked before it is written, including the one thing a
    /// database cannot check: that the letter names only what its kind has.
    pub fn checked(kind: Kind, language: &str, subject: &str, body: &str) -> Result<Self> {
        if !(1..=300).contains(&subject.trim().chars().count()) {
            return Err(Error::invalid(Say::of(
                A_SUBJECT_IS_BETWEEN_ONE_AND_THREE_HUNDRED,
            )));
        }

        if !(1..=20_000).contains(&body.trim().chars().count()) {
            return Err(Error::invalid(Say::of(
                A_LETTER_IS_BETWEEN_ONE_AND_TWENTY_THOUSAND,
            )));
        }

        for named in named_in(subject).chain(named_in(body)) {
            if !kind.names().contains(&named) {
                return Err(Error::invalid(
                    Say::of(THAT_LETTER_CANNOT_SAY_THAT).with("named", &named),
                ));
            }
        }

        Ok(Self {
            kind,
            language: language.to_owned(),
            subject: subject.to_owned(),
            body: body.to_owned(),
        })
    }

    /// This machine's own wording for a kind, which is what a site gets until
    /// it writes its own.
    #[must_use]
    pub fn ours(kind: Kind) -> Self {
        Self {
            kind,
            language: "en".to_owned(),
            subject: kind.subject().to_owned(),
            body: kind.body().to_owned(),
        }
    }
}

/// One letter, ready to send.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Pressed {
    pub subject: String,
    pub body: String,
}

/// Puts the values in, and refuses to send a letter with a hole in it.
///
/// The crate this replaces left a name it had no value for exactly as it was,
/// on the grounds that an obvious hole says more than a blank. It does — to
/// whoever wrote the letter. To the person who received one saying "follow the
/// link below: {link}" it says the site is broken, and they cannot act on it
/// either way. A hole is this machine's mistake, so this machine hears about
/// it instead.
pub fn press(wording: &Wording, values: &[(&str, String)]) -> Result<Pressed> {
    let filled = |text: &str| -> Result<String> {
        let mut out = text.to_owned();

        for (name, value) in values {
            out = out.replace(&format!("{{{name}}}"), value);
        }

        if let Some(hole) = named_in(&out).next() {
            return Err(Error::internal(std::io::Error::other(format!(
                "a {} letter went out with {{{hole}}} still in it",
                wording.kind.as_str()
            ))));
        }

        Ok(out)
    };

    Ok(Pressed {
        subject: filled(&wording.subject)?,
        body: filled(&wording.body)?,
    })
}

/// The `{names}` a piece of wording carries.
fn named_in(text: &str) -> impl Iterator<Item = &str> {
    text.split('{')
        .skip(1)
        .filter_map(|rest| rest.split_once('}'))
        .map(|(name, _)| name)
        .filter(|name| {
            !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_letter_may_only_name_what_its_kind_has() {
        // `{total}` in an invitation is a hole nobody can fill, and the person
        // who finds out is whoever was invited.
        let refused = Wording::checked(
            Kind::Invitation,
            "en",
            "You have been invited",
            "Hello {name}, you owe {total}.",
        )
        .expect_err("a refusal");

        assert_eq!(
            refused.said().expect("a sentence").key,
            THAT_LETTER_CANNOT_SAY_THAT
        );
    }

    #[test]
    fn every_letter_this_machine_writes_can_be_said_in_full() {
        // Each kind's own wording against its own list of names. This is what
        // stops a default letter shipping with a hole in it, which nothing
        // else would catch until it was in somebody's inbox.
        for kind in KINDS {
            let ours = Wording::ours(*kind);

            assert!(
                Wording::checked(*kind, "en", &ours.subject, &ours.body).is_ok(),
                "{} says something its kind does not have",
                kind.as_str()
            );

            let values: Vec<(&str, String)> = kind
                .names()
                .iter()
                .map(|name| (*name, format!("<{name}>")))
                .collect();

            let pressed = press(&ours, &values).expect("a letter");

            assert!(!pressed.body.contains('{'), "{}", pressed.body);
        }
    }

    #[test]
    fn a_letter_with_a_hole_in_it_does_not_go_out() {
        let ours = Wording::ours(Kind::ForgottenPassword);

        // Everything but the one thing the letter is for.
        let missing = press(&ours, &[("name", "A Person".to_owned())]).expect_err("a refusal");

        assert!(missing.said().is_none(), "a hole is nobody else's business");
    }

    #[test]
    fn a_kind_nothing_sends_cannot_be_written_for() {
        assert!(Kind::parse("order_paid").is_ok());
        assert!(Kind::parse("whatever_somebody_typed").is_err());
    }

    #[test]
    fn a_brace_that_is_not_a_name_is_not_a_hole() {
        // A body may be a template somebody else writes, and `{ }` in it is
        // not a hole this crate has to fill.
        let wording = Wording::checked(
            Kind::OrderOnItsWay,
            "en",
            "Your order",
            "Hello {name}, the shape is { }, and {Not A Name} stays.",
        )
        .expect("a wording");

        let pressed = press(&wording, &[("name", "A Person".to_owned())]).expect("a letter");

        assert!(pressed.body.contains("{Not A Name}"));
    }
}
