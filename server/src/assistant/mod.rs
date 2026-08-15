//! A key an assistant can actually work with, good for a day.
//!
//! What a site could hand an assistant before this was a document: an accurate
//! description of an API it had no way of reaching. So the document is handed
//! over with a key in it. It writes, it lasts a day, it is revocable, and what
//! it may not do is a shorter list of grants rather than a promise anybody has
//! to keep.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::kernel::audit::{self, Actor, Audited};
use crate::kernel::authz::{Access, Capability, Needs};
use crate::kernel::db::Tx;
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::page::{Page, Query};
use crate::kernel::secret::Shown;
use crate::kernel::token;

/// How long a key lasts.
///
/// A day, because it is pasted into somebody else's program and read by a
/// model: it will end up in a transcript, in a log, and possibly in a
/// screenshot, and none of those should still be a way in next week. Long
/// enough that an afternoon's work does not stop halfway.
pub const KEY_HOURS: i64 = 24;

/// The role a key is held by, and the name of the account that holds it.
///
/// Its own, not the person's: what an assistant did should read as an assistant
/// in the bin and in a post's author, and a key should be revocable without
/// ending somebody's own session.
pub const ACCOUNT: &str = "assistant";

/// What an assistant may do.
///
/// Writing, and the reading writing needs. Not people, not settings — the
/// settings hold other services' credentials — and nothing may be deleted for
/// good, because those are the acts there is no way back from.
#[must_use]
pub fn grants() -> Vec<String> {
    let mut grants = Vec::new();

    for capability in [
        Capability::Content,
        Capability::Media,
        Capability::Taxonomy,
        Capability::Design,
    ] {
        for access in [Access::View, Access::Write] {
            grants.push(Needs::new(capability, access).grant());
        }
    }

    for capability in [Capability::Forms, Capability::Boards, Capability::Shop] {
        grants.push(Needs::new(capability, Access::View).grant());
    }

    grants
}

fn people(access: Access) -> Needs {
    Needs::new(Capability::People, access)
}

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/assistant/keys",
            Guard {
                audience: Audience::User,
                needs: Some(people(Access::View)),
                rate: RatePolicy::None,
            },
            list,
        )
        .gives::<Page<Key>>(),
        Endpoint::post(
            "/api/assistant/handover",
            Guard {
                audience: Audience::User,
                needs: Some(people(Access::Write)),
                rate: RatePolicy::None,
            },
            handover,
        )
        .gives::<Handover>(),
        Endpoint::delete(
            "/api/assistant/keys/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(people(Access::Write)),
                rate: RatePolicy::None,
            },
            take_back,
        ),
    ]
}

/// The hash a key is named by, when it was made, when it runs out, and whether
/// somebody took it back.
type KeyRow = (Vec<u8>, DateTime<Utc>, DateTime<Utc>, Option<DateTime<Utc>>);

/// One key this site has out. Not the key itself: that was shown once.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Key {
    /// Enough of the hash to tell two keys apart, and not enough to be one.
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked: bool,
}

/// A key and the document that says what to do with it.
///
/// One call rather than two, because the two are one act: a key with no
/// document is a string somebody has to explain, and a document with no key is
/// what a site used to hand out.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Handover {
    pub id: String,
    /// Shown once, here.
    pub token: Shown,
    pub expires_at: DateTime<Utc>,
    pub grants: Vec<String>,
}

async fn list(
    State(state): State<AppState>,
    _caller: Caller,
    axum::extract::Query(page): axum::extract::Query<Query>,
) -> Result<Json<Page<Key>>> {
    let mut conn = state.db.begin().await?;
    let holder = account(&mut conn).await?;

    let rows: Vec<KeyRow> = sqlx::query_as(
        "select token_hash, created_at, expires_at, revoked_at
           from sessions
          where user_id = $1
            and ($2::timestamptz is null or created_at < $2)
          order by created_at desc
          limit $3",
    )
    .bind(holder)
    .bind(
        page.after
            .as_deref()
            .and_then(|after| DateTime::parse_from_rfc3339(after).ok())
            .map(|at| at.with_timezone(&Utc)),
    )
    .bind(page.fetch())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Page::build(
        &page,
        rows.into_iter()
            .map(|(hash, created_at, expires_at, revoked_at)| Key {
                id: fingerprint(&hash),
                created_at,
                expires_at,
                revoked: revoked_at.is_some(),
            })
            .collect(),
        |key| key.created_at.to_rfc3339(),
    )))
}

async fn handover(
    State(state): State<AppState>,
    caller: Caller,
) -> Result<Audited<(StatusCode, Json<Handover>)>> {
    let mut conn = state.db.begin().await?;
    let holder = account(&mut conn).await?;

    let secret = token::generate();
    let hash = token::hash(&secret);
    let expires_at = state.clock.now() + Duration::hours(KEY_HOURS);

    sqlx::query(
        "insert into sessions (user_id, token_hash, expires_at, user_agent)
         values ($1, $2, $3, 'assistant')",
    )
    .bind(holder)
    .bind(&hash[..])
    .bind(expires_at)
    .execute(conn.conn())
    .await?;

    let id = fingerprint(&hash);

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "handed an assistant a key",
        "assistant_key",
        Some(&id),
        &serde_json::json!({ "expires_at": expires_at }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(
        receipt,
        (
            StatusCode::CREATED,
            Json(Handover {
                id,
                token: Shown::new(secret),
                expires_at,
                grants: grants(),
            }),
        ),
    ))
}

/// Takes one back before it runs out.
async fn take_back(
    State(state): State<AppState>,
    caller: Caller,
    Path(id): Path<String>,
) -> Result<Audited<StatusCode>> {
    let mut conn = state.db.begin().await?;
    let holder = account(&mut conn).await?;

    // Named by the fingerprint the listing gave, which is not the key. Only
    // this account's are looked through, so this cannot end a colleague's
    // login: every session on the site lives in the one table.
    let taken = sqlx::query(
        "update sessions set revoked_at = now()
          where user_id = $1
            and revoked_at is null
            and left(encode(sha256(token_hash), 'hex'), 12) = $2",
    )
    .bind(holder)
    .bind(&id)
    .execute(conn.conn())
    .await?
    .rows_affected();

    if taken == 0 {
        return Err(AppError::NotFound("key"));
    }

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "took an assistant's key back",
        "assistant_key",
        Some(&id),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::NO_CONTENT))
}

/// Enough of the hash of the hash to name a key, and not enough to be one.
fn fingerprint(hash: &[u8]) -> String {
    let digest = Sha256::digest(hash);

    digest
        .iter()
        .take(6)
        .fold(String::with_capacity(12), |mut out, byte| {
            use std::fmt::Write as _;
            let _ = write!(out, "{byte:02x}");
            out
        })
}

/// The account keys are held by, made the first time one is asked for.
///
/// There is no password on it, and none is ever set: the only way to be this
/// account is to hold a key somebody made.
async fn account(conn: &mut Tx) -> Result<Uuid> {
    let found: Option<(Uuid,)> = sqlx::query_as(
        "select u.id from users u join roles r on r.id = u.role_id
                         where r.key = $1 and u.deleted_at is null",
    )
    .bind(ACCOUNT)
    .fetch_optional(conn.conn())
    .await?;

    if let Some((id,)) = found {
        return Ok(id);
    }

    let (role,): (Uuid,) = sqlx::query_as(
        "insert into roles (key, name, grants)
         values ($1, 'Assistant', $2)
         on conflict (key) do update set grants = excluded.grants
         returning id",
    )
    .bind(ACCOUNT)
    .bind(grants())
    .fetch_one(conn.conn())
    .await?;

    let (id,): (Uuid,) = sqlx::query_as(
        "insert into users (role_id, email, name, state)
         values ($1, $2, 'Assistant', 'active')
         returning id",
    )
    .bind(role)
    .bind(format!("{ACCOUNT}@assistant.invalid"))
    .fetch_one(conn.conn())
    .await?;

    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_assistant_cannot_take_anything_away_for_good() {
        let grants = grants();

        assert!(grants.contains(&"content:write".to_owned()));
        assert!(
            !grants.iter().any(|grant| grant.ends_with(":delete")),
            "an assistant was given something there is no way back from: {grants:?}"
        );
        assert!(
            !grants.iter().any(|grant| grant.starts_with("settings:")),
            "an assistant was given the settings that hold other services' credentials"
        );
        assert!(!grants.iter().any(|grant| grant.starts_with("people:")));
    }

    #[test]
    fn a_key_is_named_by_something_that_is_not_the_key() {
        let one = fingerprint(&[1_u8; 32]);
        let two = fingerprint(&[2_u8; 32]);

        assert_eq!(one.len(), 12);
        assert_ne!(one, two);
    }
}
