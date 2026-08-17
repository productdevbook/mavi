use super::helpers::THAT_IS_NOT_AN_ID;
// Domain route module: settings

use mavi_api::Who;
use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use mavi_db::Db;
use mavi_http::Answered;
use mavi_serve::{Asked, Handler, Site};
use serde_json::Value;

use super::helpers::{handling, wrote_about};

/// The site's own name, and what it writes in.
#[must_use]
pub fn what_this_site_is(mut site: Site, db: &Db) -> Site {
    for endpoint in mavi_settings::endpoints() {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "settings.read" => Some(handling(db, |db, _| {
                Box::pin(async move { read_settings(&db).await })
            })),
            "settings.change" => Some(handling(db, |db, asked| {
                Box::pin(async move { change_settings(&db, &asked).await })
            })),
            "languages.list" => Some(handling(db, |db, _| {
                Box::pin(async move { languages(&db).await })
            })),
            "languages.add" => Some(handling(db, |db, asked| {
                Box::pin(async move { add_a_language(&db, &asked).await })
            })),
            "languages.make-own" => Some(handling(db, |db, asked| {
                Box::pin(async move { make_it_ours(&db, &asked).await })
            })),
            "languages.forget" => Some(handling(db, |db, asked| {
                Box::pin(async move { forget_a_language(&db, &asked).await })
            })),
            "open.site" => Some(handling(db, |db, _| {
                Box::pin(async move { public_site(&db).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            let needs = match (endpoint.who, endpoint.changes) {
                // What a page reads about the site is open to anybody, so it
                // asks for nothing held.
                (Who::Anybody, _) => None,
                (_, true) => Some(mavi_settings::to_write()),
                (_, false) => Some(mavi_settings::to_read()),
            };

            site = site.mount(endpoint, needs, handler);
        }
    }

    site
}

/// Categories and tags, and what is filed under them.
async fn read_settings(db: &Db) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let settings = mavi_settings::store::read(&mut tx).await?;

    Ok(Answered::Read(
        serde_json::to_value(settings).map_err(Error::internal)?,
    ))
}

async fn change_settings(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let changes: mavi_settings::store::SettingsChanges = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_change_to_this_site")))?;

    let mut tx = db.begin().await?;
    let settings = mavi_settings::store::change(&mut tx, &changes).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "settings.change",
        "settings",
        None,
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(settings).map_err(Error::internal)?,
        receipt,
    ))
}

async fn languages(db: &Db) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let writing_in = mavi_settings::store::languages(&mut tx).await?;

    Ok(Answered::Read(
        serde_json::to_value(writing_in).map_err(Error::internal)?,
    ))
}

async fn add_a_language(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let tag = asked.body["tag"].as_str().unwrap_or_default().to_owned();
    let name = asked.body["name"].as_str().unwrap_or_default().to_owned();

    let mut tx = db.begin().await?;
    let language = mavi_settings::store::add(&mut tx, &tag, &name).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "languages.add",
        "language",
        Some(&tag),
        &serde_json::json!({ "name": name }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(language).map_err(Error::internal)?,
        receipt,
    ))
}

async fn make_it_ours(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let tag = asked
        .path
        .get("tag")
        .cloned()
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))?;

    let mut tx = db.begin().await?;
    let writing_in = mavi_settings::store::make_it_ours(&mut tx, &tag).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "languages.make-own",
        "language",
        Some(&tag),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(writing_in).map_err(Error::internal)?,
        receipt,
    ))
}

async fn forget_a_language(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let tag = asked
        .path
        .get("tag")
        .cloned()
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))?;

    let mut tx = db.begin().await?;
    mavi_settings::store::forget(&mut tx, &tag).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "languages.forget",
        "language",
        Some(&tag),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

async fn public_site(db: &Db) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let site = mavi_settings::store::public(&mut tx).await?;

    Ok(Answered::Read(
        serde_json::to_value(site).map_err(Error::internal)?,
    ))
}
