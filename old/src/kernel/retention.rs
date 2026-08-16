//! How to say what is kept, and for how long.
//!
//! Every table holding something about a person says how long it holds it, in
//! one list, and the machine's own housekeeping reads that list rather than
//! each domain remembering to forget. What is in the list is a domain's — the
//! tables are theirs and so are the sweeps — so the list itself is not here;
//! this is only the shape a line of it has, which an outside crate writes its
//! own in too.
/// A table that holds something belonging to a person, how long it is kept, and
/// what takes it away. A table with somebody's own words in it and no line here
/// fails a test rather than quietly keeping them forever.
#[derive(Clone, Copy, Debug)]
pub struct Policy {
    pub table: &'static str,
    /// What decides the age at which it goes. Where a site chooses, this names
    /// the column it chooses in.
    pub keeps: Keeps,
    pub swept_by: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub enum Keeps {
    Days(i32),
    /// The site says, in this column of the row it belongs to.
    AsTheSiteSays {
        column: &'static str,
    },
    /// Kept as long as the thing it describes is: a row that goes when its
    /// parent goes needs no sweep of its own.
    WithItsSubject,
}
