// Domain route module: mail

use mavi_api::Who;
use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use mavi_db::Db;
use mavi_http::Answered;
use mavi_serve::{Asked, Handler, Site};
use serde_json::Value;

use super::helpers::{THAT_IS_NOT_AN_ID, a_uuid, asking, handling, wrote_about};

/// The site's own letters, its lists, and the way out of them.
#[must_use]
pub fn what_it_writes_to_people(mut site: Site, db: &Db) -> Site {
    for endpoint in mavi_mail::endpoints() {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "letters.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { letters(&db, &asked).await })
            })),
            "letters.write" => Some(handling(db, |db, asked| {
                Box::pin(async move { wrote_a_letter(&db, &asked).await })
            })),
            "letters.forget" => Some(handling(db, |db, asked| {
                Box::pin(async move { forgot_a_letter(&db, &asked).await })
            })),
            "letters.press" => Some(handling(db, |db, asked| {
                Box::pin(async move { pressed_a_letter(&db, &asked).await })
            })),
            "lists.list" => Some(handling(db, |db, _| {
                Box::pin(async move { lists(&db).await })
            })),
            "lists.make" => Some(handling(db, |db, asked| {
                Box::pin(async move { made_a_list(&db, &asked).await })
            })),
            "readers.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { readers(&db, &asked).await })
            })),
            "readers.add" => Some(handling(db, |db, asked| {
                Box::pin(async move { added_a_reader(&db, &asked).await })
            })),
            "readers.forget" => Some(handling(db, |db, asked| {
                Box::pin(async move { forgot_a_reader(&db, &asked).await })
            })),
            "sendings.send" => Some(handling(db, |db, asked| {
                Box::pin(async move { sent_to_a_list(&db, &asked).await })
            })),
            "open.unsubscribe" => Some(handling(db, |db, asked| {
                Box::pin(async move { took_themselves_off(&db, &asked).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            let needs = match (endpoint.who, endpoint.changes) {
                (Who::Anybody, _) => None,
                (_, true) => Some(mavi_mail::to_write()),
                (_, false) => Some(mavi_mail::to_read()),
            };

            site = site.mount(endpoint, needs, handler);
        }
    }

    site
}

/// Flows, and what they have done.
/// Which language a screen is asking about. The site's own is `en` until
/// something says otherwise, and a letter answered in no language at all is a
/// screen with nothing on it.
fn in_which_language(asked: &Asked) -> String {
    asked
        .query
        .get("language")
        .cloned()
        .unwrap_or_else(|| "en".to_owned())
}

async fn letters(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let letters = mavi_mail::store::letters(&mut tx, &in_which_language(asked)).await?;

    Ok(Answered::Read(
        serde_json::to_value(letters).map_err(Error::internal)?,
    ))
}

async fn wrote_a_letter(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let kind = asked
        .path
        .get("kind")
        .cloned()
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))?;

    let language = asked.body["language"].as_str().unwrap_or("en").to_owned();
    let subject = asked.body["subject"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let body = asked.body["body"].as_str().unwrap_or_default().to_owned();

    let mut tx = db.begin().await?;
    let letter = mavi_mail::store::write(&mut tx, &kind, &language, &subject, &body).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "letters.write",
        "letter",
        Some(&kind),
        &serde_json::json!({ "language": language }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(letter).map_err(Error::internal)?,
        receipt,
    ))
}

async fn forgot_a_letter(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let kind = asked
        .path
        .get("kind")
        .cloned()
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))?;

    let language = in_which_language(asked);

    let mut tx = db.begin().await?;
    mavi_mail::store::forget(&mut tx, &kind, &language).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "letters.forget",
        "letter",
        Some(&kind),
        &serde_json::json!({ "language": language }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

async fn pressed_a_letter(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let kind = asked
        .path
        .get("kind")
        .cloned()
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))?;

    let values: Vec<(String, String)> = asked.body["values"]
        .as_object()
        .map(|values| {
            values
                .iter()
                .map(|(name, what)| {
                    (
                        name.clone(),
                        what.as_str()
                            .map_or_else(|| what.to_string(), ToOwned::to_owned),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    let borrowed: Vec<(&str, String)> = values
        .iter()
        .map(|(name, what)| (name.as_str(), what.clone()))
        .collect();

    let mut tx = db.begin().await?;
    let pressed =
        mavi_mail::store::pressed(&mut tx, &kind, &in_which_language(asked), &borrowed).await?;

    // Nothing left the machine, so there is nothing to record.
    Ok(Answered::Read(
        serde_json::to_value(pressed).map_err(Error::internal)?,
    ))
}

async fn lists(db: &Db) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let lists = mavi_mail::store::lists(&mut tx).await?;

    Ok(Answered::Read(
        serde_json::to_value(lists).map_err(Error::internal)?,
    ))
}

async fn made_a_list(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let name = asked.body["name"].as_str().unwrap_or_default().to_owned();

    let mut tx = db.begin().await?;
    let list = mavi_mail::store::add_list(&mut tx, &name).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "lists.make",
        "list",
        Some(&list.id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(list).map_err(Error::internal)?,
        receipt,
    ))
}

async fn readers(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let page = mavi_mail::store::readers(
        &mut tx,
        a_uuid(asked)?,
        asked.query.get("standing").map(String::as_str),
        &asking(asked),
    )
    .await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn added_a_reader(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let list = a_uuid(asked)?;
    let email = asked.body["email"].as_str().unwrap_or_default().to_owned();
    let name = asked.body["name"].as_str().map(ToOwned::to_owned);

    let mut tx = db.begin().await?;
    let reader = mavi_mail::store::add_reader(&mut tx, list, &email, name.as_deref()).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "readers.add",
        "reader",
        Some(&reader.id.to_string()),
        &serde_json::json!({ "list": list }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(reader).map_err(Error::internal)?,
        receipt,
    ))
}

async fn forgot_a_reader(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;

    mavi_mail::store::forget_reader(&mut tx, id).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "readers.forget",
        "reader",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

async fn sent_to_a_list(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let list = a_uuid(asked)?;

    let sending: mavi_mail::store::NewSending = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_something_to_send")))?;

    // Checked before anybody is looked up: a letter to a list that does not
    // say how to leave it is refused, and refusing it after working out who it
    // would go to is a longer way to the same answer.
    let sending = mavi_mail::Sending::checked(&sending.subject, &sending.body)?;

    let mut tx = db.begin().await?;
    let going_to = mavi_mail::store::who_it_goes_to(&mut tx, list).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "sendings.send",
        "list",
        Some(&list.to_string()),
        // How many, not who: a receipt is read by whoever asks what happened,
        // and a list of addresses in it is the list itself, copied.
        &serde_json::json!({ "letters": going_to.len(), "subject": sending.subject }),
    )
    .await?;

    tx.commit().await?;

    // What comes back is that it has been taken, and how many it is for. The
    // queue is what sends them, one at a time, so nothing here waits on a mail
    // host.
    Ok(Answered::Changed(
        serde_json::json!({ "letters": going_to.len() }),
        receipt,
    ))
}

async fn took_themselves_off(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let token = asked
        .path
        .get("token")
        .cloned()
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))?;

    let mut tx = db.begin().await?;
    mavi_mail::store::out(&mut tx, &token).await?;

    // Recorded as the machine's doing: whoever pressed the link has no account
    // and no name here, and that is the point of the link.
    let receipt = wrote_about(
        &mut tx,
        asked,
        "open.unsubscribe",
        "reader",
        None,
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}
