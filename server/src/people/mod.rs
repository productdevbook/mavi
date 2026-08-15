//! Who may sign in to a site, and what they may reach.
//!
//! An account is invited rather than given a password by whoever made it: a
//! link, and they choose one. Roles carry grants as data, and nobody hands out
//! more than they hold — a role that could be given `settings:write` by
//! somebody without it is a way around every other check there is.
use axum::Json;
use axum::extract::{Path, Query as HttpQuery, State as Injected};
use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::kernel::audit::{self, Actor, Auditable, Audited};
use crate::kernel::authz::{Access, Capability, Needs, Permit, every_grant};
use crate::kernel::db::Tx;
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::page::{Page, Query};
use crate::kernel::ratelimit::{self, Limit};
use crate::kernel::say::{self, Say};
use crate::kernel::secret::Secret;
use crate::kernel::types::{Email, Title};
use crate::kernel::{password, token};

/// How long an invitation or a reset is good for. Long enough to be read after
/// a weekend, short enough that a mailbox somebody else reads later is not a
/// way in.
const TICKET_DAYS: i64 = 3;

/// Three a minute for the address being aimed at. Asking to reset somebody
/// else's password is a way to fill their inbox.
const RESET_LIMIT: Limit = Limit::new(3, 60);

fn people(access: Access) -> Needs {
    Needs::new(Capability::People, access)
}

#[must_use]
#[expect(clippy::too_many_lines, reason = "one list of what is served")]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/people",
            Guard {
                audience: Audience::User,
                needs: Some(people(Access::View)),
                rate: RatePolicy::None,
            },
            list,
        )
        .gives::<Page<Person>>(),
        Endpoint::post(
            "/api/people",
            Guard {
                audience: Audience::User,
                needs: Some(people(Access::Write)),
                rate: RatePolicy::None,
            },
            invite,
        )
        .takes::<Invitation>()
        .gives::<Invited>(),
        Endpoint::patch(
            "/api/people/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(people(Access::Write)),
                rate: RatePolicy::None,
            },
            change,
        )
        .takes::<PersonChanges>()
        .gives::<Person>(),
        Endpoint::delete(
            "/api/people/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(people(Access::Delete)),
                rate: RatePolicy::None,
            },
            remove,
        ),
        Endpoint::get(
            "/api/roles",
            Guard {
                audience: Audience::User,
                needs: Some(people(Access::View)),
                rate: RatePolicy::None,
            },
            roles,
        )
        .gives::<Vec<Role>>(),
        Endpoint::post(
            "/api/roles",
            Guard {
                audience: Audience::User,
                needs: Some(people(Access::Write)),
                rate: RatePolicy::None,
            },
            make_role,
        )
        .takes::<NewRole>()
        .gives::<Role>(),
        Endpoint::patch(
            "/api/roles/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(people(Access::Write)),
                rate: RatePolicy::None,
            },
            change_role,
        )
        .takes::<RoleChanges>()
        .gives::<Role>(),
        Endpoint::delete(
            "/api/roles/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(people(Access::Delete)),
                rate: RatePolicy::None,
            },
            remove_role,
        ),
        Endpoint::post(
            "/api/auth/reset",
            Guard {
                audience: Audience::Public,
                needs: None,
                rate: RatePolicy::Per(RESET_LIMIT),
            },
            ask_to_reset,
        )
        .takes::<Address>(),
        Endpoint::post(
            "/api/auth/password",
            Guard {
                audience: Audience::Public,
                needs: None,
                rate: RatePolicy::Per(RESET_LIMIT),
            },
            set_password,
        )
        .takes::<Chosen>(),
        Endpoint::patch(
            "/api/auth/password",
            Guard {
                audience: Audience::User,
                needs: None,
                rate: RatePolicy::Per(RESET_LIMIT),
            },
            change_own_password,
        )
        .takes::<Changing>(),
    ]
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, utoipa::ToSchema,
)]
#[sqlx(type_name = "user_state", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum PersonState {
    Invited,
    Active,
    Suspended,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Person {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: String,
    pub state: PersonState,
    pub email_proved: bool,
    pub created_at: DateTime<Utc>,
}

impl Auditable for Person {
    const SUBJECT: &'static str = "person";

    fn subject_id(&self) -> String {
        self.id.to_string()
    }

    /// The address and the role, and nothing that would be a leak: no hash, no
    /// token, nothing about a session.
    fn summary(&self) -> serde_json::Value {
        serde_json::json!({
            "email": self.email,
            "role": self.role,
            "state": self.state,
        })
    }
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Role {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    pub grants: Vec<String>,
    pub built_in: bool,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Invitation {
    pub email: Email,
    pub name: Title,
    pub role_id: Uuid,
}

/// What comes back from an invitation: who was made, and when what was sent to
/// them stops working. The token itself is in the message and nowhere else —
/// something that hands it back is something that has it in a log.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Invited {
    pub id: Uuid,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PersonChanges {
    pub name: Option<Title>,
    /// Somebody's address, changed. What was proved about the old one is not
    /// proved about this one, so it goes back to unproved and a link is sent —
    /// an address typed by somebody else is how an account gets taken over.
    pub email: Option<Email>,
    pub role_id: Option<Uuid>,
    pub suspended: Option<bool>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewRole {
    pub key: String,
    pub name: Title,
    pub grants: Vec<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RoleChanges {
    pub name: Option<Title>,
    /// The whole set, not a change to it: a role is what it may reach, and
    /// sending only what is added leaves nobody able to take one away.
    pub grants: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Address {
    pub email: Email,
}

/// Changing one's own, knowing the old one.
///
/// The current one is asked for because a signed-in screen somebody walked
/// away from is the usual way an account is taken over, and because a password
/// changed without it is a password nobody can be sure the owner chose.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Changing {
    pub current: Secret<String>,
    pub next: Secret<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Chosen {
    pub token: Secret<String>,
    pub password: Secret<String>,
}

async fn list(
    Injected(state): Injected<AppState>,
    _caller: Caller,
    _permit: Permit,
    HttpQuery(query): HttpQuery<Query>,
) -> Result<Json<Page<Person>>> {
    let mut conn = state.db.begin().await?;

    let rows: Vec<Person> = sqlx::query_as(
        "select u.id, u.email, u.name, r.key as role, u.state,
                u.email_proved_at is not null as email_proved, u.created_at
           from users u join roles r on r.id = u.role_id
          where u.deleted_at is null
            and ($1::timestamptz is null or u.created_at < $1)
          order by u.created_at desc, u.id desc
          limit $2",
    )
    .bind(
        query
            .after
            .as_deref()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(DateTime::<Utc>::from),
    )
    .bind(query.fetch())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Page::build(&query, rows, |person| {
        person.created_at.to_rfc3339()
    })))
}

async fn invite(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Json(body): Json<Invitation>,
) -> Result<Audited<(StatusCode, Json<Invited>)>> {
    let mut conn = state.db.begin().await?;

    within_reach(&mut conn, &caller, body.role_id).await?;

    let person: Person = sqlx::query_as(
        "insert into users (role_id, email, name, state)
         values ($1, $2, $3, 'invited')
         returning id, email, name,
                   (select key from roles where id = $1) as role,
                   state,
                   email_proved_at is not null as email_proved,
                   created_at",
    )
    .bind(body.role_id)
    .bind(body.email.as_str())
    .bind(body.name.as_str())
    .fetch_one(conn.conn())
    .await
    .map_err(|error| {
        match error
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
        {
            Some(code) if code == "23505" => {
                AppError::Conflict(say::SOMEBODY_ALREADY_USES_ADDRESS.into())
            }
            Some(code) if code == "23503" => AppError::Invalid(say::THERE_NO_SUCH_ROLE.into()),
            _ => AppError::Database(error),
        }
    })?;

    let (secret, expires_at) = a_ticket(&mut conn, person.id, "invitation", &state).await?;

    // In the site's own words where it has written any: what an invitation
    // says is the first thing somebody sees of a site they have never used.
    let language = site_language(&mut conn).await?;
    let site = site_name(&mut conn).await?;

    let (subject, body) = crate::mail::letters::press(
        &mut conn,
        "invitation",
        &language,
        &[
            ("name", person.name.clone()),
            ("site", site),
            (
                "link",
                state.address.link(&format!("/forgotten?token={secret}")),
            ),
        ],
    )
    .await?;

    written_to(&mut conn, &person.email, &subject, &body).await?;

    let receipt = audit::record(
        &mut conn,
        Actor::of(&caller),
        "invited",
        None,
        Some(&person),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(
        receipt,
        (
            StatusCode::CREATED,
            Json(Invited {
                id: person.id,
                expires_at,
            }),
        ),
    ))
}

/// What this site is called, for a letter that says so. Its own name where it
/// has one, and nothing rather than somebody else's where it has not.
async fn site_name(conn: &mut Tx) -> Result<String> {
    let found: Option<(String,)> = sqlx::query_as("select name from site_settings ")
        .fetch_optional(conn.conn())
        .await?;

    Ok(found.map_or_else(String::new, |(name,)| name))
}

/// Which language a letter to somebody on this site is written in: the site's
/// own default, since what is being written to is an account on it.
async fn site_language(conn: &mut Tx) -> Result<String> {
    let found: Option<(String,)> =
        sqlx::query_as("select code from languages where is_default limit 1")
            .fetch_optional(conn.conn())
            .await?;

    Ok(found.map_or_else(|| "en".to_owned(), |(code,)| code))
}

/// A message somebody asked for by acting: an invitation, a reset. No list to
/// leave, so nothing about leaving one.
async fn written_to(conn: &mut Tx, to: &str, subject: &str, body: &str) -> Result<()> {
    crate::mail::post(
        conn,
        &crate::mail::Outgoing {
            to,
            subject,
            body,
            purpose: crate::mail::Purpose::Transactional,
            campaign_id: None,
            subscriber_id: None,
            unsubscribe: None,
        },
    )
    .await?;

    Ok(())
}

/// Nobody puts anybody into a role carrying more than they hold themselves.
///
/// Making a role was already checked this way, and checking only there was the
/// hole: a role that already carries `settings:write` could be handed out by
/// anybody with `people:write`, who could then sign in as whoever they had just
/// invited into it.
async fn within_reach(conn: &mut Tx, caller: &Caller, role_id: Uuid) -> Result<()> {
    let holding = &caller.require_user()?.grants;

    let target: Option<(Vec<String>,)> = sqlx::query_as("select grants from roles where id = $1")
        .bind(role_id)
        .fetch_optional(conn.conn())
        .await?;

    let Some((grants,)) = target else {
        return Err(AppError::Invalid(say::THERE_NO_SUCH_ROLE.into()));
    };

    if let Some(beyond) = grants.iter().find(|grant| !holding.contains(*grant)) {
        return Err(AppError::Refused(
            Say::of(say::ROLE_CARRIES_WHAT_YOU_DO_NOT).naming("beyond", beyond),
        ));
    }

    Ok(())
}

/// One use, an expiry, and only the hash kept. Anything outstanding for the
/// same purpose is spent first: an invitation sent twice leaves one way in.
async fn a_ticket(
    conn: &mut Tx,
    user_id: Uuid,
    purpose: &str,
    state: &AppState,
) -> Result<(String, DateTime<Utc>)> {
    let secret = token::generate();
    let expires_at = state.clock.now() + Duration::days(TICKET_DAYS);

    sqlx::query(
        "update tickets set spent_at = now()
          where user_id = $1 and purpose = $2::ticket_purpose and spent_at is null",
    )
    .bind(user_id)
    .bind(purpose)
    .execute(conn.conn())
    .await?;

    sqlx::query(
        "insert into tickets (user_id, purpose, token_hash, expires_at)
         values ($1, $2::ticket_purpose, $3, $4)",
    )
    .bind(user_id)
    .bind(purpose)
    .bind(&token::hash(&secret)[..])
    .bind(expires_at)
    .execute(conn.conn())
    .await?;

    Ok((secret, expires_at))
}

/// What a role may reach, changed.
///
/// The same two rules as making one: a grant nothing recognises is refused,
/// and nobody hands out more than they hold. A built-in role is left alone —
/// a site that edits `owner` into something without `people:write` is a site
/// with nobody able to put it back.
async fn change_role(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
    Json(wanted): Json<RoleChanges>,
) -> Result<Audited<Json<Role>>> {
    if let Some(grants) = &wanted.grants {
        let known = every_grant();

        if let Some(unknown) = grants.iter().find(|grant| !known.contains(grant)) {
            return Err(AppError::Invalid(
                Say::of(say::NO_SUCH_GRANT).naming("grant", unknown),
            ));
        }

        let holding = &caller.require_user()?.grants;

        if let Some(beyond) = grants.iter().find(|grant| !holding.contains(*grant)) {
            return Err(AppError::Refused(
                Say::of(say::YOU_DO_NOT_HOLD_THAT_YOURSELF).naming("beyond", beyond),
            ));
        }
    }

    let mut conn = state.db.begin().await?;

    // What it already carries has to be within reach as well, or somebody who
    // holds less could rewrite a role that holds more and then be given it.
    within_reach(&mut conn, &caller, id).await?;

    let built_in: Option<(bool,)> = sqlx::query_as("select built_in from roles where id = $1")
        .bind(id)
        .fetch_optional(conn.conn())
        .await?;

    let Some((built_in,)) = built_in else {
        return Err(AppError::Invalid(say::THERE_NO_SUCH_ROLE.into()));
    };

    if built_in {
        return Err(AppError::Refused(say::THAT_ROLE_IS_THE_BUILD_S_OWN.into()));
    }

    let after: Role = sqlx::query_as(
        "update roles
            set name = coalesce($2, name),
                grants = coalesce($3, grants)
          where id = $1
         returning id, key, name, grants, built_in",
    )
    .bind(id)
    .bind(wanted.name.as_ref().map(Title::as_str))
    .bind(wanted.grants.as_ref())
    .fetch_one(conn.conn())
    .await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "changed a role",
        "role",
        Some(&after.key),
        &serde_json::json!({ "grants": after.grants }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, Json(after)))
}

/// Taking a role away, where nobody is in it.
async fn remove_role(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
) -> Result<Audited<StatusCode>> {
    let mut conn = state.db.begin().await?;

    within_reach(&mut conn, &caller, id).await?;

    let found: Option<(bool, i64)> = sqlx::query_as(
        "select r.built_in,
                (select count(*) from users u
                  where u.role_id = r.id and u.deleted_at is null)
           from roles r where r.id = $1",
    )
    .bind(id)
    .fetch_optional(conn.conn())
    .await?;

    let Some((built_in, people)) = found else {
        return Err(AppError::Invalid(say::THERE_NO_SUCH_ROLE.into()));
    };

    if built_in {
        return Err(AppError::Refused(say::THAT_ROLE_IS_THE_BUILD_S_OWN.into()));
    }

    // Somebody in a role that has gone is somebody who can reach nothing, and
    // the database refuses it anyway.
    if people > 0 {
        return Err(AppError::Refused(
            Say::of(say::SOMEBODY_IS_IN_THAT_ROLE).naming("people", people),
        ));
    }

    sqlx::query("delete from roles where id = $1")
        .bind(id)
        .execute(conn.conn())
        .await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "took a role away",
        "role",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::NO_CONTENT))
}

async fn change_own_password(
    Injected(state): Injected<AppState>,
    caller: Caller,
    Json(body): Json<Changing>,
) -> Result<Audited<StatusCode>> {
    if body.next.expose().chars().count() < 12 {
        return Err(AppError::Invalid(
            say::PASSWORD_AT_LEAST_TWELVE_CHARACTERS.into(),
        ));
    }

    let me = caller.require_user()?;
    let mut conn = state.db.begin().await?;

    let held: Option<(Option<String>,)> =
        sqlx::query_as("select password_hash from users where id = $1 and deleted_at is null")
            .bind(me.user_id)
            .fetch_optional(conn.conn())
            .await?;

    let stored = held.and_then(|(hash,)| hash).unwrap_or_default();

    // The same work whether there is a password to compare against or not, so
    // that how long this takes says nothing about the account.
    if !password::verify(body.current.expose(), &stored) {
        return Err(AppError::Invalid(say::NOT_SIGN_WE_KNOW.into()));
    }

    sqlx::query("update users set password_hash = $2 where id = $1")
        .bind(me.user_id)
        .bind(password::hash(body.next.expose())?)
        .execute(conn.conn())
        .await?;

    // Everything that was open before a password changed is not open now,
    // including this one: whoever knew the old password may be holding one.
    sqlx::query("update sessions set revoked_at = now() where user_id = $1 and revoked_at is null")
        .bind(me.user_id)
        .execute(conn.conn())
        .await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "changed their own password",
        "person",
        Some(&me.user_id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::NO_CONTENT))
}

async fn change(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
    Json(changes): Json<PersonChanges>,
) -> Result<Audited<Json<Person>>> {
    let mut conn = state.db.begin().await?;
    let before = one(&mut conn, id).await?;

    if let Some(role_id) = changes.role_id {
        within_reach(&mut conn, &caller, role_id).await?;
    }

    // The account somebody is signed in as is not one they can take their own
    // way out of: a role changed to something with no `people:write` would
    // leave a site with nobody able to change it back.
    if caller.require_user()?.user_id == id && changes.role_id.is_some() {
        return Err(AppError::Conflict(say::SOMEBODY_ELSE_CHANGES_WHAT.into()));
    }

    if let Some(role_id) = changes.role_id {
        refuse_if_moved_away_from_last_ownership(&mut conn, id, role_id).await?;
    }

    // Suspending is locking the door from outside: the account is still
    // there, but nobody can sign into it to grant the role onward, which is
    // the same outcome `remove` and `erase` are already refused for.
    if changes.suspended == Some(true) {
        refuse_if_last_owner(&mut conn, id).await?;
    }

    let after: Person = sqlx::query_as(
        "update users
            set name = coalesce($2, name),
                email = coalesce($5, email),
                email_proved_at = case when $5 is null then email_proved_at end,
                role_id = coalesce($3, role_id),
                -- Suspending anybody, and un-suspending back to active.
                -- Somebody invited who was suspended goes back to invited:
                -- they still have no password, and 'active' would say they do.
                state = case
                    when $4 is null then state
                    when $4 then 'suspended'::user_state
                    when state = 'suspended' and password_hash is not null
                        then 'active'::user_state
                    when state = 'suspended' then 'invited'::user_state
                    else state
                end
          where id = $1 and deleted_at is null
         returning id, email, name,
                   (select key from roles where id = users.role_id) as role,
                   state,
                   email_proved_at is not null as email_proved,
                   created_at",
    )
    .bind(id)
    .bind(changes.name.as_ref().map(Title::as_str))
    .bind(changes.role_id)
    .bind(changes.suspended)
    .bind(changes.email.as_ref().map(Email::as_str))
    .fetch_one(conn.conn())
    .await
    .map_err(|error| {
        match error
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
        {
            Some(code) if code == "23505" => {
                AppError::Conflict(say::SOMEBODY_ALREADY_USES_ADDRESS.into())
            }
            _ => AppError::Database(error),
        }
    })?;

    if changes.email.is_some() {
        prove_the_new_address(&state, &mut conn, &caller, &after).await?;
    }

    // Suspending somebody takes away what they are already holding, which a
    // change of state alone would not.
    if changes.suspended == Some(true) {
        sqlx::query(
            "update sessions set revoked_at = now() where user_id = $1 and revoked_at is null",
        )
        .bind(id)
        .execute(conn.conn())
        .await?;
    }

    let receipt = audit::record(
        &mut conn,
        Actor::of(&caller),
        "changed",
        Some(&before),
        Some(&after),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, Json(after)))
}

/// A changed address is an account somebody else may now be holding open, and
/// a link to prove the new one. Both, or neither — which is why they are one
/// call rather than two things a caller could do half of.
async fn prove_the_new_address(
    state: &AppState,
    conn: &mut Tx,
    _caller: &Caller,
    person: &Person,
) -> Result<()> {
    sqlx::query("update sessions set revoked_at = now() where user_id = $1 and revoked_at is null")
        .bind(person.id)
        .execute(conn.conn())
        .await?;

    let (secret, _) = a_ticket(conn, person.id, "email_proof", state).await?;

    let language = site_language(conn).await?;
    let site = site_name(conn).await?;

    let (subject, letter) = crate::mail::letters::press(
        conn,
        "email_proof",
        &language,
        &[
            ("name", person.name.clone()),
            ("site", site),
            (
                "link",
                state.address.link(&format!("/forgotten?token={secret}")),
            ),
        ],
    )
    .await?;

    written_to(conn, &person.email, &subject, &letter).await
}

/// A no-op reassignment to the role already held, or a promotion into it,
/// cannot strand the site; only moving away from it can, and only
/// `refuse_if_last_owner` knows whether anybody else is left to grant it
/// back.
async fn refuse_if_moved_away_from_last_ownership(
    conn: &mut Tx,
    id: Uuid,
    role_id: Uuid,
) -> Result<()> {
    let (becomes_owner,): (bool,) = sqlx::query_as("select key = 'owner' from roles where id = $1")
        .bind(role_id)
        .fetch_one(conn.conn())
        .await?;

    if becomes_owner {
        return Ok(());
    }

    refuse_if_last_owner(conn, id).await
}

/// Refuses to take this account's ownership away — by deletion, by erasure,
/// by suspending it or by moving it to another role — when it is the last
/// one that could still be signed into as an owner: with it gone nobody
/// could grant that role to anyone else, and there is no operator account
/// this installation can be recovered through (#7 confirmed that door has
/// never worked). `people::remove`, `people::change` and `site::erase` each
/// take ownership away by a different route, so all three ask here rather
/// than each keeping its own idea of what a usable owner is.
///
/// "Remaining" means able to sign in as one, the same test `auth::sign_in`
/// itself uses: active, not deleted, and a password chosen. A suspended
/// owner, or one still sitting on an unused invitation, cannot be asked to
/// grant the role to anyone, so does not count as one still standing.
pub(crate) async fn refuse_if_last_owner(conn: &mut Tx, id: Uuid) -> Result<()> {
    let holds_owner: Option<(bool,)> = sqlx::query_as(
        "select r.key = 'owner'
           from users u join roles r on r.id = u.role_id
          where u.id = $1 and u.deleted_at is null",
    )
    .bind(id)
    .fetch_optional(conn.conn())
    .await?;

    if holds_owner.is_none_or(|(owner,)| !owner) {
        return Ok(());
    }

    let (remaining,): (i64,) = sqlx::query_as(
        "select count(*) from users u join roles r on r.id = u.role_id
          where r.key = 'owner' and u.id != $1 and u.deleted_at is null
            and u.state = 'active' and u.password_hash is not null",
    )
    .bind(id)
    .fetch_one(conn.conn())
    .await?;

    if remaining == 0 {
        return Err(AppError::Refused(say::THAT_IS_THE_LAST_OWNER.into()));
    }

    Ok(())
}

async fn remove(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
) -> Result<Audited<StatusCode>> {
    if caller.require_user()?.user_id == id {
        return Err(AppError::Conflict(
            say::SOMEBODY_ELSE_TAKES_ACCOUNT_AWAY.into(),
        ));
    }

    let mut conn = state.db.begin().await?;
    let before = one(&mut conn, id).await?;

    refuse_if_last_owner(&mut conn, id).await?;

    sqlx::query("update users set deleted_at = now(), state = 'suspended' where id = $1")
        .bind(id)
        .execute(conn.conn())
        .await?;

    sqlx::query("update sessions set revoked_at = now() where user_id = $1 and revoked_at is null")
        .bind(id)
        .execute(conn.conn())
        .await?;

    let receipt = audit::record(
        &mut conn,
        Actor::of(&caller),
        "removed",
        Some(&before),
        None,
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::NO_CONTENT))
}

async fn one(conn: &mut Tx, id: Uuid) -> Result<Person> {
    sqlx::query_as(
        "select u.id, u.email, u.name, r.key as role, u.state,
                u.email_proved_at is not null as email_proved, u.created_at
           from users u join roles r on r.id = u.role_id
          where u.id = $1 and u.deleted_at is null",
    )
    .bind(id)
    .fetch_optional(conn.conn())
    .await?
    .ok_or(AppError::NotFound("person"))
}

async fn roles(
    Injected(state): Injected<AppState>,
    _caller: Caller,
    _permit: Permit,
) -> Result<Json<Vec<Role>>> {
    let mut conn = state.db.begin().await?;

    let rows: Vec<Role> =
        sqlx::query_as("select id, key, name, grants, built_in from roles order by key")
            .fetch_all(conn.conn())
            .await?;

    conn.commit().await?;

    Ok(Json(rows))
}

async fn make_role(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Json(body): Json<NewRole>,
) -> Result<Audited<(StatusCode, Json<Role>)>> {
    let known = every_grant();

    // A grant nothing recognises is a role that quietly grants nothing, which
    // reads as a permissions bug for as long as it takes somebody to notice.
    if let Some(unknown) = body.grants.iter().find(|grant| !known.contains(grant)) {
        return Err(AppError::Invalid(
            Say::of(say::NO_SUCH_GRANT).naming("grant", unknown),
        ));
    }

    // Nobody hands out more than they hold: a role that could be given
    // `settings:write` by somebody without it is a way around every other
    // check there is.
    let holding = &caller.require_user()?.grants;

    if let Some(beyond) = body.grants.iter().find(|grant| !holding.contains(*grant)) {
        return Err(AppError::Refused(
            Say::of(say::YOU_DO_NOT_HOLD_THAT_YOURSELF).naming("beyond", beyond),
        ));
    }

    let mut conn = state.db.begin().await?;

    let role: Role = sqlx::query_as(
        "insert into roles (key, name, grants) values ($1, $2, $3)
         returning id, key, name, grants, built_in",
    )
    .bind(&body.key)
    .bind(body.name.as_str())
    .bind(&body.grants)
    .fetch_one(conn.conn())
    .await
    .map_err(|error| {
        match error
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
        {
            Some(code) if code == "23505" => {
                AppError::Conflict(say::THERE_ALREADY_ROLE_BY_NAME.into())
            }
            Some(code) if code == "23514" => {
                AppError::Invalid(say::ROLE_S_KEY_LOWERCASE_LETTERS_UNDERSCORES.into())
            }
            _ => AppError::Database(error),
        }
    })?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "made a role",
        "role",
        Some(&role.id.to_string()),
        &serde_json::json!({ "key": role.key, "grants": role.grants }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, (StatusCode::CREATED, Json(role))))
}

/// Always the same answer, whether or not there is an account: whether an
/// address has one here is not a question this will answer.
async fn ask_to_reset(
    Injected(state): Injected<AppState>,
    caller: Caller,
    Json(body): Json<Address>,
) -> Result<Audited<StatusCode>> {
    ratelimit::spend(&state.db, &format!("reset:{}", body.email), RESET_LIMIT).await?;

    let mut conn = state.db.begin().await?;

    // Only to an address somebody has proved is theirs. An account whose
    // address was changed and not yet proved still has the link that proves it
    // waiting in the old one's inbox; sending a way in to the new one would
    // mean a mistyped address is a way to take an account over.
    let found: Option<(Uuid,)> = sqlx::query_as(
        "select id from users
          where email = $1 and state = 'active' and deleted_at is null
            and email_proved_at is not null",
    )
    .bind(body.email.as_str())
    .fetch_optional(conn.conn())
    .await?;

    if let Some((id,)) = found {
        let (secret, _) = a_ticket(&mut conn, id, "password_reset", &state).await?;

        let language = site_language(&mut conn).await?;
        let site = site_name(&mut conn).await?;

        let (subject, letter) = crate::mail::letters::press(
            &mut conn,
            "password",
            &language,
            &[
                // Nobody is named: this letter goes to an address, and saying
                // whose account it is to whoever asked would say that there is
                // one.
                ("name", String::new()),
                ("site", site),
                (
                    "link",
                    state.address.link(&format!("/forgotten?token={secret}")),
                ),
            ],
        )
        .await?;

        written_to(&mut conn, body.email.as_str(), &subject, &letter).await?;
    }

    // Written whether or not there was an account, so that the log says what
    // was asked rather than only what existed.
    let receipt = audit::record_raw(
        &mut conn,
        Actor::system(caller.request_id),
        "asked to reset a password",
        "person",
        found.map(|(id,)| id.to_string()).as_deref(),
        &serde_json::json!({ "email": body.email.as_str() }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::ACCEPTED))
}

/// What an invitation and a reset both end at. Spending the ticket is what
/// proves the address, so an invited account arrives proved.
async fn set_password(
    Injected(state): Injected<AppState>,
    caller: Caller,
    Json(body): Json<Chosen>,
) -> Result<Audited<StatusCode>> {
    if body.password.expose().chars().count() < 12 {
        return Err(AppError::Invalid(
            say::PASSWORD_AT_LEAST_TWELVE_CHARACTERS.into(),
        ));
    }

    let mut conn = state.db.begin().await?;

    let found: Option<(Uuid, Uuid)> = sqlx::query_as(
        "select id, user_id from tickets
          where token_hash = $1 and spent_at is null and expires_at > now()",
    )
    .bind(&token::hash(body.token.expose())[..])
    .fetch_optional(conn.conn())
    .await?;

    let Some((ticket, user_id)) = found else {
        return Err(AppError::Invalid(say::LINK_BEEN_USED_OR_RUN_OUT.into()));
    };

    sqlx::query("update tickets set spent_at = now() where id = $1")
        .bind(ticket)
        .execute(conn.conn())
        .await?;

    sqlx::query(
        "update users
            set password_hash = $2,
                state = 'active',
                email_proved_at = coalesce(email_proved_at, now())
          where id = $1",
    )
    .bind(user_id)
    .bind(password::hash(body.password.expose())?)
    .execute(conn.conn())
    .await?;

    // Whatever was open before a password changed is not open now.
    sqlx::query("update sessions set revoked_at = now() where user_id = $1 and revoked_at is null")
        .bind(user_id)
        .execute(conn.conn())
        .await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::system(caller.request_id),
        "chose a password",
        "person",
        Some(&user_id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::NO_CONTENT))
}
