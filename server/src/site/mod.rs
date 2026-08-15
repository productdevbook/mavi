//! What a site is, and what it holds about a person.
//!
//! Its own name and how much room it has left, what it tells something reading
//! rather than browsing, and the two things somebody may ask of a site about
//! themselves: everything it holds, and nothing.
use axum::Json;
use axum::extract::State as Injected;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use std::fmt::Write as _;
use uuid::Uuid;

use crate::kernel::audit::{self, Actor, Audited};
use crate::kernel::authz::{Access, Capability, Needs, Permit};
use crate::kernel::db::Tx;
use crate::kernel::error::Result;
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::ratelimit::Limit;
use crate::kernel::types::{Email, Title};

const LLMS_LIMIT: Limit = Limit::new(30, 60);

/// Every table holding something a person could ask for a copy of, and the
/// column their address is in. A domain that adds one adds a line here, and a
/// test compares this list against the retention policies.
const ABOUT_A_PERSON: &[(&str, &str)] = &[
    ("users", "email"),
    ("students", "email"),
    ("subscribers", "email"),
    ("orders", "email"),
    ("email_log", "to_email"),
];

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/llms.txt",
            Guard {
                audience: Audience::Public,
                needs: None,
                rate: RatePolicy::Per(LLMS_LIMIT),
            },
            llms,
        ),
        Endpoint::get(
            "/api/site",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::Settings, Access::View)),
                rate: RatePolicy::None,
            },
            settings,
        )
        .gives::<Settings>(),
        Endpoint::patch(
            "/api/site",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::Settings, Access::Write)),
                rate: RatePolicy::None,
            },
            rename,
        )
        .takes::<SettingsChanges>()
        .gives::<Settings>(),
        Endpoint::post(
            "/api/people/export",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::People, Access::View)),
                rate: RatePolicy::None,
            },
            export,
        )
        .takes::<About>()
        .gives::<Copied>(),
        Endpoint::post(
            "/api/people/erase",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::People, Access::Delete)),
                rate: RatePolicy::None,
            },
            erase,
        )
        .takes::<About>(),
    ]
}

/// What a site's own people may see of its settings.
#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Settings {
    pub name: String,
    /// How much has been uploaded, and how much may be. Read rather than set:
    /// the ceiling is the machine's, and finding out at the point of an upload
    /// being refused is finding out too late.
    pub storage_used_bytes: i64,
    pub storage_limit_bytes: i64,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SettingsChanges {
    pub name: Title,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct About {
    pub email: Email,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Copied {
    pub email: String,
    /// One entry per table that had something, with the rows in it.
    pub found: serde_json::Value,
}

async fn settings(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
) -> Result<Json<Settings>> {
    let mut conn = state.db.begin().await?;
    let found = read_settings(&mut conn, &caller).await?;
    conn.commit().await?;

    Ok(Json(found))
}

async fn rename(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Json(wanted): Json<SettingsChanges>,
) -> Result<Audited<Json<Settings>>> {
    let mut conn = state.db.begin().await?;

    // Written rather than updated: a site carried in from elsewhere may have no
    // row here, and a name that cannot be set until somebody runs an insert by
    // hand is a name that cannot be set.
    sqlx::query(
        "insert into site_settings (name) values ($1)
         on conflict ((true)) do update set name = excluded.name",
    )
    .bind(wanted.name.as_str())
    .execute(conn.conn())
    .await?;

    let after = read_settings(&mut conn, &caller).await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "renamed the site",
        "site",
        None,
        &json!({ "name": after.name }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, Json(after)))
}

async fn read_settings(conn: &mut Tx, _caller: &Caller) -> Result<Settings> {
    let found: Option<Settings> = sqlx::query_as(
        "select s.name,
                coalesce((select sum(m.bytes) from media m
                           where m.deleted_at is null), 0)::bigint as storage_used_bytes,
                $1::bigint as storage_limit_bytes
           from site_settings s",
    )
    .bind(crate::media::MOST_BYTES_A_SITE)
    .fetch_optional(conn.conn())
    .await?;

    Ok(found.unwrap_or_else(|| Settings {
        name: String::new(),
        storage_used_bytes: 0,
        storage_limit_bytes: crate::media::MOST_BYTES_A_SITE,
    }))
}

/// What a site is, for something reading rather than browsing. Written from
/// what is published rather than from a file somebody has to remember.
async fn llms(Injected(state): Injected<AppState>, _caller: Caller) -> Result<Response> {
    let mut conn = state.db.begin().await?;

    let name: Option<(String,)> = sqlx::query_as("select name from site_settings ")
        .fetch_optional(conn.conn())
        .await?;

    let pages = sqlx::query(
        "select title, slug, excerpt from posts
          where state = 'published' and deleted_at is null
          order by published_at desc limit 200",
    )
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    let mut out = format!(
        "# {}\n\n",
        name.map_or_else(|| "A site".to_owned(), |(name,)| name)
    );

    out.push_str("What is published here, most recent first.\n\n");

    for page in &pages {
        let title: String = page.get("title");
        let slug: String = page.get("slug");
        let excerpt: Option<String> = page.get("excerpt");

        let _ = write!(out, "- [{title}](/{slug})");

        if let Some(excerpt) = excerpt.filter(|text| !text.trim().is_empty()) {
            let _ = write!(out, ": {}", excerpt.chars().take(200).collect::<String>());
        }

        out.push('\n');
    }

    Ok(([(CONTENT_TYPE, "text/plain; charset=utf-8")], out).into_response())
}

/// Everything this site holds about one address, in one answer.
async fn export(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Json(body): Json<About>,
) -> Result<Audited<Json<Copied>>> {
    let mut conn = state.db.begin().await?;
    let found = gather(&mut conn, body.email.as_str()).await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "gave somebody their copy",
        "person",
        Some(body.email.as_str()),
        &json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(
        receipt,
        Json(Copied {
            email: body.email.into_string(),
            found,
        }),
    ))
}

async fn gather(conn: &mut Tx, email: &str) -> Result<serde_json::Value> {
    let mut found = serde_json::Map::new();

    for (table, column) in ABOUT_A_PERSON {
        // Both come from the constant above and never from a request: there is
        // nothing here a caller chooses.
        let rows = sqlx::query(&format!(
            "select to_jsonb(t) - 'password_hash' - 'token_hash' as row
               from {table} t where t.{column} = $1"
        ))
        .bind(email)
        .fetch_all(conn.conn())
        .await?;

        if !rows.is_empty() {
            found.insert(
                (*table).to_owned(),
                serde_json::Value::Array(
                    rows.iter()
                        .map(|row| row.get::<serde_json::Value, _>("row"))
                        .collect(),
                ),
            );
        }
    }

    Ok(serde_json::Value::Object(found))
}

/// Takes away what a site holds about one address.
///
/// What is a financial record is emptied of the person rather than deleted: an
/// order that vanishes is a bill nobody can explain, and the rule that says
/// keep it and the rule that says remove them are both true.
///
/// An address that is the site's only owner is not touched at all, here or
/// anywhere else this reaches: blanking that account the way an order is
/// blanked would leave a row that satisfies "an owner exists" in name only —
/// nobody able to sign into it, so nobody able to grant the role onward
/// either, which is worse than refusing outright. The request answers with
/// why, and a retry after another owner exists starts from everything still
/// in place.
async fn erase(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Json(body): Json<About>,
) -> Result<Audited<Json<serde_json::Value>>> {
    let email = body.email.as_str();
    let mut conn = state.db.begin().await?;

    let holder: Option<(Uuid,)> =
        sqlx::query_as("select id from users where email = $1 and deleted_at is null")
            .bind(email)
            .fetch_optional(conn.conn())
            .await?;

    if let Some((id,)) = holder {
        crate::people::refuse_if_last_owner(&mut conn, id).await?;
    }

    let mut taken = serde_json::Map::new();

    for table in ["users", "students", "subscribers", "email_log"] {
        let column = if *table == *"email_log" {
            "to_email"
        } else {
            "email"
        };

        let gone = sqlx::query(&format!("delete from {table} where {column} = $1"))
            .bind(email)
            .execute(conn.conn())
            .await?
            .rows_affected();

        taken.insert((*table).to_owned(), json!(gone));
    }

    let emptied =
        sqlx::query("update orders set email = 'erased@example.invalid' where email = $1")
            .bind(email)
            .execute(conn.conn())
            .await?
            .rows_affected();

    taken.insert("orders".to_owned(), json!({ "emptied": emptied }));

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "erased somebody",
        "person",
        Some(email),
        &serde_json::Value::Object(taken.clone()),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(
        receipt,
        Json(serde_json::Value::Object(taken)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The list a copy is gathered from and the list a retention policy is
    /// written from describe the same thing. Drifting apart is how somebody's
    /// data ends up in a table nobody thought to look in.
    #[test]
    fn everything_a_person_is_in_has_a_retention_policy() {
        use crate::kernel::retention;

        for (table, _) in ABOUT_A_PERSON {
            assert!(
                retention::policy_for(table).is_some(),
                "{table} holds somebody's own data and says nothing about keeping it"
            );
        }
    }

    #[test]
    fn nothing_gathered_is_a_secret() {
        assert!(!ABOUT_A_PERSON.iter().any(|(table, _)| *table == "sessions"));
        assert!(!ABOUT_A_PERSON.iter().any(|(table, _)| *table == "tickets"));
    }
}
