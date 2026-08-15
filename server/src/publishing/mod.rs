//! Turning a site's design into the pages people read.
//!
//! Written to a draft, built somewhere to look at, and put live when somebody
//! says so. One publish at a time per site, said by a unique index rather than
//! by a lock in a process, and a build that fails leaves what is live alone —
//! because half a site is worse than an old one.
use axum::Json;
use axum::extract::{Query as HttpQuery, State as Injected};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::kernel::TenantId;
use crate::kernel::audit::{self, Actor, Audited};
use crate::kernel::authz::{Access, Capability, Needs, Permit};
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::page::{Page, Query as Paging, older_than};
use crate::kernel::queue::{self, Task};
use crate::kernel::ratelimit::Limit;
use crate::kernel::say;

/// What a theme file may be. Big enough for a page of markup, small enough that
/// a site cannot be used as a disk.
const MOST_BYTES: usize = 512 * 1024;

fn design(access: Access) -> Needs {
    Needs::new(Capability::Design, access)
}

/// Six an hour. A build is minutes of every core this machine has, and the
/// queue is one queue: a site publishing every time somebody saves would put
/// everybody else's site behind it. Publishing twice at once is refused
/// separately, by the database.
const PUBLISH_LIMIT: Limit = Limit::new(6, 3600);

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/design/files",
            Guard {
                audience: Audience::User,
                needs: Some(design(Access::View)),
                rate: RatePolicy::None,
            },
            files,
        )
        .gives::<Vec<File>>(),
        Endpoint::get(
            "/api/design/file",
            Guard {
                audience: Audience::User,
                needs: Some(design(Access::View)),
                rate: RatePolicy::None,
            },
            one_file,
        )
        .gives::<Written>(),
        Endpoint::put(
            "/api/design/files",
            Guard {
                audience: Audience::User,
                needs: Some(design(Access::Write)),
                rate: RatePolicy::None,
            },
            write_file,
        )
        .takes::<Writing>()
        .gives::<File>(),
        Endpoint::post(
            "/api/design/publishes",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::Publish, Access::Write)),
                rate: RatePolicy::Per(PUBLISH_LIMIT),
            },
            publish,
        )
        .gives::<Publish>(),
        Endpoint::post(
            "/api/design/publishes/{id}/cancel",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::Publish, Access::Write)),
                rate: RatePolicy::None,
            },
            cancel,
        )
        .gives::<Publish>(),
        Endpoint::get(
            "/api/design",
            Guard {
                audience: Audience::User,
                needs: Some(design(Access::View)),
                rate: RatePolicy::None,
            },
            design_status,
        )
        .gives::<Design>(),
        Endpoint::post(
            "/api/design/previews",
            Guard {
                audience: Audience::User,
                needs: Some(design(Access::Write)),
                rate: RatePolicy::Per(PUBLISH_LIMIT),
            },
            preview,
        )
        .gives::<Publish>(),
        Endpoint::get(
            "/api/design/previews",
            Guard {
                audience: Audience::User,
                needs: Some(design(Access::View)),
                rate: RatePolicy::None,
            },
            previews,
        )
        .gives::<Page<Publish>>(),
        Endpoint::get(
            "/api/design/publishes",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::Publish, Access::View)),
                rate: RatePolicy::None,
            },
            history,
        )
        .gives::<Page<Publish>>(),
    ]
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct File {
    pub path: String,
    pub branch: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, utoipa::ToSchema,
)]
#[sqlx(type_name = "publish_state", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum PublishState {
    Queued,
    Building,
    Live,
    Failed,
    Cancelled,
    Previewed,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Publish {
    pub id: Uuid,
    pub branch: String,
    pub state: PublishState,
    pub seconds: Option<i32>,
    /// What the build said, where it said anything. Read on the one day it
    /// matters: a build that failed and nothing saying why is a person
    /// guessing at somebody else's compiler.
    pub log: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// What is waiting to be looked at, and what has been.
#[derive(Clone, Debug, Serialize, utoipa::ToSchema)]
pub struct Design {
    /// What the draft has that live does not, or has differently. Empty means
    /// there is nothing to publish.
    pub changed: Vec<Change>,
    /// The last build of the draft made to look at, whatever became of it.
    pub preview: Option<Publish>,
    /// Where to look at it. Only while that preview is the newest one, because
    /// the address carries the build's own id.
    pub preview_at: Option<String>,
    pub live: Option<Publish>,
    /// A build going on now, so a screen can say so rather than poll blind.
    pub building: Option<Publish>,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Change {
    pub path: String,
    /// `added`, `changed` or `removed`, said against what is live.
    pub kind: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Writing {
    pub path: String,
    pub body: String,
    #[serde(default)]
    pub branch: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Which {
    pub branch: Option<String>,
}

async fn files(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    HttpQuery(which): HttpQuery<Which>,
) -> Result<Json<Vec<File>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let rows: Vec<File> = sqlx::query_as(
        "select path, branch, updated_at from theme_files
          where branch = coalesce($1, 'live') and deleted_at is null
          order by path",
    )
    .bind(which.branch.as_deref())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(rows))
}

/// One design file, whole.
#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Written {
    pub path: String,
    pub branch: String,
    pub body: String,
    pub updated_at: DateTime<Utc>,
}

/// What is in one file.
///
/// Asked for by name in the query rather than in the path: a theme's paths
/// have slashes in them, and a path that has to be escaped to be asked for is
/// one somebody gets wrong.
async fn one_file(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    HttpQuery(which): HttpQuery<Reading>,
) -> Result<Json<Written>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let found: Option<Written> = sqlx::query_as(
        "select path, branch, body, updated_at from theme_files
          where branch = coalesce($1, 'draft') and path = $2 and deleted_at is null",
    )
    .bind(which.branch.as_deref())
    .bind(&which.path)
    .fetch_optional(conn.conn())
    .await?;

    conn.commit().await?;

    found.map(Json).ok_or(AppError::NotFound("theme file"))
}

#[derive(Debug, Deserialize)]
pub struct Reading {
    pub path: String,
    pub branch: Option<String>,
}

/// Writing goes to a branch, and `live` is not one anybody writes to: what is
/// being served changes when a publish says so and not when somebody saves.
async fn write_file(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Json(body): Json<Writing>,
) -> Result<Audited<Json<File>>> {
    let branch = body.branch.as_deref().unwrap_or("draft");

    if branch == "live" {
        return Err(AppError::Refused(
            say::WHAT_BEING_SERVED_CHANGES_WHEN_PUBLISH.into(),
        ));
    }

    if body.body.len() > MOST_BYTES {
        return Err(AppError::Invalid(say::FILE_TOO_LARGE.into()));
    }

    // Only under src/ and public/. What decides how a site is built is not a
    // thing a site edits, and the database says so as well as this does.
    if !(body.path.starts_with("src/") || body.path.starts_with("public/"))
        || body.path.contains("..")
    {
        return Err(AppError::Refused(
            say::ONLY_WHAT_UNDER_SRC_PUBLIC_CAN.into(),
        ));
    }

    let mut conn = state.db.tenant(caller.tenant()).await?;

    let file: File = sqlx::query_as(
        "insert into theme_files (tenant_id, branch, path, body) values ($1, $2, $3, $4)
         on conflict (tenant_id, branch, path) where deleted_at is null do update
             set body = excluded.body, deleted_at = null
         returning path, branch, updated_at",
    )
    .bind(caller.tenant().0)
    .bind(branch)
    .bind(&body.path)
    .bind(&body.body)
    .fetch_one(conn.conn())
    .await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "wrote a theme file",
        "theme_file",
        Some(&body.path),
        &serde_json::json!({ "branch": branch, "bytes": body.body.len() }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, Json(file)))
}

/// What an assistant may do with a design, said once for all of it.
///
/// The same rules as the panel's own endpoints: only `src/` and `public/`, only
/// a branch that is not live, and looking at something is a build like any
/// other. Written here beside them rather than in the tool list, so that a rule
/// changed in one place is changed for both.
pub mod tools {
    use super::{Design, File, MOST_BYTES, Publish, Writing};
    use crate::kernel::audit::{self, Actor};
    use crate::kernel::error::{AppError, Result};
    use crate::kernel::http::{AppState, Caller};
    use crate::kernel::queue;
    use crate::kernel::say;

    /// What a branch holds. `draft` unless something says otherwise, because
    /// what is live is what a publish decided and not what a tool is working on.
    pub async fn files(
        state: &AppState,
        caller: &Caller,
        arguments: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let branch = branch_in(arguments);
        let mut conn = state.db.tenant(caller.tenant()).await?;

        let rows: Vec<File> = sqlx::query_as(
            "select path, branch, updated_at from theme_files
              where branch = $1 and deleted_at is null order by path",
        )
        .bind(&branch)
        .fetch_all(conn.conn())
        .await?;

        conn.commit().await?;

        Ok(serde_json::json!(rows))
    }

    pub async fn read(
        state: &AppState,
        caller: &Caller,
        arguments: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let branch = branch_in(arguments);

        let path = arguments
            .get("path")
            .and_then(serde_json::Value::as_str)
            .ok_or(AppError::NotFound("theme file"))?;

        let mut conn = state.db.tenant(caller.tenant()).await?;

        let found: Option<(String,)> = sqlx::query_as(
            "select body from theme_files
              where branch = $1 and path = $2 and deleted_at is null",
        )
        .bind(&branch)
        .bind(path)
        .fetch_optional(conn.conn())
        .await?;

        conn.commit().await?;

        let (body,) = found.ok_or(AppError::NotFound("theme file"))?;

        Ok(serde_json::json!({ "path": path, "branch": branch, "body": body }))
    }

    pub async fn write(
        state: &AppState,
        caller: &Caller,
        arguments: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let asked: Writing = serde_json::from_value(arguments.clone())
            .map_err(|_| AppError::Invalid(say::ONLY_WHAT_UNDER_SRC_PUBLIC_CAN.into()))?;

        let branch = asked.branch.clone().unwrap_or_else(|| "draft".to_owned());

        if branch == "live" {
            return Err(AppError::Refused(
                say::WHAT_BEING_SERVED_CHANGES_WHEN_PUBLISH.into(),
            ));
        }

        if asked.body.len() > MOST_BYTES {
            return Err(AppError::Invalid(say::FILE_TOO_LARGE.into()));
        }

        if !(asked.path.starts_with("src/") || asked.path.starts_with("public/"))
            || asked.path.contains("..")
        {
            return Err(AppError::Refused(
                say::ONLY_WHAT_UNDER_SRC_PUBLIC_CAN.into(),
            ));
        }

        let mut conn = state.db.tenant(caller.tenant()).await?;

        let file: File = sqlx::query_as(
            "insert into theme_files (tenant_id, branch, path, body) values ($1, $2, $3, $4)
             on conflict (tenant_id, branch, path) where deleted_at is null do update
                 set body = excluded.body, deleted_at = null
             returning path, branch, updated_at",
        )
        .bind(caller.tenant().0)
        .bind(&branch)
        .bind(&asked.path)
        .bind(&asked.body)
        .fetch_one(conn.conn())
        .await?;

        audit::record_raw(
            &mut conn,
            Actor::of(caller),
            "wrote a theme file",
            "theme_file",
            Some(&asked.path),
            &serde_json::json!({ "branch": branch, "bytes": asked.body.len() }),
        )
        .await?;

        conn.commit().await?;

        Ok(serde_json::json!(file))
    }

    /// Whether what has been written builds, and where to look at it.
    ///
    /// The answer is the build's own record: an assistant asks for one and then
    /// reads the same thing back until it says something other than building.
    pub async fn preview(
        state: &AppState,
        caller: &Caller,
        arguments: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let branch = branch_in(arguments);

        if branch == "live" {
            return Err(AppError::Refused(
                say::WHAT_BEING_SERVED_CHANGES_WHEN_PUBLISH.into(),
            ));
        }

        let mut conn = state.db.tenant(caller.tenant()).await?;

        let publish: Publish = sqlx::query_as(
            "insert into publishes (tenant_id, branch, preview) values ($1, $2, true)
             returning id, branch, state, seconds, log, created_at",
        )
        .bind(caller.tenant().0)
        .bind(&branch)
        .fetch_one(conn.conn())
        .await
        .map_err(super::already_building)?;

        queue::enqueue(
            &mut conn,
            &super::Build {
                publish_id: publish.id,
            },
            None,
        )
        .await?;

        audit::record_raw(
            &mut conn,
            Actor::of(caller),
            "asked to look at a design",
            "publish",
            Some(&publish.id.to_string()),
            &serde_json::json!({ "branch": branch }),
        )
        .await?;

        conn.commit().await?;

        Ok(serde_json::json!({
            "publish": publish,
            "at": crate::edge::preview_path(publish.id),
            "how": "read design_status until it is no longer building",
        }))
    }

    pub async fn status(
        state: &AppState,
        caller: &Caller,
        arguments: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let design: Design = super::design_now(state, caller, &branch_in(arguments)).await?;

        Ok(serde_json::json!(design))
    }

    fn branch_in(arguments: &serde_json::Value) -> String {
        arguments
            .get("branch")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("draft")
            .to_owned()
    }
}

/// What a publish would do, before anybody does it.
async fn design_status(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    HttpQuery(which): HttpQuery<Which>,
) -> Result<Json<Design>> {
    let branch = which.branch.unwrap_or_else(|| "draft".to_owned());

    Ok(Json(design_now(&state, &caller, &branch).await?))
}

/// What is waiting, whoever is asking — a panel screen or a tool.
async fn design_now(state: &AppState, caller: &Caller, branch: &str) -> Result<Design> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    // A full outer join rather than two queries: what is missing on either
    // side is as much a change as what differs, and one of them is the answer
    // to "is there anything to publish".
    let changed: Vec<Change> = sqlx::query_as(
        "select coalesce(d.path, l.path) as path,
                case when l.path is null then 'added'
                     when d.path is null then 'removed'
                     else 'changed' end as kind
           from (select path, body from theme_files
                  where branch = $1 and deleted_at is null) d
           full outer join (select path, body from theme_files
                             where branch = 'live' and deleted_at is null) l
             on l.path = d.path
          where l.path is null or d.path is null or d.body is distinct from l.body
          order by 1",
    )
    .bind(branch)
    .fetch_all(conn.conn())
    .await?;

    let preview: Option<Publish> = sqlx::query_as(
        "select id, branch, state, seconds, log, created_at
           from publishes where preview order by created_at desc limit 1",
    )
    .fetch_optional(conn.conn())
    .await?;

    let live: Option<Publish> = sqlx::query_as(
        "select id, branch, state, seconds, log, created_at
           from publishes where state = 'live'
          order by finished_at desc nulls last limit 1",
    )
    .fetch_optional(conn.conn())
    .await?;

    let building: Option<Publish> = sqlx::query_as(
        "select id, branch, state, seconds, log, created_at
           from publishes where state in ('queued', 'building')
          order by created_at desc limit 1",
    )
    .fetch_optional(conn.conn())
    .await?;

    conn.commit().await?;

    let preview_at = preview
        .as_ref()
        .filter(|publish| publish.state == PublishState::Previewed)
        .map(|publish| crate::edge::preview_path(publish.id));

    Ok(Design {
        changed,
        preview,
        preview_at,
        live,
        building,
    })
}

/// Builds the draft to look at, without putting it live.
async fn preview(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    HttpQuery(which): HttpQuery<Which>,
) -> Result<Audited<(StatusCode, Json<Publish>)>> {
    let branch = which.branch.unwrap_or_else(|| "draft".to_owned());

    if branch == "live" {
        return Err(AppError::Refused(
            say::WHAT_BEING_SERVED_CHANGES_WHEN_PUBLISH.into(),
        ));
    }

    let mut conn = state.db.tenant(caller.tenant()).await?;

    let publish: Publish = sqlx::query_as(
        "insert into publishes (tenant_id, branch, preview) values ($1, $2, true)
         returning id, branch, state, seconds, log, created_at",
    )
    .bind(caller.tenant().0)
    .bind(&branch)
    .fetch_one(conn.conn())
    .await
    .map_err(already_building)?;

    queue::enqueue(
        &mut conn,
        &Build {
            publish_id: publish.id,
        },
        None,
    )
    .await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "asked to look at a design",
        "publish",
        Some(&publish.id.to_string()),
        &serde_json::json!({ "branch": branch }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, (StatusCode::ACCEPTED, Json(publish))))
}

fn already_building(error: sqlx::Error) -> AppError {
    match error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
    {
        Some(code) if code == "23505" => {
            AppError::Conflict(say::SITE_ALREADY_BEING_PUBLISHED.into())
        }
        _ => AppError::Database(error),
    }
}

/// Puts a branch live, by way of a build.
///
/// One at a time per site, said by a partial unique index rather than by a
/// lock in a process: two publishes of one site racing each other is two builds
/// writing the same output.
async fn publish(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    HttpQuery(which): HttpQuery<Which>,
) -> Result<Audited<(StatusCode, Json<Publish>)>> {
    let branch = which.branch.unwrap_or_else(|| "draft".to_owned());
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let publish: Publish = sqlx::query_as(
        "insert into publishes (tenant_id, branch) values ($1, $2)
         returning id, branch, state, seconds, log, created_at",
    )
    .bind(caller.tenant().0)
    .bind(&branch)
    .fetch_one(conn.conn())
    .await
    .map_err(already_building)?;

    queue::enqueue(
        &mut conn,
        &Build {
            publish_id: publish.id,
        },
        None,
    )
    .await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "published",
        "publish",
        Some(&publish.id.to_string()),
        &serde_json::json!({ "branch": branch }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, (StatusCode::ACCEPTED, Json(publish))))
}

/// Says not to. A publish that has not started never runs; one that is
/// building has what comes back thrown away rather than put live.
async fn cancel(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> Result<Audited<Json<Publish>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let cancelled: Option<Publish> = sqlx::query_as(
        "update publishes
            set state = 'cancelled', cancelled_at = now(), finished_at = now()
          where id = $1 and state in ('queued', 'building')
         returning id, branch, state, seconds, log, created_at",
    )
    .bind(id)
    .fetch_optional(conn.conn())
    .await?;

    let Some(cancelled) = cancelled else {
        return Err(AppError::Conflict(say::PUBLISH_ALREADY_FINISHED.into()));
    };

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "cancelled a publish",
        "publish",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, Json(cancelled)))
}

/// The builds made to look at, newest first.
async fn previews(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    HttpQuery(page): HttpQuery<Paging>,
) -> Result<Json<Page<Publish>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let rows: Vec<Publish> = sqlx::query_as(
        "select id, branch, state, seconds, log, created_at
           from publishes
          where preview and ($1::timestamptz is null or created_at < $1)
          order by created_at desc
          limit $2",
    )
    .bind(older_than(page.after.as_deref()))
    .bind(page.fetch())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Page::build(&page, rows, |publish| {
        publish.created_at.to_rfc3339()
    })))
}

async fn history(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    axum::extract::Query(page): axum::extract::Query<Paging>,
) -> Result<Json<Page<Publish>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let rows: Vec<Publish> = sqlx::query_as(
        "select id, branch, state, seconds, log, created_at
           from publishes
          where ($1::timestamptz is null or created_at < $1)
          order by created_at desc
          limit $2",
    )
    .bind(older_than(page.after.as_deref()))
    .bind(page.fetch())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Page::build(&page, rows, |publish| {
        publish.created_at.to_rfc3339()
    })))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Build {
    pub publish_id: Uuid,
}

impl Task for Build {
    const KIND: &'static str = "publish.build";
}

#[must_use]
pub fn kinds() -> Vec<String> {
    vec![Build::KIND.to_owned()]
}

/// A publish, from claimed to live or to failed.
///
/// The building itself is handed to whatever is configured to do it; this is
/// the part that has to be right whichever builder runs — one at a time,
/// recorded, cancellable, and billed for the seconds it took. A build that
/// fails leaves what is live alone, because half a site is worse than an old
/// one.
/// Whoever builds: this process where a generator is configured for it,
/// something on the network where one is, and nobody at all — in which case the
/// files are the site.
async fn whoever_builds(
    state: &AppState,
    tenant: TenantId,
    publish: Uuid,
    branch: &str,
) -> Result<crate::kernel::builder::Built> {
    match &*state.builder {
        crate::kernel::builder::Builder::Here(generator) => {
            here(state, tenant, publish, branch, generator).await
        }
        builder => {
            builder
                .build(&crate::kernel::builder::Building {
                    tenant: tenant.0,
                    branch: branch.to_owned(),
                    publish,
                })
                .await
        }
    }
}

/// A build run by this process, in a workspace of its own.
async fn here(
    state: &AppState,
    tenant: TenantId,
    publish: Uuid,
    branch: &str,
    generator: &crate::building::Generator,
) -> Result<crate::kernel::builder::Built> {
    let mut conn = state.db.tenant(tenant).await?;

    let files: Vec<(String, String)> = sqlx::query_as(
        "select path, body from theme_files where branch = $1 and deleted_at is null",
    )
    .bind(branch)
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    let made = crate::building::run(generator, tenant, publish, files).await?;

    // A build that produced nothing is a build that failed, whatever it said
    // on the way: putting an empty folder live is a site that has gone.
    if made.files.is_empty() {
        return Ok(crate::kernel::builder::Built {
            ok: false,
            log: made.log,
        });
    }

    for (path, bytes) in made.files {
        state
            .store
            .put(tenant, &crate::edge::at(publish, &path), bytes)
            .await?;
    }

    Ok(crate::kernel::builder::Built {
        ok: true,
        log: made.log,
    })
}

/// A site with no generator: its files are its pages, so they are copied into
/// the store under this publish's own id, where the edge reads them.
async fn put_the_files_where_they_are_served(
    state: &AppState,
    tenant: TenantId,
    conn: &mut crate::kernel::db::TenantConn,
    publish: Uuid,
    branch: &str,
) -> Result<()> {
    let files: Vec<(String, String)> = sqlx::query_as(
        "select path, body from theme_files where branch = $1 and deleted_at is null",
    )
    .bind(branch)
    .fetch_all(conn.conn())
    .await?;

    for (path, body) in files {
        // Only what a theme keeps in public is a page. `src/` is what a
        // generator would have read, and serving it would put a site's source
        // on its own address.
        let Some(served) = path.strip_prefix("public/") else {
            continue;
        };

        state
            .store
            .put(tenant, &crate::edge::at(publish, served), body.into_bytes())
            .await?;
    }

    Ok(())
}

pub async fn build(state: &AppState, tenant: TenantId, task: &Build) -> Result<()> {
    let started = state.clock.now();
    let mut conn = state.db.tenant(tenant).await?;

    let publish: Option<(String, bool)> = sqlx::query_as(
        "update publishes set state = 'building', started_at = now()
          where id = $1 and state = 'queued'
         returning branch, preview",
    )
    .bind(task.publish_id)
    .fetch_optional(conn.conn())
    .await?;

    let Some((branch, preview)) = publish else {
        return Ok(());
    };

    // The claim is committed before the build starts, so that the panel can
    // see it is building and so that a cancel has something to cancel.
    conn.commit().await?;

    let built = whoever_builds(state, tenant, task.publish_id, &branch).await?;

    let mut conn = state.db.tenant(tenant).await?;

    // Cancelled while it was building: what came back is thrown away rather
    // than put live, because somebody said not to.
    let still_wanted: Option<(String,)> =
        sqlx::query_as("select state::text from publishes where id = $1")
            .bind(task.publish_id)
            .fetch_optional(conn.conn())
            .await?;

    if still_wanted.is_none_or(|(state,)| state != "building") {
        conn.commit().await?;
        return Ok(());
    }

    let seconds = (state.clock.now() - started).num_seconds().max(0);

    if !built.ok {
        // What is live stays live. Half a site is worse than an old one.
        sqlx::query(
            "update publishes
                set state = 'failed', finished_at = now(), seconds = $2, log = $3
              where id = $1",
        )
        .bind(task.publish_id)
        .bind(i32::try_from(seconds).unwrap_or(i32::MAX))
        .bind(&built.log)
        .execute(conn.conn())
        .await?;

        conn.commit().await?;

        return Ok(());
    }

    // What is served is what came out of the build, under the id of the publish
    // that made it. A builder somewhere else has already written its own; with
    // nothing to build with, the files are the site and this is what writes
    // them.
    if !state.builder.builds_elsewhere() {
        put_the_files_where_they_are_served(state, tenant, &mut conn, task.publish_id, &branch)
            .await?;
    }

    // A preview is a build somebody looks at. What it made stays where the edge
    // can serve it under its own id, and what is live is not touched: the whole
    // point of looking first is that nothing has happened yet.
    if preview {
        sqlx::query(
            "update publishes
                set state = 'previewed', finished_at = now(), seconds = $2, log = $3
              where id = $1",
        )
        .bind(task.publish_id)
        .bind(i32::try_from(seconds).unwrap_or(i32::MAX))
        .bind(&built.log)
        .execute(conn.conn())
        .await?;

        conn.commit().await?;

        return Ok(());
    }

    // Everything on the branch becomes what is live, and anything live that the
    // branch does not have goes. A publish is the whole of a site, not a patch
    // to it.
    sqlx::query("delete from theme_files where branch = 'live'")
        .execute(conn.conn())
        .await?;

    let files = sqlx::query(
        "insert into theme_files (tenant_id, branch, path, body)
         select tenant_id, 'live', path, body from theme_files
          where branch = $1 and deleted_at is null",
    )
    .bind(&branch)
    .execute(conn.conn())
    .await?
    .rows_affected();

    sqlx::query(
        "update publishes
            set state = 'live', finished_at = now(), seconds = $2, files = $3, log = $4
          where id = $1",
    )
    .bind(task.publish_id)
    .bind(i32::try_from(seconds).unwrap_or(i32::MAX))
    .bind(i32::try_from(files).unwrap_or(i32::MAX))
    .bind(&built.log)
    .execute(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(())
}
