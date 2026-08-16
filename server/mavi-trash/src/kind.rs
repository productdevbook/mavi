//! What sort of thing was thrown away.
//!
//! **A closed list, and the only place a table name comes from.** Everything
//! here is reached by a kind somebody sent in an address, and the one way to
//! write this wrongly is to put that string into a query. So a kind is parsed
//! into this type, and the table and the column come off the type — a name
//! nobody outside this file chose.

use mavi_core::error::{Error, Result};
use mavi_core::say::Say;

pub const THAT_IS_NOT_A_SORT_OF_THING: &str = "that_is_not_a_sort_of_thing";

/// Everything a site can throw away and get back.
///
/// Not every table with a `deleted_at` on it. What is here is what somebody
/// makes and might unmake by mistake; an order, a session and a ticket are
/// kept for their own reasons and are not things anybody restores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Writings,
    Files,
    Terms,
    Forms,
    Products,
    Courses,
    Boards,
    Cards,
    Flows,
}

/// Every one, for the listing that shows all of them and for the test that
/// asks whether each can actually be reached.
pub const EVERY: &[Kind] = &[
    Kind::Writings,
    Kind::Files,
    Kind::Terms,
    Kind::Forms,
    Kind::Products,
    Kind::Courses,
    Kind::Boards,
    Kind::Cards,
    Kind::Flows,
];

impl Kind {
    pub fn parse(said: &str) -> Result<Self> {
        EVERY
            .iter()
            .copied()
            .find(|kind| kind.as_str() == said)
            .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_A_SORT_OF_THING).with("sort", &said)))
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Kind::Writings => "writings",
            Kind::Files => "files",
            Kind::Terms => "terms",
            Kind::Forms => "forms",
            Kind::Products => "products",
            Kind::Courses => "courses",
            Kind::Boards => "boards",
            Kind::Cards => "cards",
            Kind::Flows => "flows",
        }
    }

    /// Which table. **From the enum, never from what somebody sent.**
    #[must_use]
    pub const fn table(self) -> &'static str {
        // The same word today, and written out anyway: a kind is what an
        // address says and a table is what the schema calls it, and the day
        // one of those moves this is where it moves.
        match self {
            Kind::Writings => "writings",
            Kind::Files => "files",
            Kind::Terms => "terms",
            Kind::Forms => "forms",
            Kind::Products => "products",
            Kind::Courses => "courses",
            Kind::Boards => "boards",
            Kind::Cards => "cards",
            Kind::Flows => "flows",
        }
    }

    /// What to show somebody so they know which one it is.
    ///
    /// Different per table because a writing has a title, a file has a name
    /// and a term has both — and a trash screen listing nine rows that all say
    /// the same thing is a screen nobody can restore from.
    #[must_use]
    pub const fn called(self) -> &'static str {
        match self {
            Kind::Writings | Kind::Courses | Kind::Cards => "title",
            Kind::Files
            | Kind::Terms
            | Kind::Forms
            | Kind::Products
            | Kind::Boards
            | Kind::Flows => "name",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_is_never_something_somebody_sent() {
        // The whole point of this file. What arrives is a word in an address;
        // what reaches a query is a `&'static str` off an enum.
        assert!(Kind::parse("writings; drop table writings").is_err());
        assert!(
            Kind::parse("people").is_err(),
            "an account is not restored here"
        );
        assert!(Kind::parse("").is_err());

        assert_eq!(Kind::parse("writings").expect("a kind").table(), "writings");
    }

    #[test]
    fn every_kind_can_be_asked_for_by_the_name_it_answers_to() {
        for kind in EVERY {
            assert_eq!(Kind::parse(kind.as_str()).expect("itself"), *kind);
        }
    }
}
