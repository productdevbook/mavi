//! Signing in, and being told apart afterwards.
//!
//! A session is a token this machine holds only the hash of, in a cookie a
//! script cannot read. A wrong password and an account with a second factor
//! are different refusals — a screen that says "wrong password" to somebody
//! whose password was right is the reason.
pub mod oauth;
pub mod second;

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::header::{SET_COOKIE, USER_AGENT};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::kernel::audit::{self, Actor, Audited};
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy, USER_COOKIE};
use crate::kernel::ratelimit::Limit;
use crate::kernel::say;
use crate::kernel::secret::{Secret, Shown};
use crate::kernel::types::Email;
use crate::kernel::{password, token};

pub(crate) const SESSION_DAYS: i64 = 30;

type StoredUser = (Uuid, String, String, String, Vec<String>, Option<String>);

/// Five a minute at one address: slow enough that a list of passwords is not
/// worth trying, fast enough that two mistypes do not lock somebody out.
const SIGN_IN_LIMIT: Limit = Limit::new(5, 60);

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    let mut endpoints = vec![
        Endpoint::post(
            "/api/auth/session",
            Guard {
                audience: Audience::Public,
                needs: None,
                rate: RatePolicy::Per(SIGN_IN_LIMIT),
            },
            sign_in,
        )
        .takes::<Credentials>()
        .gives::<Session>(),
        Endpoint::delete(
            "/api/auth/session",
            Guard {
                audience: Audience::User,
                needs: None,
                rate: RatePolicy::None,
            },
            sign_out,
        ),
        Endpoint::get(
            "/api/auth/me",
            Guard {
                audience: Audience::User,
                needs: None,
                rate: RatePolicy::None,
            },
            me,
        )
        .gives::<Me>(),
    ];

    endpoints.extend(second::endpoints());
    endpoints.extend(oauth::endpoints());
    endpoints
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Credentials {
    pub email: Email,
    pub password: Secret<String>,
    /// The six digits, or a recovery code, where the account asks for one.
    ///
    /// Sent with the password rather than exchanged for a half-signed-in
    /// token: a token that stands between the two halves is a token worth
    /// stealing, and the panel already has the password in hand when it is
    /// told the digits are wanted.
    #[serde(default)]
    pub code: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Session {
    pub token: Shown,
    pub expires_at: DateTime<Utc>,
    pub user: Me,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Me {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: String,
    pub grants: Vec<String>,
    /// What the site is called, for the tab and the header.
    pub site: String,
    /// `active`, or `on_hold` for a site that may be read and not written.
    pub site_state: String,
}

async fn sign_in(
    State(state): State<AppState>,
    caller: Caller,
    headers: HeaderMap,
    Json(credentials): Json<Credentials>,
) -> Result<Audited<Response>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let found: Option<StoredUser> = sqlx::query_as(
        "select u.id, u.email, u.name, r.key, r.grants, u.password_hash
               from users u
               join roles r on r.id = u.role_id
              where u.email = $1 and u.state = 'active' and u.deleted_at is null",
    )
    .bind(credentials.email.as_str())
    .fetch_optional(conn.conn())
    .await?;

    // The same answer either way, and the same work either way: telling them
    // apart, by wording or by timing, is a way to ask which addresses exist.
    let no = AppError::Invalid(say::NOT_SIGN_WE_KNOW.into());

    let Some((id, email, name, role, grants, Some(stored))) = found else {
        password::waste_the_same_time(credentials.password.expose());
        return Err(no);
    };

    if !password::verify(credentials.password.expose(), &stored) {
        return Err(no);
    }

    second::demand(&state, &mut conn, id, credentials.code.as_deref()).await?;

    let (secret, expires_at) =
        open_session(&state, &mut conn, caller.tenant(), id, &headers).await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor {
            id: Some(id),
            kind: audit::ActorKind::User,
            request_id: caller.request_id,
        },
        "signed in",
        "session",
        None,
        &serde_json::json!({}),
    )
    .await?;

    let (site_state, site_name) = site_of(&mut conn, caller.tenant()).await?;

    conn.commit().await?;

    let body = Session {
        token: Shown::new(secret.clone()),
        expires_at,
        user: Me {
            id,
            email,
            name,
            role,
            grants,
            site: site_name,
            site_state,
        },
    };

    Ok(Audited::new(
        receipt,
        with_cookie(
            Json(body).into_response(),
            &secret,
            SESSION_DAYS * 24 * 60 * 60,
        )?,
    ))
}

/// A session, and the end of whatever else this person was carrying.
///
/// However somebody arrived — a password, an authenticator, another site's
/// account — they leave with the same thing, made the same way.
pub(crate) async fn open_session(
    state: &AppState,
    conn: &mut crate::kernel::db::TenantConn,
    tenant: crate::kernel::tenant::TenantId,
    user_id: Uuid,
    headers: &HeaderMap,
) -> Result<(String, DateTime<Utc>)> {
    let secret = token::generate();
    let expires_at = state.clock.now() + Duration::days(SESSION_DAYS);
    let agent = headers
        .get(USER_AGENT)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.chars().take(400).collect::<String>());

    // Anything this person was carrying before stops working now: a session
    // handed to them by somebody else is not a session they keep.
    sqlx::query("update sessions set revoked_at = now() where user_id = $1 and revoked_at is null")
        .bind(user_id)
        .execute(conn.conn())
        .await?;

    sqlx::query(
        "insert into sessions (tenant_id, user_id, token_hash, expires_at, user_agent)
         values ($1, $2, $3, $4, $5)",
    )
    .bind(tenant.0)
    .bind(user_id)
    .bind(&token::hash(&secret)[..])
    .bind(expires_at)
    .bind(agent)
    .execute(conn.conn())
    .await?;

    Ok((secret, expires_at))
}

async fn sign_out(State(state): State<AppState>, caller: Caller) -> Result<Audited<Response>> {
    let user = caller.user.as_ref().ok_or(AppError::Unauthenticated)?;
    let mut conn = state.db.tenant(caller.tenant()).await?;

    sqlx::query("update sessions set revoked_at = now() where id = $1 and revoked_at is null")
        .bind(user.session_id)
        .execute(conn.conn())
        .await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "signed out",
        "session",
        Some(&user.session_id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(
        receipt,
        with_cookie(axum::http::StatusCode::NO_CONTENT.into_response(), "", 0)?,
    ))
}

async fn me(State(state): State<AppState>, caller: Caller) -> Result<Json<Me>> {
    let user = caller.user.as_ref().ok_or(AppError::Unauthenticated)?;
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let row: (Uuid, String, String, String, Vec<String>) = sqlx::query_as(
        "select u.id, u.email, u.name, r.key, r.grants
           from users u join roles r on r.id = u.role_id
          where u.id = $1",
    )
    .bind(user.user_id)
    .fetch_one(conn.conn())
    .await?;

    // What state the site itself is in. A panel that does not know a site is
    // on hold offers every button and is refused by all of them.
    let (site_state, site) = site_of(&mut conn, caller.tenant()).await?;

    conn.commit().await?;

    Ok(Json(Me {
        id: row.0,
        email: row.1,
        name: row.2,
        role: row.3,
        grants: row.4,
        site,
        site_state,
    }))
}

/// What state the site is in and what it is called, for whoever just arrived.
pub(super) async fn site_of(
    conn: &mut crate::kernel::db::TenantConn,
    tenant: crate::kernel::TenantId,
) -> Result<(String, String)> {
    let row: (String, Option<String>) = sqlx::query_as(
        "select t.state::text, s.name
           from tenants t left join site_settings s on s.tenant_id = t.id
          where t.id = $1",
    )
    .bind(tenant.0)
    .fetch_one(conn.conn())
    .await?;

    Ok((row.0, row.1.unwrap_or_default()))
}

pub(crate) fn with_cookie(mut response: Response, value: &str, max_age: i64) -> Result<Response> {
    let cookie =
        format!("{USER_COOKIE}={value}; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age={max_age}");

    response.headers_mut().insert(
        SET_COOKIE,
        cookie
            .parse()
            .map_err(|_| AppError::Bug("a cookie this shape is always a header value"))?,
    );

    Ok(response)
}
