//! What was deleted, and how it comes back.
//!
//! Soft deletion in one place: everything that can be thrown away is thrown
//! away the same way, waits the same length of time, and is restored by the
//! same call — rather than each domain inventing its own bin.
/// A table that soft-deletes, and what a person calls a row in it.
///
/// The list is what both the trash listing and the restore read, so a domain
/// that starts soft-deleting adds a line here and gets both — and a test
/// compares this against the schema, so one that does not add a line fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kept {
    /// The table, which is also what the API calls this kind of thing.
    pub table: &'static str,
    /// The column a person would recognise it by.
    pub named_by: &'static str,
    /// How long it stays in the trash before it goes for good.
    pub days: i64,
}

pub const KEPT: &[Kept] = &[
    Kept {
        table: "posts",
        named_by: "title",
        days: 30,
    },
    Kept {
        table: "forms",
        named_by: "name",
        days: 30,
    },
    Kept {
        table: "form_submissions",
        named_by: "id",
        days: 30,
    },
    Kept {
        // A video's row, not its bytes: the file it was made from is in the
        // library and goes with the library.
        table: "videos",
        named_by: "title",
        days: 30,
    },
    Kept {
        table: "media",
        named_by: "original_name",
        days: 30,
    },
    Kept {
        table: "products",
        named_by: "name",
        days: 30,
    },
    Kept {
        table: "courses",
        named_by: "title",
        days: 30,
    },
    Kept {
        table: "boards",
        named_by: "name",
        days: 30,
    },
    Kept {
        table: "cards",
        named_by: "title",
        days: 30,
    },
    Kept {
        table: "flows",
        named_by: "name",
        days: 30,
    },
    Kept {
        table: "theme_files",
        named_by: "path",
        days: 30,
    },
];

#[must_use]
pub fn of(table: &str) -> Option<&'static Kept> {
    KEPT.iter().find(|kept| kept.table == table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_in_the_list_is_there_twice() {
        let mut tables: Vec<&str> = KEPT.iter().map(|kept| kept.table).collect();
        let count = tables.len();

        tables.sort_unstable();
        tables.dedup();

        assert_eq!(tables.len(), count);
    }

    #[test]
    fn nothing_is_kept_forever_and_nothing_goes_immediately() {
        assert!(KEPT.iter().all(|kept| kept.days > 0 && kept.days <= 365));
    }
}
