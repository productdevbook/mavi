//! Files a site has uploaded.
//!
//! What kind of file something is comes from its bytes rather than from its
//! name, and what a site may keep altogether is a limit the operator sets: a
//! full disk is every site on the machine rather than one.
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query as HttpQuery, State as Injected};
use axum::http::header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::kernel::audit::{self, Actor, Auditable, Audited};
use crate::kernel::authz::{Access, Capability, Needs, Permit};
use crate::kernel::db::TenantConn;
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::page::{Page, Query};
use crate::kernel::ratelimit::Limit;
use crate::kernel::say::{self, Say};
use crate::kernel::storage;
use crate::kernel::tenant::TenantId;

/// Twenty megabytes. Big enough for a photograph off a phone, small enough that
/// a hundred of them do not fill a disk somebody else is also using.
const MOST_BYTES: usize = 20 * 1024 * 1024;

/// What one site's library may come to, unless an operator has sold it more.
///
/// A limit on a single file and none on the total is a site filling the disk
/// one legal upload at a time — and a full disk on this machine is the kubelet
/// evicting Postgres, which is every site rather than one.
const MOST_BYTES_A_SITE: i64 = 5 * 1024 * 1024 * 1024;

/// What a visitor may ask for. Generous, because this is how a page's pictures
/// arrive, and still a limit, because it is a public endpoint.
const SERVE_LIMIT: Limit = Limit::new(600, 60);

fn needs(access: Access) -> Needs {
    Needs::new(Capability::Media, access)
}

mod videos;

pub use videos::{HandOver, NewVideo, Video, hand_over, kinds};

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/media",
            Guard {
                audience: Audience::User,
                needs: Some(needs(Access::View)),
                rate: RatePolicy::None,
            },
            list,
        )
        .gives::<Page<Media>>(),
        Endpoint::post(
            "/api/media",
            Guard {
                audience: Audience::User,
                needs: Some(needs(Access::Write)),
                rate: RatePolicy::None,
            },
            upload,
        )
        .gives::<Media>(),
        Endpoint::delete(
            "/api/media/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(needs(Access::Delete)),
                rate: RatePolicy::None,
            },
            remove,
        ),
        Endpoint::get(
            "/uploads/{id}",
            Guard {
                audience: Audience::Public,
                needs: None,
                rate: RatePolicy::Per(SERVE_LIMIT),
            },
            serve,
        ),
    ]
}

/// Everything the media library serves, and everything videos do.
#[must_use]
pub fn all_endpoints() -> Vec<Endpoint> {
    let mut all = endpoints();
    all.extend(videos::endpoints());
    all
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Media {
    pub id: Uuid,
    pub original_name: String,
    pub mime: String,
    pub bytes: i64,
    pub created_at: DateTime<Utc>,
}

impl Auditable for Media {
    const SUBJECT: &'static str = "media";

    fn subject_id(&self) -> String {
        self.id.to_string()
    }

    fn summary(&self) -> serde_json::Value {
        serde_json::json!({
            "name": self.original_name,
            "mime": self.mime,
            "bytes": self.bytes,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct Named {
    /// What to call it when it is handed back. Never where it is kept.
    pub name: Option<String>,
}

async fn list(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    HttpQuery(query): HttpQuery<Query>,
) -> Result<Json<Page<Media>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let rows: Vec<Media> = sqlx::query_as(
        "select id, original_name, mime, bytes, created_at
           from media
          where deleted_at is null
            and ($1::timestamptz is null or created_at < $1)
          order by created_at desc, id desc
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

    Ok(Json(Page::build(&query, rows, |file| {
        file.created_at.to_rfc3339()
    })))
}

/// Whether this site has room for what is arriving.
///
/// Counted rather than kept as a running total: a total that is written in one
/// place and decremented in another is a total that goes wrong the first time
/// something fails halfway, and a library is not big enough for the sum to be
/// slow.
async fn room_for(conn: &mut TenantConn, tenant: TenantId, arriving: usize) -> Result<()> {
    let held: (i64, Option<i64>) = sqlx::query_as(
        // `sum` of a bigint is numeric, which is not what this reads it as.
        "select coalesce((select sum(bytes)::bigint from media where deleted_at is null), 0),
                (select storage_limit_bytes from site_settings where tenant_id = $1)",
    )
    .bind(tenant.0)
    .fetch_one(conn.conn())
    .await?;

    let limit = held.1.unwrap_or(MOST_BYTES_A_SITE);
    let arriving = i64::try_from(arriving).unwrap_or(i64::MAX);

    if held.0.saturating_add(arriving) > limit {
        return Err(AppError::Refused(
            Say::of(say::THAT_SITE_HAS_NO_ROOM_LEFT)
                .naming("used", held.0)
                .naming("limit", limit),
        ));
    }

    Ok(())
}

/// The bytes as they arrive, with the name in the query rather than in the
/// body: what a file is called is a label, and what it is is the bytes.
async fn upload(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    HttpQuery(named): HttpQuery<Named>,
    body: Bytes,
) -> Result<Audited<(StatusCode, Json<Media>)>> {
    if body.len() > MOST_BYTES {
        return Err(AppError::Invalid(say::FILE_TOO_LARGE.into()));
    }

    if body.is_empty() {
        return Err(AppError::Invalid(say::FILE_EMPTY.into()));
    }

    // What the bytes say, not what the name claimed. A name ending in .png
    // proves nothing and is not what this is read from.
    let allowed =
        storage::sniff(&body).ok_or_else(|| AppError::Invalid(say::KIND_FILE_NOT_TAKEN.into()))?;

    let checksum: [u8; 32] = Sha256::digest(&body).into();
    let id = Uuid::now_v7();
    let location = format!("{}.{}", id.simple(), allowed.extension);

    let original_name = named
        .name
        .as_deref()
        .map_or_else(|| format!("file.{}", allowed.extension), clean_name);

    let mut conn = state.db.tenant(caller.tenant()).await?;

    // Asked before the bytes are written, so a site that is full does not fill
    // up further while being told it is full.
    room_for(&mut conn, caller.tenant(), body.len()).await?;

    state
        .store
        .put(caller.tenant(), &location, body.to_vec())
        .await?;

    let file: Media = sqlx::query_as(
        "insert into media
             (id, tenant_id, uploaded_by, location, original_name, mime, bytes, checksum)
         values ($1, $2, $3, $4, $5, $6, $7, $8)
         returning id, original_name, mime, bytes, created_at",
    )
    .bind(id)
    .bind(caller.tenant().0)
    .bind(caller.user.as_ref().map(|user| user.user_id))
    .bind(&location)
    .bind(&original_name)
    .bind(allowed.mime)
    .bind(i64::try_from(body.len()).unwrap_or(i64::MAX))
    .bind(&checksum[..])
    .fetch_one(conn.conn())
    .await?;

    let receipt =
        audit::record(&mut conn, Actor::of(&caller), "uploaded", None, Some(&file)).await?;
    conn.commit().await?;

    Ok(Audited::new(receipt, (StatusCode::CREATED, Json(file))))
}

/// What a person called it, with anything that is not a name taken out. It is
/// only ever put in a header, and a header is somewhere a newline matters.
fn clean_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|c| !c.is_control() && *c != '"' && *c != '\\' && *c != '/')
        .take(300)
        .collect();

    if cleaned.trim().is_empty() {
        "file".to_owned()
    } else {
        cleaned.trim().to_owned()
    }
}

async fn serve(
    Injected(state): Injected<AppState>,
    caller: Caller,
    Path(id): Path<Uuid>,
) -> Result<Response> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let found: Option<(String, String, String)> = sqlx::query_as(
        "select location, mime, original_name from media where id = $1 and deleted_at is null",
    )
    .bind(id)
    .fetch_optional(conn.conn())
    .await?;

    conn.commit().await?;

    let Some((location, mime, original_name)) = found else {
        return Err(AppError::NotFound("file"));
    };

    let allowed =
        storage::allowed_for(&mime).ok_or(AppError::Bug("a file of a kind nothing serves"))?;

    let bytes = state.store.get(caller.tenant(), &location).await?;

    let mut headers = HeaderMap::new();

    headers.insert(CONTENT_TYPE, HeaderValue::from_static(allowed.mime).clone());

    // Never guessed from the bytes by whoever is reading: the type above is
    // the one this machine decided, and nosniff is what makes it stick.
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );

    // A picture is shown; everything else is handed over. A file that is shown
    // where it stands is a file that runs where it stands, if it turns out to
    // be something other than what it said.
    let disposition = if allowed.inline {
        "inline".to_owned()
    } else {
        format!("attachment; filename=\"{}\"", clean_name(&original_name))
    };

    headers.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition)
            .map_err(|_| AppError::Bug("a name that cannot be a header"))?,
    );

    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );

    Ok((headers, bytes).into_response())
}

async fn remove(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
) -> Result<Audited<StatusCode>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;
    let before = one(&mut conn, id).await?;

    // The row goes first and the bytes after: a row pointing at nothing is a
    // broken picture, and bytes nothing points at are a file nobody can reach.
    sqlx::query("update media set deleted_at = now() where id = $1")
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

async fn one(conn: &mut TenantConn, id: Uuid) -> Result<Media> {
    sqlx::query_as(
        "select id, original_name, mime, bytes, created_at
           from media where id = $1 and deleted_at is null",
    )
    .bind(id)
    .fetch_optional(conn.conn())
    .await?
    .ok_or(AppError::NotFound("file"))
}
