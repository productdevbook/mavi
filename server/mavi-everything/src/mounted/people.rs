use mavi_core::page::Query;
// Domain route module: people

use mavi_audit::{Actor, record};
use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use mavi_db::Db;
use mavi_http::Answered;
use mavi_serve::{Asked, Handler, Site};
use serde_json::Value;
use uuid::Uuid;

use super::helpers::{THAT_IS_NOT_AN_ID, a_uuid, handling, wrote_about};

/// Setting up, signing in, and who has an account.
#[must_use]
pub fn the_way_in(mut site: Site, db: &Db) -> Site {
    for endpoint in mavi_people::endpoints() {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "setup.once" => Some(handling(db, |db, asked| {
                Box::pin(async move { set_up(&db, &asked).await })
            })),
            "sessions.begin" => Some(handling(db, |db, asked| {
                Box::pin(async move { signed_in(&db, &asked).await })
            })),
            "people.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { people(&db, &asked).await })
            })),
            "sessions.end" => Some(handling(db, |db, asked| {
                Box::pin(async move { signed_out(&db, &asked).await })
            })),
            "people.invite" => Some(handling(db, |db, asked| {
                Box::pin(async move { invited(&db, &asked).await })
            })),
            "people.move" => Some(handling(db, |db, asked| {
                Box::pin(async move { moved_them(&db, &asked).await })
            })),
            "people.remove" => Some(handling(db, |db, asked| {
                Box::pin(async move { took_an_account_away(&db, &asked).await })
            })),
            "keys.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { the_keys(&db, &asked).await })
            })),
            "keys.make" => Some(handling(db, |db, asked| {
                Box::pin(async move { made_a_key(&db, &asked).await })
            })),
            "keys.end" => Some(handling(db, |db, asked| {
                Box::pin(async move { ended_a_key(&db, &asked).await })
            })),
            "roles.list" => Some(handling(db, |db, _| {
                Box::pin(async move { roles(&db).await })
            })),
            "roles.make" => Some(handling(db, |db, asked| {
                Box::pin(async move { made_a_role(&db, &asked).await })
            })),
            "roles.change" => Some(handling(db, |db, asked| {
                Box::pin(async move { changed_a_role(&db, &asked).await })
            })),
            "roles.remove" => Some(handling(db, |db, asked| {
                Box::pin(async move { took_a_role_away(&db, &asked).await })
            })),
            "passwords.choose" => Some(handling(db, |db, asked| {
                Box::pin(async move { chose_a_password(&db, &asked).await })
            })),
            "addresses.prove" => Some(handling(db, |db, asked| {
                Box::pin(async move { proved_an_address(&db, &asked).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            // The two ways in ask for nothing held, because whoever is using
            // them is holding nothing yet. What they answer is what the guard
            // has to work with afterwards.
            // The ways in ask for nothing held, because whoever is using them
            // is holding nothing yet. Everything else here is about accounts,
            // which is what `people` is.
            let needs = match endpoint.named {
                "people.list" | "roles.list" => Some(mavi_people::to_read()),
                // What a role holds is what an account may do, so changing one
                // is the same grant as changing who has an account. There is
                // no lesser thing it could ask for: somebody who can edit a
                // role can give themselves anything.
                "people.invite" | "people.move" | "people.remove" | "roles.make"
                | "roles.change" | "roles.remove" => Some(mavi_people::to_write()),
                // Nothing, for two different reasons that happen to arrive at
                // the same answer. The ways in are reached by somebody who is
                // holding nothing yet. And a key is whoever is asking giving
                // themselves another way in that is never more than they
                // already have — a grant on that would be a grant somebody
                // needs in order to use a script as themselves.
                _ => None,
            };

            site = site.mount(endpoint, needs, handler);
        }
    }

    site
}

/// The site's own name, and what it writes in.
async fn set_up(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let what: mavi_people::store::Setup = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_what_setting_up_asks_for")))?;

    let mut tx = db.begin().await?;
    let ready = mavi_people::store::set_up(&mut tx, &what).await?;

    // The machine did it: there is nobody to attribute it to, because the
    // account this writes down is the one being made.
    let receipt = record(
        &mut tx,
        &Actor::the_machine("setup"),
        "setup.once",
        "site",
        Some(&ready.person.id.to_string()),
        &serde_json::json!({ "site": what.site }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(ready).map_err(Error::internal)?,
        receipt,
    ))
}

/// The second thing somebody has.
async fn signed_in(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let email = asked.body["email"].as_str().unwrap_or_default().to_owned();
    let said = asked.body["password"]
        .as_str()
        .unwrap_or_default()
        .to_owned();

    let mut tx = db.begin().await?;
    let way_in = mavi_people::store::sign_in(&mut tx, &email, &said).await?;

    let receipt = record(
        &mut tx,
        &Actor::the_machine("sessions"),
        "sessions.begin",
        "session",
        None,
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    // Two answers, and `finished` says which. A client that assumed a session
    // here would walk straight past a second step, so the shape makes it
    // impossible to read one as the other by accident.
    let answer = match way_in {
        mavi_people::store::WayIn::Signed(person, token) => serde_json::json!({
            "finished": true,
            "person": person,
            "token": token,
        }),
        mavi_people::store::WayIn::NeedsTheSecondStep(moment) => serde_json::json!({
            "finished": false,
            "moment": moment,
            "how_long": mavi_second::HOW_LONG_TO_FINISH,
        }),
    };

    Ok(Answered::Changed(answer, receipt))
}

async fn people(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;

    let query = Query {
        after: asked.query.get("after").cloned(),
        limit: asked.query.get("limit").and_then(|how| how.parse().ok()),
    };

    let page = mavi_people::store::list(&mut tx, &query).await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn signed_out(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    // The session they came in on, not every session they own. Signing out of
    // one browser is not signing out of a phone left at home, and the only
    // thing that knows which session this is is whatever recognised them.
    let session = asked
        .caller
        .session()
        .and_then(|session| Uuid::parse_str(session).ok())
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))?;

    let mut tx = db.begin().await?;
    mavi_people::store::sign_out(&mut tx, session).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "sessions.end",
        "session",
        Some(&session.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

async fn invited(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let invitation: mavi_people::store::Invitation = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_an_invitation")))?;

    let mut tx = db.begin().await?;
    let (person, token) = mavi_people::store::invite(&mut tx, &invitation).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "people.invite",
        "person",
        Some(&person.id.to_string()),
        // Never the token. What is worth recording is that somebody was
        // invited; the link is theirs and a record of it is a way in.
        &serde_json::json!({ "role": invitation.role }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::json!({ "person": person, "link": token }),
        receipt,
    ))
}

/// Taking one thing away, written down the same way every time.
///
/// Four of these arrived at once when the panel was measured against the API
/// and turned out to want removals nothing answered — a card could be taken
/// off a board and the board could not be taken away. Written as one shape so
/// the fifth is a line rather than a new idea.
/// Taking one thing away, written down the same way every time.
///
/// Four of these arrived at once when the panel was measured against the API
/// and turned out to want removals nothing answered — a card could be taken
/// off a board and the board could not be taken away. Written as one shape so
/// the fifth is a line rather than a new idea.
/// Whoever is asking, as an id. Every key endpoint is about their own keys and
/// nobody else's, which is what makes them need no grant.
fn themselves(asked: &Asked) -> Result<Uuid> {
    asked
        .caller
        .id()
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))
}

async fn the_keys(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let keys = mavi_people::store::keys(&mut tx, themselves(asked)?).await?;

    Ok(Answered::Read(
        serde_json::to_value(keys).map_err(Error::internal)?,
    ))
}

async fn made_a_key(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let new: mavi_people::store::NewKey = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_key")))?;

    let person = themselves(asked)?;

    let mut tx = db.begin().await?;
    let made = mavi_people::store::make_a_key(&mut tx, person, &new).await?;

    // The name and what it may do, and never the key. A receipt carrying it
    // would be the copy that outlives handing it over once.
    let receipt = wrote_about(
        &mut tx,
        asked,
        "keys.make",
        "key",
        Some(&made.key.id.to_string()),
        &serde_json::json!({ "name": made.key.name, "grants": made.key.grants }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(made).map_err(Error::internal)?,
        receipt,
    ))
}

async fn ended_a_key(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;
    let person = themselves(asked)?;

    let mut tx = db.begin().await?;

    mavi_people::store::end_a_key(&mut tx, person, id).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "keys.end",
        "key",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

/// Which kind an address named.
async fn roles(db: &Db) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let roles = mavi_people::store::roles(&mut tx).await?;

    Ok(Answered::Read(
        serde_json::to_value(roles).map_err(Error::internal)?,
    ))
}

async fn made_a_role(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let new: mavi_people::role::NewRole = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_role")))?;

    let mut tx = db.begin().await?;
    let role = mavi_people::store::make_a_role(&mut tx, &new).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "roles.make",
        "role",
        Some(&role.id.to_string()),
        &serde_json::json!({ "name": role.name, "grants": role.grants }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(role).map_err(Error::internal)?,
        receipt,
    ))
}

async fn changed_a_role(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;
    let changes: mavi_people::role::RoleChanges = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_role")))?;

    let mut tx = db.begin().await?;
    let role = mavi_people::store::change_a_role(&mut tx, id, &changes).await?;

    // What it holds now, in the receipt. What somebody needs a year later is
    // what the role could do, not that it was edited.
    let receipt = wrote_about(
        &mut tx,
        asked,
        "roles.change",
        "role",
        Some(&id.to_string()),
        &serde_json::json!({ "name": role.name, "grants": role.grants }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(role).map_err(Error::internal)?,
        receipt,
    ))
}

async fn took_a_role_away(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;

    let mut tx = db.begin().await?;
    let role = mavi_people::store::a_role_called(&mut tx, id).await?;

    mavi_people::store::remove_a_role(&mut tx, id).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "roles.remove",
        "role",
        Some(&id.to_string()),
        &serde_json::json!({ "name": role.name, "grants": role.grants }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

async fn moved_them(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;

    let to = asked.body["role"]
        .as_str()
        .and_then(|role| Uuid::parse_str(role).ok())
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))?;

    let mut tx = db.begin().await?;
    let person = mavi_people::store::move_them(&mut tx, id, to).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "people.move",
        "person",
        Some(&id.to_string()),
        &serde_json::json!({ "to": to }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(person).map_err(Error::internal)?,
        receipt,
    ))
}

async fn took_an_account_away(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;

    let mut tx = db.begin().await?;

    mavi_people::store::remove(&mut tx, id).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "people.remove",
        "person",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

async fn chose_a_password(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let token = asked.body["token"].as_str().unwrap_or_default().to_owned();
    let said = asked.body["password"]
        .as_str()
        .unwrap_or_default()
        .to_owned();

    let mut tx = db.begin().await?;

    // Which of the two it was is the ticket's own business, and both set a
    // password — so this asks for either in turn rather than making the caller
    // say which link they are holding.
    let redeemed = mavi_people::store::redeem(
        &mut tx,
        &token,
        mavi_people::ticket::For::AnInvitation,
        Some(&said),
    )
    .await;

    if redeemed.is_err() {
        mavi_people::store::redeem(
            &mut tx,
            &token,
            mavi_people::ticket::For::AForgottenPassword,
            Some(&said),
        )
        .await?;
    }

    let receipt = wrote_about(
        &mut tx,
        asked,
        "passwords.choose",
        "password",
        None,
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

async fn proved_an_address(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let token = asked.body["token"].as_str().unwrap_or_default().to_owned();

    let mut tx = db.begin().await?;

    mavi_people::store::redeem(
        &mut tx,
        &token,
        mavi_people::ticket::For::AnAddressToProve,
        None,
    )
    .await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "addresses.prove",
        "address",
        None,
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}
