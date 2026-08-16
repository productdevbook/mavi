//! Reading and writing a site's letters, lists and readers.
//!
//! Two things here are the crate's own rules meeting the database: a wording
//! is checked against what its kind may name before it is written, and a
//! sending goes to whoever [`crate::who::may_write`] says it may go to rather
//! than to everybody on the list.

use chrono::{DateTime, Utc};
use mavi_core::email::Email;
use mavi_core::error::{Error, Result};
use mavi_core::page::{Page, Query};
use mavi_core::say::Say;
use mavi_db::{Tx, Walk};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::letter::{KINDS, Kind, Pressed, Wording, press};
use crate::who::{Purpose, Standing, may_write};
use crate::{ON_A_LIST, way_out};

pub const THERE_IS_NO_LIST_LIKE_THAT: &str = "there_is_no_list_like_that";
pub const NOBODY_HERE_IS_READING_UNDER_THAT: &str = "nobody_here_is_reading_under_that";
pub const SOMEBODY_IS_ALREADY_READING_AT_THAT_ADDRESS: &str =
    "somebody_is_already_reading_at_that_address";

/// One letter, as a screen sees it: what it says, and whether that is the
/// site's own wording or this machine's.
#[derive(Clone, Debug, Serialize)]
pub struct Letter {
    pub kind: String,
    pub language: String,
    pub subject: String,
    pub body: String,
    /// False when this is what this machine says, having been told nothing.
    pub theirs: bool,
    /// What this letter may name. Answered rather than written in a manual: a
    /// panel that has to know the list is a panel that goes out of date.
    pub names: Vec<String>,
}

/// Every letter this site sends, in one language.
///
/// Every kind, always — a kind with no wording of its own is answered with
/// this machine's, marked as such. A list that only had the ones somebody had
/// edited would be a screen that hides the letters nobody has looked at, which
/// are exactly the ones worth looking at.
pub async fn letters(tx: &mut Tx, language: &str) -> Result<Vec<Letter>> {
    let mut all = Vec::with_capacity(KINDS.len());

    for kind in KINDS {
        let theirs = wording(tx, *kind, language).await?;

        let (wording, mine) = match theirs {
            Some(theirs) => (theirs, false),
            None => (Wording::ours(*kind), true),
        };

        all.push(Letter {
            kind: kind.as_str().to_owned(),
            language: wording.language,
            subject: wording.subject,
            body: wording.body,
            theirs: !mine,
            names: kind
                .names()
                .iter()
                .map(|named| (*named).to_owned())
                .collect(),
        });
    }

    Ok(all)
}

/// What a site says for one kind, in one language, or in its own language.
///
/// Ordered rather than left to whichever row came back first: the site's own
/// language where the reader's is not written is better than a stranger's in
/// the right one, but only if the two are asked for in that order.
async fn wording(tx: &mut Tx, kind: Kind, language: &str) -> Result<Option<Wording>> {
    let row = sqlx::query(
        "select language, subject, body from letters
          where kind = $1
          order by (language = $2) desc, language
          limit 1",
    )
    .bind(kind.as_str())
    .bind(language)
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?;

    let Some(row) = row else {
        return Ok(None);
    };

    let language: String = row.try_get("language").map_err(Error::internal)?;
    let subject: String = row.try_get("subject").map_err(Error::internal)?;
    let body: String = row.try_get("body").map_err(Error::internal)?;

    // Checked on the way out as well: a row written before a kind's names
    // changed is a letter that would otherwise go out with a hole in it.
    Ok(Some(Wording::checked(kind, &language, &subject, &body)?))
}

/// Says what one letter says, in one language.
pub async fn write(
    tx: &mut Tx,
    kind: &str,
    language: &str,
    subject: &str,
    body: &str,
) -> Result<Letter> {
    let kind = Kind::parse(kind)?;
    let wording = Wording::checked(kind, language, subject, body)?;

    sqlx::query(
        "insert into letters (kind, language, subject, body) values ($1, $2, $3, $4)
         on conflict (kind, language)
         do update set subject = excluded.subject, body = excluded.body, updated_at = now()",
    )
    .bind(kind.as_str())
    .bind(&wording.language)
    .bind(&wording.subject)
    .bind(&wording.body)
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    Ok(Letter {
        kind: kind.as_str().to_owned(),
        language: wording.language,
        subject: wording.subject,
        body: wording.body,
        theirs: true,
        names: kind
            .names()
            .iter()
            .map(|named| (*named).to_owned())
            .collect(),
    })
}

/// Goes back to this machine's own wording for one letter.
pub async fn forget(tx: &mut Tx, kind: &str, language: &str) -> Result<()> {
    let kind = Kind::parse(kind)?;

    sqlx::query("delete from letters where kind = $1 and language = $2")
        .bind(kind.as_str())
        .bind(language)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    Ok(())
}

/// What one letter looks like filled in, without sending it.
pub async fn pressed(
    tx: &mut Tx,
    kind: &str,
    language: &str,
    values: &[(&str, String)],
) -> Result<Pressed> {
    let kind = Kind::parse(kind)?;

    let wording = match wording(tx, kind, language).await? {
        Some(theirs) => theirs,
        None => Wording::ours(kind),
    };

    press(&wording, values)
}

/// One of a site's lists.
#[derive(Clone, Debug, Serialize)]
pub struct List {
    pub id: Uuid,
    pub name: String,
    /// How many are on it and can still be written to. Not how many rows there
    /// are: a list of nine hundred that nobody may write to is a number that
    /// tells whoever reads it the wrong thing.
    pub reading: i64,
    pub created_at: DateTime<Utc>,
}

/// The site's lists, and how many are on each.
pub async fn lists(tx: &mut Tx) -> Result<Vec<List>> {
    let rows = sqlx::query(
        "select l.id, l.name, l.created_at,
                (select count(*) from on_a_list o
                   join readers r on r.id = o.reader_id
                  where o.list_id = l.id and r.standing = 'subscribed') as reading
           from mail_lists l
          order by l.created_at desc, l.id desc",
    )
    .fetch_all(tx.conn())
    .await
    .map_err(Error::internal)?;

    rows.iter()
        .map(|row| {
            Ok(List {
                id: row.try_get("id").map_err(Error::internal)?,
                name: row.try_get("name").map_err(Error::internal)?,
                reading: row.try_get("reading").map_err(Error::internal)?,
                created_at: row.try_get("created_at").map_err(Error::internal)?,
            })
        })
        .collect()
}

/// Makes one.
pub async fn add_list(tx: &mut Tx, name: &str) -> Result<List> {
    let id = Uuid::now_v7();

    sqlx::query("insert into mail_lists (id, name) values ($1, $2)")
        .bind(id)
        .bind(name.trim())
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    Ok(List {
        id,
        name: name.trim().to_owned(),
        reading: 0,
        created_at: Utc::now(),
    })
}

/// Somebody a site writes to.
#[derive(Clone, Debug, Serialize)]
pub struct Reader {
    pub id: Uuid,
    pub email: String,
    pub name: Option<String>,
    pub standing: String,
    pub created_at: DateTime<Utc>,
}

/// Who is on one list.
pub async fn readers(
    tx: &mut Tx,
    list: Uuid,
    standing: Option<&str>,
    query: &Query,
) -> Result<Page<Reader>> {
    let walk = Walk::new(ON_A_LIST, query.after(ON_A_LIST)?);
    let mut wheres = vec!["o.list_id = $1".to_owned()];
    let mut binds: Vec<String> = Vec::new();

    if let Some(standing) = standing {
        binds.push(standing.to_owned());
        wheres.push(format!("r.standing = ${}", binds.len() + 1));
    }

    let cursor = walk.after(binds.len() + 2);
    if let Some((sql, _)) = &cursor {
        wheres.push(sql.clone());
    }

    // The pairing's own columns, because the order is the pairing's: when
    // somebody was put on this list, not when the site first heard of them.
    let sql = format!(
        "select * from (
             select r.id, r.email, r.name, r.standing, o.created_at, o.reader_id
               from on_a_list o join readers r on r.id = o.reader_id
              where {}
         ) as reader order by {} limit {}",
        wheres.join(" and "),
        walk.order(),
        query.fetch(),
    );

    let mut asking = sqlx::query(&sql).bind(list);

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
        .map(|row| {
            Ok(Reader {
                id: row.try_get("id").map_err(Error::internal)?,
                email: row.try_get("email").map_err(Error::internal)?,
                name: row.try_get("name").map_err(Error::internal)?,
                standing: row.try_get("standing").map_err(Error::internal)?,
                created_at: row.try_get("created_at").map_err(Error::internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Page::build(query, ON_A_LIST, rows, |reader| {
        vec![reader.created_at.to_rfc3339(), reader.id.to_string()]
    })
}

/// Puts somebody on a list.
///
/// Somebody the site already writes to keeps the standing they had. Adding
/// them to a second list is not a way to undo their having left the first one.
pub async fn add_reader(
    tx: &mut Tx,
    list: Uuid,
    email: &str,
    name: Option<&str>,
) -> Result<Reader> {
    let email = Email::parse(email)?;
    let minted = way_out();

    let row = sqlx::query(
        "insert into readers (id, email, name, way_out) values ($1, $2, $3, $4)
         on conflict (email) do update set name = coalesce(excluded.name, readers.name)
         returning id, email, name, standing, created_at",
    )
    .bind(Uuid::now_v7())
    .bind(email.as_str())
    .bind(name)
    .bind(minted.hash.as_slice())
    .fetch_one(tx.conn())
    .await
    .map_err(Error::internal)?;

    let id: Uuid = row.try_get("id").map_err(Error::internal)?;

    sqlx::query(
        "insert into on_a_list (reader_id, list_id) values ($1, $2) on conflict do nothing",
    )
    .bind(id)
    .bind(list)
    .execute(tx.conn())
    .await
    .map_err(|_| Error::not_found(Say::of(THERE_IS_NO_LIST_LIKE_THAT)))?;

    Ok(Reader {
        id,
        email: row.try_get("email").map_err(Error::internal)?,
        name: row.try_get("name").map_err(Error::internal)?,
        standing: row.try_get("standing").map_err(Error::internal)?,
        created_at: row.try_get("created_at").map_err(Error::internal)?,
    })
}

/// Forgets somebody entirely, lists and all.
pub async fn forget_reader(tx: &mut Tx, id: Uuid) -> Result<()> {
    let gone = sqlx::query("delete from readers where id = $1")
        .bind(id)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    if gone.rows_affected() == 0 {
        return Err(Error::not_found(Say::of(NOBODY_HERE_IS_READING_UNDER_THAT)));
    }

    Ok(())
}

/// Who a sending would actually go to.
///
/// Asked here rather than by whatever sends it, so the number a person sees
/// before pressing the button is the number of letters that will leave. What
/// somebody did about the newsletter is read per reader by [`may_write`], so
/// somebody who left is left out and somebody who bounced is left out, and
/// each for its own reason.
pub async fn who_it_goes_to(tx: &mut Tx, list: Uuid) -> Result<Vec<(Uuid, String)>> {
    let rows = sqlx::query(
        "select r.id, r.email, r.standing from on_a_list o
           join readers r on r.id = o.reader_id
          where o.list_id = $1",
    )
    .bind(list)
    .fetch_all(tx.conn())
    .await
    .map_err(Error::internal)?;

    let mut going = Vec::new();

    for row in &rows {
        let standing: String = row.try_get("standing").map_err(Error::internal)?;

        let standing = match standing.as_str() {
            "unsubscribed" => Standing::Unsubscribed,
            "bounced" => Standing::Bounced,
            "complained" => Standing::Complained,
            _ => Standing::Subscribed,
        };

        if may_write(standing, Purpose::ToAList).is_ok() {
            going.push((
                row.try_get("id").map_err(Error::internal)?,
                row.try_get("email").map_err(Error::internal)?,
            ));
        }
    }

    Ok(going)
}

/// Takes somebody off, by the link at the bottom of a letter.
///
/// Answers the same whether the token was theirs or was never anything. A link
/// in an inbox is a link in whatever else reads that inbox, and the difference
/// between "that was yours" and "that was nobody's" is a way to ask who is on
/// this site's lists.
pub async fn out(tx: &mut Tx, token: &str) -> Result<()> {
    sqlx::query(
        "update readers set standing = 'unsubscribed', left_at = now(), updated_at = now()
          where way_out = $1 and standing <> 'complained'",
    )
    .bind(mavi_people::token::hash(token).as_slice())
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    Ok(())
}

/// What a sending asks for.
#[derive(Clone, Debug, Deserialize)]
pub struct NewSending {
    pub subject: String,
    pub body: String,
}
