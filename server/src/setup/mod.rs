//! The first run of a machine.
//!
//! A machine with nothing on it has to be given somebody to run it, and there
//! is exactly one moment when that can happen without anybody being signed in.
//! Everything here exists to make that moment as small as it can be: it is
//! offered while there is no operator, it is taken once, and from the second
//! account onwards it is somebody signed in who invites the next.

use axum::Json;
use axum::extract::State as Injected;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::kernel::audit::{Audited, Receipt};
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{AppState, Audience, Console, Endpoint, Guard, RatePolicy};
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
                audience: Audience::Operator,
                needs: None,
                rate: RatePolicy::Per(SETUP_LIMIT),
            },
            waiting,
        )
        .gives::<Waiting>(),
        Endpoint::post(
            "/api/setup",
            Guard {
                audience: Audience::Operator,
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

async fn waiting(Injected(state): Injected<AppState>, _console: Console) -> Result<Json<Waiting>> {
    let mut conn = state.db.operator().await?;
    let (any,): (i64,) = sqlx::query_as("select count(*) from operators")
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

/// Makes the first operator, once.
async fn begin(
    Injected(state): Injected<AppState>,
    console: Console,
    Json(body): Json<First>,
) -> Result<Audited<(StatusCode, Json<Waiting>)>> {
    if body.password.expose().chars().count() < 12 {
        return Err(AppError::Invalid(
            say::PASSWORD_AT_LEAST_TWELVE_CHARACTERS.into(),
        ));
    }

    let hash = password::hash(body.password.expose())?;
    let mut conn = state.db.operator().await?;

    sqlx::query("select pg_advisory_xact_lock($1)")
        .bind(SETTING_UP)
        .execute(conn.conn())
        .await?;

    let made: Option<(Uuid,)> = sqlx::query_as(
        "insert into operators (email, name, password_hash)
         select $1, $2, $3
          where not exists (select 1 from operators)
         returning id",
    )
    .bind(body.email.as_str())
    .bind(body.name.as_str())
    .bind(&hash)
    .fetch_optional(conn.conn())
    .await?;

    // Said the same way whether somebody was a second too late or is trying it
    // on a machine that has been running for a year: that this door is shut is
    // the whole of what anybody is told.
    let Some((id,)) = made else {
        return Err(AppError::Refused(
            say::THIS_MACHINE_IS_ALREADY_SET_UP.into(),
        ));
    };

    sqlx::query(
        "insert into console_log (operator_id, action, subject, subject_id, detail, request_id)
         values ($1, 'set the machine up', 'operator', $2, '{}'::jsonb, $3)",
    )
    .bind(id)
    .bind(id.to_string())
    .bind(console.request_id.0.to_string())
    .execute(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Audited::new(
        Receipt::for_the_console(),
        (StatusCode::CREATED, Json(Waiting { needed: false })),
    ))
}
