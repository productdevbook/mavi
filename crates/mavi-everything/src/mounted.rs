//! What actually answers.
//!
//! [`crate::endpoints`] is what this installation *describes*. This is what it
//! *serves*, and the two are compared rather than assumed: whatever is
//! described and not here comes back from [`mavi_serve::Site::not_reachable`],
//! by name, which is how "written and tested" stops being mistaken for
//! reachable.
//!
//! A handler is a small thing on purpose. It takes what arrived, opens a
//! transaction, calls the domain, writes the receipt where it changed
//! something, and commits. Nothing in a handler decides who may do it: that
//! was decided before it was reached, out of what the endpoint declared.

use std::future::Future;
use std::sync::Arc;

use mavi_api::Who;
use mavi_audit::{Actor, Who as Whom, record};
use mavi_content::listing::Filter;
use mavi_content::store::{self, Changes};
use mavi_content::writing::{New, WritingId};
use mavi_core::error::{Error, Result};
use mavi_core::page::Query;
use mavi_core::say::Say;
use mavi_db::{Db, Tx};
use mavi_http::{Answered, Caller};
use mavi_serve::{Asked, Handler, Site, WhoIsAsking};
use serde_json::Value;
use uuid::Uuid;

pub const THAT_IS_NOT_AN_ID: &str = "that_is_not_an_id";

/// Everything this installation serves today.
///
/// It is not everything it describes, and that is measured rather than
/// implied — see the test beside this, which prints what is still to do.
#[must_use]
pub fn site(db: &Db, who_is_asking: WhoIsAsking) -> Site {
    let site = Site::new(who_is_asking);

    // One function per domain, in the order somebody meets them: getting in,
    // what the site is, what it files things under, and what it wrote.
    let site = the_way_in(site, db);
    let site = what_this_site_is(site, db);
    let site = what_it_files_things_under(site, db);

    let site = what_it_wrote(site, db);
    let site = what_it_asks_people(site, db);
    let site = what_is_being_worked_on(site, db);

    what_has_been_done(site, db)
}

/// The forms, and what people sent them — the one domain here whose writing
/// side is open to anybody at all.
fn what_it_asks_people(mut site: Site, db: &Db) -> Site {
    for endpoint in mavi_forms::endpoints() {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "forms.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { forms(&db, &asked).await })
            })),
            "forms.make" => Some(handling(db, |db, asked| {
                Box::pin(async move { made_a_form(&db, &asked).await })
            })),
            "forms.change" => Some(handling(db, |db, asked| {
                Box::pin(async move { changed_a_form(&db, &asked).await })
            })),
            "forms.remove" => Some(handling(db, |db, asked| {
                Box::pin(async move { removed_a_form(&db, &asked).await })
            })),
            "forms.filled" => Some(handling(db, |db, asked| {
                Box::pin(async move { what_came_in(&db, &asked).await })
            })),
            "forms.mark-seen" => Some(handling(db, |db, asked| {
                Box::pin(async move { all_seen(&db, &asked).await })
            })),
            "filled.forget" => Some(handling(db, |db, asked| {
                Box::pin(async move { forget_one(&db, &asked).await })
            })),
            "open.form" => Some(handling(db, |db, asked| {
                Box::pin(async move { an_open_form(&db, &asked).await })
            })),
            "open.fill-in" => Some(handling(db, |db, asked| {
                Box::pin(async move { filled_one_in(&db, &asked).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            let needs = match (endpoint.who, endpoint.changes) {
                (Who::Anybody, _) => None,
                (_, true) => Some(mavi_forms::to_write()),
                (_, false) => Some(mavi_forms::to_read()),
            };

            site = site.mount(endpoint, needs, handler);
        }
    }

    site
}

/// Boards, and where a card sits on one.
fn what_is_being_worked_on(mut site: Site, db: &Db) -> Site {
    for endpoint in mavi_boards::endpoints() {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "boards.list" => Some(handling(db, |db, _| {
                Box::pin(async move { boards(&db).await })
            })),
            "boards.make" => Some(handling(db, |db, asked| {
                Box::pin(async move { made_a_board(&db, &asked).await })
            })),
            "boards.read" => Some(handling(db, |db, asked| {
                Box::pin(async move { one_board(&db, &asked).await })
            })),
            "cards.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { cards(&db, &asked).await })
            })),
            "cards.make" => Some(handling(db, |db, asked| {
                Box::pin(async move { made_a_card(&db, &asked).await })
            })),
            "cards.change" => Some(handling(db, |db, asked| {
                Box::pin(async move { changed_a_card(&db, &asked).await })
            })),
            "cards.move" => Some(handling(db, |db, asked| {
                Box::pin(async move { moved_a_card(&db, &asked).await })
            })),
            "cards.remove" => Some(handling(db, |db, asked| {
                Box::pin(async move { removed_a_card(&db, &asked).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            let needs = if endpoint.changes {
                mavi_boards::to_write()
            } else {
                mavi_boards::to_read()
            };

            site = site.mount(endpoint, Some(needs), handler);
        }
    }

    site
}

/// What was done here, which is read and never written through the API.
fn what_has_been_done(mut site: Site, db: &Db) -> Site {
    for endpoint in mavi_audit::endpoints() {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "audit.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { what_was_done(&db, &asked).await })
            })),
            "audit.read" => Some(handling(db, |db, asked| {
                Box::pin(async move { one_receipt(&db, &asked).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            site = site.mount(endpoint, Some(mavi_audit::to_read()), handler);
        }
    }

    site
}

/// Setting up, signing in, and who has an account.
fn the_way_in(mut site: Site, db: &Db) -> Site {
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
                "people.list" => Some(mavi_people::to_read()),
                "people.invite" => Some(mavi_people::to_write()),
                _ => None,
            };

            site = site.mount(endpoint, needs, handler);
        }
    }

    site
}

/// The site's own name, and what it writes in.
fn what_this_site_is(mut site: Site, db: &Db) -> Site {
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
fn what_it_files_things_under(mut site: Site, db: &Db) -> Site {
    for endpoint in mavi_taxonomy::endpoints() {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "terms.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { terms(&db, &asked).await })
            })),
            "terms.make" => Some(handling(db, |db, asked| {
                Box::pin(async move { made_a_term(&db, &asked).await })
            })),
            "terms.change" => Some(handling(db, |db, asked| {
                Box::pin(async move { changed_a_term(&db, &asked).await })
            })),
            "terms.remove" => Some(handling(db, |db, asked| {
                Box::pin(async move { removed_a_term(&db, &asked).await })
            })),
            "writings.file-under" => Some(handling(db, |db, asked| {
                Box::pin(async move { filed_under(&db, &asked).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            let needs = if endpoint.changes {
                mavi_taxonomy::to_write()
            } else {
                mavi_taxonomy::to_read()
            };

            site = site.mount(endpoint, Some(needs), handler);
        }
    }

    site
}

/// Posts, pages, and whatever else a site decides a thing is.
fn what_it_wrote(mut site: Site, db: &Db) -> Site {
    for endpoint in mavi_content::endpoints() {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "writings.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { listed(&db, &asked).await })
            })),
            "writings.read" => Some(handling(db, |db, asked| {
                Box::pin(async move { one(&db, &asked).await })
            })),
            "writings.write" => Some(handling(db, |db, asked| {
                Box::pin(async move { made(&db, &asked).await })
            })),
            "writings.change" => Some(handling(db, |db, asked| {
                Box::pin(async move { changed(&db, &asked).await })
            })),
            "writings.throw-away" => Some(handling(db, |db, asked| {
                Box::pin(async move { thrown(&db, &asked).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            let needs = if endpoint.changes {
                mavi_content::to_write()
            } else {
                mavi_content::to_read()
            };

            site = site.mount(endpoint, Some(needs), handler);
        }
    }

    site
}

type Answering = std::pin::Pin<Box<dyn Future<Output = Result<Answered<Value>>> + Send>>;

/// One handler, with the database it needs already in hand.
fn handling(db: Db, what: fn(Db, Asked) -> Answering) -> Handler {
    Arc::new(move |asked| what(db.clone(), asked))
}

async fn listed(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;

    let filter = Filter {
        kind: asked.query.get("kind").cloned(),
        language: asked.query.get("language").cloned(),
        state: asked.query.get("state").cloned(),
    };

    let query = Query {
        after: asked.query.get("after").cloned(),
        limit: asked.query.get("limit").and_then(|how| how.parse().ok()),
    };

    let page = store::list(&mut tx, false, &filter, &query).await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn one(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let writing = store::read(&mut tx, which(asked)?).await?;

    Ok(Answered::Read(
        serde_json::to_value(writing).map_err(Error::internal)?,
    ))
}

async fn made(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let new: New = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_writing")))?;

    let mut tx = db.begin().await?;
    let writing = store::make(&mut tx, &new).await?;

    // In the same transaction as the change. If the commit below never
    // happens, neither the writing nor the record of it exists.
    let receipt = wrote(
        &mut tx,
        asked,
        "writings.write",
        &writing.id,
        &serde_json::json!({
            "kind": writing.kind.as_str(),
            "slug": writing.slug.as_str(),
        }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(writing).map_err(Error::internal)?,
        receipt,
    ))
}

async fn changed(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let changes: Changes = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_change_to_a_writing")))?;

    let id = which(asked)?;
    let mut tx = db.begin().await?;
    let writing = store::change(&mut tx, id, &changes).await?;

    let receipt = wrote(
        &mut tx,
        asked,
        "writings.change",
        &id,
        &serde_json::json!({ "state": writing.state.as_str() }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(writing).map_err(Error::internal)?,
        receipt,
    ))
}

async fn thrown(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = which(asked)?;
    let mut tx = db.begin().await?;

    store::remove(&mut tx, id).await?;

    let receipt = wrote(
        &mut tx,
        asked,
        "writings.throw-away",
        &id,
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

/// Which one the path is about.
fn which(asked: &Asked) -> Result<WritingId> {
    let id = asked
        .path
        .get("id")
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))?;

    Uuid::parse_str(id)
        .map(WritingId)
        .map_err(|_| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))
}

/// The receipt, written where the change is being made.
///
/// What it is called is the endpoint's own name, so that "what happened to
/// this" has one answer rather than one per call site.
async fn wrote(
    tx: &mut Tx,
    asked: &Asked,
    did: &str,
    about: &WritingId,
    what: &Value,
) -> Result<mavi_audit::Receipt> {
    let actor = match &asked.caller {
        Caller::AnAccount { id, .. } => Actor {
            who: Whom::AnAccount,
            id: Some(id.clone()),
            request: "a-request".to_owned(),
        },
        Caller::AStudent { id } => Actor {
            who: Whom::AStudent,
            id: Some(id.clone()),
            request: "a-request".to_owned(),
        },
        Caller::Nobody => Actor::the_machine("a-request"),
    };

    record(tx, &actor, did, "writing", Some(&about.to_string()), what).await
}

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

async fn signed_in(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let email = asked.body["email"].as_str().unwrap_or_default().to_owned();
    let said = asked.body["password"]
        .as_str()
        .unwrap_or_default()
        .to_owned();

    let mut tx = db.begin().await?;
    let (person, token) = mavi_people::store::sign_in(&mut tx, &email, &said).await?;

    let receipt = record(
        &mut tx,
        &Actor {
            who: Whom::AnAccount,
            id: Some(person.id.to_string()),
            request: "a-request".to_owned(),
        },
        "sessions.begin",
        "session",
        Some(&person.id.to_string()),
        // Never the token, and never the address they typed. What is worth
        // recording is that somebody signed in, not what they signed in with.
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::json!({ "person": person, "token": token }),
        receipt,
    ))
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

async fn terms(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let sort = match asked.query.get("sort").map(String::as_str) {
        Some("tag") => Some(mavi_taxonomy::Sort::Tag),
        Some("category") => Some(mavi_taxonomy::Sort::Category),
        _ => None,
    };

    let query = Query {
        after: asked.query.get("after").cloned(),
        limit: asked.query.get("limit").and_then(|how| how.parse().ok()),
    };

    let mut tx = db.begin().await?;
    let page = mavi_taxonomy::store::list(
        &mut tx,
        sort,
        asked.query.get("language").map(String::as_str),
        &query,
    )
    .await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn made_a_term(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let new: mavi_taxonomy::store::NewTerm = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_term")))?;

    let mut tx = db.begin().await?;
    let term = mavi_taxonomy::store::make(&mut tx, &new).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "terms.make",
        "term",
        Some(&term.id.to_string()),
        &serde_json::json!({ "sort": term.sort.as_str() }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(term).map_err(Error::internal)?,
        receipt,
    ))
}

async fn changed_a_term(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let changes: mavi_taxonomy::store::TermChanges = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_change_to_a_term")))?;

    let id = mavi_taxonomy::term::TermId(a_uuid(asked)?);

    let mut tx = db.begin().await?;
    let term = mavi_taxonomy::store::change(&mut tx, id, &changes).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "terms.change",
        "term",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(term).map_err(Error::internal)?,
        receipt,
    ))
}

async fn removed_a_term(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = mavi_taxonomy::term::TermId(a_uuid(asked)?);

    let mut tx = db.begin().await?;
    mavi_taxonomy::store::remove(&mut tx, id).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "terms.remove",
        "term",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

async fn filed_under(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let writing = a_uuid(asked)?;

    let terms: Vec<Uuid> = asked.body["terms"]
        .as_array()
        .map(|terms| {
            terms
                .iter()
                .filter_map(|term| term.as_str())
                .filter_map(|term| Uuid::parse_str(term).ok())
                .collect()
        })
        .unwrap_or_default();

    let mut tx = db.begin().await?;
    let filed = mavi_taxonomy::store::file_under(&mut tx, writing, &terms).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "writings.file-under",
        "writing",
        Some(&writing.to_string()),
        &serde_json::json!({ "under": terms.len() }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(filed).map_err(Error::internal)?,
        receipt,
    ))
}

/// The id in the path, whatever the endpoint calls it.
fn a_uuid(asked: &Asked) -> Result<Uuid> {
    let id = asked
        .path
        .get("id")
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))?;

    Uuid::parse_str(id).map_err(|_| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))
}

/// A receipt about anything, for the handlers whose subject is not a writing.
async fn wrote_about(
    tx: &mut Tx,
    asked: &Asked,
    did: &str,
    about: &str,
    about_id: Option<&str>,
    what: &Value,
) -> Result<mavi_audit::Receipt> {
    let actor = match &asked.caller {
        Caller::AnAccount { id, .. } => Actor {
            who: Whom::AnAccount,
            id: Some(id.clone()),
            request: "a-request".to_owned(),
        },
        Caller::AStudent { id } => Actor {
            who: Whom::AStudent,
            id: Some(id.clone()),
            request: "a-request".to_owned(),
        },
        Caller::Nobody => Actor::the_machine("a-request"),
    };

    record(tx, &actor, did, about, about_id, what).await
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

async fn what_was_done(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let query = Query {
        after: asked.query.get("after").cloned(),
        limit: asked.query.get("limit").and_then(|how| how.parse().ok()),
    };

    let mut tx = db.begin().await?;
    let page = mavi_audit::reading::list(
        &mut tx,
        asked.query.get("about").map(String::as_str),
        asked.query.get("about_id").map(String::as_str),
        asked.query.get("who_id").map(String::as_str),
        &query,
    )
    .await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn one_receipt(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let written = mavi_audit::reading::read(&mut tx, a_uuid(asked)?).await?;

    Ok(Answered::Read(
        serde_json::to_value(written).map_err(Error::internal)?,
    ))
}

async fn forms(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let page = mavi_forms::store::list(&mut tx, &asking(asked)).await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn made_a_form(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let new: mavi_forms::store::NewForm = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_form")))?;

    let mut tx = db.begin().await?;
    let form = mavi_forms::store::make(&mut tx, &new).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "forms.make",
        "form",
        Some(&form.id.to_string()),
        &serde_json::json!({ "asks": form.fields.fields().len() }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(form).map_err(Error::internal)?,
        receipt,
    ))
}

async fn changed_a_form(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let changes: mavi_forms::store::FormChanges = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_change_to_a_form")))?;

    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;
    let form = mavi_forms::store::change(&mut tx, id, &changes).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "forms.change",
        "form",
        Some(&id.to_string()),
        &serde_json::json!({ "open": form.open }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(form).map_err(Error::internal)?,
        receipt,
    ))
}

async fn removed_a_form(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;

    mavi_forms::store::remove(&mut tx, id).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "forms.remove",
        "form",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

async fn what_came_in(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let unseen = asked.query.get("unseen").is_some_and(|said| said == "true");

    let mut tx = db.begin().await?;
    let page =
        mavi_forms::store::what_came_in(&mut tx, a_uuid(asked)?, unseen, &asking(asked)).await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn all_seen(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;

    // Up to this moment, taken here rather than in the query, so that what is
    // marked read is what the person was actually looking at.
    let seen = mavi_forms::store::all_seen(&mut tx, id, chrono::Utc::now()).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "forms.mark-seen",
        "form",
        Some(&id.to_string()),
        &serde_json::json!({ "seen": seen }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::json!({ "seen": seen }),
        receipt,
    ))
}

async fn forget_one(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;

    mavi_forms::store::forget(&mut tx, id).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "filled.forget",
        "filled",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

async fn an_open_form(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let slug = asked
        .path
        .get("slug")
        .cloned()
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))?;

    let mut tx = db.begin().await?;
    let (_, form, _) = mavi_forms::store::open_form(&mut tx, &slug).await?;

    Ok(Answered::Read(
        serde_json::to_value(form).map_err(Error::internal)?,
    ))
}

async fn filled_one_in(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let slug = asked
        .path
        .get("slug")
        .cloned()
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))?;

    let filled: mavi_forms::Filled = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_what_a_form_takes")))?;

    let mut tx = db.begin().await?;
    let id = mavi_forms::store::fill_in(&mut tx, &slug, &filled, None).await?;

    // A visitor has no account, so what is recorded is the submission itself
    // and the machine as who did it. "Nobody did this" is an answer somebody
    // will need one day.
    let receipt = wrote_about(
        &mut tx,
        asked,
        "open.fill-in",
        "filled",
        Some(&id.to_string()),
        // Never what they wrote. The record is that something came in; what
        // they said is in the row, behind the grant that reads it.
        &serde_json::json!({ "form": slug }),
    )
    .await?;

    tx.commit().await?;

    // What a visitor is told: that it arrived. Nothing about the site, and
    // nothing about what else is on it.
    Ok(Answered::Changed(serde_json::json!({ "id": id }), receipt))
}

/// What a listing was asked for, in the one shape every listing takes.
fn asking(asked: &Asked) -> Query {
    Query {
        after: asked.query.get("after").cloned(),
        limit: asked.query.get("limit").and_then(|how| how.parse().ok()),
    }
}

async fn boards(db: &Db) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let boards = mavi_boards::store::list(&mut tx).await?;

    Ok(Answered::Read(
        serde_json::to_value(boards).map_err(Error::internal)?,
    ))
}

async fn made_a_board(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let new: mavi_boards::store::NewBoard = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_board")))?;

    let mut tx = db.begin().await?;
    let board = mavi_boards::store::make(&mut tx, &new).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "boards.make",
        "board",
        Some(&board.id.to_string()),
        &serde_json::json!({ "stages": board.stages.len() }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(board).map_err(Error::internal)?,
        receipt,
    ))
}

async fn one_board(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let board = mavi_boards::store::read(&mut tx, a_uuid(asked)?).await?;

    Ok(Answered::Read(
        serde_json::to_value(board).map_err(Error::internal)?,
    ))
}

async fn cards(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let stage = asked
        .query
        .get("stage")
        .and_then(|stage| Uuid::parse_str(stage).ok());

    let mut tx = db.begin().await?;
    let cards = mavi_boards::store::cards(&mut tx, a_uuid(asked)?, stage).await?;

    Ok(Answered::Read(
        serde_json::to_value(cards).map_err(Error::internal)?,
    ))
}

async fn made_a_card(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let new: mavi_boards::store::NewCard = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_card")))?;

    let board = a_uuid(asked)?;
    let mut tx = db.begin().await?;
    let card = mavi_boards::store::add(&mut tx, board, &new).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "cards.make",
        "card",
        Some(&card.id.to_string()),
        &serde_json::json!({ "board": board }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(card).map_err(Error::internal)?,
        receipt,
    ))
}

async fn changed_a_card(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let changes: mavi_boards::store::CardChanges = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_change_to_a_card")))?;

    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;
    let card = mavi_boards::store::change(&mut tx, id, &changes).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "cards.change",
        "card",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(card).map_err(Error::internal)?,
        receipt,
    ))
}

async fn moved_a_card(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let dropped: mavi_boards::store::Between = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_where_a_card_was_dropped")))?;

    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;
    let card = mavi_boards::store::moved(&mut tx, id, &dropped).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "cards.move",
        "card",
        Some(&id.to_string()),
        &serde_json::json!({ "stage": dropped.stage }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(card).map_err(Error::internal)?,
        receipt,
    ))
}

async fn removed_a_card(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;

    mavi_boards::store::remove(&mut tx, id).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "cards.remove",
        "card",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}
