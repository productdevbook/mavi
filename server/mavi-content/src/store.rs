//! Reading and writing what a site wrote.
//!
//! Every query in here is built out of the declarations beside it — the
//! keyset, the filter — rather than typed out with its order and its cursor
//! written separately. That is the whole reason the crate this replaces had
//! fourteen listings whose cursor addressed less than their order.
//!
//! Nothing here decides whether the caller may do it. That was decided before
//! this was reached, by the guard, out of what the endpoint declared.

use chrono::{DateTime, Utc};
use mavi_core::error::{Error, Result};
use mavi_core::page::{Keyset, Page, Query};
use mavi_core::say::Say;
use mavi_core::slug::Slug;
use mavi_db::{Tx, Walk};
use sqlx::Row;
use sqlx::postgres::PgRow;
use uuid::Uuid;

use crate::listing::{BY_FEED, BY_RECENT, Filter};
use crate::writing::{
    Kind, NOTHING_IS_WRITTEN_AT_THAT_ADDRESS, New, SOMETHING_ELSE_ANSWERS_AT_THAT_ADDRESS, State,
    Writing, WritingId,
};

/// What a row is, in one place.
///
/// Written once rather than at each query, because a listing and a read that
/// build the same type out of different columns are two answers to what a
/// writing is.
const COLUMNS: &str = "id, kind, language, slug, title, excerpt, body, fields, state, \
                       published_at, created_at, updated_at";

fn a_writing(row: &PgRow) -> Result<Writing> {
    let slug: String = row.try_get("slug").map_err(Error::internal)?;
    let kind: String = row.try_get("kind").map_err(Error::internal)?;
    let state: String = row.try_get("state").map_err(Error::internal)?;

    Ok(Writing {
        id: WritingId(row.try_get("id").map_err(Error::internal)?),
        kind: Kind::parse(&kind)?,
        language: row.try_get("language").map_err(Error::internal)?,
        slug: Slug::parse(&slug)?,
        title: row.try_get("title").map_err(Error::internal)?,
        excerpt: row.try_get("excerpt").map_err(Error::internal)?,
        body: row.try_get("body").map_err(Error::internal)?,
        fields: row.try_get("fields").map_err(Error::internal)?,
        state: if state == State::Published.as_str() {
            State::Published
        } else {
            State::Draft
        },
        published_at: row.try_get("published_at").map_err(Error::internal)?,
        created_at: row.try_get("created_at").map_err(Error::internal)?,
        updated_at: row.try_get("updated_at").map_err(Error::internal)?,
    })
}

/// Where a row sits in an order, for the cursor.
fn at(keyset: Keyset, writing: &Writing) -> Vec<String> {
    keyset
        .keys()
        .iter()
        .map(|key| match key.column {
            "published_at" => writing
                .published_at
                .map(|when| when.to_rfc3339())
                .unwrap_or_default(),
            "created_at" => writing.created_at.to_rfc3339(),
            _ => writing.id.to_string(),
        })
        .collect()
}

/// What a site has written, narrowed and walked.
///
/// The order, the cursor predicate and the values to bind all come from the
/// same two declarations. The numbering is the one thing a person could get
/// wrong here, so it is done once: the filter's binds come first and the
/// cursor's follow them.
pub async fn list(
    tx: &mut Tx,
    feed: bool,
    filter: &Filter,
    query: &Query,
) -> Result<Page<Writing>> {
    let keyset = ordered_by(feed);
    let (mut wheres, binds) = filter.narrows(1);
    let walk = Walk::new(keyset, query.after(keyset)?);

    let cursor = walk.after(binds.len() + 1);
    if let Some((sql, _)) = &cursor {
        wheres.push(sql.clone());
    }

    // A feed is what is out. Said here rather than left to the caller to
    // remember, because a feed that shows drafts is a draft published by
    // accident.
    if feed {
        wheres.push("state = 'published'".to_owned());
        wheres.push("published_at <= now()".to_owned());
    }

    wheres.push("deleted_at is null".to_owned());

    let sql = format!(
        "select {COLUMNS} from writings where {} order by {} limit {}",
        wheres.join(" and "),
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
        .map(a_writing)
        .collect::<Result<Vec<_>>>()?;

    Page::build(query, keyset, rows, |writing| at(keyset, writing))
}

/// One writing, or the refusal that says nothing is written there.
pub async fn read(tx: &mut Tx, id: WritingId) -> Result<Writing> {
    let row = sqlx::query(&format!(
        "select {COLUMNS} from writings where id = $1 and deleted_at is null"
    ))
    .bind(Uuid::from(id))
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?;

    row.as_ref()
        .map(a_writing)
        .transpose()?
        .ok_or_else(|| Error::not_found(Say::of(NOTHING_IS_WRITTEN_AT_THAT_ADDRESS)))
}

/// Writes one.
///
/// The address is taken by the database rather than by a look first: between
/// looking and writing is exactly where two people publishing at the same
/// address both find it free.
pub async fn make(tx: &mut Tx, new: &New) -> Result<Writing> {
    let (kind, slug) = new.checked()?;
    let (state, published_at) = new.goes_out();

    let row = sqlx::query(&format!(
        "insert into writings (id, kind, language, slug, title, excerpt, body, fields, state, published_at)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
         returning {COLUMNS}"
    ))
    .bind(Uuid::now_v7())
    .bind(kind.as_str())
    .bind(&new.language)
    .bind(slug.as_str())
    .bind(new.title.trim())
    .bind(new.excerpt.as_deref())
    .bind(&new.body)
    .bind(&new.fields)
    .bind(state.as_str())
    .bind(published_at)
    .fetch_one(tx.conn())
    .await
    .map_err(|cause| {
        if crate::writing::taken(&cause) {
            Error::conflict(Say::of(SOMETHING_ELSE_ANSWERS_AT_THAT_ADDRESS))
        } else {
            Error::internal(cause)
        }
    })?;

    a_writing(&row)
}

/// What may be changed about one.
///
/// Serialised as well as read, for the same reason [`crate::writing::New`] is:
/// so what it says it takes is held against what it takes.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Changes {
    /// Where it answers. Renaming leaves the old address working: a link
    /// somebody made last year is not something to break because a title was
    /// improved.
    pub slug: Option<String>,
    pub title: Option<String>,
    pub excerpt: Option<String>,
    pub body: Option<String>,
    pub fields: Option<serde_json::Value>,
    /// Absent leaves it where it is. `draft` takes something back off the
    /// site; a date sends it out then.
    pub publish_at: Option<Option<DateTime<Utc>>>,
}

/// Where a page went, for an address the site has no page at.
///
/// Asked by the edge, and answered with every language the name was used in —
/// choosing between them is the edge's, because only the address says which
/// language somebody was reading.
pub async fn moved(tx: &mut Tx, slug: &str) -> Result<Vec<(String, String)>> {
    let rows = sqlx::query("select language, now_at from redirects where was = $1")
        .bind(slug)
        .fetch_all(tx.conn())
        .await
        .map_err(Error::internal)?;

    rows.iter()
        .map(|row| {
            Ok((
                row.try_get("language").map_err(Error::internal)?,
                row.try_get("now_at").map_err(Error::internal)?,
            ))
        })
        .collect()
}

/// Changes one.
///
/// Only what was said. A change that writes every column writes the ones the
/// caller never mentioned back over whatever somebody else changed a second
/// ago.
pub async fn change(tx: &mut Tx, id: WritingId, changes: &Changes) -> Result<Writing> {
    let now = read(tx, id).await?;

    let title = changes.title.as_deref().unwrap_or(&now.title).trim();
    if !(1..=200).contains(&title.chars().count()) {
        return Err(Error::invalid(Say::of(
            crate::writing::A_TITLE_IS_BETWEEN_ONE_AND_TWO_HUNDRED,
        )));
    }

    let (state, published_at) = match changes.publish_at {
        None => (now.state, now.published_at),
        Some(None) => (State::Draft, None),
        Some(Some(when)) => (State::Published, Some(when)),
    };

    // The address, and what the old one now points at. Written in the same
    // transaction as the rename, because a rename that leaves no redirect is
    // every link anybody made answering "not here" — and a redirect written
    // afterwards is one a crash between the two loses.
    let slug = match &changes.slug {
        Some(asked) => {
            let slug = Slug::parse(asked)?;

            if slug.as_str() != now.slug.as_str() {
                sqlx::query(
                    "insert into redirects (was, language, now_at) values ($1, $2, $3)
                     on conflict (was, language) do update set now_at = excluded.now_at",
                )
                .bind(now.slug.as_str())
                .bind(&now.language)
                .bind(slug.as_str())
                .execute(tx.conn())
                .await
                .map_err(Error::internal)?;
            }

            slug.as_str().to_owned()
        }
        None => now.slug.as_str().to_owned(),
    };

    let row = sqlx::query(&format!(
        "update writings
            set slug = $8,
                title = $2,
                excerpt = coalesce($3, excerpt),
                body = coalesce($4, body),
                fields = coalesce($5, fields),
                state = $6,
                published_at = $7,
                updated_at = now()
          where id = $1 and deleted_at is null
         returning {COLUMNS}"
    ))
    .bind(Uuid::from(id))
    .bind(title)
    .bind(changes.excerpt.as_deref())
    .bind(changes.body.as_deref())
    .bind(changes.fields.as_ref())
    .bind(state.as_str())
    .bind(published_at)
    .bind(&slug)
    .fetch_optional(tx.conn())
    .await
    .map_err(|cause| {
        if crate::writing::taken(&cause) {
            Error::conflict(Say::of(SOMETHING_ELSE_ANSWERS_AT_THAT_ADDRESS))
        } else {
            Error::internal(cause)
        }
    })?;

    row.as_ref()
        .map(a_writing)
        .transpose()?
        .ok_or_else(|| Error::not_found(Say::of(NOTHING_IS_WRITTEN_AT_THAT_ADDRESS)))
}

/// Throws one away.
///
/// Kept, with the moment it went. The row holds its address until something
/// sweeps it, and the index that makes an address unique ignores it — so the
/// address is free the moment it is thrown away, which is what somebody
/// deleting a page and writing a new one at the same address expects.
pub async fn remove(tx: &mut Tx, id: WritingId) -> Result<()> {
    let gone =
        sqlx::query("update writings set deleted_at = now() where id = $1 and deleted_at is null")
            .bind(Uuid::from(id))
            .execute(tx.conn())
            .await
            .map_err(Error::internal)?;

    if gone.rows_affected() == 0 {
        return Err(Error::not_found(Say::of(
            NOTHING_IS_WRITTEN_AT_THAT_ADDRESS,
        )));
    }

    Ok(())
}

/// Which order a listing is walked in.
///
/// A feed and the panel are different orders — what is out, most recently
/// published, against everything, most recently written — and each has an
/// index in the schema matching it column for column.
#[must_use]
pub const fn ordered_by(feed: bool) -> Keyset {
    if feed { BY_FEED } else { BY_RECENT }
}
