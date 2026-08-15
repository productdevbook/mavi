//! How a list is walked.
//!
//! Everything that can grow is walked with a cursor rather than an offset: an
//! offset re-counts the rows it skips, so page one thousand costs a thousand
//! pages, while a cursor asks "after this" and costs the same wherever it is.
//!
//! The part that is designed rather than merely provided is **which columns a
//! cursor addresses**. It measured out badly in the crate this replaces: a
//! listing ordered by one column and cursored on another, twice — and twelve
//! more found by a rule written afterwards. The failure is silent. A cursor
//! that names fewer columns than the order does cannot address a position
//! *inside* a run of equal values, so whole groups of rows are skipped or
//! repeated, and nothing anywhere reports it.
//!
//! It bites more often than it sounds. PostgreSQL's `now()` is fixed for a
//! whole transaction, so everything written together shares a timestamp
//! exactly — which is most rows, in a system where things are created in
//! batches.
//!
//! So a listing declares a [`Keyset`] **once**, and both the order and the
//! cursor are read from it. A cursor whose arity does not match the keyset is
//! refused rather than half-understood: a listing whose order changed and
//! whose cursor did not fails on the first request for a second page, loudly,
//! instead of quietly losing rows for a year.

use serde::{Deserialize, Serialize};

use crate::error::{Code, Error, Result};
use crate::say::Say;

pub const CURSOR_IS_NOT_ONE_THIS_LISTING_GAVE: &str = "cursor_is_not_one_this_listing_gave";

/// Which way a column runs. Newest first is what a person reading a list
/// expects; oldest first is what something replaying a log wants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Newest,
    Oldest,
}

/// One column of a listing's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    pub column: &'static str,
    pub direction: Direction,
}

impl Key {
    #[must_use]
    pub const fn newest(column: &'static str) -> Self {
        Self {
            column,
            direction: Direction::Newest,
        }
    }

    #[must_use]
    pub const fn oldest(column: &'static str) -> Self {
        Self {
            column,
            direction: Direction::Oldest,
        }
    }
}

/// The columns a listing is ordered by, and therefore the columns its cursor
/// carries. Declared once so the two cannot disagree.
///
/// The last key should be unique on its own — a row's id will do. Without one,
/// two rows that agree on every declared column occupy the same position and
/// the cursor cannot tell them apart, which is the bug this type exists to
/// prevent, arrived at from the other side.
#[derive(Clone, Copy, Debug)]
pub struct Keyset(pub &'static [Key]);

impl Keyset {
    #[must_use]
    pub const fn keys(&self) -> &'static [Key] {
        self.0
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Where a page starts: the values of a keyset's columns, in its order.
///
/// Opaque to whoever holds it. What it is made of is this crate's business,
/// and a caller who takes it apart is relying on something that will move.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cursor(Vec<String>);

impl Cursor {
    /// The values a row sits at, in the keyset's own order. The caller is
    /// handing over what it read from the row; the arity is checked here so
    /// that a row read with the wrong number of columns is caught where it
    /// happens rather than when somebody asks for page two.
    pub fn at(keyset: Keyset, values: Vec<String>) -> Result<Self> {
        if values.len() != keyset.len() {
            return Err(Error::internal(std::io::Error::other(format!(
                "a cursor of {} values for a keyset of {}",
                values.len(),
                keyset.len()
            ))));
        }

        Ok(Self(values))
    }

    #[must_use]
    pub fn values(&self) -> &[String] {
        &self.0
    }

    /// What goes over the wire.
    #[must_use]
    pub fn token(&self) -> String {
        let json = serde_json::to_string(&self.0).unwrap_or_else(|_| "[]".to_owned());
        base64url(json.as_bytes())
    }

    /// What came back, held against the keyset it claims to be for.
    ///
    /// A token from a different listing, or from this one before its order
    /// changed, is refused. That refusal is the whole point: the alternative
    /// is a filter that matches nothing in particular.
    pub fn from_token(keyset: Keyset, token: &str) -> Result<Self> {
        let refused = || Error::new(Code::Invalid, Say::of(CURSOR_IS_NOT_ONE_THIS_LISTING_GAVE));

        let raw = unbase64url(token).ok_or_else(refused)?;
        let values: Vec<String> = serde_json::from_slice(&raw).map_err(|_| refused())?;

        if values.len() != keyset.len() {
            return Err(refused());
        }

        Ok(Self(values))
    }
}

/// What a caller asks for.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Query {
    /// The token from the last page. Absent for the first.
    pub after: Option<String>,
    pub limit: Option<u16>,
}

/// The most rows one page may hold, whatever was asked for. A caller asking
/// for everything is asking for a query with no bound on it.
pub const MOST: u16 = 100;
/// What a caller who says nothing gets.
pub const SOME: u16 = 25;

impl Query {
    #[must_use]
    pub fn limit(&self) -> u16 {
        self.limit.unwrap_or(SOME).clamp(1, MOST)
    }

    /// One more than asked for, which is how a page knows whether there is
    /// another without counting the rest.
    #[must_use]
    pub fn fetch(&self) -> i64 {
        i64::from(self.limit()) + 1
    }

    /// The cursor this asks to start after, checked against the listing it was
    /// given to.
    pub fn after(&self, keyset: Keyset) -> Result<Option<Cursor>> {
        self.after
            .as_deref()
            .map(|token| Cursor::from_token(keyset, token))
            .transpose()
    }
}

/// A page, and where the next one starts.
#[derive(Debug, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    /// Absent when this is the last page. Present means there is more, always
    /// — never a token that answers an empty page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
}

impl<T> Page<T> {
    /// Builds a page out of what was fetched — which is one more row than was
    /// asked for. That extra row is dropped, and its existence is what `next`
    /// means.
    ///
    /// `at` is asked where each row sits, in the keyset's order. It is the
    /// same keyset the query was ordered by, because there is only one.
    pub fn build(
        query: &Query,
        keyset: Keyset,
        mut rows: Vec<T>,
        at: impl Fn(&T) -> Vec<String>,
    ) -> Result<Self> {
        let asked = usize::from(query.limit());

        let next = if rows.len() > asked {
            rows.truncate(asked);
            rows.last()
                .map(|last| Cursor::at(keyset, at(last)).map(|cursor| cursor.token()))
                .transpose()?
        } else {
            None
        };

        Ok(Self { items: rows, next })
    }
}

fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);

    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);

        for i in 0..=chunk.len() {
            let shift = 18 - 6 * i;
            out.push(ALPHABET[((n >> shift) & 0b0011_1111) as usize] as char);
        }
    }

    out
}

fn unbase64url(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        })
    };

    let mut out = Vec::with_capacity(text.len() / 4 * 3);

    for chunk in text.as_bytes().chunks(4) {
        if chunk.len() < 2 {
            return None;
        }

        let mut n = 0_u32;
        for (i, c) in chunk.iter().enumerate() {
            n |= value(*c)? << (18 - 6 * i);
        }

        for i in 0..chunk.len() - 1 {
            out.push(((n >> (16 - 8 * i)) & 0xFF) as u8);
        }
    }

    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BY_WHEN: Keyset = Keyset(&[Key::newest("created_at"), Key::newest("id")]);
    const BY_WEIGHT: Keyset = Keyset(&[
        Key::newest("weight"),
        Key::newest("created_at"),
        Key::newest("id"),
    ]);

    #[test]
    fn a_cursor_from_another_listing_is_refused() {
        let given = Cursor::at(BY_WHEN, vec!["2026-01-01".into(), "an-id".into()])
            .expect("a cursor")
            .token();

        // The same token, offered to a listing ordered by three columns. It
        // decodes as base64 and as json; only the arity says it is wrong, and
        // that is exactly the case that used to lose rows quietly.
        let read = Cursor::from_token(BY_WEIGHT, &given);

        assert!(read.is_err(), "a two-column cursor answered a three-column listing");
        assert_eq!(read.unwrap_err().code(), Code::Invalid);
    }

    #[test]
    fn a_cursor_survives_the_wire() {
        let values = vec!["2026-01-01T00:00:00Z".to_owned(), "a/b+c=d".to_owned()];
        let there = Cursor::at(BY_WHEN, values.clone()).expect("a cursor");
        let back = Cursor::from_token(BY_WHEN, &there.token()).expect("read back");

        assert_eq!(back.values(), values.as_slice());
    }

    #[test]
    fn a_page_says_there_is_another_only_when_there_is() {
        let query = Query {
            after: None,
            limit: Some(2),
        };

        let full = Page::build(&query, BY_WHEN, vec![1, 2, 3], |n| {
            vec![n.to_string(), n.to_string()]
        })
        .expect("a page");

        assert_eq!(full.items, vec![1, 2]);
        assert!(full.next.is_some(), "three rows fetched for a page of two");

        let last = Page::build(&query, BY_WHEN, vec![1, 2], |n| {
            vec![n.to_string(), n.to_string()]
        })
        .expect("a page");

        assert_eq!(last.items, vec![1, 2]);
        assert!(last.next.is_none(), "a page nobody can walk past said otherwise");
    }

    #[test]
    fn nobody_asks_for_more_than_a_page_holds() {
        let greedy = Query {
            after: None,
            limit: Some(10_000),
        };

        assert_eq!(greedy.limit(), MOST);
        assert_eq!(greedy.fetch(), i64::from(MOST) + 1);
    }
}
