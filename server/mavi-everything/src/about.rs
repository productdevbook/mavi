//! What a site holds about one person, and taking it away.
//!
//! Here rather than in a domain because it is not one domain's question. An
//! address turns up as an account, as somebody on a mailing list, as a
//! student, as an order, and inside what somebody sent through a form — five
//! crates, and no one of them may ask the others. This is the crate whose job
//! is exactly that.
//!
//! ## The rule that makes erasing possible at all
//!
//! **What is a financial record is emptied of the person rather than deleted.**
//! An order that vanishes is a bill nobody can explain, and the rule that says
//! keep it and the rule that says remove them are both true at once. So an
//! order keeps its number, its lines and its total, and stops saying who
//! bought it.
//!
//! Everything else goes: the account, the place on a mailing list, the
//! enrolment, what they sent through a form.
//!
//! ## And the one thing that is refused
//!
//! An address that is the site's only owner is not touched at all. Blanking
//! that account would leave a row satisfying "an owner exists" with nobody
//! able to sign into it — so nobody able to grant the role onward either,
//! which is worse than refusing. The refusal says why, and a second go once
//! somebody else is an owner starts from everything still in place.

use mavi_api::{Answers, Endpoint, Field, Is, Method, Of, Shape, Who};
use mavi_core::error::{Error, Result};
use mavi_core::grant::{Access, Needs};
use mavi_core::say::Say;
use mavi_db::Tx;
use serde_json::{Value, json};
use sqlx::Row;

pub const THAT_IS_THE_ONLY_WAY_IN: &str = "that_is_the_only_way_in";

#[must_use]
pub fn to_read() -> Needs {
    Needs::new("people", Access::View)
}

#[must_use]
pub fn to_erase() -> Needs {
    Needs::new("people", Access::Write)
}

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Post,
            path: "/api/about",
            named: "about.gather",
            about: "Everything this site holds about one address, in one \
                    answer.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            // A post rather than an address in the path, because an address in
            // a path is an address in a log, in a browser's history, and in
            // whatever sits in front of this.
            takes: Some("Somebody"),
            answers: Answers::With("Held"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Post,
            path: "/api/about/forget",
            named: "about.forget",
            about: "Takes away what this site holds about one address. What is \
                    a financial record is emptied of them rather than deleted.",
            who: Who::AnAccount,
            parameters: Vec::new(),
            takes: Some("Somebody"),
            answers: Answers::With("Forgotten"),
            refuses: &[mavi_core::error::Code::Conflict],
            changes: true,
        },
    ]
}

#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "Somebody",
            "Which address this is about.",
            vec![Field::new(
                "email",
                Of::One(Is::Text),
                "Where they are reached.",
            )],
        ),
        Shape::new(
            "Held",
            "What this site holds about one address, counted, across \
             everything that could hold it.",
            vec![
                Field::new("account", Of::One(Is::Number), "An account here."),
                Field::new("on_lists", Of::One(Is::Number), "Places on a mailing list."),
                Field::new(
                    "learning",
                    Of::One(Is::Number),
                    "As somebody learning here.",
                ),
                Field::new("orders", Of::One(Is::Number), "Orders placed."),
                Field::new(
                    "sent_through_forms",
                    Of::One(Is::Number),
                    "Things sent through a form whose answers name this \
                     address anywhere in them.",
                ),
            ],
        ),
        Shape::new(
            "Forgotten",
            "What went, and what was emptied rather than taken away.",
            vec![
                Field::new("account", Of::One(Is::Number), "Accounts removed."),
                Field::new("on_lists", Of::One(Is::Number), "Places on a list removed."),
                Field::new("learning", Of::One(Is::Number), "Enrolments removed."),
                Field::new(
                    "orders_emptied",
                    Of::One(Is::Number),
                    "Orders kept and emptied of the person. A bill that \
                     vanished is one nobody can explain.",
                ),
                Field::new(
                    "sent_through_forms",
                    Of::One(Is::Number),
                    "Things sent through a form, removed.",
                ),
            ],
        ),
    ]
}

/// What is held, counted.
pub async fn gather(tx: &mut Tx, email: &str) -> Result<Value> {
    let row = sqlx::query(
        "select
            (select count(*) from people where email = $1 and deleted_at is null) as account,
            (select count(*) from on_a_list o join readers r on r.id = o.reader_id
              where r.email = $1) as on_lists,
            (select count(*) from students where email = $1 and deleted_at is null) as learning,
            (select count(*) from orders where email = $1) as orders,
            (select count(*) from filled where answers::text ilike '%' || $1 || '%')
                as sent_through_forms",
    )
    .bind(email)
    .fetch_one(tx.conn())
    .await
    .map_err(Error::internal)?;

    let of = |named: &str| -> Result<i64> { row.try_get(named).map_err(Error::internal) };

    Ok(json!({
        "account": of("account")?,
        "on_lists": of("on_lists")?,
        "learning": of("learning")?,
        "orders": of("orders")?,
        "sent_through_forms": of("sent_through_forms")?,
    }))
}

/// What this installation says instead of somebody's address, once it has been
/// asked to forget them.
///
/// A reserved domain, so it is not an address anybody could ever hold and a
/// letter sent to it goes nowhere.
const NOBODY: &str = "forgotten@example.invalid";

/// Takes away what is held.
pub async fn forget(tx: &mut Tx, email: &str) -> Result<Value> {
    // Asked before anything is written. An owner nobody could sign in as is
    // worse than an address that is still here, and a refusal that arrives
    // half way through is a site in a state nobody chose.
    let last_way_in: i64 = sqlx::query_scalar(
        "select count(*) from people p join roles r on r.id = p.role_id
          where r.is_the_owner and p.deleted_at is null and p.standing = 'here'
            and p.password is not null and p.email <> $1",
    )
    .bind(email)
    .fetch_one(tx.conn())
    .await
    .map_err(Error::internal)?;

    let is_an_owner: i64 = sqlx::query_scalar(
        "select count(*) from people p join roles r on r.id = p.role_id
          where r.is_the_owner and p.email = $1 and p.deleted_at is null",
    )
    .bind(email)
    .fetch_one(tx.conn())
    .await
    .map_err(Error::internal)?;

    if is_an_owner > 0 && last_way_in == 0 {
        return Err(Error::conflict(Say::of(THAT_IS_THE_ONLY_WAY_IN)));
    }

    let account = gone(
        tx,
        "update people set deleted_at = now() where email = $1 and deleted_at is null",
        email,
    )
    .await?;
    let on_lists = gone(
        tx,
        "delete from on_a_list where reader_id in (select id from readers where email = $1)",
        email,
    )
    .await?;

    gone(tx, "delete from readers where email = $1", email).await?;

    let learning = gone(
        tx,
        "update students set deleted_at = now() where email = $1 and deleted_at is null",
        email,
    )
    .await?;

    let sent_through_forms = gone(
        tx,
        "delete from filled where answers::text ilike '%' || $1 || '%'",
        email,
    )
    .await?;

    // Kept, and emptied. The order still adds up and still says what was
    // bought; it stops saying who bought it.
    let orders_emptied = sqlx::query("update orders set email = $2 where email = $1")
        .bind(email)
        .bind(NOBODY)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?
        .rows_affected();

    Ok(json!({
        "account": account,
        "on_lists": on_lists,
        "learning": learning,
        "orders_emptied": orders_emptied,
        "sent_through_forms": sent_through_forms,
    }))
}

async fn gone(tx: &mut Tx, sql: &str, email: &str) -> Result<u64> {
    Ok(sqlx::query(sql)
        .bind(email)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?
        .rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mavi_api::Api;

    #[test]
    fn it_says_everything_about_itself() {
        let holes = Api::of(endpoints()).holes();

        assert!(holes.is_empty(), "{holes:#?}");
    }

    #[test]
    fn an_address_is_not_put_in_a_path() {
        // An address in a path is an address in a log, in a browser's history,
        // and in whatever sits in front of this. Both of these take it in a
        // body for that reason, including the one that only reads.
        for endpoint in endpoints() {
            assert!(
                !endpoint.path.contains('{'),
                "{} carries something in its address",
                endpoint.named
            );
            assert_eq!(endpoint.takes, Some("Somebody"));
        }
    }

    #[test]
    fn what_is_gathered_and_what_is_forgotten_line_up() {
        // Two shapes on purpose — one counts what is held, the other counts
        // what happened — and the difference between them is the whole rule:
        // orders are emptied rather than taken away, so the second says
        // `orders_emptied` where the first says `orders`.
        let held: Vec<&str> = shapes()
            .iter()
            .find(|shape| shape.named == "Held")
            .expect("held")
            .fields()
            .iter()
            .map(|field| field.name)
            .collect();

        let forgotten: Vec<&str> = shapes()
            .iter()
            .find(|shape| shape.named == "Forgotten")
            .expect("forgotten")
            .fields()
            .iter()
            .map(|field| field.name)
            .collect();

        assert!(held.contains(&"orders"));
        assert!(forgotten.contains(&"orders_emptied"));
        assert!(!forgotten.contains(&"orders"));
    }
}
