//! Walking what a site has written.
//!
//! Two orders, and each one is a [`Keyset`] rather than an `order by` written
//! into a query. That is the whole reason this file is short: the order, the
//! cursor and the index all read the same declaration, so the three cannot
//! drift apart.
//!
//! There is an index in the schema matching each of these, column for column,
//! in the same order. An index that does not match the order is an index the
//! planner ignores, and the listing walks the table instead — which nothing
//! reports, because the answer is still correct.

use mavi_core::page::{Key, Keyset, Kind as Sorts};
use serde::Deserialize;

/// The panel's order: everything, newest written first.
pub const BY_RECENT: Keyset = Keyset(&[
    Key::newest("created_at", Sorts::Moment),
    Key::newest("id", Sorts::Id),
]);

/// A feed's order: what is out, most recently published first.
pub const BY_FEED: Keyset = Keyset(&[
    Key::newest("published_at", Sorts::Moment),
    Key::newest("id", Sorts::Id),
]);

/// What a caller narrows a listing by.
///
/// Every one of these is exact. There is no free-text search, here or
/// anywhere — see the issue about it rather than adding a `like` to this
/// struct, because a `like '%word%'` over a growing table is the thing that
/// looks like search until the day it is the reason a page takes four seconds.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Filter {
    pub kind: Option<String>,
    pub language: Option<String>,
    pub state: Option<String>,
}

impl Filter {
    /// The `where` this narrows to, and what to bind, numbered from `from`.
    ///
    /// Returned as pieces rather than as a finished query because the listing
    /// also has a cursor predicate to place, and one function assembling both
    /// is one function that can get the numbering wrong.
    #[must_use]
    pub fn narrows(&self, from: usize) -> (Vec<String>, Vec<String>) {
        let mut sql = Vec::new();
        let mut binds = Vec::new();

        for (column, value) in [
            ("kind", self.kind.as_ref()),
            ("language", self.language.as_ref()),
            ("state", self.state.as_ref()),
        ] {
            if let Some(value) = value {
                sql.push(format!("{column} = ${}", from + binds.len()));
                binds.push(value.clone());
            }
        }

        (sql, binds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_order_ends_with_something_unique() {
        // The rule the whole cursor design rests on. Without a unique last
        // column, two rows written in the same transaction share a position
        // and the cursor cannot tell them apart — which is how a listing
        // starts skipping whole batches.
        for keyset in [BY_RECENT, BY_FEED] {
            let last = keyset.keys().last().expect("a key");
            assert_eq!(last.column, "id", "an order that cannot break a tie");
        }
    }

    #[test]
    fn narrowing_numbers_its_binds_from_where_it_was_told() {
        let filter = Filter {
            kind: Some("post".into()),
            language: None,
            state: Some("published".into()),
        };

        let (sql, binds) = filter.narrows(3);

        assert_eq!(sql, vec!["kind = $3".to_owned(), "state = $4".to_owned()]);
        assert_eq!(binds, vec!["post".to_owned(), "published".to_owned()]);
    }

    #[test]
    fn narrowing_by_nothing_narrows_nothing() {
        let (sql, binds) = Filter::default().narrows(1);

        assert!(sql.is_empty());
        assert!(binds.is_empty());
    }
}
