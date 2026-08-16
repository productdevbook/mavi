// Domain route module: about

use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use mavi_db::Db;
use mavi_http::Answered;
use mavi_serve::{Asked, Handler, Site};
use serde_json::Value;

use super::helpers::{handling, wrote_about};

/// What a site holds about one person.
#[must_use]
pub fn what_it_holds_about_somebody(mut site: Site, db: &Db) -> Site {
    for endpoint in crate::about::endpoints() {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "about.gather" => Some(handling(db, |db, asked| {
                Box::pin(async move { what_is_held(&db, &asked).await })
            })),
            "about.forget" => Some(handling(db, |db, asked| {
                Box::pin(async move { forgot_them(&db, &asked).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            let needs = match endpoint.named {
                "about.gather" => crate::about::to_read(),
                _ => crate::about::to_erase(),
            };

            site = site.mount(endpoint, Some(needs), handler);
        }
    }

    site
}

/// The address this is about, lowered the way every address here is kept.
/// The address this is about, lowered the way every address here is kept.
fn about_whom(asked: &Asked) -> Result<String> {
    let email = asked.body["email"]
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_lowercase();

    if email.is_empty() {
        return Err(Error::invalid(Say::of("that_is_not_an_address")));
    }

    Ok(email)
}

async fn what_is_held(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let email = about_whom(asked)?;

    let mut tx = db.begin().await?;
    let held = crate::about::gather(&mut tx, &email).await?;

    Ok(Answered::Read(held))
}

async fn forgot_them(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let email = about_whom(asked)?;

    let mut tx = db.begin().await?;
    let forgotten = crate::about::forget(&mut tx, &email).await?;

    // What was done, without the address it was done about. A receipt naming
    // somebody is the one row that survives forgetting them, which would make
    // the whole thing pointless — so it says how much went and not who.
    let receipt = wrote_about(&mut tx, asked, "about.forget", "person", None, &forgotten).await?;

    tx.commit().await?;

    Ok(Answered::Changed(forgotten, receipt))
}
