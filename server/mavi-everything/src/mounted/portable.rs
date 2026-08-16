// Domain route module: portable

use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use mavi_db::Db;
use mavi_http::Answered;
use mavi_serve::{Asked, Handler, Site};
use serde_json::Value;

use super::helpers::{handling, wrote_about};

/// A site, as a file.
#[must_use]
pub fn how_a_site_leaves(mut site: Site, db: &Db) -> Site {
    for endpoint in mavi_portable::endpoints() {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "portable.take" => Some(handling(db, |db, _| {
                Box::pin(async move { the_whole_site(&db).await })
            })),
            "portable.read-in" => Some(handling(db, |db, asked| {
                Box::pin(async move { read_a_site_in(&db, &asked).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            let needs = match endpoint.named {
                "portable.take" => mavi_portable::to_take(),
                _ => mavi_portable::to_read_one_in(),
            };

            site = site.mount(endpoint, Some(needs), handler);
        }
    }

    site
}

async fn the_whole_site(db: &Db) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let bundle = mavi_portable::store::take(&mut tx).await?;

    Ok(Answered::Read(
        serde_json::to_value(bundle).map_err(Error::internal)?,
    ))
}

async fn read_a_site_in(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let bundle: mavi_portable::Bundle = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_site_as_a_file")))?;

    let mut tx = db.begin().await?;
    let read = mavi_portable::store::read_in(&mut tx, &bundle).await?;

    // What was added and what was left alone, both. A receipt saying only that
    // somebody read a file in is one nobody can tell apart from a file that
    // did nothing.
    let receipt = wrote_about(
        &mut tx,
        asked,
        "portable.read-in",
        "site",
        None,
        &serde_json::json!({
            "writings": read.writings,
            "terms": read.terms,
            "languages": read.languages,
            "left_alone": read.left_alone,
        }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(read).map_err(Error::internal)?,
        receipt,
    ))
}
