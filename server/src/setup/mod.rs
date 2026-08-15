//! The first run of a machine.
//!
//! A machine with nothing on it has to be given somebody to run it, and there
//! is exactly one moment when that can happen without anybody being signed in.
//! Everything here exists to make that moment as small as it can be: it is
//! offered while there is no site, it is taken once, and from the second
//! account onwards it is somebody signed in who invites the next.

use axum::Json;
use axum::extract::State as Injected;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::kernel::audit::{self, Actor, ActorKind, Audited};
use crate::kernel::authz::every_grant;
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::ratelimit::Limit;
use crate::kernel::secret::Secret;
use crate::kernel::types::{Email, Title};
use crate::kernel::{password, say};

/// Slowly. The window is small and this is behind no account at all, so what is
/// counted here is somebody hammering an address that answers with a machine.
const SETUP_LIMIT: Limit = Limit::new(5, 60);

pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/setup",
            Guard {
                audience: Audience::Public,
                needs: None,
                rate: RatePolicy::Per(SETUP_LIMIT),
            },
            waiting,
        )
        .gives::<Waiting>(),
        Endpoint::post(
            "/api/setup",
            Guard {
                audience: Audience::Public,
                needs: None,
                rate: RatePolicy::Per(SETUP_LIMIT),
            },
            begin,
        )
        .takes::<First>()
        .gives::<Waiting>(),
    ]
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Waiting {
    /// Whether this machine still has nobody to run it. False for ever after
    /// the first account, and nothing here says anything else about it.
    pub needed: bool,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct First {
    pub email: Email,
    pub name: Title,
    pub password: Secret<String>,
}

async fn waiting(Injected(state): Injected<AppState>, _caller: Caller) -> Result<Json<Waiting>> {
    let mut conn = state.db.begin().await?;
    let (any,): (i64,) = sqlx::query_as("select count(*) from site_settings")
        .fetch_one(conn.conn())
        .await?;
    conn.commit().await?;

    Ok(Json(Waiting { needed: any == 0 }))
}

/// The lock two requests arriving together queue behind.
///
/// `where not exists` is not enough on its own: both transactions read the
/// empty table before either wrote, so both found nothing there and both
/// inserted. A test does exactly that, and it is how this was found.
const SETTING_UP: i64 = 0x73_65_74_75;

/// Makes the site and the one account that owns it, once.
///
/// Public, and nothing is weakened by that. What kept this door shut was never
/// an audience: it is the rate policy, the advisory lock and the `where not
/// exists`, all three of which are still here. The audience it used to declare
/// was the console's, and the console's guard could not fire on it anyway
/// because it asked for no grant.
async fn begin(
    Injected(state): Injected<AppState>,
    caller: Caller,
    Json(body): Json<First>,
) -> Result<Audited<(StatusCode, Json<Waiting>)>> {
    if body.password.expose().chars().count() < 12 {
        return Err(AppError::Invalid(
            say::PASSWORD_AT_LEAST_TWELVE_CHARACTERS.into(),
        ));
    }

    let hash = password::hash(body.password.expose())?;
    let mut conn = state.db.begin().await?;

    sqlx::query("select pg_advisory_xact_lock($1)")
        .bind(SETTING_UP)
        .execute(conn.conn())
        .await?;

    // The site's own settings are what says this machine has been set up: they
    // are written once, they are the first thing written, and there is at most
    // one row of them. The guard used to be on `operators`, which no longer
    // exists; this is the same guard on the row that replaced it.
    let made: Option<(String,)> = sqlx::query_as(
        "insert into site_settings (name)
         select $1
          where not exists (select 1 from site_settings)
         returning name",
    )
    .bind(body.name.as_str())
    .fetch_optional(conn.conn())
    .await?;

    // Said the same way whether somebody was a second too late or is trying it
    // on a machine that has been running for a year: that this door is shut is
    // the whole of what anybody is told.
    if made.is_none() {
        return Err(AppError::Refused(
            say::THIS_MACHINE_IS_ALREADY_SET_UP.into(),
        ));
    }

    let (role_id,): (Uuid,) = sqlx::query_as(
        "insert into roles (key, name, grants, built_in)
         values ('owner', 'Owner', $1, true)
         returning id",
    )
    .bind(every_grant())
    .fetch_one(conn.conn())
    .await?;

    let (user_id,): (Uuid,) = sqlx::query_as(
        "insert into users (role_id, email, name, password_hash, state)
         values ($1, $2, $3, $4, 'active')
         returning id",
    )
    .bind(role_id)
    .bind(body.email.as_str())
    .bind(body.name.as_str())
    .bind(&hash)
    .fetch_one(conn.conn())
    .await?;

    // A real receipt in the site's own log, rather than a line in a console's
    // and a receipt made out of nothing. The owner is the actor: they are the
    // account this transaction just wrote, and they are who did it.
    let receipt = audit::record_raw(
        &mut conn,
        Actor {
            id: Some(user_id),
            kind: ActorKind::User,
            request_id: caller.request_id,
        },
        "set up",
        "site",
        None,
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(
        receipt,
        (StatusCode::CREATED, Json(Waiting { needed: false })),
    ))
}
