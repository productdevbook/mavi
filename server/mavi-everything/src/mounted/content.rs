// Domain route module: content

use mavi_audit::{Actor, Who as Whom, record};
use mavi_content::listing::Filter;
use mavi_content::store::{self, Changes};
use mavi_content::writing::{New, WritingId};
use mavi_core::error::{Error, Result};
use mavi_core::page::Query;
use mavi_core::say::Say;
use mavi_db::{Db, Tx};
use mavi_http::{Answered, Caller};
use mavi_serve::{Asked, Handler, Site};
use serde_json::Value;
use uuid::Uuid;

use super::helpers::{THAT_IS_NOT_AN_ID, handling, wrote_about};

/// Posts, pages, and whatever else a site decides a thing is.
#[must_use]
pub fn what_it_wrote(mut site: Site, db: &Db) -> Site {
    for endpoint in mavi_content::endpoints() {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "kinds.list" => Some(handling(db, |db, _| {
                Box::pin(async move { the_kinds(&db).await })
            })),
            "kinds.declare" => Some(handling(db, |db, asked| {
                Box::pin(async move { declared_a_kind(&db, &asked).await })
            })),
            "kinds.stop-saying" => Some(handling(db, |db, asked| {
                Box::pin(async move { stopped_saying(&db, &asked).await })
            })),
            "writings.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { listed(&db, &asked).await })
            })),
            "writings.read" => Some(handling(db, |db, asked| {
                Box::pin(async move { one(&db, &asked).await })
            })),
            "writings.write" => Some(handling(db, |db, asked| {
                Box::pin(async move { made(&db, &asked).await })
            })),
            "writings.change" => Some(handling(db, |db, asked| {
                Box::pin(async move { changed(&db, &asked).await })
            })),
            "writings.throw-away" => Some(handling(db, |db, asked| {
                Box::pin(async move { thrown(&db, &asked).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            let needs = if endpoint.changes {
                mavi_content::to_write()
            } else {
                mavi_content::to_read()
            };

            site = site.mount(endpoint, Some(needs), handler);
        }
    }

    site
}

/// One handler, with the database it needs already in hand.
async fn listed(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;

    let filter = Filter {
        kind: asked.query.get("kind").cloned(),
        language: asked.query.get("language").cloned(),
        state: asked.query.get("state").cloned(),
    };

    let query = Query {
        after: asked.query.get("after").cloned(),
        limit: asked.query.get("limit").and_then(|how| how.parse().ok()),
    };

    let page = store::list(&mut tx, false, &filter, &query).await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn one(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let writing = store::read(&mut tx, which(asked)?).await?;

    Ok(Answered::Read(
        serde_json::to_value(writing).map_err(Error::internal)?,
    ))
}

async fn made(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let new: New = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_writing")))?;

    let mut tx = db.begin().await?;
    let writing = store::make(&mut tx, &new).await?;

    // In the same transaction as the change. If the commit below never
    // happens, neither the writing nor the record of it exists.
    let receipt = wrote(
        &mut tx,
        asked,
        "writings.write",
        &writing.id,
        &serde_json::json!({
            "kind": writing.kind.as_str(),
            "slug": writing.slug.as_str(),
        }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(writing).map_err(Error::internal)?,
        receipt,
    ))
}

async fn changed(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let changes: Changes = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_change_to_a_writing")))?;

    let id = which(asked)?;
    let mut tx = db.begin().await?;
    let writing = store::change(&mut tx, id, &changes).await?;

    let receipt = wrote(
        &mut tx,
        asked,
        "writings.change",
        &id,
        &serde_json::json!({ "state": writing.state.as_str() }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(writing).map_err(Error::internal)?,
        receipt,
    ))
}

async fn thrown(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = which(asked)?;
    let mut tx = db.begin().await?;

    store::remove(&mut tx, id).await?;

    let receipt = wrote(
        &mut tx,
        asked,
        "writings.throw-away",
        &id,
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

/// Which one the path is about.
/// Which one the path is about.
fn which(asked: &Asked) -> Result<WritingId> {
    let id = asked
        .path
        .get("id")
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))?;

    Uuid::parse_str(id)
        .map(WritingId)
        .map_err(|_| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))
}

/// The receipt, written where the change is being made.
///
/// What it is called is the endpoint's own name, so that "what happened to
/// this" has one answer rather than one per call site.
/// The receipt, written where the change is being made.
///
/// What it is called is the endpoint's own name, so that "what happened to
/// this" has one answer rather than one per call site.
async fn wrote(
    tx: &mut Tx,
    asked: &Asked,
    did: &str,
    about: &WritingId,
    what: &Value,
) -> Result<mavi_audit::Receipt> {
    let actor = match &asked.caller {
        Caller::AnAccount { id, .. } => Actor {
            who: Whom::AnAccount,
            id: Some(id.clone()),
            request: "a-request".to_owned(),
        },
        Caller::AStudent { id } => Actor {
            who: Whom::AStudent,
            id: Some(id.clone()),
            request: "a-request".to_owned(),
        },
        Caller::Nobody => Actor::the_machine("a-request"),
    };

    record(tx, &actor, did, "writing", Some(&about.to_string()), what).await
}

/// Which kind an address named.
fn which_kind(asked: &Asked) -> String {
    asked.path.get("kind").cloned().unwrap_or_default()
}

async fn the_kinds(db: &Db) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let kinds = mavi_content::kinds::every(&mut tx).await?;

    Ok(Answered::Read(
        serde_json::to_value(kinds).map_err(Error::internal)?,
    ))
}

async fn declared_a_kind(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let kind = which_kind(asked);
    let said: mavi_content::kinds::Declaring = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of(mavi_content::kinds::THAT_IS_NOT_A_KIND)))?;

    let mut tx = db.begin().await?;
    let declared = mavi_content::kinds::declare(&mut tx, &kind, &said).await?;

    // What it asks for now, in the receipt. What somebody needs a year later
    // is what the shape was, not that somebody edited it.
    let receipt = wrote_about(
        &mut tx,
        asked,
        "kinds.declare",
        "kind",
        Some(&declared.kind),
        &serde_json::json!({ "fields": declared.fields.fields().len() }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(declared).map_err(Error::internal)?,
        receipt,
    ))
}

async fn stopped_saying(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let kind = which_kind(asked);

    let mut tx = db.begin().await?;

    mavi_content::kinds::stop_saying(&mut tx, &kind).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "kinds.stop-saying",
        "kind",
        Some(&kind),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}
