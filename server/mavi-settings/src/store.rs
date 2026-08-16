//! Reading and writing what this site is.
//!
//! One row, and a list of languages with exactly one of them the site's own.
//! Both halves of that second rule live here, because only one of them is
//! something a constraint can hold.

use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use mavi_db::Tx;
use serde::Deserialize;
use sqlx::Row;

use crate::language::{
    A_SITE_WRITES_IN_SOMETHING, Language, THE_SITES_OWN_LANGUAGE_IS_PASSED_ON_RATHER_THAN_DROPPED,
    THIS_SITE_DOES_NOT_WRITE_IN_THAT, Tag, may_forget,
};
use crate::{PublicSite, Settings};

pub const THIS_SITE_IS_NOT_SET_UP_YET: &str = "this_site_is_not_set_up_yet";

/// What this site is.
pub async fn read(tx: &mut Tx) -> Result<Settings> {
    let row = sqlx::query("select name, about, time_zone from settings")
        .fetch_optional(tx.conn())
        .await
        .map_err(Error::internal)?
        .ok_or_else(|| Error::not_found(Say::of(THIS_SITE_IS_NOT_SET_UP_YET)))?;

    Ok(Settings {
        name: row.try_get("name").map_err(Error::internal)?,
        about: row.try_get("about").map_err(Error::internal)?,
        time_zone: row.try_get("time_zone").map_err(Error::internal)?,
    })
}

/// What may be changed.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct SettingsChanges {
    pub name: Option<String>,
    pub about: Option<String>,
    pub time_zone: Option<String>,
}

/// Changes it, having checked the whole of what it would become.
///
/// Checked as a whole rather than field by field: a name and a time zone that
/// are each fine and a site that has neither is not a thing this can produce,
/// because what is written is the value that came out of [`Settings::checked`].
pub async fn change(tx: &mut Tx, changes: &SettingsChanges) -> Result<Settings> {
    let now = read(tx).await?;

    let would_be = Settings::checked(
        changes.name.as_deref().unwrap_or(&now.name),
        changes.about.as_deref().or(now.about.as_deref()),
        changes.time_zone.as_deref().unwrap_or(&now.time_zone),
    )?;

    sqlx::query("update settings set name = $1, about = $2, time_zone = $3, updated_at = now()")
        .bind(&would_be.name)
        .bind(would_be.about.as_deref())
        .bind(&would_be.time_zone)
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    Ok(would_be)
}

/// What this site writes in, the site's own first.
pub async fn languages(tx: &mut Tx) -> Result<Vec<Language>> {
    let rows = sqlx::query(
        "select tag, name, is_the_sites_own from languages
          order by is_the_sites_own desc, name",
    )
    .fetch_all(tx.conn())
    .await
    .map_err(Error::internal)?;

    rows.iter()
        .map(|row| {
            let tag: String = row.try_get("tag").map_err(Error::internal)?;
            let name: String = row.try_get("name").map_err(Error::internal)?;
            let own: bool = row.try_get("is_the_sites_own").map_err(Error::internal)?;

            Language::checked(&tag, &name, own)
        })
        .collect()
}

/// Adds one.
pub async fn add(tx: &mut Tx, tag: &str, name: &str) -> Result<Language> {
    let language = Language::checked(tag, name, false)?;

    sqlx::query("insert into languages (tag, name) values ($1, $2)")
        .bind(language.tag.as_str())
        .bind(&language.name)
        .execute(tx.conn())
        .await
        .map_err(|_| Error::conflict(Say::of("this_site_already_writes_in_that")))?;

    Ok(language)
}

/// Makes one the site's own, and every other one not.
///
/// One statement. Two — take the crown off, put it on — is a moment with none,
/// and that moment is exactly when something else reads the list.
pub async fn make_it_ours(tx: &mut Tx, tag: &str) -> Result<Vec<Language>> {
    let tag = Tag::parse(tag)?;

    let changed = sqlx::query(
        "update languages set is_the_sites_own = (tag = $1), updated_at = now()
          where is_the_sites_own <> (tag = $1)",
    )
    .bind(tag.as_str())
    .execute(tx.conn())
    .await
    .map_err(Error::internal)?;

    // Nothing changed and nothing is wrong only if it was already ours.
    if changed.rows_affected() == 0 {
        let there: bool =
            sqlx::query_scalar("select exists (select 1 from languages where tag = $1)")
                .bind(tag.as_str())
                .fetch_one(tx.conn())
                .await
                .map_err(Error::internal)?;

        if !there {
            return Err(Error::not_found(
                Say::of(THIS_SITE_DOES_NOT_WRITE_IN_THAT).with("language", &tag.as_str()),
            ));
        }
    }

    languages(tx).await
}

/// Stops writing in one — never the last, and never the site's own.
pub async fn forget(tx: &mut Tx, tag: &str) -> Result<()> {
    let tag = Tag::parse(tag)?;
    let writing_in = languages(tx).await?;

    // The rule is asked of the whole list, because it is a rule about the rest
    // of the rows and no constraint on one of them can see it.
    may_forget(&writing_in, &tag)?;

    sqlx::query("delete from languages where tag = $1")
        .bind(tag.as_str())
        .execute(tx.conn())
        .await
        .map_err(Error::internal)?;

    Ok(())
}

/// What anybody at all is told about this site.
pub async fn public(tx: &mut Tx) -> Result<PublicSite> {
    let settings = read(tx).await?;

    Ok(PublicSite {
        name: settings.name,
        about: settings.about,
        languages: languages(tx).await?,
    })
}

/// Named here so the two refusals a caller can get from forgetting a language
/// are findable from one place.
pub const REFUSALS: &[&str] = &[
    A_SITE_WRITES_IN_SOMETHING,
    THE_SITES_OWN_LANGUAGE_IS_PASSED_ON_RATHER_THAN_DROPPED,
];
