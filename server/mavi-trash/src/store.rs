//! Reading what was thrown away, and putting it back.

use chrono::{DateTime, Utc};
use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use mavi_db::Tx;
use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::kind::{EVERY, Kind};

pub const THERE_IS_NOTHING_LIKE_THAT_IN_THE_BIN: &str = "there_is_nothing_like_that_in_the_bin";
pub const SOMETHING_ELSE_ANSWERS_AT_THAT_ADDRESS: &str = "something_else_answers_at_that_address";

/// One thing somebody threw away.
#[derive(Clone, Debug, Serialize)]
pub struct Thrown {
    pub kind: &'static str,
    pub id: Uuid,
    /// Enough to know which one it is. A trash screen where nine rows say the
    /// same thing is one nobody can restore from.
    pub called: String,
    pub thrown_away_at: DateTime<Utc>,
}

/// Everything a site threw away, newest first.
///
/// One query per sort rather than a union, because the sorts have different
/// columns and a union that made them agree would be a query nobody can read
/// against a screen that shows nine kinds of thing.
pub async fn everything(tx: &mut Tx, how_many: i64) -> Result<Vec<Thrown>> {
    let mut all = Vec::new();

    for kind in EVERY {
        all.extend(of_one_sort(tx, *kind, how_many).await?);
    }

    all.sort_by(|one, other| other.thrown_away_at.cmp(&one.thrown_away_at));
    all.truncate(usize::try_from(how_many).unwrap_or(usize::MAX));

    Ok(all)
}

/// What was thrown away of one sort.
///
/// The table and the column come off the kind, so nothing somebody sent
/// reaches the query — see [`crate::kind`], which is the whole of why this is
/// safe.
pub async fn of_one_sort(tx: &mut Tx, kind: Kind, how_many: i64) -> Result<Vec<Thrown>> {
    let rows = sqlx::query(&format!(
        "select id, {} as called, deleted_at from {}
          where deleted_at is not null
          order by deleted_at desc limit $1",
        kind.called(),
        kind.table(),
    ))
    .bind(how_many)
    .fetch_all(tx.conn())
    .await
    .map_err(Error::internal)?;

    rows.iter()
        .map(|row| {
            Ok(Thrown {
                kind: kind.as_str(),
                id: row.try_get("id").map_err(Error::internal)?,
                called: row.try_get("called").map_err(Error::internal)?,
                thrown_away_at: row.try_get("deleted_at").map_err(Error::internal)?,
            })
        })
        .collect()
}

/// Puts one back.
///
/// What makes this able to fail is the thing a trash screen has to say out
/// loud: an address is free the moment something is thrown away, so somebody
/// may have written a new page at it since. Putting the old one back would be
/// two things answering at one address, and the database refuses that — which
/// is the right answer, said as a refusal somebody can act on rather than a
/// constraint they read in a log.
pub async fn put_back(tx: &mut Tx, kind: Kind, id: Uuid) -> Result<()> {
    let back = sqlx::query(&format!(
        "update {} set deleted_at = null where id = $1 and deleted_at is not null",
        kind.table()
    ))
    .bind(id)
    .execute(tx.conn())
    .await
    .map_err(|cause| match &cause {
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            Error::conflict(Say::of(SOMETHING_ELSE_ANSWERS_AT_THAT_ADDRESS))
        }
        _ => Error::internal(cause),
    })?;

    if back.rows_affected() == 0 {
        return Err(Error::not_found(Say::of(
            THERE_IS_NOTHING_LIKE_THAT_IN_THE_BIN,
        )));
    }

    Ok(())
}

/// Takes one away for good.
///
/// The other half of a bin, and the one that makes keeping things defensible:
/// a row kept for ever because nothing could delete it is not "kept", it is
/// data nobody decided to hold on to.
pub async fn for_good(tx: &mut Tx, kind: Kind, id: Uuid) -> Result<()> {
    let gone = sqlx::query(&format!(
        "delete from {} where id = $1 and deleted_at is not null",
        kind.table()
    ))
    .bind(id)
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    if gone.rows_affected() == 0 {
        return Err(Error::not_found(Say::of(
            THERE_IS_NOTHING_LIKE_THAT_IN_THE_BIN,
        )));
    }

    Ok(())
}
