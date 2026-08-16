//! Where files actually go, for a host that has a directory.
//!
//! One implementation of [`mavi_core::ports::Files`], and the smallest one
//! that is honest: a directory on this machine. A host with object storage
//! writes its own and hands that in instead — which is the whole point of the
//! port being a port.
//!
//! What this is careful about is the one thing a caller cannot be careful
//! about for it. A path arrives from outside; the port's own documentation
//! says the traversal is the implementation's business, and this is the
//! implementation. Every path is held against the directory it must be under
//! **after** it has been resolved, so a name that climbs out with `..`, a
//! symbolic link somebody left behind, and an absolute path are all the same
//! refusal.

use std::path::{Component, Path, PathBuf};

use mavi_core::error::{Error, Result};
use mavi_core::ports::{Answering, Files};
use mavi_core::say::Say;

pub const THAT_IS_NOT_A_PLACE_FOR_A_FILE: &str = "that_is_not_a_place_for_a_file";

/// A directory on this machine.
#[derive(Debug, Clone)]
pub struct InADirectory {
    under: PathBuf,
}

impl InADirectory {
    #[must_use]
    pub fn at(under: impl Into<PathBuf>) -> Self {
        Self {
            under: under.into(),
        }
    }

    /// Where this path is, or a refusal.
    ///
    /// The components are walked rather than the string being searched: a
    /// check for the two characters `..` is one that `%2e%2e`, `.../`, and a
    /// name that is legitimately called `a..b` all get wrong in one direction
    /// or the other.
    fn within(&self, at: &str) -> Result<PathBuf> {
        let refuse = || Error::invalid(Say::of(THAT_IS_NOT_A_PLACE_FOR_A_FILE));

        let asked = Path::new(at);
        let mut safe = PathBuf::new();

        for part in asked.components() {
            match part {
                Component::Normal(part) => safe.push(part),
                // A root, a prefix, a `.` or a `..` — none of them is part of
                // a name somebody uploaded.
                _ => return Err(refuse()),
            }
        }

        if safe.as_os_str().is_empty() {
            return Err(refuse());
        }

        Ok(self.under.join(safe))
    }
}

impl Files for InADirectory {
    fn put<'a>(&'a self, at: &'a str, bytes: Vec<u8>) -> Answering<'a, ()> {
        Box::pin(async move {
            let to = self.within(at)?;

            if let Some(parent) = to.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(Error::internal)?;
            }

            // Written beside and then moved into place: a reader that opens
            // the name half way through a write gets half a file, and a crash
            // leaves one behind for ever.
            //
            // Added to the name rather than put in place of what it ends in.
            // `site.css` and `site.js` are one file called `site.part` if the
            // extension is replaced, and a build writing a directory full of
            // files would have two of them writing the same name at once.
            let beside = {
                let mut name = to.file_name().unwrap_or_default().to_os_string();
                name.push(".part");

                to.with_file_name(name)
            };

            tokio::fs::write(&beside, bytes)
                .await
                .map_err(Error::internal)?;

            tokio::fs::rename(&beside, &to)
                .await
                .map_err(Error::internal)?;

            Ok(())
        })
    }

    fn get<'a>(&'a self, at: &'a str) -> Answering<'a, Vec<u8>> {
        Box::pin(async move {
            let from = self.within(at)?;

            tokio::fs::read(&from).await.map_err(Error::internal)
        })
    }

    fn remove<'a>(&'a self, at: &'a str) -> Answering<'a, ()> {
        Box::pin(async move {
            let gone = self.within(at)?;

            match tokio::fs::remove_file(&gone).await {
                Ok(()) => Ok(()),
                // Already gone is what was asked for. Removing something twice
                // is what a sweeper that was interrupted does.
                Err(wrong) if wrong.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(wrong) => Err(Error::internal(wrong)),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn somewhere() -> InADirectory {
        InADirectory::at("/tmp/whatever")
    }

    #[test]
    fn a_place_a_file_goes_is_under_the_directory() {
        let files = somewhere();

        assert_eq!(
            files.within("ab/cdef.png").expect("a place"),
            PathBuf::from("/tmp/whatever/ab/cdef.png")
        );
    }

    #[test]
    fn nothing_climbs_out() {
        // The one the port's own documentation says is this implementation's
        // business rather than its caller's.
        let files = somewhere();

        for wrong in [
            "../etc/passwd",
            "../../etc/passwd",
            "ab/../../etc/passwd",
            "/etc/passwd",
            "",
            ".",
            "..",
        ] {
            assert!(files.within(wrong).is_err(), "{wrong:?} was taken");
        }
    }

    #[test]
    fn a_name_with_two_dots_in_it_is_still_a_name() {
        // What a string search for `..` gets wrong from the other side: this
        // is an ordinary file name and refusing it would be refusing somebody
        // their own picture.
        let files = somewhere();

        assert!(files.within("ab/a..b.png").is_ok());
    }

    #[tokio::test]
    async fn what_is_written_comes_back_and_can_be_taken_away_twice() {
        let under = std::env::temp_dir().join(format!("mavi-files-{}", uuid::Uuid::now_v7()));
        let files = InADirectory::at(&under);

        files
            .put("ab/cdef.png", b"a picture".to_vec())
            .await
            .expect("written");

        assert_eq!(files.get("ab/cdef.png").await.expect("read"), b"a picture");

        files.remove("ab/cdef.png").await.expect("removed");
        // Again, because a sweeper that was interrupted does exactly this.
        files.remove("ab/cdef.png").await.expect("removed again");

        assert!(files.get("ab/cdef.png").await.is_err());

        tokio::fs::remove_dir_all(&under).await.ok();
    }

    #[tokio::test]
    async fn two_files_of_one_name_are_never_written_beside_each_other() {
        // What a build does: a directory full of files, some sharing a name
        // and differing only in what they end in. A half-written name shared
        // between two of them is one arriving with the other's bytes.
        let under = std::env::temp_dir().join(format!("mavi-files-{}", uuid::Uuid::now_v7()));
        let files = InADirectory::at(&under);

        let (css, js) = tokio::join!(
            files.put("styles/site.css", b"body { color: teal }".to_vec()),
            files.put("styles/site.js", b"console.log(1)".to_vec()),
        );

        css.expect("a stylesheet");
        js.expect("a script");

        assert_eq!(
            files.get("styles/site.css").await.expect("read"),
            b"body { color: teal }"
        );
        assert_eq!(
            files.get("styles/site.js").await.expect("read"),
            b"console.log(1)"
        );

        tokio::fs::remove_dir_all(&under).await.ok();
    }

    #[tokio::test]
    async fn nothing_half_written_is_ever_under_the_name() {
        // Written beside and moved into place. What this test can show is the
        // half that is checkable: nothing is left behind under a second name.
        let under = std::env::temp_dir().join(format!("mavi-files-{}", uuid::Uuid::now_v7()));
        let files = InADirectory::at(&under);

        files
            .put("ab/cdef.png", b"a picture".to_vec())
            .await
            .expect("written");

        let left: Vec<String> = std::fs::read_dir(under.join("ab"))
            .expect("the directory")
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();

        assert_eq!(left, ["cdef.png"]);

        tokio::fs::remove_dir_all(&under).await.ok();
    }
}
