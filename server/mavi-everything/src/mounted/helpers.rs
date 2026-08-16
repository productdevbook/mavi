use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use mavi_audit::{Actor, Who as Whom, record};
use mavi_core::error::{Error, Result};
use mavi_core::page::Query;
use mavi_core::ports::{Files, Seals};
use mavi_core::say::Say;
use mavi_db::{Db, Tx};
use mavi_http::{Answered, Caller};
use mavi_serve::{Asked, Handler};
use serde_json::Value;
use uuid::Uuid;

pub const THAT_IS_NOT_AN_ID: &str = "that_is_not_an_id";

pub type Answering = std::pin::Pin<Box<dyn Future<Output = Result<Answered<Value>>> + Send>>;

/// One handler, with the database it needs already in hand.
#[must_use]
pub fn handling(db: Db, what: fn(Db, Asked) -> Answering) -> Handler {
    Arc::new(move |asked| what(db.clone(), asked))
}

/// The same, for the handlers that also need somewhere to put files.
#[must_use]
pub fn with_files(
    db: Db,
    files: Arc<dyn Files>,
    what: fn(Db, Arc<dyn Files>, Asked) -> Answering,
) -> Handler {
    Arc::new(move |asked| what(db.clone(), Arc::clone(&files), asked))
}

/// The same, for the port that may not be there.
#[must_use]
pub fn with_seals(
    db: Db,
    seals: Option<Arc<dyn Seals>>,
    what: fn(Db, Option<Arc<dyn Seals>>, Asked) -> Answering,
) -> Handler {
    Arc::new(move |asked| what(db.clone(), seals.clone(), asked))
}

/// The id in the path, whatever the endpoint calls it.
pub fn a_uuid(asked: &Asked) -> Result<Uuid> {
    let id = asked
        .path
        .get("id")
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))?;

    Uuid::parse_str(id).map_err(|_| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))
}

/// The page a screen is asking for.
#[must_use]
pub fn asking(asked: &Asked) -> Query {
    Query {
        after: asked.query.get("after").cloned(),
        limit: asked.query.get("limit").and_then(|how| how.parse().ok()),
    }
}

/// Whoever is asking, as an id.
pub fn themselves(asked: &Asked) -> Result<Uuid> {
    asked
        .caller
        .id()
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))
}

/// Taking one thing away, written down the same way every time.
pub async fn took_it_away<F>(
    db: &Db,
    asked: &Asked,
    did: &'static str,
    about: &'static str,
    remove: F,
) -> Result<Answered<Value>>
where
    F: for<'a> FnOnce(&'a mut Tx, Uuid) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>,
{
    let id = a_uuid(asked)?;

    let mut tx = db.begin().await?;

    remove(&mut tx, id).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        did,
        about,
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

/// A receipt about anything, for the handlers whose subject is not a writing.
pub async fn wrote_about(
    tx: &mut Tx,
    asked: &Asked,
    did: &str,
    about: &str,
    about_id: Option<&str>,
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

    record(tx, &actor, did, about, about_id, what).await
}
