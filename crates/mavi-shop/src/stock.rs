//! What is on the shelf, and the order it is reached for in.
//!
//! Two people checking out at the same moment take rows in whatever order
//! their baskets happen to list them. One basket says [salt, pepper] and the
//! other says [pepper, salt]; each locks its first row, each waits for the
//! other's, and neither ever gets it. Postgres notices after a second and
//! kills one of them — so one shopper sees the shop break for no reason they
//! could ever describe, and it happens more the busier the shop is.
//!
//! The fix is not a bigger lock. It is that everybody reaches in the same
//! order, and the order is any order at all as long as it is the same one:
//! by id, because an id is the one thing every basket agrees on.

use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const NOT_THAT_MANY_LEFT: &str = "not_that_many_left";
pub const ASKING_FOR_ONE_THING_TWICE: &str = "asking_for_one_thing_twice";

/// One thing somebody wants, and how many.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wanted {
    pub product: Uuid,
    pub how_many: u32,
}

/// What a basket asks for, in the order every basket asks for it.
///
/// Sorted by id, and deduplicated on the way: two lines for one product is a
/// basket that would lock the same row twice and add up wrong, and it is what
/// pressing "add" twice looks like from here. Asking for three and then two
/// more is a line saying five.
pub fn reached_for(wanted: &[Wanted]) -> Result<Vec<Wanted>> {
    let mut in_order = wanted.to_vec();
    in_order.sort_unstable_by_key(|one| one.product);

    let mut together: Vec<Wanted> = Vec::with_capacity(in_order.len());

    for one in in_order {
        match together.last_mut() {
            Some(before) if before.product == one.product => {
                before.how_many = before
                    .how_many
                    .checked_add(one.how_many)
                    .ok_or_else(|| Error::invalid(Say::of(ASKING_FOR_ONE_THING_TWICE)))?;
            }
            _ => together.push(one),
        }
    }

    Ok(together)
}

/// Whether there are that many.
///
/// Answered against the row that has been locked, never against one read a
/// moment earlier: between the reading and the taking is exactly where the
/// last one of something gets sold twice.
pub fn enough(name: &str, on_the_shelf: i32, how_many: u32) -> Result<()> {
    let asked = i64::from(how_many);

    if i64::from(on_the_shelf) >= asked {
        return Ok(());
    }

    Err(Error::conflict(
        Say::of(NOT_THAT_MANY_LEFT)
            .with("name", &name)
            .with("left", &on_the_shelf),
    ))
}

/// How long stock is held for a checkout nobody has paid for yet.
///
/// Long enough for somebody to find their card, short enough that a basket
/// abandoned at lunchtime is not still holding the last one of something at
/// closing.
pub const HELD_FOR_MINUTES: i64 = 30;

#[cfg(test)]
mod tests {
    use super::*;

    fn thing(n: u128, how_many: u32) -> Wanted {
        Wanted {
            product: Uuid::from_u128(n),
            how_many,
        }
    }

    #[test]
    fn two_baskets_of_the_same_things_reach_for_them_in_the_same_order() {
        // The whole point. These two baskets deadlocked; now they queue.
        let salt_then_pepper = vec![thing(1, 1), thing(2, 1)];
        let pepper_then_salt = vec![thing(2, 1), thing(1, 1)];

        assert_eq!(
            reached_for(&salt_then_pepper).expect("an order"),
            reached_for(&pepper_then_salt).expect("an order"),
        );
    }

    #[test]
    fn asking_for_three_and_then_two_more_is_a_line_saying_five() {
        let twice = vec![thing(1, 3), thing(2, 1), thing(1, 2)];

        let together = reached_for(&twice).expect("an order");

        assert_eq!(together.len(), 2);
        assert_eq!(together[0].how_many, 5);
    }

    #[test]
    fn there_are_that_many_or_there_are_not() {
        assert!(enough("Salt", 3, 3).is_ok());
        assert!(enough("Salt", 3, 0).is_ok());

        let refused = enough("Salt", 2, 3).expect_err("a refusal");
        let said = refused.said().expect("a sentence");

        assert_eq!(said.key, NOT_THAT_MANY_LEFT);
        // What is left is named, because "no" alone makes somebody guess how
        // many times to press the button.
        assert_eq!(said.named.get("left").map(String::as_str), Some("2"));
    }

    #[test]
    fn asking_for_more_than_a_number_holds_is_refused_rather_than_wrapped() {
        let absurd = vec![thing(1, u32::MAX), thing(1, u32::MAX)];

        assert_eq!(
            reached_for(&absurd)
                .expect_err("a refusal")
                .said()
                .expect("a sentence")
                .key,
            ASKING_FOR_ONE_THING_TWICE
        );
    }
}
