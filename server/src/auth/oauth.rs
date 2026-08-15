//! Signing in with an account somebody already has somewhere else.

use std::time::Duration as Wait;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use chrono::Duration;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::kernel::audit::{self, Actor, Audited};
use crate::kernel::authz::{Access, Capability, Needs};
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::ratelimit::Limit;
use crate::kernel::say;
use crate::kernel::secret::{Secret, Shown};
use crate::kernel::{crypto, outbound, token};

/// How long somebody has to come back with an answer. Long enough to sign in
/// and choose an account, short enough that a link left in a history is stale.
const ATTEMPT_MINUTES: i64 = 10;

/// Somebody else's machine is being asked, so this is not held open long.
const EXCHANGE_TIMEOUT: Wait = Wait::from_secs(10);

const START_LIMIT: Limit = Limit::new(10, 60);

fn settings(access: Access) -> Needs {
    Needs::new(Capability::Settings, access)
}

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/auth/oauth",
            Guard {
                audience: Audience::Public,
                needs: None,
                rate: RatePolicy::Per(START_LIMIT),
            },
            offered,
        )
        .gives::<Vec<Offered>>(),
        Endpoint::post(
            "/api/auth/oauth/{key}/start",
            Guard {
                audience: Audience::Public,
                needs: None,
                rate: RatePolicy::Per(START_LIMIT),
            },
            start,
        )
        .takes::<Leaving>()
        .gives::<Sent>(),
        Endpoint::post(
            "/api/auth/oauth/{key}/callback",
            Guard {
                audience: Audience::Public,
                needs: None,
                rate: RatePolicy::Per(START_LIMIT),
            },
            callback,
        )
        .takes::<Returned>()
        .gives::<Arrival>(),
        Endpoint::put(
            "/api/auth/oauth/{key}",
            Guard {
                audience: Audience::User,
                needs: Some(settings(Access::Write)),
                rate: RatePolicy::None,
            },
            configure,
        )
        .takes::<Configuration>()
        .gives::<Offered>(),
        Endpoint::delete(
            "/api/auth/oauth/{key}",
            Guard {
                audience: Audience::User,
                needs: Some(settings(Access::Write)),
                rate: RatePolicy::None,
            },
            forget,
        ),
    ]
}

/// What a sign-in screen needs to draw a button, and nothing else.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Offered {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub label: String,
    pub client_id: String,
    /// Write-only. What is stored is sealed, and no endpoint reads it back.
    pub client_secret: Secret<String>,
    pub authorize_url: String,
    pub token_url: String,
    pub profile_url: String,
    #[serde(default = "default_scope")]
    pub scope: String,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn default_scope() -> String {
    "openid email profile".to_owned()
}

const fn yes() -> bool {
    true
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = LeavingForProvider)]
pub struct Leaving {
    /// Where in the panel to land afterwards, and where the provider sends
    /// them back to.
    #[serde(default)]
    pub redirect: Option<String>,
    pub redirect_uri: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Sent {
    /// Where to send the browser.
    pub url: String,
    /// What comes back with the answer, so the panel can hand it over again.
    pub state: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Returned {
    pub code: String,
    pub state: String,
    pub redirect_uri: String,
    /// The six digits, where the account it turns out to be asks for them.
    #[serde(default)]
    pub second_factor: Option<String>,
}

/// What one waiting attempt and its provider come back as: the sealed
/// verifier, where to land, and everything needed to finish the exchange.
type Attempt = (String, String, Uuid, String, String, String, String);

/// A session, and where the panel should land afterwards.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Arrival {
    pub token: Shown,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub user: super::Me,
    pub redirect: String,
}

async fn offered(State(state): State<AppState>, _caller: Caller) -> Result<Json<Vec<Offered>>> {
    let mut conn = state.db.begin().await?;

    let rows: Vec<(String, String)> =
        sqlx::query_as("select key, label from oauth_providers where enabled order by label")
            .fetch_all(conn.conn())
            .await?;

    conn.commit().await?;

    Ok(Json(
        rows.into_iter()
            .map(|(key, label)| Offered { key, label })
            .collect(),
    ))
}

async fn configure(
    State(state): State<AppState>,
    caller: Caller,
    Path(key): Path<String>,
    Json(body): Json<Configuration>,
) -> Result<Audited<Json<Offered>>> {
    for url in [&body.authorize_url, &body.token_url, &body.profile_url] {
        let parsed =
            reqwest::Url::parse(url).map_err(|_| AppError::Invalid(say::NOT_ADDRESS.into()))?;

        if parsed.scheme() != "https" && !state.allow_private_destinations {
            return Err(AppError::Invalid(
                say::PROVIDER_ONLY_REACHED_OVER_HTTPS.into(),
            ));
        }
    }

    let sealed = crypto::seal(&state.keyring, body.client_secret.expose())?;
    let mut conn = state.db.begin().await?;

    let row: (Uuid,) = sqlx::query_as(
        "insert into oauth_providers
            (key, label, client_id, sealed_secret,
             authorize_url, token_url, profile_url, scope, enabled)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         on conflict (key) do update set
            label = excluded.label,
            client_id = excluded.client_id,
            sealed_secret = excluded.sealed_secret,
            authorize_url = excluded.authorize_url,
            token_url = excluded.token_url,
            profile_url = excluded.profile_url,
            scope = excluded.scope,
            enabled = excluded.enabled
         returning id",
    )
    .bind(&key)
    .bind(&body.label)
    .bind(&body.client_id)
    .bind(&sealed)
    .bind(&body.authorize_url)
    .bind(&body.token_url)
    .bind(&body.profile_url)
    .bind(&body.scope)
    .bind(body.enabled)
    .fetch_one(conn.conn())
    .await
    .map_err(|failure| match failure {
        sqlx::Error::Database(ref inner) if inner.constraint().is_some() => {
            AppError::Invalid(say::NOT_NAME_PROVIDER_CAN.into())
        }
        other => AppError::from(other),
    })?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "configured a sign-in provider",
        "oauth_provider",
        Some(&row.0.to_string()),
        &serde_json::json!({ "key": key }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(
        receipt,
        Json(Offered {
            key,
            label: body.label,
        }),
    ))
}

async fn forget(
    State(state): State<AppState>,
    caller: Caller,
    Path(key): Path<String>,
) -> Result<Audited<axum::http::StatusCode>> {
    let mut conn = state.db.begin().await?;

    let gone = sqlx::query("delete from oauth_providers where key = $1")
        .bind(&key)
        .execute(conn.conn())
        .await?
        .rows_affected();

    if gone == 0 {
        return Err(AppError::NotFound("provider"));
    }

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "took away a sign-in provider",
        "oauth_provider",
        Some(&key),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, axum::http::StatusCode::NO_CONTENT))
}

async fn start(
    State(state): State<AppState>,
    caller: Caller,
    Path(key): Path<String>,
    Json(body): Json<Leaving>,
) -> Result<Audited<Json<Sent>>> {
    let redirect = landing(body.redirect.as_deref())?;
    let mut conn = state.db.begin().await?;

    let provider: Option<(Uuid, String, String, String)> = sqlx::query_as(
        "select id, client_id, authorize_url, scope
           from oauth_providers where key = $1 and enabled",
    )
    .bind(&key)
    .fetch_optional(conn.conn())
    .await?;

    let (id, client_id, authorize_url, scope) = provider.ok_or(AppError::NotFound("provider"))?;

    let secret = token::generate();
    let verifier = token::generate();
    let challenge = B64URL.encode(Sha256::digest(verifier.as_bytes()));

    sqlx::query(
        "insert into oauth_attempts
            (provider_id, state_hash, sealed, redirect, expires_at)
         values ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(&token::hash(&secret)[..])
    .bind(crypto::seal(&state.keyring, &verifier)?)
    .bind(&redirect)
    .bind(state.clock.now() + Duration::minutes(ATTEMPT_MINUTES))
    .execute(conn.conn())
    .await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "asked a sign-in provider",
        "oauth_provider",
        Some(&id.to_string()),
        &serde_json::json!({ "key": key }),
    )
    .await?;

    conn.commit().await?;

    let mut url = reqwest::Url::parse(&authorize_url)
        .map_err(|_| AppError::Bug("a provider address that was checked and is not one"))?;

    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", &body.redirect_uri)
        .append_pair("scope", &scope)
        .append_pair("state", &secret)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256");

    Ok(Audited::new(
        receipt,
        Json(Sent {
            url: url.to_string(),
            state: secret,
        }),
    ))
}

async fn callback(
    State(state): State<AppState>,
    caller: Caller,
    headers: HeaderMap,
    Path(key): Path<String>,
    Json(body): Json<Returned>,
) -> Result<Audited<Response>> {
    let mut conn = state.db.begin().await?;

    // Spent as it is found: an answer that arrives twice is one this machine
    // asked for once, and the second one is somebody replaying it.
    let attempt: Option<Attempt> = sqlx::query_as(
        "update oauth_attempts a
            set used_at = now()
           from oauth_providers p
          where a.provider_id = p.id
            and a.state_hash = $1
            and a.used_at is null
            and a.expires_at > now()
            and p.key = $2
            and p.enabled
        returning a.sealed, a.redirect, p.id, p.client_id, p.sealed_secret,
                  p.token_url, p.profile_url",
    )
    .bind(&token::hash(&body.state)[..])
    .bind(&key)
    .fetch_optional(conn.conn())
    .await?;

    let (sealed, redirect, provider_id, client_id, sealed_secret, token_url, profile_url) =
        attempt.ok_or_else(|| AppError::Refused(say::SIGN_NOT_ONE_SITE_WAITING_FOR.into()))?;

    let verifier = crypto::open(&state.keyring, &sealed)?;
    let client_secret = crypto::open(&state.keyring, &sealed_secret)?;

    let access = exchange(
        &state,
        &token_url,
        &client_id,
        client_secret.expose(),
        &body.code,
        verifier.expose(),
        &body.redirect_uri,
    )
    .await?;

    let email = whoever_that_is(&state, &profile_url, &access).await?;

    // An account is not made here. Somebody arriving with an address nobody
    // invited would otherwise be a way into a site by owning a mailbox.
    let found: Option<(Uuid,)> = sqlx::query_as(
        "select id from users
          where lower(email) = lower($1) and state = 'active' and deleted_at is null",
    )
    .bind(&email)
    .fetch_optional(conn.conn())
    .await?;

    let (user_id,) =
        found.ok_or_else(|| AppError::Refused(say::ACCOUNT_NOT_BEEN_INVITED_SITE.into()))?;

    // The second factor is a second factor whichever door was used.
    super::second::demand(&state, &mut conn, user_id, body.second_factor.as_deref()).await?;

    let (secret, expires_at) = super::open_session(&state, &mut conn, user_id, &headers).await?;

    let me: (Uuid, String, String, String, Vec<String>) = sqlx::query_as(
        "select u.id, u.email, u.name, r.key, r.grants
           from users u join roles r on r.id = u.role_id
          where u.id = $1",
    )
    .bind(user_id)
    .fetch_one(conn.conn())
    .await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor {
            id: Some(user_id),
            kind: audit::ActorKind::User,
            request_id: caller.request_id,
        },
        "signed in",
        "session",
        None,
        &serde_json::json!({ "with": key, "provider": provider_id }),
    )
    .await?;

    let site_name = super::site_named(&mut conn).await?;

    conn.commit().await?;

    let answer = Arrival {
        token: Shown::new(secret.clone()),
        expires_at,
        user: super::Me {
            id: me.0,
            email: me.1,
            name: me.2,
            role: me.3,
            grants: me.4,
            site: site_name,
        },
        redirect,
    };

    Ok(Audited::new(
        receipt,
        super::with_cookie(
            Json(answer).into_response(),
            &secret,
            super::SESSION_DAYS * 24 * 60 * 60,
        )?,
    ))
}

/// Where somebody may be sent afterwards: somewhere on this site, and nowhere
/// else. An address here is an open redirect, which is how a sign-in screen
/// becomes a way to make a link to somebody else's page look like ours.
fn landing(asked: Option<&str>) -> Result<String> {
    let Some(asked) = asked.map(str::trim).filter(|path| !path.is_empty()) else {
        return Ok("/admin".to_owned());
    };

    let looks_absolute = !asked.starts_with('/')
        || asked.starts_with("//")
        || asked.starts_with("/\\")
        || asked.contains(':');

    if looks_absolute {
        return Err(AppError::Invalid(
            say::SOMEBODY_MAY_ONLY_SENT_BACK_INTO.into(),
        ));
    }

    Ok(asked.to_owned())
}

#[derive(Deserialize)]
struct Granted {
    access_token: String,
}

#[derive(Deserialize)]
struct Profile {
    email: Option<String>,
    email_verified: Option<bool>,
}

async fn exchange(
    state: &AppState,
    token_url: &str,
    client_id: &str,
    client_secret: &str,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> Result<String> {
    let reaching = outbound::reach(
        token_url,
        EXCHANGE_TIMEOUT,
        state.allow_private_destinations,
    )
    .await?;

    let answered = reaching
        .client
        .post(reaching.url)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code_verifier", verifier),
        ])
        .send()
        .await
        .map_err(|_| AppError::Refused(say::PROVIDER_COULD_NOT_REACHED.into()))?;

    if !answered.status().is_success() {
        return Err(AppError::Refused(
            say::PROVIDER_WOULD_NOT_EXCHANGE_CODE.into(),
        ));
    }

    let granted: Granted = answered
        .json()
        .await
        .map_err(|_| AppError::Refused(say::PROVIDER_ANSWERED_NOTHING_USABLE.into()))?;

    Ok(granted.access_token)
}

async fn whoever_that_is(state: &AppState, profile_url: &str, access: &str) -> Result<String> {
    let reaching = outbound::reach(
        profile_url,
        EXCHANGE_TIMEOUT,
        state.allow_private_destinations,
    )
    .await?;

    let answered = reaching
        .client
        .get(reaching.url)
        .bearer_auth(access)
        .send()
        .await
        .map_err(|_| AppError::Refused(say::PROVIDER_COULD_NOT_REACHED.into()))?;

    if !answered.status().is_success() {
        return Err(AppError::Refused(say::PROVIDER_WOULD_NOT_SAY_WHO.into()));
    }

    let profile: Profile = answered
        .json()
        .await
        .map_err(|_| AppError::Refused(say::PROVIDER_ANSWERED_NOTHING_USABLE.into()))?;

    // An unverified address is an address somebody typed, not one they hold —
    // and typing an editor's address is otherwise the whole attack.
    if profile.email_verified == Some(false) {
        return Err(AppError::Refused(say::PROVIDER_NOT_VERIFIED_ADDRESS.into()));
    }

    profile
        .email
        .filter(|email| email.contains('@'))
        .ok_or_else(|| AppError::Refused(say::PROVIDER_DID_NOT_SAY_ADDRESS.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn somebody_is_only_sent_back_into_this_site() {
        assert_eq!(landing(None).unwrap(), "/admin");
        assert_eq!(landing(Some("/admin/posts")).unwrap(), "/admin/posts");

        for elsewhere in [
            "https://example.invalid",
            "//example.invalid",
            "/\\example.invalid",
            "javascript:alert(1)",
            "/admin?x=1:2",
        ] {
            assert!(
                landing(Some(elsewhere)).is_err(),
                "{elsewhere} was taken as somewhere on this site"
            );
        }
    }
}
