//! What a file is, and where it is kept.
//!
//! Two rules that came out of the crate this replaces, both learned the hard
//! way:
//!
//! **A file is never kept under the name somebody chose.** A name arrives from
//! outside and may be `../../etc/passwd`, or `.htaccess`, or four hundred
//! characters of Cyrillic. What is kept is its id; the name somebody typed is a
//! column, shown back to them and used for nothing.
//!
//! **What a file is, is read from its bytes and not from its name.** A
//! `holiday.png` full of `<script>` is not an image, and a site that serves it
//! as one is a site serving somebody else's script from its own address.

use chrono::{DateTime, Utc};
use mavi_core::error::{Error, Result};
use mavi_core::id;
use mavi_core::say::Say;
use serde::{Deserialize, Serialize};

id!(
    /// One uploaded file.
    FileId
);

pub const THAT_IS_NOT_A_KIND_OF_FILE_THIS_TAKES: &str = "that_is_not_a_kind_of_file_this_takes";
pub const THAT_FILE_IS_EMPTY: &str = "that_file_is_empty";
pub const THAT_FILE_IS_TOO_BIG: &str = "that_file_is_too_big";

/// What sort of thing was uploaded, decided by looking at it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Image,
    Video,
    Audio,
    Document,
}

impl Kind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Kind::Image => "image",
            Kind::Video => "video",
            Kind::Audio => "audio",
            Kind::Document => "document",
        }
    }
}

/// One kind of file this takes, and how to recognise it.
struct Recognised {
    kind: Kind,
    mime: &'static str,
    extension: &'static str,
    /// What a file of this sort has in it, and how far in.
    ///
    /// The offset is a field rather than an assumption because one of these is
    /// not at the start: an mp4 begins with the length of its first box and
    /// says `ftyp` four bytes later. Written as "starts with" instead, that
    /// one format is a special case beside the table — and a special case
    /// beside a table is the half somebody forgets to update.
    signature: &'static [u8],
    at: usize,
}

/// Everything this takes, and nothing else.
///
/// An allowlist rather than a list of what to refuse: a denylist is a list
/// somebody has to keep up with, and the thing it misses is the thing that
/// hurts. Adding a format here is a decision, which is the point.
const TAKEN: &[Recognised] = &[
    Recognised {
        kind: Kind::Image,
        mime: "image/png",
        extension: "png",
        signature: b"\x89PNG\r\n\x1a\n",
        at: 0,
    },
    Recognised {
        kind: Kind::Image,
        mime: "image/jpeg",
        extension: "jpg",
        signature: b"\xff\xd8\xff",
        at: 0,
    },
    Recognised {
        kind: Kind::Image,
        mime: "image/gif",
        extension: "gif",
        signature: b"GIF8",
        at: 0,
    },
    Recognised {
        kind: Kind::Image,
        mime: "image/webp",
        extension: "webp",
        signature: b"WEBP",
        at: 8,
    },
    Recognised {
        kind: Kind::Video,
        mime: "video/mp4",
        extension: "mp4",
        signature: b"ftyp",
        at: 4,
    },
    Recognised {
        kind: Kind::Audio,
        mime: "audio/mpeg",
        extension: "mp3",
        signature: b"ID3",
        at: 0,
    },
    Recognised {
        kind: Kind::Document,
        mime: "application/pdf",
        extension: "pdf",
        signature: b"%PDF-",
        at: 0,
    },
];

/// The most one file may be. A limit exists because an upload with none is a
/// disk somebody else decides the size of.
pub const AT_MOST: usize = 100 * 1024 * 1024;

/// What was uploaded, decided by reading it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Looked {
    pub kind: Kind,
    pub mime: &'static str,
    pub extension: &'static str,
}

/// One file the site is holding.
///
/// `name` is what somebody called it and is shown back to them. `kept_at` is
/// where the bytes actually are, and the two are separate fields because they
/// are separate things — the moment one is used for the other, a name is a
/// path.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct File {
    pub id: FileId,
    pub kind: Kind,
    pub mime: String,
    pub name: String,
    pub kept_at: String,
    pub bytes: i64,
    pub created_at: DateTime<Utc>,
}

/// What this is, from its bytes.
///
/// The name is not consulted. That is the whole of it: a name is what somebody
/// typed, and what they typed is not evidence.
pub fn look(bytes: &[u8]) -> Result<Looked> {
    if bytes.is_empty() {
        return Err(Error::invalid(Say::of(THAT_FILE_IS_EMPTY)));
    }

    if bytes.len() > AT_MOST {
        return Err(Error::invalid(
            Say::of(THAT_FILE_IS_TOO_BIG).with("at_most", &AT_MOST),
        ));
    }

    TAKEN
        .iter()
        .find(|taken| {
            bytes
                .get(taken.at..taken.at + taken.signature.len())
                .is_some_and(|there| there == taken.signature)
        })
        .map(|taken| Looked {
            kind: taken.kind,
            mime: taken.mime,
            extension: taken.extension,
        })
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_A_KIND_OF_FILE_THIS_TAKES)))
}

/// Where a file is kept: its id and the extension its bytes earned.
///
/// Never the name somebody chose. There is no argument here that takes one,
/// which is what makes that true rather than remembered.
#[must_use]
pub fn kept_at(id: FileId, looked: Looked) -> String {
    let id = id.to_string().replace('-', "");
    let (front, back) = id.split_at(2);

    // Two characters of the id as a directory: a hundred thousand files in one
    // folder is a folder nothing can list, and the id is already random enough
    // to spread them evenly.
    format!("{front}/{back}.{}", looked.extension)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A_PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";

    #[test]
    fn what_a_file_is_comes_from_its_bytes() {
        assert_eq!(look(A_PNG).expect("a png").kind, Kind::Image);
        assert_eq!(look(b"%PDF-1.7 ...").expect("a pdf").kind, Kind::Document);
    }

    #[test]
    fn a_name_is_not_evidence() {
        // The failure this prevents: a file called `holiday.png` full of
        // script, served back as an image from the site's own address.
        let lying = b"<!doctype html><script>alert(1)</script>";

        assert!(look(lying).is_err(), "a script called itself a picture");
    }

    #[test]
    fn nothing_is_kept_under_the_name_somebody_chose() {
        // There is no argument here that could carry one, which is what makes
        // this true rather than remembered. The test says so in the one way a
        // test can: what comes out has the id in it and nothing else.
        let id = FileId::new();
        let where_it_goes = kept_at(id, look(A_PNG).expect("a png"));

        let flat = id.to_string().replace('-', "");
        assert!(where_it_goes.contains(&flat[..2]), "{where_it_goes}");
        assert!(where_it_goes.ends_with(".png"), "{where_it_goes}");
        assert!(!where_it_goes.contains(".."), "{where_it_goes}");
    }

    #[test]
    fn every_extension_fits_what_the_schema_will_accept() {
        // `files.kept_at` has a check on it — two hex characters, a slash, the
        // rest of the id, a dot, an extension of two to five. Adding a format
        // here whose extension is longer than that is an upload the database
        // refuses, and a refusal from a check constraint reaches somebody as
        // "something went wrong" rather than as a sentence.
        for taken in TAKEN {
            assert!(
                (2..=5).contains(&taken.extension.len())
                    && taken
                        .extension
                        .bytes()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
                "{} is longer or stranger than the column allows",
                taken.extension
            );
        }
    }

    #[test]
    fn no_two_formats_are_recognised_by_the_same_bytes() {
        // Two entries with the same signature at the same offset means the
        // second is unreachable, and unreachable is invisible: the file is
        // taken, and taken as the wrong thing.
        let mut seen: Vec<(usize, &[u8])> = TAKEN.iter().map(|t| (t.at, t.signature)).collect();
        let count = seen.len();
        seen.sort_unstable();
        seen.dedup();

        assert_eq!(seen.len(), count, "one format shadows another");
    }

    #[test]
    fn an_empty_file_and_a_huge_one_are_both_refused() {
        assert_eq!(
            look(b"").expect_err("empty").said().expect("a refusal").key,
            THAT_FILE_IS_EMPTY
        );

        let too_much = vec![0_u8; AT_MOST + 1];
        assert_eq!(
            look(&too_much)
                .expect_err("too big")
                .said()
                .expect("a refusal")
                .key,
            THAT_FILE_IS_TOO_BIG
        );
    }

    #[test]
    fn a_video_is_recognised_from_where_its_signature_actually_is() {
        // Four bytes of length, then `ftyp`. It is the one format in this list
        // whose signature is not at the start, and looking only at the start
        // means every mp4 is refused.
        let mp4 = b"\x00\x00\x00\x20ftypisom\x00\x00\x02\x00";

        assert_eq!(look(mp4).expect("an mp4").kind, Kind::Video);
    }

    #[test]
    fn something_too_short_to_hold_a_signature_is_not_one() {
        // The slice this reads sits four bytes in. Asking for it out of three
        // bytes is a panic if it is taken rather than asked for, and this is a
        // file somebody can upload.
        assert!(look(b"ftp").is_err());
        assert!(look(b"\x00").is_err());
    }
}
