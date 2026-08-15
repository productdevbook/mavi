//! One thing a writing can be filed under.

use chrono::{DateTime, Utc};
use mavi_core::error::{Error, Result};
use mavi_core::id;
use mavi_core::say::Say;
use serde::{Deserialize, Serialize};

id!(
    /// One term.
    TermId
);

pub const ONLY_A_CATEGORY_HAS_A_PARENT: &str = "only_a_category_has_a_parent";
pub const NOTHING_GOES_UNDER_ITSELF: &str = "nothing_goes_under_itself";
pub const A_CATEGORY_GOES_UNDER_A_CATEGORY: &str = "a_category_goes_under_a_category";
pub const SOMETHING_ELSE_IS_FILED_UNDER_THAT_NAME: &str = "something_else_is_filed_under_that_name";

/// Somewhere a writing lives, or something it is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sort {
    /// Somewhere it lives. May be under another.
    Category,
    /// Something it is about. Flat, always.
    Tag,
}

impl Sort {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Sort::Category => "category",
            Sort::Tag => "tag",
        }
    }

    #[must_use]
    pub const fn may_have_a_parent(self) -> bool {
        matches!(self, Sort::Category)
    }
}

/// One term.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Term {
    pub id: TermId,
    pub sort: Sort,
    pub language: String,
    pub slug: String,
    pub name: String,
    pub parent: Option<TermId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Whether this term may go under that one, decided in one place.
///
/// Three ways it cannot, and each is a different sentence because they are
/// different mistakes — a caller told "no" learns nothing, and a caller told
/// which of the three can fix it.
///
/// The fourth way, a cycle deeper than one step, cannot be answered from two
/// rows. It is checked where the parent is set, by walking up; this is the
/// half a constraint can hold.
pub fn goes_under(sort: Sort, id: TermId, parent: Option<(TermId, Sort)>) -> Result<()> {
    let Some((parent_id, parent_sort)) = parent else {
        return Ok(());
    };

    if !sort.may_have_a_parent() {
        return Err(Error::invalid(Say::of(ONLY_A_CATEGORY_HAS_A_PARENT)));
    }

    if parent_id == id {
        return Err(Error::invalid(Say::of(NOTHING_GOES_UNDER_ITSELF)));
    }

    if !parent_sort.may_have_a_parent() {
        return Err(Error::invalid(Say::of(A_CATEGORY_GOES_UNDER_A_CATEGORY)));
    }

    Ok(())
}

/// What to say when the address is in use.
#[must_use]
pub fn name_is_taken() -> Error {
    Error::conflict(Say::of(SOMETHING_ELSE_IS_FILED_UNDER_THAT_NAME))
}

/// Whether what the database refused was the address being taken.
#[must_use]
pub fn taken(cause: &sqlx::Error) -> bool {
    matches!(cause, sqlx::Error::Database(db) if db.constraint() == Some("terms_address"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_is_flat() {
        let tag = TermId::new();
        let under = (TermId::new(), Sort::Category);

        let refused = goes_under(Sort::Tag, tag, Some(under)).expect_err("a tag took a parent");

        assert_eq!(
            refused.said().expect("a refusal").key,
            ONLY_A_CATEGORY_HAS_A_PARENT
        );
    }

    #[test]
    fn nothing_goes_under_itself() {
        let it = TermId::new();

        let refused = goes_under(Sort::Category, it, Some((it, Sort::Category)))
            .expect_err("a category went under itself");

        assert_eq!(
            refused.said().expect("a refusal").key,
            NOTHING_GOES_UNDER_ITSELF
        );
    }

    #[test]
    fn a_category_does_not_go_under_a_tag() {
        // The one that reads as fine and produces a shape nothing can draw:
        // a tree whose parent is not part of the tree.
        let category = TermId::new();

        let refused = goes_under(Sort::Category, category, Some((TermId::new(), Sort::Tag)))
            .expect_err("a category went under a tag");

        assert_eq!(
            refused.said().expect("a refusal").key,
            A_CATEGORY_GOES_UNDER_A_CATEGORY
        );
    }

    #[test]
    fn a_category_under_a_category_is_fine() {
        assert!(
            goes_under(
                Sort::Category,
                TermId::new(),
                Some((TermId::new(), Sort::Category))
            )
            .is_ok()
        );
    }

    #[test]
    fn under_nothing_is_fine_for_either() {
        assert!(goes_under(Sort::Tag, TermId::new(), None).is_ok());
        assert!(goes_under(Sort::Category, TermId::new(), None).is_ok());
    }

    #[test]
    fn each_way_it_cannot_says_which_way() {
        // Three refusals, three keys. A caller told "no" learns nothing; one
        // told which of the three can fix it.
        let it = TermId::new();

        let keys: Vec<&str> = [
            goes_under(Sort::Tag, it, Some((TermId::new(), Sort::Category))),
            goes_under(Sort::Category, it, Some((it, Sort::Category))),
            goes_under(Sort::Category, it, Some((TermId::new(), Sort::Tag))),
        ]
        .iter()
        .map(|refused| {
            refused
                .as_ref()
                .expect_err("a refusal")
                .said()
                .expect("a sentence")
                .key
        })
        .collect();

        let mut all = keys.clone();
        all.sort_unstable();
        all.dedup();

        assert_eq!(all.len(), keys.len(), "two mistakes say the same thing");
    }
}
