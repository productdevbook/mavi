//! What is being worked on.
//!
//! A board is stages and cards: the site's own work, or the deals it is
//! chasing, or whatever else somebody has decided to keep track of. It is the
//! smallest domain here, and the only interesting thing in it is [`place`] —
//! where a card sits, and what happens when the numbers run out.

pub mod place;
pub mod store;

use mavi_api::{Answers, Endpoint, Is, Method, Parameter, Who};
use mavi_core::error::Code;
use mavi_core::grant::{Access, Needs};
use mavi_core::id;
use mavi_core::page::{Key, Keyset, Kind};

pub use place::{between, spread};

id!(
    /// One board.
    BoardId
);

id!(
    /// One column of one.
    StageId
);

id!(
    /// One card.
    CardId
);

pub const BOARDS: &str = "boards";

#[must_use]
pub const fn to_read() -> Needs {
    Needs::new(BOARDS, Access::View)
}

#[must_use]
pub const fn to_write() -> Needs {
    Needs::new(BOARDS, Access::Write)
}

/// A card's own order within its stage. Not by when it was made: a board is
/// the one place where the order is the whole point, and it is what somebody
/// dragged rather than what a clock said.
/// Ascending, because a board is read from the top: the smallest number is
/// the card at the top of its column. Every other listing here is newest
/// first, which is why this one says which way it goes.
pub const BY_PLACE: Keyset = Keyset(&[
    Key::oldest("place", Kind::Number),
    Key::oldest("id", Kind::Id),
]);

pub const BY_RECENT: Keyset = Keyset(&[
    Key::newest("created_at", Kind::Moment),
    Key::newest("id", Kind::Id),
]);

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    let mut all = the_boards();
    all.extend(the_cards());
    all
}

/// The boards themselves.
fn the_boards() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/boards",
            named: "boards.list",
            about: "The boards this site keeps.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: None,
            answers: Answers::With("BoardList"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/boards",
            named: "boards.make",
            about: "Makes one, with the stages it starts with.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: Some("NewBoard"),
            answers: Answers::Made("Board"),
            refuses: &[],
            changes: true,
        },
        Endpoint {
            method: Method::Get,
            path: "/api/boards/{id}",
            named: "boards.read",
            about: "One board: its stages, and the cards in each, in order.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which board.")],
            takes: None,
            answers: Answers::With("Board"),
            refuses: &[Code::NotFound],
            changes: false,
        },
    ]
}

/// What is on them.
fn the_cards() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/boards/{id}/cards",
            named: "cards.list",
            about: "The cards on one board, in the order somebody put them in.",
            who: Who::AnAccount,
            parameters: vec![
                Parameter::path("id", Is::Id, "Which board."),
                Parameter::query("stage", Is::Id, "Only this column."),
                Parameter::query("after", Is::Text, "The cursor the last page ended with."),
                Parameter::query("limit", Is::Number, "How many, at most a hundred."),
            ],
            takes: None,
            answers: Answers::With("CardPage"),
            refuses: &[Code::NotFound],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/boards/{id}/cards",
            named: "cards.make",
            about: "Puts a card on a board.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which board.")],
            takes: Some("NewCard"),
            answers: Answers::Made("Card"),
            refuses: &[Code::NotFound],
            changes: true,
        },
        Endpoint {
            method: Method::Patch,
            path: "/api/cards/{id}",
            named: "cards.change",
            about: "Changes what a card says, or whose it is.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which card.")],
            takes: Some("CardChanges"),
            answers: Answers::With("Card"),
            refuses: &[Code::NotFound],
            changes: true,
        },
        Endpoint {
            method: Method::Put,
            path: "/api/cards/{id}/place",
            named: "cards.move",
            about: "Drags a card: which stage, and between which two cards.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which card.")],
            // Its neighbours rather than a number: what a person did is drop
            // it between two cards, and the number is this crate's business.
            takes: Some("Between"),
            answers: Answers::With("Card"),
            // `Conflict` when there is no room left between those two, which
            // is answered by spreading the stage out and asking again.
            refuses: &[Code::NotFound, Code::Conflict],
            changes: true,
        },
        Endpoint {
            method: Method::Delete,
            path: "/api/cards/{id}",
            named: "cards.remove",
            about: "Takes a card off.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which card.")],
            takes: None,
            answers: Answers::Nothing,
            refuses: &[Code::NotFound],
            changes: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use mavi_api::Api;

    #[test]
    fn everything_this_domain_answers_is_described_completely() {
        let holes = Api::of(endpoints()).holes();

        assert!(holes.is_empty(), "{holes:#?}");
    }

    #[test]
    fn no_two_of_these_are_the_same_route() {
        assert!(Api::of(endpoints()).clashes().is_empty());
    }

    #[test]
    fn a_card_is_moved_by_saying_where_it_landed_rather_than_by_a_number() {
        // What a person did is drop it between two cards. The number is this
        // crate's business, and a caller that sends one is a caller that has
        // to know when the numbers have run out.
        let moving = endpoints()
            .into_iter()
            .find(|e| e.named == "cards.move")
            .expect("a way to move one");

        assert_eq!(moving.takes, Some("Between"));
        assert!(moving.refuses.contains(&Code::Conflict));
    }

    #[test]
    fn a_board_is_read_from_the_top() {
        // The one listing in this workspace that is not newest first, so the
        // one where getting the direction wrong draws every board upside down.
        assert!(
            BY_PLACE
                .keys()
                .iter()
                .all(|key| key.direction == mavi_core::page::Direction::Oldest)
        );
    }

    #[test]
    fn what_this_domain_asks_for_is_a_capability_the_site_has() {
        assert!(mavi_people::is_a_capability(BOARDS));
    }
}
