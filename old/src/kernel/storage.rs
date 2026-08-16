//! Where a file goes.
//!
//! One interface over the disk and over a bucket, because a site should not
//! know which it has. What kind of file something is comes from its bytes
//! rather than from its name: a name ending in `.png` proves nothing.
use std::path::{Path, PathBuf};

use super::error::{AppError, Result};
use crate::kernel::say;

/// Where uploaded bytes live. A local folder now; an object store is another
/// arm, and these three calls are what one answers.
#[derive(Clone, Debug)]
pub enum Store {
    Disk(LocalDisk),
}

impl Store {
    /// A directory beside the process. Right while somebody is looking at
    /// this on their own machine, and lost on the first restart anywhere
    /// else, which is why a deployment says where instead.
    #[must_use]
    pub fn beside_the_process() -> Self {
        Store::Disk(LocalDisk::at("uploads"))
    }

    /// Where the environment says uploads go. The image sets this to a
    /// volume; `UPLOADS_DIR` is what it was called before the project had a
    /// name of its own.
    #[must_use]
    pub fn from_env() -> Self {
        let Ok(directory) =
            std::env::var("MAVI_DATA_DIR").or_else(|_| std::env::var("UPLOADS_DIR"))
        else {
            return Store::beside_the_process();
        };

        Store::Disk(LocalDisk::at(directory))
    }

    pub async fn put(&self, location: &str, bytes: Vec<u8>) -> Result<()> {
        match self {
            Store::Disk(disk) => disk.put(location, bytes).await,
        }
    }

    pub async fn get(&self, location: &str) -> Result<Vec<u8>> {
        match self {
            Store::Disk(disk) => disk.get(location).await,
        }
    }

    pub async fn delete(&self, location: &str) -> Result<()> {
        match self {
            Store::Disk(disk) => disk.delete(location).await,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LocalDisk {
    root: PathBuf,
}

impl LocalDisk {
    #[must_use]
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// A path built from a name this process chose, never from anything a
    /// caller sent. The check is here anyway, because the day somebody passes
    /// one through is the day it matters — and it is the reason this function
    /// survived losing the site it used to sit under.
    fn path(&self, location: &str) -> Result<PathBuf> {
        let safe = location
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != ".." && !part.contains('\\'));

        if !safe {
            return Err(AppError::Invalid(say::NOT_PLACE.into()));
        }

        Ok(self.root.join(location))
    }

    async fn put(&self, location: &str, bytes: Vec<u8>) -> Result<()> {
        let path = self.path(location)?;

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|_| AppError::Bug("somewhere to keep an upload"))?;
        }

        write(&path, bytes).await
    }

    async fn get(&self, location: &str) -> Result<Vec<u8>> {
        tokio::fs::read(self.path(location)?)
            .await
            .map_err(|_| AppError::NotFound("file"))
    }

    async fn delete(&self, location: &str) -> Result<()> {
        match tokio::fs::remove_file(self.path(location)?).await {
            Ok(()) => Ok(()),
            // Already gone is the state that was wanted.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(AppError::Bug("removing a file")),
        }
    }
}

async fn write(path: &Path, bytes: Vec<u8>) -> Result<()> {
    tokio::fs::write(path, bytes)
        .await
        .map_err(|_| AppError::Bug("writing an upload"))
}

/// What may be uploaded, decided by what the bytes say rather than by what the
/// name claims. Anything a browser would run — HTML, SVG, anything scriptable —
/// is not here, and what is served is served with the type from this list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Allowed {
    pub mime: &'static str,
    pub extension: &'static str,
    /// Whether a browser may show it where it stands. Everything else is
    /// handed over as a download.
    pub inline: bool,
}

const ALLOWED: &[Allowed] = &[
    Allowed {
        mime: "image/jpeg",
        extension: "jpg",
        inline: true,
    },
    Allowed {
        mime: "image/png",
        extension: "png",
        inline: true,
    },
    Allowed {
        mime: "image/gif",
        extension: "gif",
        inline: true,
    },
    Allowed {
        mime: "image/webp",
        extension: "webp",
        inline: true,
    },
    Allowed {
        mime: "application/pdf",
        extension: "pdf",
        inline: false,
    },
    Allowed {
        mime: "video/mp4",
        extension: "mp4",
        inline: false,
    },
];

/// What these bytes are. Read from the front of the file: an extension is
/// something a person types and a magic number is something the file is.
#[must_use]
pub fn sniff(bytes: &[u8]) -> Option<Allowed> {
    let starts = |prefix: &[u8]| bytes.starts_with(prefix);

    let mime = if starts(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if starts(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        "image/gif"
    } else if bytes.len() > 12 && starts(b"RIFF") && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else if starts(b"%PDF-") {
        "application/pdf"
    } else if bytes.len() > 12 && &bytes[4..8] == b"ftyp" {
        "video/mp4"
    } else {
        return None;
    };

    ALLOWED.iter().copied().find(|allowed| allowed.mime == mime)
}

#[must_use]
pub fn allowed_for(mime: &str) -> Option<Allowed> {
    ALLOWED.iter().copied().find(|allowed| allowed.mime == mime)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_the_bytes_say_rather_than_what_the_name_claims() {
        assert_eq!(
            sniff(b"\x89PNG\r\n\x1a\n rest").map(|a| a.mime),
            Some("image/png")
        );
        assert_eq!(
            sniff(b"%PDF-1.7 rest").map(|a| a.mime),
            Some("application/pdf")
        );
        assert_eq!(sniff(b"GIF89a rest").map(|a| a.mime), Some("image/gif"));
    }

    #[test]
    fn nothing_a_browser_would_run_gets_in() {
        for refused in [
            &b"<svg xmlns=\"http://www.w3.org/2000/svg\"><script/></svg>"[..],
            &b"<!doctype html><script>alert(1)</script>"[..],
            &b"#!/bin/sh\nrm -rf /"[..],
            &b"MZ\x90\x00"[..],
            &b""[..],
        ] {
            assert!(
                sniff(refused).is_none(),
                "{:?} was allowed",
                &refused[..4.min(refused.len())]
            );
        }
    }

    #[test]
    fn a_place_is_never_somewhere_else() {
        let disk = LocalDisk::at("/tmp/nowhere");

        assert!(disk.path("../../etc/passwd").is_err());
        assert!(disk.path("a//b").is_err());
        assert!(disk.path("fine/enough.png").is_ok());
    }
}
