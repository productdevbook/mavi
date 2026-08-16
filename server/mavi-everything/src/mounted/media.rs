// Domain route module: media

use std::sync::Arc;

use mavi_core::error::{Error, Result};
use mavi_core::ports::Files;
use mavi_db::Db;
use mavi_http::Answered;
use mavi_serve::{Asked, Handler, Site};
use serde_json::Value;

use super::helpers::{a_uuid, asking, with_files, wrote_about};

/// Uploads, which are the one place bytes and a row have to agree.
#[must_use]
pub fn what_somebody_uploaded(mut site: Site, db: &Db, files: &Arc<dyn Files>) -> Site {
    for endpoint in mavi_media::endpoints() {
        let db = db.clone();
        let files = Arc::clone(files);

        let handler: Option<Handler> = match endpoint.named {
            "files.list" => Some(with_files(db, files, |db, _, asked| {
                Box::pin(async move { uploaded(&db, &asked).await })
            })),
            "files.upload" => Some(with_files(db, files, |db, files, asked| {
                Box::pin(async move { took_a_file(&db, files.as_ref(), &asked).await })
            })),
            "files.read" => Some(with_files(db, files, |db, _, asked| {
                Box::pin(async move { one_file(&db, &asked).await })
            })),
            "files.remove" => Some(with_files(db, files, |db, files, asked| {
                Box::pin(async move { removed_a_file(&db, files.as_ref(), &asked).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            let needs = if endpoint.changes {
                mavi_media::to_write()
            } else {
                mavi_media::to_read()
            };

            site = site.mount(endpoint, Some(needs), handler);
        }
    }

    site
}

/// What was done here, which is read and never written through the API.
async fn uploaded(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let page = mavi_media::store::list(
        &mut tx,
        asked.query.get("kind").map(String::as_str),
        &asking(asked),
    )
    .await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn took_a_file(db: &Db, files: &dyn Files, asked: &Asked) -> Result<Answered<Value>> {
    // The name is a query parameter and the body is the file itself. What it
    // is comes from the bytes; the name is only what to call it on a screen.
    let name = asked
        .query
        .get("name")
        .cloned()
        .unwrap_or_else(|| "A file".to_owned());

    let mut tx = db.begin().await?;
    let file = mavi_media::store::take(&mut tx, files, &name, asked.raw.clone()).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "files.upload",
        "file",
        Some(&file.id.to_string()),
        &serde_json::json!({ "kind": file.kind.as_str(), "bytes": file.bytes }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(file).map_err(Error::internal)?,
        receipt,
    ))
}

async fn one_file(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let file = mavi_media::store::read(&mut tx, mavi_media::FileId(a_uuid(asked)?)).await?;

    Ok(Answered::Read(
        serde_json::to_value(file).map_err(Error::internal)?,
    ))
}

async fn removed_a_file(db: &Db, files: &dyn Files, asked: &Asked) -> Result<Answered<Value>> {
    let id = mavi_media::FileId(a_uuid(asked)?);

    let mut tx = db.begin().await?;
    mavi_media::store::remove(&mut tx, files, id).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "files.remove",
        "file",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}
