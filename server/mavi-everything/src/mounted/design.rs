use super::helpers::asking;
// Domain route module: design

use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use mavi_db::Db;
use mavi_http::Answered;
use mavi_serve::{Asked, Handler, Site};
use serde_json::Value;
use uuid::Uuid;

use super::helpers::{THAT_IS_NOT_AN_ID, a_uuid, handling, wrote_about};

/// The site's own project, and what goes live.
#[must_use]
pub fn how_it_looks(mut site: Site, db: &Db) -> Site {
    for endpoint in mavi_design::endpoints() {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "design.files" => Some(handling(db, |db, asked| {
                Box::pin(async move { design_files(&db, &asked).await })
            })),
            "design.read" => Some(handling(db, |db, asked| {
                Box::pin(async move { read_a_file(&db, &asked).await })
            })),
            "design.write" => Some(handling(db, |db, asked| {
                Box::pin(async move { wrote_a_file(&db, &asked).await })
            })),
            "changes.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { changes(&db, &asked).await })
            })),
            "changes.start" => Some(handling(db, |db, asked| {
                Box::pin(async move { started_changes(&db, &asked).await })
            })),
            "changes.read" => Some(handling(db, |db, asked| {
                Box::pin(async move { one_change(&db, &asked).await })
            })),
            "changes.build" => Some(handling(db, |db, asked| {
                Box::pin(async move { asked_for_a_build(&db, &asked).await })
            })),
            "changes.publish" => Some(handling(db, |db, asked| {
                Box::pin(async move { published_it(&db, &asked).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            // Putting a design in front of everybody is its own capability:
            // laying out a page and publishing it are different jobs.
            let needs = match endpoint.named {
                "changes.publish" => Some(mavi_design::to_publish()),
                _ if endpoint.changes => Some(mavi_design::to_write_design()),
                _ => Some(mavi_design::to_read_design()),
            };

            site = site.mount(endpoint, needs, handler);
        }
    }

    site
}

/// Uploads, which are the one place bytes and a row have to agree.
/// Which set of changes a request is about, where it says.
fn which_change(asked: &Asked) -> Option<Uuid> {
    asked
        .query
        .get("change")
        .and_then(|change| Uuid::parse_str(change).ok())
}

async fn design_files(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let files = mavi_design::store::files(&mut tx, which_change(asked)).await?;

    Ok(Answered::Read(
        serde_json::to_value(files).map_err(Error::internal)?,
    ))
}

/// The path a request is about. Everything after the prefix, so a path with
/// slashes in it arrives whole rather than as its first segment.
/// The path a request is about. Everything after the prefix, so a path with
/// slashes in it arrives whole rather than as its first segment.
fn which_path(asked: &Asked) -> Result<String> {
    asked
        .path
        .get("path")
        .cloned()
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))
}

async fn read_a_file(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let file =
        mavi_design::store::read_file(&mut tx, which_change(asked), &which_path(asked)?).await?;

    Ok(Answered::Read(
        serde_json::to_value(file).map_err(Error::internal)?,
    ))
}

async fn wrote_a_file(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let path = which_path(asked)?;
    let contents = asked.body["contents"]
        .as_str()
        .unwrap_or_default()
        .to_owned();

    // Which set of changes, said rather than assumed: writing into "whatever
    // is live" is the one thing this crate exists to make impossible.
    let change = asked.body["change"]
        .as_str()
        .and_then(|change| Uuid::parse_str(change).ok())
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))?;

    let mut tx = db.begin().await?;
    let file = mavi_design::store::write_file(&mut tx, change, &path, &contents).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "design.write",
        "file",
        Some(&file.path),
        &serde_json::json!({ "change": change }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(file).map_err(Error::internal)?,
        receipt,
    ))
}

async fn changes(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let page = mavi_design::store::changes(&mut tx, &asking(asked)).await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn started_changes(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let name = asked.body["name"].as_str().unwrap_or("A change").to_owned();

    let mut tx = db.begin().await?;
    let change = mavi_design::store::start(&mut tx, &name).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "changes.start",
        "change",
        Some(&change.id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(change).map_err(Error::internal)?,
        receipt,
    ))
}

async fn one_change(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let change = mavi_design::store::read(&mut tx, a_uuid(asked)?).await?;

    Ok(Answered::Read(
        serde_json::to_value(change).map_err(Error::internal)?,
    ))
}

async fn asked_for_a_build(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;

    let mut tx = db.begin().await?;

    // It exists and is not the published one — asked before the work is
    // queued, so a build for something that cannot be built is refused rather
    // than taken and failed.
    let change = mavi_design::store::read(&mut tx, id).await?;

    let queue = mavi_work::Queue::of(&crate::work());
    queue
        .add(
            &mut tx,
            mavi_design::BUILD_A_LOOK.name,
            &serde_json::json!({ "change": id }),
            None,
        )
        .await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "changes.build",
        "change",
        Some(&id.to_string()),
        &serde_json::json!({ "at": change.at.as_str() }),
    )
    .await?;

    tx.commit().await?;

    // What comes back is that it has been asked for. Building is somebody
    // else's minute, and a page held open for it is a page that times out.
    Ok(Answered::Changed(Value::Null, receipt))
}

async fn published_it(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;

    // Publishing is the row changing, and nothing else: the edge answers from
    // whichever set of changes says it is published, so there is no moment
    // between "live" and "serving" for something to go wrong in.
    let mut tx = db.begin().await?;
    let change = mavi_design::store::publish(&mut tx, id).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "changes.publish",
        "change",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(change).map_err(Error::internal)?,
        receipt,
    ))
}
