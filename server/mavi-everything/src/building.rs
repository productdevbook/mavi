//! Turning a set of changes into something a visitor can be served.
//!
//! Nothing in here decides **how** a site is built. That is [`Builds`], a
//! port, because a design that has to be built is a project with its own
//! dependencies and its own command — a machine running whatever a customer
//! wrote, which is a sandbox and a quota rather than a function, and none of
//! that belongs in a library anybody installs.
//!
//! What ships is [`WhatIsInPublic`]: a site of plain files, served as it is.
//! That is a real site rather than a degenerate case, and it is what an
//! installation gets unless whoever runs it hands in something else.
//!
//! Where the files go is [`mavi_edge::at`], which the edge reads and nothing
//! else writes: a build is a folder named by what made it, and going live is a
//! row saying which folder.

use mavi_core::error::Result;
use mavi_core::ports::{Answering, Builds, Built, Files};
use mavi_db::Db;
use uuid::Uuid;

/// Where a design's files sit in a project, and what that means for a page.
///
/// `public/` is served as it is, at the root. `src/` is what a generator would
/// read, and to something that does not run one it is not a page — serving
/// somebody's templates would be publishing the thing that makes the pages.
pub const SERVED_FROM: &str = "public/";

/// A site of plain files.
///
/// The whole of it: what is under `public/` is what a visitor gets. No command
/// runs, so there is nothing to sandbox and nothing to wait for, and a site
/// built this way is live the moment the row changes.
#[derive(Clone, Copy, Debug)]
pub struct WhatIsInPublic;

impl Builds for WhatIsInPublic {
    fn build<'a>(
        &'a self,
        _change: Uuid,
        everything: &'a [(String, Vec<u8>)],
    ) -> Answering<'a, Built> {
        Box::pin(async move {
            let serve = everything
                .iter()
                .filter_map(|(path, contents)| {
                    path.strip_prefix(SERVED_FROM)
                        .map(|under| (under.to_owned(), contents.clone()))
                })
                .collect::<Vec<_>>();

            // Said rather than served empty. A design with nothing under
            // `public/` is the answer to "why is my site not there", and it is
            // only an answer if somebody is told it.
            if serve.is_empty() {
                return Ok(Built::WentWrong(format!(
                    "there is nothing under {SERVED_FROM} to serve"
                )));
            }

            Ok(Built::Serve(serve))
        })
    }
}

/// Builds one set of changes, and says how it went.
///
/// What comes back is how many files a visitor can now be served, which is
/// zero when it did not build.
pub async fn build(db: &Db, files: &dyn Files, builds: &dyn Builds, change: Uuid) -> Result<u32> {
    let mut tx = db.begin().await?;
    let everything = mavi_design::store::everything_in(&mut tx, change).await?;
    tx.commit().await?;

    let built = builds.build(change, &everything).await?;

    let written = match &built {
        Built::Serve(serve) => {
            for (under, contents) in serve {
                files
                    .put(&mavi_edge::at(change, under), contents.clone())
                    .await?;
            }

            u32::try_from(serve.len()).unwrap_or(u32::MAX)
        }
        Built::WentWrong(_) => 0,
    };

    // Written down either way, and in one place, because a set of changes that
    // says nothing about its last build is one nobody can tell apart from one
    // that was never built.
    let mut tx = db.begin().await?;

    mavi_design::store::was_built(
        &mut tx,
        change,
        &mavi_design::store::Built {
            look_at: matches!(built, Built::Serve(_)).then(|| mavi_edge::to_look_at(change)),
            went_wrong: match built {
                Built::WentWrong(said) => Some(said),
                Built::Serve(_) => None,
            },
        },
    )
    .await?;

    tx.commit().await?;

    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_project() -> Vec<(String, Vec<u8>)> {
        vec![
            ("public/index.html".to_owned(), b"<h1>Hello</h1>".to_vec()),
            (
                "src/pages/index.astro".to_owned(),
                b"---\n---\n<h1>not this</h1>".to_vec(),
            ),
        ]
    }

    #[tokio::test]
    async fn what_is_served_is_what_a_design_put_under_public() {
        let built = WhatIsInPublic
            .build(Uuid::nil(), &a_project())
            .await
            .expect("a build");

        // `src/` is what a generator reads. Serving it would be publishing the
        // templates a site is made from rather than the site.
        match built {
            Built::Serve(serve) => assert_eq!(
                serve,
                vec![("index.html".to_owned(), b"<h1>Hello</h1>".to_vec())]
            ),
            Built::WentWrong(said) => panic!("{said}"),
        }
    }

    #[tokio::test]
    async fn a_design_with_nothing_to_serve_is_told_so() {
        let built = WhatIsInPublic
            .build(Uuid::nil(), &[("src/only.astro".to_owned(), Vec::new())])
            .await
            .expect("an answer");

        // Not an error. Somebody has to go and put a file somewhere, and what
        // they need is the sentence.
        match built {
            Built::WentWrong(said) => assert!(said.contains(SERVED_FROM), "{said}"),
            Built::Serve(serve) => panic!("{} files out of nothing", serve.len()),
        }
    }
}
