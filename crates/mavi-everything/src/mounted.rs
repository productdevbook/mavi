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
    let site = what_it_teaches(site, db);
    let site = what_it_sells(site, db);
    let site = what_it_writes_to_people(site, db);
    let site = what_it_does_by_itself(site, db);
    let site = how_it_looks(site, db);

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

/// Courses, who is on them, and what a student reaches.
fn what_it_teaches(mut site: Site, db: &Db) -> Site {
    for endpoint in mavi_courses::endpoints() {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "courses.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { courses(&db, &asked).await })
            })),
            "courses.make" => Some(handling(db, |db, asked| {
                Box::pin(async move { made_a_course(&db, &asked).await })
            })),
            "courses.read" => Some(handling(db, |db, asked| {
                Box::pin(async move { one_course(&db, &asked).await })
            })),
            "courses.change" => Some(handling(db, |db, asked| {
                Box::pin(async move { changed_a_course(&db, &asked).await })
            })),
            "courses.reorder" => Some(handling(db, |db, asked| {
                Box::pin(async move { reordered_modules(&db, &asked).await })
            })),
            "modules.make" => Some(handling(db, |db, asked| {
                Box::pin(async move { added_a_module(&db, &asked).await })
            })),
            "modules.reorder" => Some(handling(db, |db, asked| {
                Box::pin(async move { reordered_lessons(&db, &asked).await })
            })),
            "lessons.make" => Some(handling(db, |db, asked| {
                Box::pin(async move { added_a_lesson(&db, &asked).await })
            })),
            "lessons.change" => Some(handling(db, |db, asked| {
                Box::pin(async move { changed_a_lesson(&db, &asked).await })
            })),
            "students.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { students(&db, &asked).await })
            })),
            "students.ask" => Some(handling(db, |db, asked| {
                Box::pin(async move { asked_somebody(&db, &asked).await })
            })),
            "enrolments.add" => Some(handling(db, |db, asked| {
                Box::pin(async move { put_on_a_course(&db, &asked).await })
            })),
            "enrolments.remove" => Some(handling(db, |db, asked| {
                Box::pin(async move { taken_off_a_course(&db, &asked).await })
            })),
            "learning.mine" => Some(handling(db, |db, asked| {
                Box::pin(async move { what_they_are_on(&db, &asked).await })
            })),
            "learning.lesson" => Some(handling(db, |db, asked| {
                Box::pin(async move { a_students_lesson(&db, &asked).await })
            })),
            "learning.done" => Some(handling(db, |db, asked| {
                Box::pin(async move { marked_done(&db, &asked).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            // A student holds no grants at all, so what they reach asks for
            // nothing held — and what they may see is decided by the three
            // questions the store asks, not by a capability.
            let needs = match (endpoint.who, endpoint.changes) {
                (Who::AStudent, _) => None,
                (_, true) => Some(mavi_courses::to_write()),
                (_, false) => Some(mavi_courses::to_read()),
            };

            site = site.mount(endpoint, needs, handler);
        }
    }

    site
}

/// The shelf, the orders, and the basket a visitor brings.
fn what_it_sells(mut site: Site, db: &Db) -> Site {
    for endpoint in mavi_shop::endpoints() {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "products.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { products(&db, &asked).await })
            })),
            "products.make" => Some(handling(db, |db, asked| {
                Box::pin(async move { added_a_product(&db, &asked).await })
            })),
            "products.change" => Some(handling(db, |db, asked| {
                Box::pin(async move { changed_a_product(&db, &asked).await })
            })),
            "products.remove" => Some(handling(db, |db, asked| {
                Box::pin(async move { removed_a_product(&db, &asked).await })
            })),
            "coupons.list" => Some(handling(db, |db, _| {
                Box::pin(async move { coupons(&db).await })
            })),
            "coupons.make" => Some(handling(db, |db, asked| {
                Box::pin(async move { made_a_coupon(&db, &asked).await })
            })),
            "orders.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { orders(&db, &asked).await })
            })),
            "orders.read" => Some(handling(db, |db, asked| {
                Box::pin(async move { one_order(&db, &asked).await })
            })),
            "orders.move" => Some(handling(db, |db, asked| {
                Box::pin(async move { moved_an_order(&db, &asked).await })
            })),
            "open.products" => Some(handling(db, |db, asked| {
                Box::pin(async move { what_is_for_sale(&db, &asked).await })
            })),
            "open.order" => Some(handling(db, |db, asked| {
                Box::pin(async move { placed_an_order(&db, &asked).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            let needs = match (endpoint.who, endpoint.changes) {
                (Who::Anybody, _) => None,
                (_, true) => Some(mavi_shop::to_write()),
                (_, false) => Some(mavi_shop::to_read()),
            };

            site = site.mount(endpoint, needs, handler);
        }
    }

    site
}

/// The site's own letters, its lists, and the way out of them.
fn what_it_writes_to_people(mut site: Site, db: &Db) -> Site {
    for endpoint in mavi_mail::endpoints() {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "letters.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { letters(&db, &asked).await })
            })),
            "letters.write" => Some(handling(db, |db, asked| {
                Box::pin(async move { wrote_a_letter(&db, &asked).await })
            })),
            "letters.forget" => Some(handling(db, |db, asked| {
                Box::pin(async move { forgot_a_letter(&db, &asked).await })
            })),
            "letters.press" => Some(handling(db, |db, asked| {
                Box::pin(async move { pressed_a_letter(&db, &asked).await })
            })),
            "lists.list" => Some(handling(db, |db, _| {
                Box::pin(async move { lists(&db).await })
            })),
            "lists.make" => Some(handling(db, |db, asked| {
                Box::pin(async move { made_a_list(&db, &asked).await })
            })),
            "readers.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { readers(&db, &asked).await })
            })),
            "readers.add" => Some(handling(db, |db, asked| {
                Box::pin(async move { added_a_reader(&db, &asked).await })
            })),
            "readers.forget" => Some(handling(db, |db, asked| {
                Box::pin(async move { forgot_a_reader(&db, &asked).await })
            })),
            "sendings.send" => Some(handling(db, |db, asked| {
                Box::pin(async move { sent_to_a_list(&db, &asked).await })
            })),
            "open.unsubscribe" => Some(handling(db, |db, asked| {
                Box::pin(async move { took_themselves_off(&db, &asked).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            let needs = match (endpoint.who, endpoint.changes) {
                (Who::Anybody, _) => None,
                (_, true) => Some(mavi_mail::to_write()),
                (_, false) => Some(mavi_mail::to_read()),
            };

            site = site.mount(endpoint, needs, handler);
        }
    }

    site
}

/// Flows, and what they have done.
fn what_it_does_by_itself(mut site: Site, db: &Db) -> Site {
    for endpoint in mavi_flows::endpoints() {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "flows.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { flows(&db, &asked).await })
            })),
            // The one answer in this whole file that is not a query: what can
            // start a flow is a fact about the code.
            "flows.triggers" => Some(handling(db, |_, _| Box::pin(async move { Ok(triggers()) }))),
            "flows.make" => Some(handling(db, |db, asked| {
                Box::pin(async move { arranged_a_flow(&db, &asked).await })
            })),
            "flows.change" => Some(handling(db, |db, asked| {
                Box::pin(async move { changed_a_flow(&db, &asked).await })
            })),
            "flows.remove" => Some(handling(db, |db, asked| {
                Box::pin(async move { removed_a_flow(&db, &asked).await })
            })),
            "runs.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { runs(&db, &asked).await })
            })),
            "runs.read" => Some(handling(db, |db, asked| {
                Box::pin(async move { one_run(&db, &asked).await })
            })),
            "flows.try" => Some(handling(db, |db, asked| {
                Box::pin(async move { tried_a_flow(&db, &asked).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            let needs = if endpoint.changes {
                mavi_flows::to_write()
            } else {
                mavi_flows::to_read()
            };

            site = site.mount(endpoint, Some(needs), handler);
        }
    }

    site
}

/// The site's own project, and what goes live.
fn how_it_looks(mut site: Site, db: &Db) -> Site {
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

async fn courses(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let page = mavi_courses::store::list(
        &mut tx,
        asked.query.get("state").map(String::as_str),
        &asking(asked),
    )
    .await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn made_a_course(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let new: mavi_courses::store::NewCourse = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_course")))?;

    let mut tx = db.begin().await?;
    let course = mavi_courses::store::make(&mut tx, &new).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "courses.make",
        "course",
        Some(&course.id.to_string()),
        &serde_json::json!({ "slug": course.slug }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(course).map_err(Error::internal)?,
        receipt,
    ))
}

async fn one_course(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let course = mavi_courses::store::read(&mut tx, a_uuid(asked)?).await?;

    Ok(Answered::Read(
        serde_json::to_value(course).map_err(Error::internal)?,
    ))
}

async fn changed_a_course(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let changes: mavi_courses::store::CourseChanges = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_change_to_a_course")))?;

    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;
    let course = mavi_courses::store::change(&mut tx, id, &changes).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "courses.change",
        "course",
        Some(&id.to_string()),
        &serde_json::json!({ "state": course.state.as_str() }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(course).map_err(Error::internal)?,
        receipt,
    ))
}

/// The order somebody dragged things into, as ids.
fn the_order(asked: &Asked) -> Vec<Uuid> {
    asked.body["order"]
        .as_array()
        .map(|order| {
            order
                .iter()
                .filter_map(|id| id.as_str())
                .filter_map(|id| Uuid::parse_str(id).ok())
                .collect()
        })
        .unwrap_or_default()
}

async fn reordered_modules(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;

    let course = mavi_courses::store::reorder_modules(&mut tx, id, &the_order(asked)).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "courses.reorder",
        "course",
        Some(&id.to_string()),
        &serde_json::json!({ "parts": course.modules.len() }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(course).map_err(Error::internal)?,
        receipt,
    ))
}

async fn reordered_lessons(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;

    let lessons = mavi_courses::store::reorder_lessons(&mut tx, id, &the_order(asked)).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "modules.reorder",
        "module",
        Some(&id.to_string()),
        &serde_json::json!({ "lessons": lessons.len() }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::json!({ "id": id, "lessons": lessons }),
        receipt,
    ))
}

async fn added_a_module(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let title = asked.body["title"].as_str().unwrap_or_default().to_owned();
    let course = a_uuid(asked)?;

    let mut tx = db.begin().await?;
    let module = mavi_courses::store::add_module(&mut tx, course, &title).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "modules.make",
        "module",
        Some(&module.id.to_string()),
        &serde_json::json!({ "course": course }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(module).map_err(Error::internal)?,
        receipt,
    ))
}

async fn added_a_lesson(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let title = asked.body["title"].as_str().unwrap_or_default().to_owned();
    let body = asked.body["body"].as_str().unwrap_or_default().to_owned();
    let module = a_uuid(asked)?;

    let mut tx = db.begin().await?;
    let lesson = mavi_courses::store::add_lesson(&mut tx, module, &title, &body).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "lessons.make",
        "lesson",
        Some(&lesson.id.to_string()),
        &serde_json::json!({ "module": module }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(lesson).map_err(Error::internal)?,
        receipt,
    ))
}

async fn changed_a_lesson(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let changes: mavi_courses::store::LessonChanges = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_change_to_a_lesson")))?;

    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;
    let lesson = mavi_courses::store::change_lesson(&mut tx, id, &changes).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "lessons.change",
        "lesson",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(lesson).map_err(Error::internal)?,
        receipt,
    ))
}

async fn students(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let page = mavi_courses::store::students(&mut tx, &asking(asked)).await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn asked_somebody(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let email = asked.body["email"].as_str().unwrap_or_default().to_owned();
    let name = asked.body["name"].as_str().unwrap_or_default().to_owned();

    let mut tx = db.begin().await?;
    let student = mavi_courses::store::ask(&mut tx, &email, &name).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "students.ask",
        "student",
        Some(&student.id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(student).map_err(Error::internal)?,
        receipt,
    ))
}

async fn put_on_a_course(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let course = a_uuid(asked)?;
    let student = asked.body["student"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))?;

    let mut tx = db.begin().await?;
    let id = mavi_courses::store::enrol(&mut tx, course, student).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "enrolments.add",
        "enrolment",
        Some(&id.to_string()),
        &serde_json::json!({ "course": course, "student": student }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::json!({ "id": id, "course": course, "student": student }),
        receipt,
    ))
}

async fn taken_off_a_course(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;

    mavi_courses::store::unenrol(&mut tx, id).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "enrolments.remove",
        "enrolment",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

/// Which student is asking. A student holds no grants, so this is the whole of
/// who they are.
fn a_student(asked: &Asked) -> Result<Uuid> {
    asked
        .caller
        .id()
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))
}

async fn what_they_are_on(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let courses = mavi_courses::store::learning(&mut tx, a_student(asked)?).await?;

    Ok(Answered::Read(
        serde_json::to_value(courses).map_err(Error::internal)?,
    ))
}

async fn a_students_lesson(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let lesson =
        mavi_courses::store::a_students_lesson(&mut tx, a_student(asked)?, a_uuid(asked)?).await?;

    Ok(Answered::Read(
        serde_json::to_value(lesson).map_err(Error::internal)?,
    ))
}

async fn marked_done(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let student = a_student(asked)?;
    let lesson = a_uuid(asked)?;

    let mut tx = db.begin().await?;
    let at = mavi_courses::store::done(&mut tx, student, lesson).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "learning.done",
        "lesson",
        Some(&lesson.to_string()),
        &serde_json::json!({ "student": student }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::json!({ "lesson": lesson, "at": at }),
        receipt,
    ))
}

async fn products(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let page = mavi_shop::store::products(&mut tx, &asking(asked)).await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn what_is_for_sale(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let page = mavi_shop::store::for_sale(&mut tx, &asking(asked)).await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn added_a_product(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let new: mavi_shop::store::NewProduct = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_something_to_sell")))?;

    let mut tx = db.begin().await?;
    let product = mavi_shop::store::add(&mut tx, &new).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "products.make",
        "product",
        Some(&product.id.to_string()),
        &serde_json::json!({ "slug": product.slug }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(product).map_err(Error::internal)?,
        receipt,
    ))
}

async fn changed_a_product(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let changes: mavi_shop::store::ProductChanges = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_change_to_something_for_sale")))?;

    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;
    let product = mavi_shop::store::change(&mut tx, id, &changes).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "products.change",
        "product",
        Some(&id.to_string()),
        // What was changed, not what it became: a price is worth being able to
        // read back, and reading it out of the row is what the row is for.
        &serde_json::json!({ "for_sale": product.for_sale }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(product).map_err(Error::internal)?,
        receipt,
    ))
}

async fn removed_a_product(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;

    mavi_shop::store::remove(&mut tx, id).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "products.remove",
        "product",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

async fn coupons(db: &Db) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let coupons = mavi_shop::store::coupons(&mut tx).await?;

    Ok(Answered::Read(
        serde_json::to_value(coupons).map_err(Error::internal)?,
    ))
}

async fn made_a_coupon(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let new: mavi_shop::store::NewCoupon = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_code")))?;

    let mut tx = db.begin().await?;
    let coupon = mavi_shop::store::add_coupon(&mut tx, &new).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "coupons.make",
        "coupon",
        Some(&coupon.code),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(coupon).map_err(Error::internal)?,
        receipt,
    ))
}

async fn orders(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let page = mavi_shop::store::orders(
        &mut tx,
        asked.query.get("state").map(String::as_str),
        &asking(asked),
    )
    .await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn one_order(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let order = mavi_shop::store::read(&mut tx, a_uuid(asked)?).await?;

    Ok(Answered::Read(
        serde_json::to_value(order).map_err(Error::internal)?,
    ))
}

async fn moved_an_order(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let to = asked.body["to"].as_str().unwrap_or_default().to_owned();
    let id = a_uuid(asked)?;

    let mut tx = db.begin().await?;
    let order = mavi_shop::store::move_to(&mut tx, id, &to).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "orders.move",
        "order",
        Some(&id.to_string()),
        &serde_json::json!({ "to": order.state.as_str() }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(order).map_err(Error::internal)?,
        receipt,
    ))
}

async fn placed_an_order(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let basket: mavi_shop::store::Basket = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_basket")))?;

    let mut tx = db.begin().await?;
    let order = mavi_shop::store::place(&mut tx, &basket).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "open.order",
        "order",
        Some(&order.id.to_string()),
        // The number and what it came to. Never the address they typed: what
        // is worth recording is that an order was placed.
        &serde_json::json!({ "number": order.number, "total": order.total.minor }),
    )
    .await?;

    tx.commit().await?;

    // What a visitor is told: the order they placed and what it came to. The
    // number, because that is what somebody reads down a telephone.
    Ok(Answered::Changed(
        serde_json::json!({
            "id": order.id,
            "number": order.number,
            "total": order.total,
        }),
        receipt,
    ))
}

/// Which language a screen is asking about. The site's own is `en` until
/// something says otherwise, and a letter answered in no language at all is a
/// screen with nothing on it.
fn in_which_language(asked: &Asked) -> String {
    asked
        .query
        .get("language")
        .cloned()
        .unwrap_or_else(|| "en".to_owned())
}

async fn letters(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let letters = mavi_mail::store::letters(&mut tx, &in_which_language(asked)).await?;

    Ok(Answered::Read(
        serde_json::to_value(letters).map_err(Error::internal)?,
    ))
}

async fn wrote_a_letter(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let kind = asked
        .path
        .get("kind")
        .cloned()
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))?;

    let language = asked.body["language"].as_str().unwrap_or("en").to_owned();
    let subject = asked.body["subject"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let body = asked.body["body"].as_str().unwrap_or_default().to_owned();

    let mut tx = db.begin().await?;
    let letter = mavi_mail::store::write(&mut tx, &kind, &language, &subject, &body).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "letters.write",
        "letter",
        Some(&kind),
        &serde_json::json!({ "language": language }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(letter).map_err(Error::internal)?,
        receipt,
    ))
}

async fn forgot_a_letter(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let kind = asked
        .path
        .get("kind")
        .cloned()
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))?;

    let language = in_which_language(asked);

    let mut tx = db.begin().await?;
    mavi_mail::store::forget(&mut tx, &kind, &language).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "letters.forget",
        "letter",
        Some(&kind),
        &serde_json::json!({ "language": language }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

async fn pressed_a_letter(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let kind = asked
        .path
        .get("kind")
        .cloned()
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))?;

    let values: Vec<(String, String)> = asked.body["values"]
        .as_object()
        .map(|values| {
            values
                .iter()
                .map(|(name, what)| {
                    (
                        name.clone(),
                        what.as_str()
                            .map_or_else(|| what.to_string(), ToOwned::to_owned),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    let borrowed: Vec<(&str, String)> = values
        .iter()
        .map(|(name, what)| (name.as_str(), what.clone()))
        .collect();

    let mut tx = db.begin().await?;
    let pressed =
        mavi_mail::store::pressed(&mut tx, &kind, &in_which_language(asked), &borrowed).await?;

    // Nothing left the machine, so there is nothing to record.
    Ok(Answered::Read(
        serde_json::to_value(pressed).map_err(Error::internal)?,
    ))
}

async fn lists(db: &Db) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let lists = mavi_mail::store::lists(&mut tx).await?;

    Ok(Answered::Read(
        serde_json::to_value(lists).map_err(Error::internal)?,
    ))
}

async fn made_a_list(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let name = asked.body["name"].as_str().unwrap_or_default().to_owned();

    let mut tx = db.begin().await?;
    let list = mavi_mail::store::add_list(&mut tx, &name).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "lists.make",
        "list",
        Some(&list.id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(list).map_err(Error::internal)?,
        receipt,
    ))
}

async fn readers(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let page = mavi_mail::store::readers(
        &mut tx,
        a_uuid(asked)?,
        asked.query.get("standing").map(String::as_str),
        &asking(asked),
    )
    .await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn added_a_reader(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let list = a_uuid(asked)?;
    let email = asked.body["email"].as_str().unwrap_or_default().to_owned();
    let name = asked.body["name"].as_str().map(ToOwned::to_owned);

    let mut tx = db.begin().await?;
    let reader = mavi_mail::store::add_reader(&mut tx, list, &email, name.as_deref()).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "readers.add",
        "reader",
        Some(&reader.id.to_string()),
        &serde_json::json!({ "list": list }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(reader).map_err(Error::internal)?,
        receipt,
    ))
}

async fn forgot_a_reader(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;

    mavi_mail::store::forget_reader(&mut tx, id).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "readers.forget",
        "reader",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

async fn sent_to_a_list(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let list = a_uuid(asked)?;

    let sending: mavi_mail::store::NewSending = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_something_to_send")))?;

    // Checked before anybody is looked up: a letter to a list that does not
    // say how to leave it is refused, and refusing it after working out who it
    // would go to is a longer way to the same answer.
    let sending = mavi_mail::Sending::checked(&sending.subject, &sending.body)?;

    let mut tx = db.begin().await?;
    let going_to = mavi_mail::store::who_it_goes_to(&mut tx, list).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "sendings.send",
        "list",
        Some(&list.to_string()),
        // How many, not who: a receipt is read by whoever asks what happened,
        // and a list of addresses in it is the list itself, copied.
        &serde_json::json!({ "letters": going_to.len(), "subject": sending.subject }),
    )
    .await?;

    tx.commit().await?;

    // What comes back is that it has been taken, and how many it is for. The
    // queue is what sends them, one at a time, so nothing here waits on a mail
    // host.
    Ok(Answered::Changed(
        serde_json::json!({ "letters": going_to.len() }),
        receipt,
    ))
}

async fn took_themselves_off(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let token = asked
        .path
        .get("token")
        .cloned()
        .ok_or_else(|| Error::invalid(Say::of(THAT_IS_NOT_AN_ID)))?;

    let mut tx = db.begin().await?;
    mavi_mail::store::out(&mut tx, &token).await?;

    // Recorded as the machine's doing: whoever pressed the link has no account
    // and no name here, and that is the point of the link.
    let receipt = wrote_about(
        &mut tx,
        asked,
        "open.unsubscribe",
        "reader",
        None,
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

async fn flows(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let page = mavi_flows::store::list(&mut tx, &asking(asked)).await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

/// Everything that can start a flow, and what each one may name.
///
/// Answered rather than written in a manual: a panel that has to know the list
/// is a panel that goes out of date on its own.
fn triggers() -> Answered<Value> {
    let triggers: Vec<Value> = mavi_flows::step::TRIGGERS
        .iter()
        .map(|trigger| serde_json::json!({ "name": trigger.as_str() }))
        .collect();

    let does: Vec<Value> = [
        mavi_flows::Does::SendALetter,
        mavi_flows::Does::CallAnAddress,
        mavi_flows::Does::Wait,
        mavi_flows::Does::PutOnAList,
    ]
    .iter()
    .map(|does| serde_json::json!({ "name": does.as_str(), "needs": does.needs() }))
    .collect();

    Answered::Read(serde_json::json!({ "triggers": triggers, "does": does }))
}

async fn arranged_a_flow(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let new: mavi_flows::store::NewFlow = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_flow")))?;

    let mut tx = db.begin().await?;
    let flow = mavi_flows::store::make(&mut tx, &new).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "flows.make",
        "flow",
        Some(&flow.id.to_string()),
        &serde_json::json!({ "trigger": flow.trigger, "steps": flow.steps.len() }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(flow).map_err(Error::internal)?,
        receipt,
    ))
}

async fn changed_a_flow(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let changes: mavi_flows::store::FlowChanges = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_change_to_a_flow")))?;

    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;
    let flow = mavi_flows::store::change(&mut tx, id, &changes).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "flows.change",
        "flow",
        Some(&id.to_string()),
        &serde_json::json!({ "on": flow.on }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(flow).map_err(Error::internal)?,
        receipt,
    ))
}

async fn removed_a_flow(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;

    mavi_flows::store::remove(&mut tx, id).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "flows.remove",
        "flow",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

async fn runs(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let page = mavi_flows::store::runs(
        &mut tx,
        a_uuid(asked)?,
        asked.query.get("state").map(String::as_str),
        &asking(asked),
    )
    .await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn one_run(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let run = mavi_flows::store::a_run_of_it(&mut tx, a_uuid(asked)?).await?;

    Ok(Answered::Read(
        serde_json::to_value(run).map_err(Error::internal)?,
    ))
}

async fn tried_a_flow(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let would = mavi_flows::store::would_do(&mut tx, a_uuid(asked)?, &asked.body).await?;

    // Nothing left the machine and no run was written, so there is nothing to
    // record. A `POST` because it carries what to try it against.
    Ok(Answered::Read(
        serde_json::to_value(would).map_err(Error::internal)?,
    ))
}

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

    let mut tx = db.begin().await?;
    let change = mavi_design::store::publish(&mut tx, id).await?;

    let queue = mavi_work::Queue::of(&crate::work());
    queue
        .add(
            &mut tx,
            mavi_design::PUT_IT_LIVE.name,
            &serde_json::json!({ "change": id }),
            None,
        )
        .await?;

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
