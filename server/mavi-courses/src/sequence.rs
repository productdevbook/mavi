//! The order things are in.
//!
//! A course is modules in an order and a module is lessons in an order, and
//! the order is the thing a teacher spends their time on. So it is a number
//! per row, unique within its parent — and that pair of decisions is where the
//! trouble is.
//!
//! Swapping two lessons means writing one of them into a place the other is
//! still in. With an ordinary unique constraint that is refused half way
//! through, so every reorder becomes a dance: move one to minus one, move the
//! other up, move the first back. Three writes, and a crash in the middle
//! leaves a lesson at minus one.
//!
//! The constraint is `deferrable initially deferred` instead. Postgres checks
//! it when the transaction commits rather than at each row, so a reorder is
//! one statement that says what the new order is, and a duplicate is still
//! refused — just at the end, when the answer is actually known.

use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use uuid::Uuid;

pub const THAT_IS_NOT_THE_SAME_THINGS_IN_A_NEW_ORDER: &str =
    "that_is_not_the_same_things_in_a_new_order";

/// Where each thing goes, given the order somebody dragged them into.
///
/// Takes what is there now and what the new order is, and refuses anything
/// that is not a rearrangement of it — a list with something missing is a
/// lesson quietly dropped out of a course, and a list with something extra is
/// a lesson from somebody else's course being pulled into this one.
pub fn in_this_order(there_now: &[Uuid], new_order: &[Uuid]) -> Result<Vec<(Uuid, i32)>> {
    let same_things = {
        let mut before = there_now.to_vec();
        let mut after = new_order.to_vec();
        before.sort_unstable();
        after.sort_unstable();

        before == after
    };

    if !same_things {
        return Err(Error::invalid(Say::of(
            THAT_IS_NOT_THE_SAME_THINGS_IN_A_NEW_ORDER,
        )));
    }

    Ok(new_order
        .iter()
        .enumerate()
        .map(|(at, id)| (*id, i32::try_from(at).unwrap_or(i32::MAX)))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn things(how_many: u128) -> Vec<Uuid> {
        (1..=how_many).map(Uuid::from_u128).collect()
    }

    #[test]
    fn a_new_order_is_the_same_things_somewhere_else() {
        let there = things(3);
        let dragged = vec![there[2], there[0], there[1]];

        let places = in_this_order(&there, &dragged).expect("an order");

        assert_eq!(places, vec![(there[2], 0), (there[0], 1), (there[1], 2)]);
    }

    #[test]
    fn a_lesson_left_out_of_the_new_order_is_not_a_reorder() {
        // It is a lesson quietly dropped out of a course, and the person who
        // finds out is a student who cannot get to it.
        let there = things(3);
        let missing = vec![there[0], there[1]];

        assert_eq!(
            in_this_order(&there, &missing)
                .expect_err("a refusal")
                .said()
                .expect("a sentence")
                .key,
            THAT_IS_NOT_THE_SAME_THINGS_IN_A_NEW_ORDER
        );
    }

    #[test]
    fn something_from_somewhere_else_cannot_be_dragged_in() {
        let there = things(2);
        let mut smuggled = there.clone();
        smuggled.push(Uuid::from_u128(99));

        assert!(in_this_order(&there, &smuggled).is_err());
    }

    #[test]
    fn the_same_thing_twice_is_not_a_reorder_either() {
        let there = things(2);
        let doubled = vec![there[0], there[0]];

        assert!(in_this_order(&there, &doubled).is_err());
    }
}
