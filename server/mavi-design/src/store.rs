//! Reading and writing how a site looks.
//!
//! Everything written here goes into a set of changes. There is no function in
//! this file that writes a file into what is published, which is the whole
//! shape of the crate said in the one place somebody would look for a way
//! around it.

use chrono::{DateTime, Utc};
use mavi_core::error::{Error, Result};
use mavi_core::page::{Page, Query};
use mavi_core::say::Say;
use mavi_db::{Tx, Walk};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::where_it_goes::to_write;
use crate::{BY_RECENT, Where};

pub const THERE_IS_NO_CHANGE_LIKE_THAT: &str = "there_is_no_change_like_that";
pub const THAT_FILE_IS_NOT_IN_THIS_PROJECT: &str = "that_file_is_not_in_this_project";
pub const NOTHING_IS_PUBLISHED_YET: &str = "nothing_is_published_yet";
pub const THAT_HAS_NOT_BEEN_BUILT_AND_LOOKED_AT: &str = "that_has_not_been_built_and_looked_at";

/// One set of changes.
#[derive(Clone, Debug, Serialize)]
pub struct Change {
    pub id: Uuid,
    pub name: String,
    pub at: Where,
    pub look_at: Option<String>,
    pub went_wrong: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// One file in a project.
#[derive(Clone, Debug, Serialize)]
pub struct File {
    pub path: String,
    pub contents: String,
    pub removed: bool,
}

fn a_where(said: &str) -> Where {
    match said {
        "to_look_at" => Where::ToLookAt,
        "broken" => Where::Broken,
        "published" => Where::Published,
        _ => Where::Writing,
    }
}

fn a_change(row: &sqlx::postgres::PgRow) -> Result<Change> {
    let at: String = row.try_get("at").map_err(Error::internal)?;

    Ok(Change {
        id: row.try_get("id").map_err(Error::internal)?,
        name: row.try_get("name").map_err(Error::internal)?,
        at: a_where(&at),
        look_at: row.try_get("look_at").map_err(Error::internal)?,
        went_wrong: row.try_get("went_wrong").map_err(Error::internal)?,
        created_at: row.try_get("created_at").map_err(Error::internal)?,
    })
}

const COLUMNS: &str = "id, name, at, look_at, went_wrong, created_at";

/// Sets of changes, newest first.
pub async fn changes(tx: &mut Tx, query: &Query) -> Result<Page<Change>> {
    let walk = Walk::new(BY_RECENT, query.after(BY_RECENT)?);
    let cursor = walk.after(1);

    let narrowed = match &cursor {
        Some((sql, _)) => format!("where {sql}"),
        None => String::new(),
    };

    let sql = format!(
        "select {COLUMNS} from changes {narrowed} order by {} limit {}",
        walk.order(),
        query.fetch(),
    );

    let mut asking = sqlx::query(&sql);

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
        .map(a_change)
        .collect::<Result<Vec<_>>>()?;

    Page::build(query, BY_RECENT, rows, |change| {
        vec![change.created_at.to_rfc3339(), change.id.to_string()]
    })
}

/// Starts a set of changes from what is published now.
pub async fn start(tx: &mut Tx, name: &str) -> Result<Change> {
    let id = Uuid::now_v7();

    let row = sqlx::query(&format!(
        "insert into changes (id, name) values ($1, $2) returning {COLUMNS}"
    ))
    .bind(id)
    .bind(name.trim())
    .fetch_one(tx.conn())
    .await
    .map_err(Error::internal)?;

    // What is published is where a new set of changes starts from, so the
    // files that are live are copied in rather than left to be worked out
    // later by whoever builds it.
    if let Some(published) = published(tx).await? {
        sqlx::query(
            "insert into design_files (change_id, path, contents, removed)
             select $1, path, contents, removed from design_files where change_id = $2",
        )
        .bind(id)
        .bind(published)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;
    }

    a_change(&row)
}

/// Which set of changes is the live one, if any.
pub async fn published(tx: &mut Tx) -> Result<Option<Uuid>> {
    sqlx::query_scalar("select id from changes where at = 'published'")
        .fetch_optional(tx.conn())
        .await
        .map_err(Error::internal)
}

/// Where a set of changes has got to.
pub async fn read(tx: &mut Tx, id: Uuid) -> Result<Change> {
    let row = sqlx::query(&format!("select {COLUMNS} from changes where id = $1"))
        .bind(id)
        .fetch_optional(tx.conn())
        .await
        .map_err(Error::internal)?
        .ok_or_else(|| Error::not_found(Say::of(THERE_IS_NO_CHANGE_LIKE_THAT)))?;

    a_change(&row)
}

/// Which set of changes a request is about: the one it named, or the published
/// one.
async fn which(tx: &mut Tx, change: Option<Uuid>) -> Result<Uuid> {
    match change {
        Some(id) => Ok(id),
        None => published(tx)
            .await?
            .ok_or_else(|| Error::not_found(Say::of(NOTHING_IS_PUBLISHED_YET))),
    }
}

/// Everything in the project that the panel may change.
pub async fn files(tx: &mut Tx, change: Option<Uuid>) -> Result<Vec<String>> {
    let change = which(tx, change).await?;

    sqlx::query_scalar(
        "select path from design_files where change_id = $1 and not removed order by path",
    )
    .bind(change)
    .fetch_all(tx.conn())
    .await
    .map_err(Error::internal)
}

/// One file, as it stands.
pub async fn read_file(tx: &mut Tx, change: Option<Uuid>, path: &str) -> Result<File> {
    let change = which(tx, change).await?;
    let path = to_write(path)?;

    let row = sqlx::query(
        "select path, contents, removed from design_files
          where change_id = $1 and path = $2",
    )
    .bind(change)
    .bind(&path)
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?
    .ok_or_else(|| Error::not_found(Say::of(THAT_FILE_IS_NOT_IN_THIS_PROJECT)))?;

    let contents: Vec<u8> = row.try_get("contents").map_err(Error::internal)?;

    Ok(File {
        path: row.try_get("path").map_err(Error::internal)?,
        // Whatever is in it, as text. A file that is not text is a picture,
        // and a picture is not something the panel's editor opens.
        contents: String::from_utf8_lossy(&contents).into_owned(),
        removed: row.try_get("removed").map_err(Error::internal)?,
    })
}

/// Writes one file into a set of changes.
///
/// Never into what is published: this takes the set of changes as an argument
/// and refuses the published one, so "nothing written here reaches the live
/// site" is a rule with nowhere to be forgotten.
pub async fn write_file(tx: &mut Tx, change: Uuid, path: &str, contents: &str) -> Result<File> {
    let path = to_write(path)?;
    let at = read(tx, change).await?;

    if at.at == Where::Published {
        return Err(Error::conflict(Say::of(
            crate::where_it_goes::THAT_FILE_IS_NOT_PART_OF_HOW_A_SITE_LOOKS,
        )));
    }

    sqlx::query(
        "insert into design_files (change_id, path, contents) values ($1, $2, $3)
         on conflict (change_id, path)
         do update set contents = excluded.contents, removed = false, updated_at = now()",
    )
    .bind(change)
    .bind(&path)
    .bind(contents.as_bytes())
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    // Written into a set of changes puts it back where it is being written,
    // which is what an editor expects after a build failed.
    sqlx::query(
        "update changes set at = 'writing', went_wrong = null, updated_at = now() where id = $1",
    )
    .bind(change)
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    Ok(File {
        path,
        contents: contents.to_owned(),
        removed: false,
    })
}

/// Everything in a set of changes, as bytes.
///
/// What a build is given. Read whole rather than one file at a time because a
/// build reads all of them and a file read after another has been written is a
/// build of two different things.
///
/// A design's files are text — that is what the column is, and what the panel
/// writes. A picture belongs in what a site uploaded rather than in how it
/// looks, so the bytes here are always somebody's typing encoded.
pub async fn everything_in(tx: &mut Tx, change: Uuid) -> Result<Vec<(String, Vec<u8>)>> {
    let rows = sqlx::query(
        "select path, contents from design_files
          where change_id = $1 and not removed order by path",
    )
    .bind(change)
    .fetch_all(tx.conn())
    .await
    .map_err(Error::internal)?;

    rows.iter()
        .map(|row| {
            let path: String = row.try_get("path").map_err(Error::internal)?;
            let contents: String = row.try_get("contents").map_err(Error::internal)?;

            Ok((path, contents.into_bytes()))
        })
        .collect()
}

/// What building one is told about afterwards.
#[derive(Clone, Debug, Deserialize)]
pub struct Built {
    pub look_at: Option<String>,
    pub went_wrong: Option<String>,
}

/// Says a set of changes has been built, or has failed to build.
pub async fn was_built(tx: &mut Tx, change: Uuid, built: &Built) -> Result<Change> {
    let at = if built.went_wrong.is_some() {
        Where::Broken
    } else {
        Where::ToLookAt
    };

    sqlx::query(
        "update changes
            set at = $2, look_at = $3, went_wrong = $4, built_at = now(), updated_at = now()
          where id = $1",
    )
    .bind(change)
    .bind(at.as_str())
    .bind(built.look_at.as_deref())
    .bind(built.went_wrong.as_deref())
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    read(tx, change).await
}

/// Puts a built set of changes in front of everybody.
///
/// Only something that has been built and looked at, and the one that was
/// published goes back to being a set of changes — so what was live is still
/// there to go back to rather than gone.
pub async fn publish(tx: &mut Tx, change: Uuid) -> Result<Change> {
    let at = read(tx, change).await?;

    if !at.at.may_be_published() {
        return Err(Error::conflict(Say::of(
            THAT_HAS_NOT_BEEN_BUILT_AND_LOOKED_AT,
        )));
    }

    // One statement, so there is never a moment with two published sets or
    // none — and the site has one look, which the schema says as well.
    sqlx::query(
        "update changes
            set at = case when id = $1 then 'published' else 'to_look_at' end,
                published_at = case when id = $1 then now() else published_at end,
                updated_at = now()
          where id = $1 or at = 'published'",
    )
    .bind(change)
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    read(tx, change).await
}
