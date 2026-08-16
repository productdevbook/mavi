//! Turning a set of changes into something a visitor can be served.
//!
//! With no generator configured this is a copy: what a design put under
//! `public/` **is** the site, and a site of plain files is a real site rather
//! than a degenerate case. A design that has to be built — a project with its
//! own dependencies and its own command — is a machine running somebody else's
//! code, and that is a decision with its own shape rather than an `if` in
//! here. Until it is written, `public/` is what goes out.
//!
//! Where the files go is [`mavi_edge::at`], which the edge reads and nothing
//! else writes: a build is a folder named by what made it, and going live is a
//! row saying which folder.

use mavi_core::error::Result;
use mavi_core::ports::Files;
use mavi_db::Db;
use uuid::Uuid;

/// Where a design's files sit in a project, and what that means for a page.
///
/// `public/` is served as it is, at the root. `src/` is what a generator would
/// read, and with no generator it is not served at all — serving somebody's
/// templates as pages is publishing the thing that makes the pages.
const SERVED_FROM: &str = "public/";

/// Builds one set of changes.
pub async fn build(db: &Db, files: &dyn Files, change: Uuid) -> Result<u32> {
    let mut tx = db.begin().await?;
    let everything = mavi_design::store::everything_in(&mut tx, change).await?;
    tx.commit().await?;

    let mut written = 0;

    for (path, contents) in everything {
        let Some(under) = path.strip_prefix(SERVED_FROM) else {
            continue;
        };

        files.put(&mavi_edge::at(change, under), contents).await?;
        written += 1;
    }

    // Said even when it is nothing, because a build that wrote nothing is a
    // design with no `public/` in it, and the answer to "why is my site
    // empty" is this number.
    let mut tx = db.begin().await?;

    mavi_design::store::was_built(
        &mut tx,
        change,
        &mavi_design::store::Built {
            look_at: Some(mavi_edge::to_look_at(change)),
            went_wrong: (written == 0)
                .then(|| "there is nothing under public/ to serve".to_owned()),
        },
    )
    .await?;

    tx.commit().await?;

    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_served_is_what_a_design_put_under_public() {
        // `src/` is what a generator reads. Serving it would be publishing the
        // templates a site is made from rather than the site.
        assert!("public/logo.svg".starts_with(SERVED_FROM));
        assert!(!"src/pages/index.astro".starts_with(SERVED_FROM));
    }
}
