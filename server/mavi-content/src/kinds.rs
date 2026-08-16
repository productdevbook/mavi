//! What a site decided one of its kinds of writing is.
//!
//! A writing's `kind` is free text, because a CMS whose kinds are fixed at
//! compile time is a CMS for one site. But free text alone means a site can
//! have a `recipe` and **nothing anywhere knows a recipe has a cooking time**
//! — so `fields` was a column somebody typed JSON into, checked by nothing and
//! drawable by nothing.
//!
//! This is where a site says what one of its kinds asks for, in the same
//! vocabulary a form uses. A kind with nothing declared keeps working exactly
//! as it did: whatever is in `fields` is kept, unread.

use chrono::{DateTime, Utc};
use mavi_core::asked::Declared;
use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use mavi_db::Tx;
use serde::{Deserialize, Serialize};
use sqlx::Row;

pub const THAT_IS_NOT_A_KIND: &str = "that_is_not_a_kind";
pub const THERE_IS_NO_KIND_LIKE_THAT: &str = "there_is_no_kind_like_that";

/// One kind, as a site declared it.
#[derive(Clone, Debug, Serialize)]
pub struct AKind {
    pub kind: String,
    pub name: String,
    pub fields: Declared,
    pub created_at: DateTime<Utc>,
}

/// What declaring one asks for.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Declaring {
    pub name: String,
    #[serde(default)]
    pub fields: Vec<mavi_core::asked::Field>,
}

fn one(row: &sqlx::postgres::PgRow) -> Result<AKind> {
    let fields: serde_json::Value = row.try_get("fields").map_err(Error::internal)?;

    Ok(AKind {
        kind: row.try_get("kind").map_err(Error::internal)?,
        name: row.try_get("name").map_err(Error::internal)?,
        // Checked on the way out as well as on the way in. A row written by an
        // older build, or by hand, is still held to what a declaration is —
        // and the alternative is a panel drawing a box out of something this
        // build would have refused.
        fields: Declared::checked(serde_json::from_value(fields).map_err(Error::internal)?)?,
        created_at: row.try_get("created_at").map_err(Error::internal)?,
    })
}

/// Every kind a site has declared.
pub async fn every(tx: &mut Tx) -> Result<Vec<AKind>> {
    let rows = sqlx::query("select kind, name, fields, created_at from kinds order by kind")
        .fetch_all(tx.conn())
        .await
        .map_err(Error::internal)?;

    rows.iter().map(one).collect()
}

/// What one kind asks for, where the site said.
///
/// `None` for a kind nothing was declared about, which is not an error: it is
/// what every kind is until somebody says otherwise.
pub async fn asked_for(tx: &mut Tx, kind: &str) -> Result<Option<Declared>> {
    let row = sqlx::query("select kind, name, fields, created_at from kinds where kind = $1")
        .bind(kind)
        .fetch_optional(tx.conn())
        .await
        .map_err(Error::internal)?;

    row.as_ref()
        .map(one)
        .transpose()
        .map(|it| it.map(|it| it.fields))
}

/// Says what a kind is, or says it differently.
///
/// One endpoint for both, because a site editing what a `recipe` asks for is
/// doing the same thing as first saying what one is — and two endpoints would
/// be two places the checking has to be the same.
pub async fn declare(tx: &mut Tx, kind: &str, said: &Declaring) -> Result<AKind> {
    let kind = a_kind(kind)?;
    let fields = Declared::checked(said.fields.clone())?;

    let name = said.name.trim();
    if !(1..=100).contains(&name.chars().count()) {
        return Err(Error::invalid(Say::of(
            crate::writing::A_TITLE_IS_BETWEEN_ONE_AND_TWO_HUNDRED,
        )));
    }

    let row = sqlx::query(
        "insert into kinds (kind, name, fields) values ($1, $2, $3)
         on conflict (kind) do update
            set name = excluded.name, fields = excluded.fields, updated_at = now()
         returning kind, name, fields, created_at",
    )
    .bind(&kind)
    .bind(name)
    .bind(serde_json::to_value(fields.fields()).map_err(Error::internal)?)
    .fetch_one(tx.conn())
    .await
    .map_err(Error::internal)?;

    one(&row)
}

/// Stops saying what a kind is.
///
/// The writings stay, and so does whatever is in their `fields`. What a site
/// declared is a thing about the kind, not a thing about what was written —
/// and deleting somebody's content because they stopped describing its shape
/// would be the worst possible reading of this.
pub async fn stop_saying(tx: &mut Tx, kind: &str) -> Result<()> {
    let gone = sqlx::query("delete from kinds where kind = $1")
        .bind(kind)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    if gone.rows_affected() == 0 {
        return Err(Error::not_found(Say::of(THERE_IS_NO_KIND_LIKE_THAT)));
    }

    Ok(())
}

/// A kind, checked the same way a writing's own is.
fn a_kind(said: &str) -> Result<String> {
    crate::writing::Kind::parse(said).map(|kind| kind.as_str().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kind_here_is_a_kind_there() {
        // The same rule as a writing's own kind, from the same function. Two
        // ideas of what a kind may be called is a site that can declare one it
        // could never write.
        assert!(a_kind("recipe").is_ok());
        assert!(a_kind("Recipe").is_err());
        assert!(a_kind("").is_err());
        assert!(a_kind("a-recipe").is_err());
    }
}
