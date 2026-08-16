//! Where a card sits.
//!
//! Dragging a card between two others should be one row changed rather than
//! every row below it, so a place is a number between its neighbours' numbers
//! rather than a position in a list.
//!
//! That works until it does not, and the way it stops working is quiet. Each
//! drop between the same two cards halves the gap; after about fifty, the two
//! numbers are as close as a `double` can hold, the midpoint **is** one of the
//! endpoints, and two cards have the same place. Nothing fails. The board just
//! stops keeping the order somebody put it in, in a way that looks like the
//! browser being wrong.
//!
//! So [`between`] refuses when there is no room left, and whoever is holding
//! the cards spreads them out again with [`spread`]. Fifty drops between one
//! pair is rare; being unable to say what happened when it does is not
//! acceptable.

use mavi_core::error::{Error, Result};
use mavi_core::say::Say;

pub const THERE_IS_NO_ROOM_LEFT_THERE: &str = "there_is_no_room_left_there";

/// How far apart cards are put when a stage is spread out. Big enough that
/// halving it has a long way to go before it runs out.
pub const APART: f64 = 1024.0;

/// A place between these two, or a refusal.
///
/// `None` on either side means the end of the stage — dropping a card at the
/// top has nothing before it.
pub fn between(before: Option<f64>, after: Option<f64>) -> Result<f64> {
    let place = match (before, after) {
        (None, None) => 0.0,
        (Some(before), None) => before + APART,
        (None, Some(after)) => after - APART,
        (Some(before), Some(after)) => {
            // Halfway, written so that two enormous numbers do not add up to
            // something that is not a number at all.
            before + (after - before) / 2.0
        }
    };

    if !place.is_finite() {
        return Err(Error::conflict(Say::of(THERE_IS_NO_ROOM_LEFT_THERE)));
    }

    // The one that matters: the midpoint of two numbers a `double` cannot tell
    // apart is one of them, and two cards in one place is a board that quietly
    // stops keeping the order somebody put it in.
    let touching =
        before.is_some_and(|edge| place <= edge) || after.is_some_and(|edge| place >= edge);

    if touching {
        return Err(Error::conflict(Say::of(THERE_IS_NO_ROOM_LEFT_THERE)));
    }

    Ok(place)
}

/// Places for a whole stage, spread out again.
///
/// What is done when [`between`] says there is no room: the cards keep their
/// order and get room around them.
#[must_use]
pub fn spread(how_many: usize) -> Vec<f64> {
    (0..how_many)
        .map(|at| {
            #[expect(
                clippy::cast_precision_loss,
                reason = "a stage with more cards than a double counts exactly is not a stage"
            )]
            let at = at as f64;

            at * APART
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_dropped_between_two_others_sits_between_them() {
        let place = between(Some(0.0), Some(APART)).expect("a place");

        assert!(place > 0.0 && place < APART, "{place}");
    }

    #[test]
    fn a_card_dropped_at_either_end_has_room() {
        assert!(between(Some(0.0), None).expect("the bottom") > 0.0);
        assert!(between(None, Some(0.0)).expect("the top") < 0.0);
        assert!(between(None, None).is_ok());
    }

    #[test]
    fn dropping_between_the_same_two_cards_runs_out_and_says_so() {
        // Measured rather than assumed: this is the loop that used to end in
        // two cards holding the same number and a board that had quietly
        // stopped keeping anybody's order.
        let mut before = 0.0;
        let after = APART;
        let mut drops = 0;

        loop {
            match between(Some(before), Some(after)) {
                Ok(place) => {
                    assert!(place > before && place < after, "a place outside its gap");
                    before = place;
                    drops += 1;

                    assert!(drops < 200, "this should have run out long before now");
                }
                Err(refused) => {
                    assert_eq!(
                        refused.said().expect("a sentence").key,
                        THERE_IS_NO_ROOM_LEFT_THERE
                    );
                    break;
                }
            }
        }

        // Around fifty, which is the number of times a double can be halved
        // before the halves stop being different numbers.
        assert!(drops > 40, "it ran out after {drops}, which is too soon");
    }

    #[test]
    fn spreading_a_stage_out_gives_everything_room_again() {
        let places = spread(4);

        assert_eq!(places.len(), 4);
        assert!(places.windows(2).all(|two| two[1] - two[0] >= APART));

        // And a card can be dropped between any two of them again.
        assert!(between(Some(places[0]), Some(places[1])).is_ok());
    }

    #[test]
    fn nothing_that_is_not_a_number_comes_out() {
        assert!(between(Some(f64::MAX), None).is_err());
        assert!(between(None, Some(f64::MIN)).is_err());
    }
}
