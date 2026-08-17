use mavi_core::page::Query;
// Domain route module: taxonomy

use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use mavi_db::Db;
use mavi_http::Answered;
use mavi_serve::{Asked, Handler, Site};
use serde_json::Value;
use uuid::Uuid;

use super::helpers::{a_uuid, handling, wrote_about};

/// Categories and tags, and what is filed under them.
#[must_use]
pub fn what_it_files_things_under(mut site: Site, db: &Db) -> Site {
    for endpoint in mavi_taxonomy::endpoints() {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "terms.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { terms(&db, &asked).await })
            })),
            "terms.make" => Some(handling(db, |db, asked| {
                Box::pin(async move { made_a_term(&db, &asked).await })
            })),
            "terms.change" => Some(handling(db, |db, asked| {
                Box::pin(async move { changed_a_term(&db, &asked).await })
            })),
            "terms.remove" => Some(handling(db, |db, asked| {
                Box::pin(async move { removed_a_term(&db, &asked).await })
            })),
            "writings.file-under" => Some(handling(db, |db, asked| {
                Box::pin(async move { filed_under(&db, &asked).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            let needs = if endpoint.changes {
                mavi_taxonomy::to_write()
            } else {
                mavi_taxonomy::to_read()
            };

            site = site.mount(endpoint, Some(needs), handler);
        }
    }

    site
}

/// Posts, pages, and whatever else a site decides a thing is.
async fn terms(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let sort = match asked.query.get("sort").map(String::as_str) {
        Some("tag") => Some(mavi_taxonomy::Sort::Tag),
        Some("category") => Some(mavi_taxonomy::Sort::Category),
        _ => None,
    };

    let query = Query {
        after: asked.query.get("after").cloned(),
        limit: asked.query.get("limit").and_then(|how| how.parse().ok()),
    };

    let mut tx = db.begin().await?;
    let page = mavi_taxonomy::store::list(
        &mut tx,
        sort,
        asked.query.get("language").map(String::as_str),
        &query,
    )
    .await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn made_a_term(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let new: mavi_taxonomy::store::NewTerm = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_term")))?;

    let mut tx = db.begin().await?;
    let term = mavi_taxonomy::store::make(&mut tx, &new).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "terms.make",
        "term",
        Some(&term.id.to_string()),
        &serde_json::json!({ "sort": term.sort.as_str() }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(term).map_err(Error::internal)?,
        receipt,
    ))
}

async fn changed_a_term(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let changes: mavi_taxonomy::store::TermChanges = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_change_to_a_term")))?;

    let id = mavi_taxonomy::term::TermId(a_uuid(asked)?);

    let mut tx = db.begin().await?;
    let term = mavi_taxonomy::store::change(&mut tx, id, &changes).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "terms.change",
        "term",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(term).map_err(Error::internal)?,
        receipt,
    ))
}

async fn removed_a_term(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = mavi_taxonomy::term::TermId(a_uuid(asked)?);

    let mut tx = db.begin().await?;
    mavi_taxonomy::store::remove(&mut tx, id).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "terms.remove",
        "term",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

async fn filed_under(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let writing = a_uuid(asked)?;

    let terms: Vec<Uuid> = asked.body["terms"]
        .as_array()
        .map(|terms| {
            terms
                .iter()
                .filter_map(|term| term.as_str())
                .filter_map(|term| Uuid::parse_str(term).ok())
                .collect()
        })
        .unwrap_or_default();

    let mut tx = db.begin().await?;
    let filed = mavi_taxonomy::store::file_under(&mut tx, writing, &terms).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "writings.file-under",
        "writing",
        Some(&writing.to_string()),
        &serde_json::json!({ "under": terms.len() }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(filed).map_err(Error::internal)?,
        receipt,
    ))
}
