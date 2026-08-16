use super::helpers::{a_uuid, handling};
use mavi_core::page::Query;
// Domain route module: audit

use mavi_core::error::{Error, Result};
use mavi_db::Db;
use mavi_http::Answered;
use mavi_serve::{Asked, Handler, Site};
use serde_json::Value;

/// What was done here, which is read and never written through the API.
#[must_use]
pub fn what_has_been_done(mut site: Site, db: &Db) -> Site {
    for endpoint in mavi_audit::endpoints() {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "audit.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { what_was_done(&db, &asked).await })
            })),
            "audit.read" => Some(handling(db, |db, asked| {
                Box::pin(async move { one_receipt(&db, &asked).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            site = site.mount(endpoint, Some(mavi_audit::to_read()), handler);
        }
    }

    site
}

/// How many people read the site.
///
/// The beacon writes and answers nothing; everything that reads needs an
/// account. A reader's browser is not something to answer questions about the
/// site to.
async fn what_was_done(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let query = Query {
        after: asked.query.get("after").cloned(),
        limit: asked.query.get("limit").and_then(|how| how.parse().ok()),
    };

    let mut tx = db.begin().await?;
    let page = mavi_audit::reading::list(
        &mut tx,
        asked.query.get("about").map(String::as_str),
        asked.query.get("about_id").map(String::as_str),
        asked.query.get("who_id").map(String::as_str),
        &query,
    )
    .await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn one_receipt(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let written = mavi_audit::reading::read(&mut tx, a_uuid(asked)?).await?;

    Ok(Answered::Read(
        serde_json::to_value(written).map_err(Error::internal)?,
    ))
}
