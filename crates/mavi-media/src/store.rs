//! Reading and writing what somebody uploaded.
//!
//! The row and the bytes are two things in two places, and the order they are
//! written in is the decision: the bytes first, then the row. A row pointing
//! at bytes that are not there is a broken picture on somebody's page; bytes
//! with no row are a few kilobytes nobody will ever look at, and a sweeper
//! can find them.

use mavi_core::error::{Error, Result};
use mavi_core::page::{Page, Query};
use mavi_core::ports::Files;
use mavi_core::say::Say;
use mavi_db::{Tx, Walk};
use sqlx::Row;
use sqlx::postgres::PgRow;
use uuid::Uuid;

use crate::BY_RECENT;
use crate::kept::{File, FileId, Kind, kept_at, look};

pub const THERE_IS_NO_FILE_LIKE_THAT: &str = "there_is_no_file_like_that";

const COLUMNS: &str = "id, kind, mime, name, kept_at, bytes, created_at";

fn a_file(row: &PgRow) -> Result<File> {
    let kind: String = row.try_get("kind").map_err(Error::internal)?;

    Ok(File {
        id: FileId(row.try_get("id").map_err(Error::internal)?),
        kind: match kind.as_str() {
            "video" => Kind::Video,
            "audio" => Kind::Audio,
            "document" => Kind::Document,
            _ => Kind::Image,
        },
        mime: row.try_get("mime").map_err(Error::internal)?,
        name: row.try_get("name").map_err(Error::internal)?,
        kept_at: row.try_get("kept_at").map_err(Error::internal)?,
        bytes: row.try_get("bytes").map_err(Error::internal)?,
        created_at: row.try_get("created_at").map_err(Error::internal)?,
    })
}

/// What has been uploaded, newest first.
pub async fn list(tx: &mut Tx, kind: Option<&str>, query: &Query) -> Result<Page<File>> {
    let walk = Walk::new(BY_RECENT, query.after(BY_RECENT)?);
    let mut wheres = vec!["deleted_at is null".to_owned()];
    let mut binds: Vec<String> = Vec::new();

    if let Some(kind) = kind {
        binds.push(kind.to_owned());
        wheres.push(format!("kind = ${}", binds.len()));
    }

    let cursor = walk.after(binds.len() + 1);
    if let Some((sql, _)) = &cursor {
        wheres.push(sql.clone());
    }

    let sql = format!(
        "select {COLUMNS} from files where {} order by {} limit {}",
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
        .map(a_file)
        .collect::<Result<Vec<_>>>()?;

    Page::build(query, BY_RECENT, rows, |file| {
        vec![file.created_at.to_rfc3339(), file.id.to_string()]
    })
}

/// Takes a file.
///
/// What it is comes from the bytes. Where it is kept comes from its id. The
/// name somebody chose is a column, and there is no argument here that could
/// carry it anywhere else.
pub async fn take(tx: &mut Tx, files: &dyn Files, name: &str, bytes: Vec<u8>) -> Result<File> {
    let looked = look(&bytes)?;

    let id = FileId::new();
    let at = kept_at(id, looked);
    let how_big = i64::try_from(bytes.len()).unwrap_or(i64::MAX);

    // The bytes first. A row pointing at bytes that are not there is a broken
    // picture on somebody's page; bytes with no row are a few kilobytes a
    // sweeper can find.
    files.put(&at, bytes).await?;

    let row = sqlx::query(&format!(
        "insert into files (id, kind, mime, name, kept_at, bytes)
         values ($1, $2, $3, $4, $5, $6)
         returning {COLUMNS}"
    ))
    .bind(Uuid::from(id))
    .bind(looked.kind.as_str())
    .bind(looked.mime)
    .bind(a_name(name))
    .bind(&at)
    .bind(how_big)
    .fetch_one(tx.conn())
    .await
    .map_err(Error::internal)?;

    a_file(&row)
}

/// What to call it on a screen.
///
/// Trimmed, bounded, and never empty — a file called nothing is a row somebody
/// cannot pick out of a list. What it is *not* is where the file goes.
fn a_name(name: &str) -> String {
    let name = name.trim();

    if name.is_empty() {
        return "A file".to_owned();
    }

    name.chars().take(255).collect()
}

/// One file's details.
pub async fn read(tx: &mut Tx, id: FileId) -> Result<File> {
    let row = sqlx::query(&format!(
        "select {COLUMNS} from files where id = $1 and deleted_at is null"
    ))
    .bind(Uuid::from(id))
    .fetch_optional(tx.conn())
    .await
    .map_err(Error::internal)?
    .ok_or_else(|| Error::not_found(Say::of(THERE_IS_NO_FILE_LIKE_THAT)))?;

    a_file(&row)
}

/// Removes one, and the bytes with it.
///
/// The row first this time, and for the same reason as the other way round on
/// the way in: what must never happen is a row that points at nothing. Bytes
/// left behind by a failure here are bytes nothing refers to.
pub async fn remove(tx: &mut Tx, files: &dyn Files, id: FileId) -> Result<()> {
    let file = read(tx, id).await?;

    sqlx::query("update files set deleted_at = now() where id = $1")
        .bind(Uuid::from(id))
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    files.remove(&file.kept_at).await?;

    Ok(())
}
