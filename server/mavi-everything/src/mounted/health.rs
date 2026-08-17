// Domain route module: health

use mavi_core::error::{Error, Result};
use mavi_db::Db;
use mavi_http::Answered;
use mavi_serve::{Handler, Site};
use serde_json::Value;

use super::helpers::handling;

/// Whether this installation is well.
///
/// Two endpoints and two audiences: one for whatever keeps the process up,
/// which is told yes and nothing else, and one for a person looking at a
/// screen, which needs a grant like anything else.
#[must_use]
pub fn whether_it_is_well(mut site: Site, db: &Db) -> Site {
    for endpoint in mavi_health::endpoints() {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "health.alive" => Some(handling(db, |_, _| {
                Box::pin(async move { Ok(Answered::Read(serde_json::json!({ "alive": true }))) })
            })),
            "health.read" => Some(handling(db, |db, _| {
                Box::pin(async move { how_it_is(&db).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            // Nothing for the one anybody may ask. What it answers is that the
            // process is up, which is not a thing to hold a grant over.
            let needs = match endpoint.named {
                "health.alive" => None,
                _ => Some(mavi_health::to_read()),
            };

            site = site.mount(endpoint, needs, handler);
        }
    }

    site
}

async fn how_it_is(db: &Db) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let health = mavi_health::look_at(&mut tx).await?;
    tx.commit().await?;

    Ok(Answered::Read(
        serde_json::to_value(health).map_err(Error::internal)?,
    ))
}
