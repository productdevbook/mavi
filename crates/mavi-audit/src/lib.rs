//! What was done, and by whom.
//!
//! Every change a site makes leaves one row here, written **in the same
//! transaction as the change itself**. Not beside it, not after it: a receipt
//! written afterwards is one that a crash between the two loses, and what it
//! loses is the record of the thing that did happen.
//!
//! That is why [`Receipt`] lives here rather than beside the guard that
//! demands one. [`record`] is the only thing that makes one, so a handler
//! holding a receipt has written a row — the guard's rule stops being a rule
//! everybody remembers and starts being one the compiler asks about.
//!
//! Nothing here answers a question about the future. A receipt says what was
//! done; whether it should have been is what the grant decided, one moment
//! earlier and somewhere else.

use chrono::{DateTime, Utc};
use mavi_api::{Answers, Endpoint, Is, Method, Parameter, Who as Audience};
use mavi_core::error::{Error, Result};
use mavi_core::grant::{Access, Needs};
use mavi_core::id;
use mavi_core::page::{Key, Keyset, Kind};
use mavi_db::Tx;
use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

id!(
    /// One receipt.
    ReceiptId
);

pub const AUDIT: &str = "audit";

#[must_use]
pub const fn to_read() -> Needs {
    Needs::new(AUDIT, Access::View)
}

/// Proof that a change was written down before it answered.
///
/// Held rather than checked: a handler that changes something returns one of
/// these, and **one cannot be made without writing the row** — there is no
/// public way to build one, in this crate or any other. The alternative, a
/// rule everybody remembers, is the version that had a hole in it for as long
/// as there were two ways in.
#[derive(Debug)]
pub struct Receipt {
    /// The row this change wrote.
    pub wrote: Uuid,
}

impl Receipt {
    const fn of(wrote: Uuid) -> Self {
        Self { wrote }
    }

    /// A receipt for a change nothing made, for tests that need one and have
    /// no database.
    ///
    /// Behind a feature that nothing shipping turns on, because a receipt that
    /// can be conjured is not proof of anything.
    #[cfg(feature = "pretend")]
    #[must_use]
    pub fn pretend() -> Self {
        Self::of(Uuid::now_v7())
    }
}

/// Who did it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Who {
    AnAccount,
    AStudent,
    /// The site itself: a scheduled publish, a sweep, a letter going out.
    /// Written down like anything else, because "nobody did this" is an answer
    /// somebody will need one day.
    TheMachine,
}

impl Who {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Who::AnAccount => "an_account",
            Who::AStudent => "a_student",
            Who::TheMachine => "the_machine",
        }
    }
}

/// Whoever the change is attributed to, and which request it arrived on.
#[derive(Clone, Debug)]
pub struct Actor {
    pub who: Who,
    /// Their id, where there is one. `None` for the machine.
    pub id: Option<String>,
    /// What ties one request's rows together, and ties them to whatever the
    /// logs say about the same moment.
    pub request: String,
}

impl Actor {
    #[must_use]
    pub fn the_machine(request: impl Into<String>) -> Self {
        Self {
            who: Who::TheMachine,
            id: None,
            request: request.into(),
        }
    }
}

/// One receipt, as a screen reads it.
#[derive(Clone, Debug, Serialize)]
pub struct Written {
    pub id: ReceiptId,
    pub who: Who,
    pub who_id: Option<String>,
    /// The endpoint's own name — `writings.publish` — rather than a verb
    /// somebody chose at the call site. Two names for one action is two
    /// answers to "what happened to this".
    pub did: String,
    pub about: String,
    pub about_id: Option<String>,
    pub what: serde_json::Value,
    pub request: String,
    pub created_at: DateTime<Utc>,
}

/// Writes the receipt, in the transaction the change is being made in.
///
/// `what` is whatever somebody reading this in a year needs in order to
/// understand it without the row it describes — which may since have been
/// deleted, and often has been.
pub async fn record(
    tx: &mut Tx,
    actor: &Actor,
    did: &str,
    about: &str,
    about_id: Option<&str>,
    what: &impl Serialize,
) -> Result<Receipt> {
    let what = serde_json::to_value(what).map_err(Error::internal)?;

    let row = sqlx::query(
        "insert into receipts (id, who, who_id, did, about, about_id, what, request)
         values ($1, $2, $3, $4, $5, $6, $7, $8)
         returning id",
    )
    .bind(Uuid::now_v7())
    .bind(actor.who.as_str())
    .bind(actor.id.as_deref())
    .bind(did)
    .bind(about)
    .bind(about_id)
    .bind(what)
    .bind(&actor.request)
    .fetch_one(tx.conn())
    .await
    .map_err(Error::internal)?;

    Ok(Receipt::of(row.get("id")))
}

pub const BY_RECENT: Keyset = Keyset(&[
    Key::newest("created_at", Kind::Moment),
    Key::newest("id", Kind::Id),
]);

/// What this domain answers — and what it does not.
///
/// There is no endpoint here that writes a receipt and none that removes one.
/// A record somebody can add to is a record somebody can write into; a record
/// somebody can delete from is not a record. Both are refused by the database
/// as well, so the absence is not merely an absence.
#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint {
            method: Method::Get,
            path: "/api/audit",
            named: "audit.list",
            about: "What has been done here, newest first.",
            who: Audience::AnAccount,
            parameters: vec![
                Parameter::query(
                    "about",
                    Is::Text,
                    "Only what happened to this sort of thing.",
                ),
                Parameter::query("about_id", Is::Text, "Only what happened to this one."),
                Parameter::query("who_id", Is::Text, "Only what this account did."),
                Parameter::query("after", Is::Text, "The cursor the last page ended with."),
                Parameter::query("limit", Is::Number, "How many, at most a hundred."),
            ],
            takes: None,
            answers: Answers::With("ReceiptPage"),
            refuses: &[],
            changes: false,
        },
        Endpoint {
            method: Method::Get,
            path: "/api/audit/{id}",
            named: "audit.read",
            about: "One receipt, and everything it recorded.",
            who: Audience::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which receipt.")],
            takes: None,
            answers: Answers::With("Receipt"),
            refuses: &[mavi_core::error::Code::NotFound],
            changes: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use mavi_api::Api;

    #[test]
    fn everything_this_domain_answers_is_described_completely() {
        let holes = Api::of(endpoints()).holes();

        assert!(holes.is_empty(), "{holes:#?}");
    }

    #[test]
    fn nothing_here_changes_anything() {
        // A record somebody can add to is a record somebody can write into,
        // and one somebody can delete from is not a record. The database
        // refuses both as well; this says the API never offers them.
        for endpoint in endpoints() {
            assert!(
                !endpoint.changes,
                "{} offers a way to write the record of what was done",
                endpoint.named
            );
        }
    }

    #[test]
    fn what_this_domain_asks_for_is_a_capability_the_site_has() {
        assert!(mavi_people::is_a_capability(AUDIT));
    }
}

/// Reading what was done.
///
/// A record nothing can read is a record in name only, so this is the other
/// half of the crate: the listing the panel opens, and one receipt in full.
pub mod reading {
    use mavi_core::error::{Error, Result};
    use mavi_core::page::{Page, Query};
    use mavi_core::say::Say;
    use mavi_db::{Tx, Walk};
    use sqlx::Row;
    use sqlx::postgres::PgRow;
    use uuid::Uuid;

    use super::{BY_RECENT, ReceiptId, Who, Written};

    pub const NOTHING_WAS_DONE_UNDER_THAT: &str = "nothing_was_done_under_that";

    const COLUMNS: &str = "id, who, who_id, did, about, about_id, what, request, created_at";

    fn a_receipt(row: &PgRow) -> Result<Written> {
        let who: String = row.try_get("who").map_err(Error::internal)?;

        Ok(Written {
            id: ReceiptId(row.try_get("id").map_err(Error::internal)?),
            who: match who.as_str() {
                "an_account" => Who::AnAccount,
                "a_student" => Who::AStudent,
                _ => Who::TheMachine,
            },
            who_id: row.try_get("who_id").map_err(Error::internal)?,
            did: row.try_get("did").map_err(Error::internal)?,
            about: row.try_get("about").map_err(Error::internal)?,
            about_id: row.try_get("about_id").map_err(Error::internal)?,
            what: row.try_get("what").map_err(Error::internal)?,
            request: row.try_get("request").map_err(Error::internal)?,
            created_at: row.try_get("created_at").map_err(Error::internal)?,
        })
    }

    /// What has been done here, newest first.
    ///
    /// Narrowed by what it was about rather than by anything free-text: the
    /// question somebody actually asks is "what happened to this", and there
    /// is an index for exactly that.
    pub async fn list(
        tx: &mut Tx,
        about: Option<&str>,
        about_id: Option<&str>,
        who_id: Option<&str>,
        query: &Query,
    ) -> Result<Page<Written>> {
        let walk = Walk::new(BY_RECENT, query.after(BY_RECENT)?);
        let mut wheres: Vec<String> = Vec::new();
        let mut binds: Vec<String> = Vec::new();

        for (column, value) in [("about", about), ("about_id", about_id), ("who_id", who_id)] {
            if let Some(value) = value {
                binds.push(value.to_owned());
                wheres.push(format!("{column} = ${}", binds.len()));
            }
        }

        let cursor = walk.after(binds.len() + 1);
        if let Some((sql, _)) = &cursor {
            wheres.push(sql.clone());
        }

        let narrowed = if wheres.is_empty() {
            String::new()
        } else {
            format!("where {}", wheres.join(" and "))
        };

        let sql = format!(
            "select {COLUMNS} from receipts {narrowed} order by {} limit {}",
            walk.order(),
            query.fetch(),
        );

        let mut asking = sqlx::query(&sql);

        for bind in binds {
            asking = asking.bind(bind);
        }

        if let Some((_, values)) = cursor {
            for value in values {
                asking = asking.bind(value);
            }
        }

        let rows = asking
            .fetch_all(tx.conn())
            .await
            .map_err(Error::internal)?
            .iter()
            .map(a_receipt)
            .collect::<Result<Vec<_>>>()?;

        Page::build(query, BY_RECENT, rows, |written| {
            vec![written.created_at.to_rfc3339(), written.id.to_string()]
        })
    }

    /// One receipt, and everything it recorded.
    pub async fn read(tx: &mut Tx, id: Uuid) -> Result<Written> {
        let row = sqlx::query(&format!("select {COLUMNS} from receipts where id = $1"))
            .bind(id)
            .fetch_optional(tx.conn())
            .await
            .map_err(Error::internal)?;

        row.as_ref()
            .map(a_receipt)
            .transpose()?
            .ok_or_else(|| Error::not_found(Say::of(NOTHING_WAS_DONE_UNDER_THAT)))
    }
}
