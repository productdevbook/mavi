//! Which languages a site writes in.
//!
//! Every post is written in one of these and the database refuses one that is
//! not here, so until now a site could only be given a language by whoever
//! migrated its database — which is to say, not at all.

use axum::Json;
use axum::extract::{Path, State as Injected};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::kernel::audit::{self, Actor, Audited};
use crate::kernel::authz::{Access, Capability, Needs, Permit};
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::say::{self, Say};
use crate::kernel::types::Title;

fn needs(access: Access) -> Needs {
    Needs::new(Capability::Content, access)
}

pub(super) fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/languages",
            Guard {
                audience: Audience::User,
                needs: Some(needs(Access::View)),
                rate: RatePolicy::None,
            },
            list,
        )
        .gives::<Vec<Language>>(),
        Endpoint::post(
            "/api/languages",
            Guard {
                audience: Audience::User,
                needs: Some(needs(Access::Write)),
                rate: RatePolicy::None,
            },
            add,
        )
        .takes::<NewLanguage>()
        .gives::<Language>(),
        Endpoint::patch(
            "/api/languages/{code}",
            Guard {
                audience: Audience::User,
                needs: Some(needs(Access::Write)),
                rate: RatePolicy::None,
            },
            change,
        )
        .takes::<LanguageChanges>()
        .gives::<Language>(),
        Endpoint::delete(
            "/api/languages/{code}",
            Guard {
                audience: Audience::User,
                needs: Some(needs(Access::Delete)),
                rate: RatePolicy::None,
            },
            remove,
        ),
    ]
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Language {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub is_default: bool,
    /// How much is written in it. What somebody wants to know before taking one
    /// away, and the reason taking one away is refused.
    pub posts: i64,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewLanguage {
    /// `en`, or `en-GB`. What the database already insists on, said here as
    /// well so that the refusal is a sentence rather than a constraint name.
    pub code: String,
    pub name: Title,
    /// Whether this becomes the one a post is written in when nobody said. The
    /// first language a site adds is, whatever it says.
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LanguageChanges {
    pub name: Option<Title>,
    pub is_default: Option<bool>,
}

const COLUMNS: &str = "l.id, l.code, l.name, l.is_default,
     (select count(*) from posts p where p.language = l.code and p.deleted_at is null) as posts";

async fn list(
    Injected(state): Injected<AppState>,
    _caller: Caller,
    _permit: Permit,
) -> Result<Json<Vec<Language>>> {
    let mut conn = state.db.begin().await?;

    let rows: Vec<Language> = sqlx::query_as(&format!(
        "select {COLUMNS} from languages l order by l.is_default desc, l.code"
    ))
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(rows))
}

async fn add(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Json(body): Json<NewLanguage>,
) -> Result<Audited<(StatusCode, Json<Language>)>> {
    let mut conn = state.db.begin().await?;

    let (already,): (i64,) = sqlx::query_as("select count(*) from languages")
        .fetch_one(conn.conn())
        .await?;

    // The first one is the default whatever was asked for: a site with a
    // language and no default is a site where nothing can be written.
    let default = body.is_default || already == 0;

    if default {
        sqlx::query("update languages set is_default = false where is_default")
            .execute(conn.conn())
            .await?;
    }

    let added: Language = sqlx::query_as(&format!(
        "with added as (
             insert into languages (code, name, is_default)
             values ($1, $2, $3)
             returning id, code, name, is_default
         )
         select {COLUMNS} from added l"
    ))
    .bind(&body.code)
    .bind(body.name.as_str())
    .bind(default)
    .fetch_one(conn.conn())
    .await
    .map_err(named_wrongly)?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "added a language",
        "language",
        Some(&added.code),
        &serde_json::json!({ "name": added.name, "default": added.is_default }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, (StatusCode::CREATED, Json(added))))
}

async fn change(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(code): Path<String>,
    Json(changes): Json<LanguageChanges>,
) -> Result<Audited<Json<Language>>> {
    let mut conn = state.db.begin().await?;

    if changes.is_default == Some(true) {
        sqlx::query("update languages set is_default = false where is_default")
            .execute(conn.conn())
            .await?;
    }

    let after: Option<Language> = sqlx::query_as(&format!(
        "with changed as (
             update languages
                set name = coalesce($2, name),
                    is_default = coalesce($3, is_default)
              where code = $1
             returning id, code, name, is_default
         )
         select {COLUMNS} from changed l"
    ))
    .bind(&code)
    .bind(changes.name.as_ref().map(Title::as_str))
    .bind(changes.is_default)
    .fetch_optional(conn.conn())
    .await?;

    let after = after.ok_or(AppError::NotFound("language"))?;

    // Turning off the last default leaves a site that cannot write anything,
    // so it is refused rather than left to be discovered.
    let (defaults,): (i64,) = sqlx::query_as("select count(*) from languages where is_default")
        .fetch_one(conn.conn())
        .await?;

    if defaults == 0 {
        return Err(AppError::Refused(
            say::A_SITE_WRITES_IN_ONE_LANGUAGE_BY_DEFAULT.into(),
        ));
    }

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "changed a language",
        "language",
        Some(&after.code),
        &serde_json::json!({ "default": after.is_default }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, Json(after)))
}

/// Taking one away, where nothing is written in it.
///
/// A language with posts in it is not taken away: what would happen to them is
/// a decision nobody has made, and the database would refuse it anyway.
async fn remove(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(code): Path<String>,
) -> Result<Audited<StatusCode>> {
    let mut conn = state.db.begin().await?;

    let found: Option<(bool, i64)> = sqlx::query_as(
        "select l.is_default,
                (select count(*) from posts p
                  where p.language = l.code and p.deleted_at is null)
           from languages l where l.code = $1",
    )
    .bind(&code)
    .fetch_optional(conn.conn())
    .await?;

    let (is_default, posts) = found.ok_or(AppError::NotFound("language"))?;

    if posts > 0 {
        return Err(AppError::Refused(
            Say::of(say::SOMETHING_IS_WRITTEN_IN_THAT_LANGUAGE).naming("posts", posts),
        ));
    }

    if is_default {
        return Err(AppError::Refused(
            say::A_SITE_WRITES_IN_ONE_LANGUAGE_BY_DEFAULT.into(),
        ));
    }

    sqlx::query("delete from languages where code = $1")
        .bind(&code)
        .execute(conn.conn())
        .await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "took a language away",
        "language",
        Some(&code),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::NO_CONTENT))
}

fn named_wrongly(error: sqlx::Error) -> AppError {
    match error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
    {
        Some(code) if code == "23505" => {
            AppError::Conflict(say::THIS_SITE_ALREADY_WRITES_IN_THAT.into())
        }
        Some(code) if code == "23514" => {
            AppError::Invalid(say::A_LANGUAGE_IS_TWO_LETTERS_AND_A_PLACE.into())
        }
        other => {
            let _ = other;
            AppError::Database(error)
        }
    }
}
