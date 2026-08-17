// Domain route module: forms

use mavi_api::Who;
use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use mavi_db::Db;
use mavi_http::Answered;
use mavi_serve::{Asked, Handler, Site};
use serde_json::Value;

use super::helpers::{THAT_IS_NOT_AN_ID, a_uuid, asking, handling, wrote_about};

/// The forms, and what people sent them — the one domain here whose writing
/// side is open to anybody at all.
#[must_use]
pub fn what_it_asks_people(mut site: Site, db: &Db) -> Site {
    for endpoint in mavi_forms::endpoints() {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "forms.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { forms(&db, &asked).await })
            })),
            "forms.make" => Some(handling(db, |db, asked| {
                Box::pin(async move { made_a_form(&db, &asked).await })
            })),
            "forms.read" => Some(handling(db, |db, asked| {
                Box::pin(async move { one_form(&db, &asked).await })
            })),
            "forms.change" => Some(handling(db, |db, asked| {
                Box::pin(async move { changed_a_form(&db, &asked).await })
            })),
            "forms.remove" => Some(handling(db, |db, asked| {
                Box::pin(async move { removed_a_form(&db, &asked).await })
            })),
            "forms.filled" => Some(handling(db, |db, asked| {
                Box::pin(async move { what_came_in(&db, &asked).await })
            })),
            "forms.mark-seen" => Some(handling(db, |db, asked| {
                Box::pin(async move { all_seen(&db, &asked).await })
            })),
            "filled.forget" => Some(handling(db, |db, asked| {
                Box::pin(async move { forget_one(&db, &asked).await })
            })),
            "open.form" => Some(handling(db, |db, asked| {
                Box::pin(async move { an_open_form(&db, &asked).await })
            })),
            "open.fill-in" => Some(handling(db, |db, asked| {
                Box::pin(async move { filled_one_in(&db, &asked).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            let needs = match (endpoint.who, endpoint.changes) {
                (Who::Anybody, _) => None,
                (_, true) => Some(mavi_forms::to_write()),
                (_, false) => Some(mavi_forms::to_read()),
            };

            site = site.mount(endpoint, needs, handler);
        }
    }

    site
}

/// Boards, and where a card sits on one.
async fn forms(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let page = mavi_forms::store::list(&mut tx, &asking(asked)).await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn made_a_form(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let new: mavi_forms::store::NewForm = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_form")))?;

    let mut tx = db.begin().await?;
    let form = mavi_forms::store::make(&mut tx, &new).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "forms.make",
        "form",
        Some(&form.id.to_string()),
        &serde_json::json!({ "asks": form.fields.fields().len() }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(form).map_err(Error::internal)?,
        receipt,
    ))
}

async fn one_form(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let form = mavi_forms::store::read(&mut tx, a_uuid(asked)?).await?;

    Ok(Answered::Read(
        serde_json::to_value(form).map_err(Error::internal)?,
    ))
}

async fn changed_a_form(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let changes: mavi_forms::store::FormChanges = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_change_to_a_form")))?;

    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;
    let form = mavi_forms::store::change(&mut tx, id, &changes).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "forms.change",
        "form",
        Some(&id.to_string()),
        &serde_json::json!({ "open": form.open }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(form).map_err(Error::internal)?,
        receipt,
    ))
}

async fn removed_a_form(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;

    mavi_forms::store::remove(&mut tx, id).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "forms.remove",
        "form",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

async fn what_came_in(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let unseen = asked.query.get("unseen").is_some_and(|said| said == "true");

    let mut tx = db.begin().await?;
    let page =
        mavi_forms::store::what_came_in(&mut tx, a_uuid(asked)?, unseen, &asking(asked)).await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn all_seen(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;

    // Up to this moment, taken here rather than in the query, so that what is
    // marked read is what the person was actually looking at.
    let seen = mavi_forms::store::all_seen(&mut tx, id, chrono::Utc::now()).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "forms.mark-seen",
        "form",
        Some(&id.to_string()),
        &serde_json::json!({ "seen": seen }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::json!({ "seen": seen }),
        receipt,
    ))
}

async fn forget_one(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;

    mavi_forms::store::forget(&mut tx, id).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "filled.forget",
        "filled",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

async fn an_open_form(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let slug = asked
        .path
        .get("slug")
        .cloned()
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))?;

    let mut tx = db.begin().await?;
    let (_, form, _) = mavi_forms::store::open_form(&mut tx, &slug).await?;

    Ok(Answered::Read(
        serde_json::to_value(form).map_err(Error::internal)?,
    ))
}

async fn filled_one_in(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let slug = asked
        .path
        .get("slug")
        .cloned()
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))?;

    let filled: mavi_forms::Filled = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_what_a_form_takes")))?;

    let mut tx = db.begin().await?;
    let id = mavi_forms::store::fill_in(&mut tx, &slug, &filled, None).await?;

    // A visitor has no account, so what is recorded is the submission itself
    // and the machine as who did it. "Nobody did this" is an answer somebody
    // will need one day.
    let receipt = wrote_about(
        &mut tx,
        asked,
        "open.fill-in",
        "filled",
        Some(&id.to_string()),
        // Never what they wrote. The record is that something came in; what
        // they said is in the row, behind the grant that reads it.
        &serde_json::json!({ "form": slug }),
    )
    .await?;

    tx.commit().await?;

    // What a visitor is told: that it arrived. Nothing about the site, and
    // nothing about what else is on it.
    Ok(Answered::Changed(serde_json::json!({ "id": id }), receipt))
}
