//! What a visitor sees.
//!
//! Every address a site answers on reaches this process, so a site's pages are
//! served by the same thing that serves its panel. One deployment, and a page
//! appears the moment it is published rather than when somebody adds a
//! container for it.
//!
//! What is served is **a build, named by the set of changes that made it**.
//! Nothing here reads a site's source: what a build produced is in the store
//! under that id, and going live is one row saying which id.
//!
//! This crate answers questions and holds no state. Which build is live and
//! where a page went are asked of the caller, which is what makes every rule
//! in here testable without a database.

pub mod moved;
pub mod page;

pub use moved::{Went, slug_of, went};
pub use page::{Kind, file_for, kind_of};

use uuid::Uuid;

/// What a page may be held for.
///
/// A minute, because an address does not carry the id of the build that
/// answered it: longer would serve yesterday's page out of somebody's browser
/// after a publish, and the fix for that is names with a fingerprint in them
/// rather than a bigger number here.
pub const HELD_FOR: &str = "public, max-age=60";

/// Where a build's files are kept.
///
/// The id of the set of changes is the whole of the name. A new build is a new
/// folder, and going live is a row changing rather than files moving — so a
/// publish cannot be half done, and going back is the row changing again.
#[must_use]
pub fn at(build: Uuid, path: &str) -> String {
    format!("builds/{}/{}", build.simple(), path.trim_start_matches('/'))
}

/// Where a build somebody asked to look at answers, on the site's own address.
///
/// Under the build's own id, which nobody guesses and nothing links to: a look
/// is for whoever asked for it, and a design that is not published should not
/// be found by walking a site's addresses. The id is the whole of the secret,
/// and it is enough of one — an id that is not a build finds no files, so
/// there is nothing further to ask a database.
#[must_use]
pub fn to_look_at(build: Uuid) -> String {
    format!("/_looking/{}/", build.simple())
}

/// Whether an address is somebody looking at a build rather than reading the
/// site, and which build if it is.
#[must_use]
pub fn looking(path: &str) -> Option<(Uuid, String)> {
    let rest = path.strip_prefix("/_looking/")?;
    let (id, rest) = rest.split_once('/').unwrap_or((rest, ""));

    let build = Uuid::parse_str(id).ok()?;

    Some((build, format!("/{rest}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_is_a_folder_named_by_what_made_it() {
        let build = Uuid::from_u128(1);

        assert_eq!(
            at(build, "/about/index.html"),
            "builds/00000000000000000000000000000001/about/index.html"
        );
        // Leading slash or not, one place: an address arrives with one and a
        // file name is written without one.
        assert_eq!(
            at(build, "about/index.html"),
            at(build, "/about/index.html")
        );
    }

    #[test]
    fn looking_at_a_build_is_asked_of_the_address_rather_than_guessed() {
        let build = Uuid::from_u128(7);
        let at = to_look_at(build);

        assert_eq!(looking(&at), Some((build, "/".to_owned())));
        assert_eq!(
            looking(&format!("{at}about/")),
            Some((build, "/about/".to_owned()))
        );

        // Everything else is the site itself, including something that only
        // looks like a build id.
        assert_eq!(looking("/about"), None);
        assert_eq!(looking("/_looking/not-an-id/about"), None);
    }
}
