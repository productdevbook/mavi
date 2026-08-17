// Domain route module: trash

use mavi_core::error::{Error, Result};
use mavi_db::Db;
use mavi_http::Answered;
use mavi_serve::{Asked, Handler, Site};
use serde_json::Value;

use super::helpers::{a_uuid, handling, wrote_about};

/// What a site threw away.
#[must_use]
pub fn what_it_threw_away(mut site: Site, db: &Db) -> Site {
    for endpoint in mavi_trash::endpoints() {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "trash.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { in_the_bin(&db, &asked).await })
            })),
            "trash.put-back" => Some(handling(db, |db, asked| {
                Box::pin(async move { put_it_back(&db, &asked).await })
            })),
            "trash.for-good" => Some(handling(db, |db, asked| {
                Box::pin(async move { gone_for_good(&db, &asked).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            let needs = match endpoint.named {
                "trash.list" => mavi_trash::to_read(),
                _ => mavi_trash::to_change(),
            };

            site = site.mount(endpoint, Some(needs), handler);
        }
    }

    site
}

/// Which sort, parsed rather than passed along. Everything below this takes a
/// `Kind`, so nothing somebody sent reaches a query.
/// Which sort, parsed rather than passed along. Everything below this takes a
/// `Kind`, so nothing somebody sent reaches a query.
fn which_sort(asked: &Asked) -> Result<mavi_trash::Kind> {
    mavi_trash::Kind::parse(asked.path.get("sort").map_or("", String::as_str))
}

async fn in_the_bin(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let how_many = asked
        .query
        .get("how_many")
        .and_then(|how_many| how_many.parse::<i64>().ok())
        .unwrap_or(50)
        .clamp(1, mavi_trash::AT_MOST);

    let mut tx = db.begin().await?;
    let thrown = mavi_trash::store::everything(&mut tx, how_many).await?;

    Ok(Answered::Read(
        serde_json::to_value(thrown).map_err(Error::internal)?,
    ))
}

async fn put_it_back(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let sort = which_sort(asked)?;
    let id = a_uuid(asked)?;

    let mut tx = db.begin().await?;

    mavi_trash::store::put_back(&mut tx, sort, id).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "trash.put-back",
        sort.as_str(),
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

async fn gone_for_good(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let sort = which_sort(asked)?;
    let id = a_uuid(asked)?;

    let mut tx = db.begin().await?;

    // The receipt before the row goes, because after it there is nothing left
    // to say what was taken away — and this is the one deletion in the whole
    // API that nothing can be brought back from.
    let receipt = wrote_about(
        &mut tx,
        asked,
        "trash.for-good",
        sort.as_str(),
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    mavi_trash::store::for_good(&mut tx, sort, id).await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}
