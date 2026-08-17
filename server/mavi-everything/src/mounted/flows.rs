// Domain route module: flows

use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use mavi_db::Db;
use mavi_http::Answered;
use mavi_serve::{Asked, Handler, Site};
use serde_json::Value;

use super::helpers::{a_uuid, asking, handling, wrote_about};

/// Flows, and what they have done.
#[must_use]
pub fn what_it_does_by_itself(mut site: Site, db: &Db) -> Site {
    for endpoint in mavi_flows::endpoints() {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "flows.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { flows(&db, &asked).await })
            })),
            // The one answer in this whole file that is not a query: what can
            // start a flow is a fact about the code.
            "flows.triggers" => Some(handling(db, |_, _| Box::pin(async move { Ok(triggers()) }))),
            "flows.make" => Some(handling(db, |db, asked| {
                Box::pin(async move { arranged_a_flow(&db, &asked).await })
            })),
            "flows.change" => Some(handling(db, |db, asked| {
                Box::pin(async move { changed_a_flow(&db, &asked).await })
            })),
            "flows.remove" => Some(handling(db, |db, asked| {
                Box::pin(async move { removed_a_flow(&db, &asked).await })
            })),
            "runs.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { runs(&db, &asked).await })
            })),
            "runs.read" => Some(handling(db, |db, asked| {
                Box::pin(async move { one_run(&db, &asked).await })
            })),
            "flows.try" => Some(handling(db, |db, asked| {
                Box::pin(async move { tried_a_flow(&db, &asked).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            let needs = if endpoint.changes {
                mavi_flows::to_write()
            } else {
                mavi_flows::to_read()
            };

            site = site.mount(endpoint, Some(needs), handler);
        }
    }

    site
}

/// The site's own project, and what goes live.
async fn flows(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let page = mavi_flows::store::list(&mut tx, &asking(asked)).await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

/// Everything that can start a flow, and what each one may name.
///
/// Answered rather than written in a manual: a panel that has to know the list
/// is a panel that goes out of date on its own.
/// Everything that can start a flow, and what each one may name.
///
/// Answered rather than written in a manual: a panel that has to know the list
/// is a panel that goes out of date on its own.
fn triggers() -> Answered<Value> {
    let triggers: Vec<Value> = mavi_flows::step::TRIGGERS
        .iter()
        .map(|trigger| serde_json::json!({ "name": trigger.as_str() }))
        .collect();

    let does: Vec<Value> = [
        mavi_flows::Does::SendALetter,
        mavi_flows::Does::CallAnAddress,
        mavi_flows::Does::Wait,
        mavi_flows::Does::PutOnAList,
    ]
    .iter()
    .map(|does| serde_json::json!({ "name": does.as_str(), "needs": does.needs() }))
    .collect();

    Answered::Read(serde_json::json!({ "triggers": triggers, "does": does }))
}

async fn arranged_a_flow(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let new: mavi_flows::store::NewFlow = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_flow")))?;

    let mut tx = db.begin().await?;
    let flow = mavi_flows::store::make(&mut tx, &new).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "flows.make",
        "flow",
        Some(&flow.id.to_string()),
        &serde_json::json!({ "trigger": flow.trigger, "steps": flow.steps.len() }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(flow).map_err(Error::internal)?,
        receipt,
    ))
}

async fn changed_a_flow(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let changes: mavi_flows::store::FlowChanges = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_change_to_a_flow")))?;

    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;
    let flow = mavi_flows::store::change(&mut tx, id, &changes).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "flows.change",
        "flow",
        Some(&id.to_string()),
        &serde_json::json!({ "on": flow.on }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(flow).map_err(Error::internal)?,
        receipt,
    ))
}

async fn removed_a_flow(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;

    mavi_flows::store::remove(&mut tx, id).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "flows.remove",
        "flow",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

async fn runs(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let page = mavi_flows::store::runs(
        &mut tx,
        a_uuid(asked)?,
        asked.query.get("state").map(String::as_str),
        &asking(asked),
    )
    .await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn one_run(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let run = mavi_flows::store::a_run_of_it(&mut tx, a_uuid(asked)?).await?;

    Ok(Answered::Read(
        serde_json::to_value(run).map_err(Error::internal)?,
    ))
}

async fn tried_a_flow(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let would = mavi_flows::store::would_do(&mut tx, a_uuid(asked)?, &asked.body).await?;

    // Nothing left the machine and no run was written, so there is nothing to
    // record. A `POST` because it carries what to try it against.
    Ok(Answered::Read(
        serde_json::to_value(would).map_err(Error::internal)?,
    ))
}
