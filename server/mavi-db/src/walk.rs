//! Turning a listing's order into SQL, and the only place that happens.
//!
//! A [`Keyset`] says what a listing is ordered by. This says what that is in
//! SQL — the `order by`, and the predicate that starts a page after a cursor —
//! and both come out of the same value, so they cannot disagree.
//!
//! That is the whole design. In the crate this replaces the two were written
//! separately, by hand, at each listing: fourteen of them ended up cursoring
//! on fewer columns than they ordered by, and the symptom was rows quietly
//! skipped rather than an error. Nobody writes those two clauses here.

use mavi_core::page::{Cursor, Direction, Key, Keyset, Kind};

/// A listing being walked: its order, and where this page starts.
#[derive(Clone, Debug)]
pub struct Walk {
    keyset: Keyset,
    after: Option<Cursor>,
}

impl Walk {
    #[must_use]
    pub const fn new(keyset: Keyset, after: Option<Cursor>) -> Self {
        Self { keyset, after }
    }

    /// `order by weight desc, created_at desc, id desc`, without the `order
    /// by` — callers put it where it belongs in their own query.
    #[must_use]
    pub fn order(&self) -> String {
        self.keyset
            .keys()
            .iter()
            .map(|key| format!("{} {}", key.column, way(key.direction)))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The predicate that starts this page after the cursor, and the values to
    /// bind, numbered from `from`.
    ///
    /// `None` when there is no cursor — the first page has nothing to be after.
    ///
    /// Where every column runs the same way this is a row comparison,
    /// `(a, b, c) < ($1, $2, $3)`, which `PostgreSQL` can walk an index with.
    /// Where they do not, it is the expanded form, which is longer and means
    /// exactly the same thing. Both are generated; neither is typed by anybody.
    #[must_use]
    pub fn after(&self, from: usize) -> Option<(String, Vec<String>)> {
        let cursor = self.after.as_ref()?;
        let keys = self.keyset.keys();
        let values = cursor.values();

        if keys.is_empty() {
            return None;
        }

        let placed: Vec<String> = keys
            .iter()
            .enumerate()
            .map(|(i, key)| format!("${}{}", from + i, cast(key.kind)))
            .collect();

        let one_way = keys.iter().all(|key| key.direction == keys[0].direction);

        let sql = if one_way {
            format!(
                "({}) {} ({})",
                keys.iter()
                    .map(|key| key.column)
                    .collect::<Vec<_>>()
                    .join(", "),
                past(keys[0].direction),
                placed.join(", ")
            )
        } else {
            // Every prefix equal, then the next one past. The rows this
            // describes are the same rows; only the planner can tell them
            // apart.
            let mut ors = Vec::with_capacity(keys.len());

            for at in 0..keys.len() {
                let mut and = Vec::with_capacity(at + 1);

                for (i, key) in keys.iter().enumerate().take(at) {
                    and.push(format!("{} = {}", key.column, placed[i]));
                }

                and.push(format!(
                    "{} {} {}",
                    keys[at].column,
                    past(keys[at].direction),
                    placed[at]
                ));

                ors.push(format!("({})", and.join(" and ")));
            }

            format!("({})", ors.join(" or "))
        };

        Some((sql, values.to_vec()))
    }
}

const fn way(direction: Direction) -> &'static str {
    match direction {
        Direction::Newest => "desc",
        Direction::Oldest => "asc",
    }
}

/// Past a row, in the direction the listing runs: newest-first walks
/// downwards, oldest-first walks up.
const fn past(direction: Direction) -> &'static str {
    match direction {
        Direction::Newest => "<",
        Direction::Oldest => ">",
    }
}

/// What the cursor's text has to be read back as before it can be compared.
/// A timestamp compared as text sorts `10:00` before `9:00`.
const fn cast(kind: Kind) -> &'static str {
    match kind {
        Kind::Text => "",
        Kind::Number => "::numeric",
        Kind::Moment => "::timestamptz",
        Kind::Id => "::uuid",
    }
}

/// The columns of a keyset, for a `select` that has to read back what a cursor
/// will be built from. Saves each listing naming them twice.
#[must_use]
pub fn columns(keyset: Keyset) -> Vec<&'static str> {
    keyset.keys().iter().map(|key: &Key| key.column).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NEWEST: Keyset = Keyset(&[
        Key::newest("created_at", Kind::Moment),
        Key::newest("id", Kind::Id),
    ]);

    const MIXED: Keyset = Keyset(&[
        Key::newest("weight", Kind::Number),
        Key::oldest("created_at", Kind::Moment),
        Key::newest("id", Kind::Id),
    ]);

    fn at(keyset: Keyset, values: &[&str]) -> Cursor {
        Cursor::at(keyset, values.iter().map(ToString::to_string).collect()).expect("a cursor")
    }

    #[test]
    fn the_order_and_the_predicate_name_the_same_columns() {
        // The assertion this file exists for. Not that the SQL reads well —
        // that the two clauses cannot be written to disagree, because both are
        // read from the one keyset.
        for keyset in [NEWEST, MIXED] {
            let walk = Walk::new(keyset, Some(at(keyset, &["a", "b", "c"][..keyset.len()])));
            let order = walk.order();
            let (predicate, _) = walk.after(1).expect("a predicate");

            for key in keyset.keys() {
                assert!(
                    order.contains(key.column),
                    "{} missing from the order",
                    key.column
                );
                assert!(
                    predicate.contains(key.column),
                    "{} missing from the predicate",
                    key.column
                );
            }
        }
    }

    #[test]
    fn one_direction_is_a_row_comparison() {
        let walk = Walk::new(NEWEST, Some(at(NEWEST, &["2026-01-01", "an-id"])));
        let (sql, values) = walk.after(1).expect("a predicate");

        assert_eq!(sql, "(created_at, id) < ($1::timestamptz, $2::uuid)");
        assert_eq!(values, vec!["2026-01-01".to_owned(), "an-id".to_owned()]);
        assert_eq!(walk.order(), "created_at desc, id desc");
    }

    #[test]
    fn two_directions_are_expanded_rather_than_wrong() {
        // A row comparison means nothing when the columns run different ways,
        // and writing one anyway is how a listing silently returns the wrong
        // half of itself.
        let walk = Walk::new(MIXED, Some(at(MIXED, &["3", "2026-01-01", "an-id"])));
        let (sql, _) = walk.after(1).expect("a predicate");

        assert_eq!(
            sql,
            "((weight < $1::numeric) \
             or (weight = $1::numeric and created_at > $2::timestamptz) \
             or (weight = $1::numeric and created_at = $2::timestamptz and id < $3::uuid))"
        );
        assert_eq!(walk.order(), "weight desc, created_at asc, id desc");
    }

    #[test]
    fn a_first_page_has_nothing_to_be_after() {
        assert!(Walk::new(NEWEST, None).after(1).is_none());
    }

    #[test]
    fn binds_are_numbered_from_where_the_caller_says() {
        let walk = Walk::new(NEWEST, Some(at(NEWEST, &["2026-01-01", "an-id"])));
        let (sql, _) = walk.after(4).expect("a predicate");

        assert!(sql.contains("$4::timestamptz"), "{sql}");
        assert!(sql.contains("$5::uuid"), "{sql}");
    }
}
