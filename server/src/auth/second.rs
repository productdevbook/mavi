//! The second thing somebody has, besides the password they know.

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::kernel::audit::{self, Actor, Audited};
use crate::kernel::db::TenantConn;
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::ratelimit::Limit;
use crate::kernel::say;
use crate::kernel::secret::{Secret, Shown};
use crate::kernel::tenant::TenantId;
use crate::kernel::{crypto, password, token, totp};

/// How many ways back in somebody is given. Ten is enough to lose a few and
/// still have some, and few enough to be worth writing down.
const RECOVERY_CODES: usize = 10;

/// Confirming, and using a code to get in, are both guessing games: slowly.
const CODE_LIMIT: Limit = Limit::new(5, 60);

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/auth/second-factor",
            Guard {
                audience: Audience::User,
                needs: None,
                rate: RatePolicy::None,
            },
            state_of,
        )
        .gives::<Standing>(),
        Endpoint::post(
            "/api/auth/second-factor",
            Guard {
                audience: Audience::User,
                needs: None,
                rate: RatePolicy::Per(CODE_LIMIT),
            },
            begin,
        )
        .gives::<Begun>(),
        Endpoint::post(
            "/api/auth/second-factor/confirm",
            Guard {
                audience: Audience::User,
                needs: None,
                rate: RatePolicy::Per(CODE_LIMIT),
            },
            confirm,
        )
        .takes::<Digits>()
        .gives::<Recovery>(),
        Endpoint::delete(
            "/api/auth/second-factor",
            Guard {
                audience: Audience::User,
                needs: None,
                rate: RatePolicy::Per(CODE_LIMIT),
            },
            remove,
        )
        .takes::<Password>(),
    ]
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Standing {
    pub enabled: bool,
    /// Null where it is not enabled. Zero here is somebody one lost phone away
    /// from being locked out, and the panel says so.
    pub recovery_codes_left: Option<i64>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Begun {
    /// What is typed in by hand where a camera will not do.
    pub secret: Shown,
    /// What goes in the QR code.
    pub uri: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Digits {
    pub code: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Password {
    pub password: Secret<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Recovery {
    /// Shown once, here, and never again: what is stored is their hashes.
    pub codes: Vec<String>,
}

async fn state_of(State(state): State<AppState>, caller: Caller) -> Result<Json<Standing>> {
    let user = caller.user.as_ref().ok_or(AppError::Unauthenticated)?;
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let row: Option<(i64,)> = sqlx::query_as(
        "select count(c.id)
           from second_factors f
           left join recovery_codes c
             on c.user_id = f.user_id and c.used_at is null
          where f.user_id = $1 and f.confirmed_at is not null
          group by f.id",
    )
    .bind(user.user_id)
    .fetch_optional(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Standing {
        enabled: row.is_some(),
        recovery_codes_left: row.map(|(left,)| left),
    }))
}

async fn begin(State(state): State<AppState>, caller: Caller) -> Result<Audited<Json<Begun>>> {
    let user = caller.user.as_ref().ok_or(AppError::Unauthenticated)?;
    let mut conn = state.db.tenant(caller.tenant()).await?;

    if confirmed_secret(&mut conn, user.user_id).await?.is_some() {
        return Err(AppError::Conflict(
            say::THERE_ALREADY_AUTHENTICATOR_ON_ACCOUNT.into(),
        ));
    }

    let secret = totp::invent();
    let sealed = crypto::seal(&state.keyring, &totp::to_base64(&secret))?;

    // Beginning again replaces what was begun before: somebody who scanned a
    // code and lost the phone before confirming starts over rather than being
    // stuck with a secret nothing holds.
    sqlx::query(
        "insert into second_factors (tenant_id, user_id, sealed)
         values ($1, $2, $3)
         on conflict (tenant_id, user_id)
           do update set sealed = excluded.sealed, confirmed_at = null, last_step = null",
    )
    .bind(caller.tenant().0)
    .bind(user.user_id)
    .bind(&sealed)
    .execute(conn.conn())
    .await?;

    let (email, name): (String, Option<String>) = sqlx::query_as(
        "select u.email, s.name from users u
           left join site_settings s on s.tenant_id = u.tenant_id
          where u.id = $1",
    )
    .bind(user.user_id)
    .fetch_one(conn.conn())
    .await?;

    // An installation with no name yet, or an empty one, still needs an
    // issuer an authenticator app can show: "A site" is what `/llms.txt`
    // falls back to for the same reason, so a phone and a crawler agree.
    let issuer = name
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "A site".to_owned());

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "asked for an authenticator",
        "second_factor",
        Some(&user.user_id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(
        receipt,
        Json(Begun {
            secret: Shown::new(totp::to_base32(&secret)),
            uri: totp::otpauth(&secret, &issuer, &email),
        }),
    ))
}

async fn confirm(
    State(state): State<AppState>,
    caller: Caller,
    Json(body): Json<Digits>,
) -> Result<Audited<Json<Recovery>>> {
    let user = caller.user.as_ref().ok_or(AppError::Unauthenticated)?;
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let begun: Option<(Uuid, String)> = sqlx::query_as(
        "select id, sealed from second_factors
          where user_id = $1 and confirmed_at is null",
    )
    .bind(user.user_id)
    .fetch_optional(conn.conn())
    .await?;

    let (id, sealed) = begun.ok_or(AppError::Refused(
        say::THERE_NO_AUTHENTICATOR_WAITING_CONFIRMED.into(),
    ))?;

    let secret = totp::from_base64(crypto::open(&state.keyring, &sealed)?.expose())?;
    let step = totp::check(&secret, &body.code, state.clock.now(), None)
        .ok_or_else(|| AppError::Refused(say::THOSE_DIGITS_NOT_ONES_APP_SHOWING.into()))?;

    sqlx::query("update second_factors set confirmed_at = now(), last_step = $2 where id = $1")
        .bind(id)
        .bind(step)
        .execute(conn.conn())
        .await?;

    let codes = write_recovery_codes(&mut conn, caller.tenant(), user.user_id).await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "turned on a second factor",
        "second_factor",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, Json(Recovery { codes })))
}

async fn remove(
    State(state): State<AppState>,
    caller: Caller,
    Json(body): Json<Password>,
) -> Result<Audited<axum::http::StatusCode>> {
    let user = caller.user.as_ref().ok_or(AppError::Unauthenticated)?;
    let mut conn = state.db.tenant(caller.tenant()).await?;

    // The password again, because taking the second factor off is the one
    // change a borrowed session would want to make first.
    let stored: Option<(Option<String>,)> =
        sqlx::query_as("select password_hash from users where id = $1")
            .bind(user.user_id)
            .fetch_optional(conn.conn())
            .await?;

    let Some((Some(stored),)) = stored else {
        password::waste_the_same_time(body.password.expose());
        return Err(AppError::Forbidden);
    };

    if !password::verify(body.password.expose(), &stored) {
        return Err(AppError::Forbidden);
    }

    sqlx::query("delete from second_factors where user_id = $1")
        .bind(user.user_id)
        .execute(conn.conn())
        .await?;

    sqlx::query("delete from recovery_codes where user_id = $1")
        .bind(user.user_id)
        .execute(conn.conn())
        .await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "turned off a second factor",
        "second_factor",
        Some(&user.user_id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, axum::http::StatusCode::NO_CONTENT))
}

/// What sign-in calls once the password is right.
///
/// Nothing is said about the second factor before the password is: whether an
/// account has one is not a question an unauthenticated caller gets answered.
pub async fn demand(
    state: &AppState,
    conn: &mut TenantConn,
    user_id: Uuid,
    code: Option<&str>,
) -> Result<()> {
    let Some((id, sealed, last_step)) = confirmed_secret(conn, user_id).await? else {
        return Ok(());
    };

    let Some(code) = code.map(str::trim).filter(|code| !code.is_empty()) else {
        return Err(AppError::SecondFactorRequired);
    };

    let secret = totp::from_base64(crypto::open(&state.keyring, &sealed)?.expose())?;

    if let Some(step) = totp::check(&secret, code, state.clock.now(), last_step) {
        sqlx::query("update second_factors set last_step = $2 where id = $1")
            .bind(id)
            .bind(step)
            .execute(conn.conn())
            .await?;

        return Ok(());
    }

    if spend_recovery_code(conn, user_id, code).await? {
        return Ok(());
    }

    Err(AppError::SecondFactorRequired)
}

type Confirmed = (Uuid, String, Option<i64>);

async fn confirmed_secret(conn: &mut TenantConn, user_id: Uuid) -> Result<Option<Confirmed>> {
    Ok(sqlx::query_as(
        "select id, sealed, last_step from second_factors
          where user_id = $1 and confirmed_at is not null",
    )
    .bind(user_id)
    .fetch_optional(conn.conn())
    .await?)
}

/// A recovery code works once. Marking it used in the same statement that
/// finds it is what makes two sign-ins racing each other spend two codes
/// rather than one code twice.
async fn spend_recovery_code(conn: &mut TenantConn, user_id: Uuid, code: &str) -> Result<bool> {
    let code = tidy(code);

    let spent = sqlx::query(
        "update recovery_codes set used_at = now()
          where user_id = $1 and code_hash = $2 and used_at is null",
    )
    .bind(user_id)
    .bind(&token::hash(&code)[..])
    .execute(conn.conn())
    .await?
    .rows_affected();

    Ok(spent > 0)
}

async fn write_recovery_codes(
    conn: &mut TenantConn,
    tenant: TenantId,
    user_id: Uuid,
) -> Result<Vec<String>> {
    sqlx::query("delete from recovery_codes where user_id = $1")
        .bind(user_id)
        .execute(conn.conn())
        .await?;

    let mut codes = Vec::with_capacity(RECOVERY_CODES);
    let mut hashes = Vec::with_capacity(RECOVERY_CODES);

    for _ in 0..RECOVERY_CODES {
        // Half a token: eighty bits, which nobody guesses, and short enough to
        // be copied off a screen onto paper without a mistake.
        let raw: String = token::generate().chars().take(20).collect();
        hashes.push(token::hash(&raw).to_vec());
        codes.push(format!("{}-{}", &raw[..10], &raw[10..]));
    }

    sqlx::query(
        "insert into recovery_codes (tenant_id, user_id, code_hash)
         select $1, $2, unnest($3::bytea[])",
    )
    .bind(tenant.0)
    .bind(user_id)
    .bind(&hashes)
    .execute(conn.conn())
    .await?;

    Ok(codes)
}

/// Nothing here is time-stamped or ordered, so a row that came back is one
/// that had not been used.
pub async fn codes_left(conn: &mut TenantConn, user_id: Uuid) -> Result<i64> {
    let row = sqlx::query(
        "select count(*) as left from recovery_codes where user_id = $1 and used_at is null",
    )
    .bind(user_id)
    .fetch_one(conn.conn())
    .await?;

    Ok(row.get::<i64, _>("left"))
}

/// What a recovery code looks like when it comes back in, for a test that has
/// to type one.
#[must_use]
pub fn tidy(code: &str) -> String {
    code.trim().to_ascii_lowercase().replace('-', "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recovery_code_is_read_back_however_it_was_written_down() {
        assert_eq!(tidy(" AB12-CD34 "), "ab12cd34");
        assert_eq!(tidy("ab12cd34"), "ab12cd34");
    }
}
