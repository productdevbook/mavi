use super::helpers::themselves;
// Domain route module: second

use std::sync::Arc;

use mavi_core::error::{Error, Result};
use mavi_core::ports::Seals;
use mavi_core::say::Say;
use mavi_db::Db;
use mavi_http::Answered;
use mavi_serve::{Asked, Handler, Site};
use serde_json::Value;

use super::helpers::{handling, with_seals, wrote_about};

/// The second thing somebody has.
#[must_use]
pub fn the_second_step(mut site: Site, db: &Db, seals: Option<&Arc<dyn Seals>>) -> Site {
    for endpoint in mavi_second::endpoints() {
        let db = db.clone();
        let seals = seals.cloned();

        let handler: Option<Handler> = match endpoint.named {
            "second.standing" => Some(handling(db, |db, asked| {
                Box::pin(async move { how_it_stands(&db, &asked).await })
            })),
            "second.set-up" => Some(with_seals(db, seals, |db, seals, asked| {
                Box::pin(async move { set_a_second_step_up(&db, seals.as_ref(), &asked).await })
            })),
            "second.confirm" => Some(with_seals(db, seals, |db, seals, asked| {
                Box::pin(async move { confirmed_it(&db, seals.as_ref(), &asked).await })
            })),
            "second.take-off" => Some(with_seals(db, seals, |db, seals, asked| {
                Box::pin(async move { took_it_off(&db, seals.as_ref(), &asked).await })
            })),
            "sessions.finish" => Some(with_seals(db, seals, |db, seals, asked| {
                Box::pin(async move { finished_signing_in(&db, seals.as_ref(), &asked).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            // Nothing. Every one of these is about whoever is asking and
            // nobody else — a grant on somebody's own second step would be a
            // grant they need in order to protect their own account.
            site = site.mount(endpoint, None, handler);
        }
    }

    site
}

/// What an installation with no sealing key says.
///
/// Said where somebody asks for a second step, rather than sealed with a key
/// baked into the source — which would be the appearance of the thing without
/// the thing.
/// What an installation with no sealing key says.
///
/// Said where somebody asks for a second step, rather than sealed with a key
/// baked into the source — which would be the appearance of the thing without
/// the thing.
fn nothing_seals() -> Error {
    Error::internal(std::io::Error::other(
        "this installation was given no sealing key, so it has no second step",
    ))
}

async fn how_it_stands(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let standing = mavi_second::store::standing(&mut tx, themselves(asked)?).await?;

    Ok(Answered::Read(
        serde_json::to_value(standing).map_err(Error::internal)?,
    ))
}

async fn set_a_second_step_up(
    db: &Db,
    seals: Option<&Arc<dyn Seals>>,
    asked: &Asked,
) -> Result<Answered<Value>> {
    let seals = seals.ok_or_else(nothing_seals)?;
    let person = themselves(asked)?;

    let mut tx = db.begin().await?;

    // What the site calls itself, so somebody with several rows in an
    // authenticator can tell which is which. A site that cannot say is still
    // one somebody can set a second step up on.
    let what_it_is_called = mavi_settings::store::read(&mut tx)
        .await
        .map_or_else(|_| "Mavi".to_owned(), |settings| settings.name);

    let account = mavi_people::store::one(&mut tx, person).await?.email;

    let to_set_up = mavi_second::store::set_up(
        &mut tx,
        seals.as_ref(),
        person,
        &what_it_is_called,
        &account,
    )
    .await?;

    // Started, not finished. The receipt says which, because a second step
    // somebody began and abandoned is a different thing from one that guards
    // an account.
    let receipt = wrote_about(
        &mut tx,
        asked,
        "second.set-up",
        "person",
        Some(&person.to_string()),
        &serde_json::json!({ "confirmed": false }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(to_set_up).map_err(Error::internal)?,
        receipt,
    ))
}

/// The digits somebody sent.
/// The digits somebody sent.
fn some_digits(asked: &Asked) -> String {
    asked.body["code"].as_str().unwrap_or_default().to_owned()
}

async fn confirmed_it(
    db: &Db,
    seals: Option<&Arc<dyn Seals>>,
    asked: &Asked,
) -> Result<Answered<Value>> {
    let seals = seals.ok_or_else(nothing_seals)?;
    let person = themselves(asked)?;

    let mut tx = db.begin().await?;

    let ways = mavi_second::store::confirm(
        &mut tx,
        seals.as_ref(),
        person,
        &some_digits(asked),
        chrono::Utc::now(),
    )
    .await?;

    // How many, never which. A receipt carrying the codes would be the copy
    // that outlives showing them once.
    let receipt = wrote_about(
        &mut tx,
        asked,
        "second.confirm",
        "person",
        Some(&person.to_string()),
        &serde_json::json!({ "ways_back_in": ways.codes.len() }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(ways).map_err(Error::internal)?,
        receipt,
    ))
}

async fn took_it_off(
    db: &Db,
    seals: Option<&Arc<dyn Seals>>,
    asked: &Asked,
) -> Result<Answered<Value>> {
    let seals = seals.ok_or_else(nothing_seals)?;
    let person = themselves(asked)?;

    let mut tx = db.begin().await?;

    // The digits, before it comes off. Taking it off is the first thing
    // somebody who stole a session would do, so this door asks for the phone
    // as well.
    let past = mavi_second::store::gets_past(
        &mut tx,
        seals.as_ref(),
        person,
        &some_digits(asked),
        chrono::Utc::now(),
    )
    .await?;

    if !past {
        return Err(Error::invalid(Say::of(
            mavi_second::store::THAT_IS_NOT_THE_RIGHT_CODE,
        )));
    }

    mavi_second::store::take_it_off(&mut tx, person).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "second.take-off",
        "person",
        Some(&person.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

async fn finished_signing_in(
    db: &Db,
    seals: Option<&Arc<dyn Seals>>,
    asked: &Asked,
) -> Result<Answered<Value>> {
    let seals = seals.ok_or_else(nothing_seals)?;

    let moment = asked.body["moment"].as_str().unwrap_or_default().to_owned();
    let code = some_digits(asked);

    let mut tx = db.begin().await?;

    // The moment is spent whether or not the digits are right. Otherwise a
    // moment is a thing somebody can try codes against until one works.
    let person = mavi_people::store::redeem(
        &mut tx,
        &moment,
        mavi_people::ticket::For::AMomentToFinish,
        None,
    )
    .await
    .map_err(|_| {
        Error::forbidden(Say::of(
            mavi_people::store::THAT_IS_NOT_AN_ADDRESS_AND_A_PASSWORD,
        ))
    })?;

    let past =
        mavi_second::store::gets_past(&mut tx, seals.as_ref(), person, &code, chrono::Utc::now())
            .await?;

    if !past {
        return Err(Error::forbidden(Say::of(
            mavi_people::store::THAT_IS_NOT_AN_ADDRESS_AND_A_PASSWORD,
        )));
    }

    let (person, token) = mavi_people::store::finish(&mut tx, person).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "sessions.finish",
        "person",
        Some(&person.id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::json!({ "person": person, "token": token }),
        receipt,
    ))
}
