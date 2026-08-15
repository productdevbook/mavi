//! Which of a site's own files may be written, and where they end up.
//!
//! A site's look is a project — pages, styles, images — and somebody editing it
//! from the panel is choosing a path and some bytes. Two things follow from
//! that, and neither is optional.
//!
//! **The path is somebody's typing, so it is not a path until it is checked.**
//! `../../etc/passwd` is the obvious one and the least likely; `src/../../.ssh/
//! authorized_keys` is the same thing written by somebody who has read the
//! first check.
//!
//! **Not every file in a project is design.** What a page looks like is
//! design. What command builds it, what it installs, and what runs before it
//! are not: they are a way to run whatever somebody likes on the machine that
//! does the building. So they are refused by name, and a site that wants them
//! changed changes them where the project actually lives.

use mavi_core::error::{Error, Result};
use mavi_core::say::Say;

pub const THAT_IS_NOT_A_PLACE_IN_A_PROJECT: &str = "that_is_not_a_place_in_a_project";
pub const THAT_FILE_IS_NOT_PART_OF_HOW_A_SITE_LOOKS: &str =
    "that_file_is_not_part_of_how_a_site_looks";

/// Where a site's own files live. Everything else in the project is how it is
/// built, and that is not something the panel changes.
pub const WRITABLE: &[&str] = &["src/", "public/"];

/// The longest a path may be. A limit exists because a file name is somebody's
/// typing and a thousand characters of it is not a file name.
pub const AT_MOST: usize = 200;

/// A path inside a site's own project, or a refusal.
///
/// What comes back is the path as it will be used, so nothing downstream has
/// to remember to check it again — the check and the value are the same thing.
pub fn to_write(path: &str) -> Result<String> {
    let path = path.trim();

    let shaped_right = !path.is_empty()
        && path.len() <= AT_MOST
        && !path.starts_with('/')
        && path.split('/').all(|part| {
            !part.is_empty() && part != "." && part != ".." && !part.contains('\\') && part != "~"
        })
        // A null byte ends a string in whatever C library eventually opens the
        // file, so `src/page.html\0.png` is two different names depending on
        // who is reading it.
        && !path.contains('\0');

    if !shaped_right {
        return Err(Error::invalid(Say::of(THAT_IS_NOT_A_PLACE_IN_A_PROJECT)));
    }

    if !WRITABLE.iter().any(|under| path.starts_with(under)) {
        return Err(Error::invalid(
            Say::of(THAT_FILE_IS_NOT_PART_OF_HOW_A_SITE_LOOKS).with("under", &WRITABLE.join(", ")),
        ));
    }

    Ok(path.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(path: &str) -> &'static str {
        to_write(path)
            .expect_err("a refusal")
            .said()
            .expect("a sentence")
            .key
    }

    #[test]
    fn a_page_and_a_picture_are_written() {
        for right in [
            "src/pages/index.astro",
            "src/styles/site.css",
            "public/logo.svg",
        ] {
            assert!(to_write(right).is_ok(), "{right} was refused");
        }
    }

    #[test]
    fn nothing_climbs_out_of_the_project() {
        for wrong in [
            "../../etc/passwd",
            "/etc/passwd",
            // The one written by somebody who has read the first check.
            "src/../../.ssh/authorized_keys",
            "src/./../../etc/passwd",
            "~/.ssh/id_ed25519",
            "src\\..\\..\\windows",
            "src/page.html\0.png",
        ] {
            assert_eq!(
                refused(wrong),
                THAT_IS_NOT_A_PLACE_IN_A_PROJECT,
                "{wrong:?} was taken for a place in a project"
            );
        }
    }

    #[test]
    fn what_builds_the_site_is_not_how_it_looks() {
        // Every one of these is a way to run whatever somebody likes on the
        // machine that does the building, which is not a thing an editor
        // should be able to do by changing the footer.
        for wrong in [
            "package.json",
            "astro.config.mjs",
            "Dockerfile",
            ".github/workflows/ci.yml",
            "scripts/build.sh",
            ".env",
        ] {
            assert_eq!(
                refused(wrong),
                THAT_FILE_IS_NOT_PART_OF_HOW_A_SITE_LOOKS,
                "{wrong} was written"
            );
        }
    }

    #[test]
    fn a_name_that_only_looks_like_one_of_ours_is_still_refused() {
        // `src` is a directory, not a prefix: `srcret/x` starts with the same
        // three letters and is somewhere else entirely.
        assert_eq!(
            refused("srcret/keys.json"),
            THAT_FILE_IS_NOT_PART_OF_HOW_A_SITE_LOOKS
        );
    }

    #[test]
    fn a_thousand_characters_is_not_a_file_name() {
        let long = format!("src/{}.css", "a".repeat(AT_MOST));

        assert_eq!(refused(&long), THAT_IS_NOT_A_PLACE_IN_A_PROJECT);
    }
}
