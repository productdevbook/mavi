// Domain route module: analytics

use mavi_core::error::{Error, Result};
use mavi_db::Db;
use mavi_http::Answered;
use mavi_serve::{Asked, Handler, Site};
use serde_json::Value;
use sqlx::Row;

use super::helpers::handling;

/// How many people read the site.
///
/// The beacon writes and answers nothing; everything that reads needs an
/// account. A reader's browser is not something to answer questions about the
/// site to.
#[must_use]
pub fn how_many_read_it(mut site: Site, db: &Db) -> Site {
    for endpoint in mavi_analytics::endpoints()
        .into_iter()
        .chain(std::iter::once(crate::overview::endpoint()))
    {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "open.read" => Some(handling(db, |db, asked| {
                Box::pin(async move { somebody_read_a_page(&db, &asked).await })
            })),
            "analytics.read" => Some(handling(db, |db, asked| {
                Box::pin(async move { how_many(&db, &asked).await })
            })),
            "analytics.felt" => Some(handling(db, |db, asked| {
                Box::pin(async move { how_it_felt(&db, &asked).await })
            })),
            "site.overview" => Some(handling(db, |db, _| {
                Box::pin(async move { what_this_site_has(&db).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            let needs = match endpoint.named {
                "open.read" => None,
                "site.overview" => Some(crate::overview::to_read()),
                _ => Some(mavi_analytics::to_read()),
            };

            site = site.mount(endpoint, needs, handler);
        }
    }

    site
}

/// What a site has, in one answer.
///
/// The one endpoint in this workspace that reaches across every domain, and it
/// lives here for that reason: no crate may ask about another, and this is the
/// crate whose whole job is the questions no one of them can ask. Eleven
/// counts in one query rather than eleven calls, because the screen that shows
/// them is the first one anybody opens.
/// What a site has, in one answer.
///
/// The one endpoint in this workspace that reaches across every domain, and it
/// lives here for that reason: no crate may ask about another, and this is the
/// crate whose whole job is the questions no one of them can ask. Eleven
/// counts in one query rather than eleven calls, because the screen that shows
/// them is the first one anybody opens.
async fn what_this_site_has(db: &Db) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;

    let row = sqlx::query(
        "select
            (select count(*) from writings where deleted_at is null) as writings,
            (select count(*) from writings
              where state = 'published' and deleted_at is null) as published,
            (select count(*) from files where deleted_at is null) as files,
            (select coalesce(sum(bytes), 0)::bigint from files
              where deleted_at is null) as bytes,
            (select count(*) from forms where deleted_at is null) as forms,
            (select count(*) from filled where seen_at is null) as unread,
            (select count(*) from readers where standing = 'subscribed') as readers,
            (select count(*) from students where deleted_at is null) as students,
            (select count(*) from orders) as orders,
            (select count(*) from flows where on_) as flows_on,
            (select count(*) from jobs where state = 'dead') as work_given_up_on",
    )
    .fetch_one(tx.conn())
    .await
    .map_err(Error::internal)?;

    let of = |named: &str| -> Result<i64> { row.try_get(named).map_err(Error::internal) };

    Ok(Answered::Read(serde_json::json!({
        "writings": of("writings")?,
        "published": of("published")?,
        "files": of("files")?,
        "bytes": of("bytes")?,
        "forms": of("forms")?,
        "unread": of("unread")?,
        "readers": of("readers")?,
        "students": of("students")?,
        "orders": of("orders")?,
        "flows_on": of("flows_on")?,
        "work_given_up_on": of("work_given_up_on")?,
    })))
}

/// How many days a screen asked about, held to what may be asked for.
/// How many days a screen asked about, held to what may be asked for.
fn over_how_many_days(asked: &Asked) -> i32 {
    asked
        .query
        .get("days")
        .and_then(|days| days.parse::<i32>().ok())
        .unwrap_or(30)
        .clamp(1, mavi_analytics::AT_MOST_DAYS)
}

async fn somebody_read_a_page(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let path = asked.body["path"].as_str().unwrap_or_default();
    let on_day = chrono::Utc::now().date_naive();

    let mut tx = db.begin().await?;

    mavi_analytics::store::was_read(&mut tx, on_day, path).await?;

    if let (Some(felt), Some(value)) = (
        asked.body["felt"].as_str(),
        asked.body["value"]
            .as_i64()
            .and_then(|v| i32::try_from(v).ok()),
    ) {
        mavi_analytics::store::felt(&mut tx, on_day, path, felt, value).await?;
    }

    tx.commit().await?;

    // Nothing goes back, and no receipt is written. A receipt per page read is
    // an audit log that is entirely one thing, and what happened here is not
    // something anybody did.
    Ok(Answered::Read(Value::Null))
}

async fn how_many(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let read = mavi_analytics::store::how_many(&mut tx, over_how_many_days(asked)).await?;

    Ok(Answered::Read(
        serde_json::to_value(read).map_err(Error::internal)?,
    ))
}

async fn how_it_felt(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let felt = mavi_analytics::store::how_it_felt(&mut tx, over_how_many_days(asked)).await?;

    Ok(Answered::Read(
        serde_json::to_value(felt).map_err(Error::internal)?,
    ))
}
