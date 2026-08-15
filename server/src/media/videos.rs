//! A video, from the file that was uploaded to the thing that plays.
//!
//! Kept apart from the media library because the two are not the same: a
//! picture is finished when it has been uploaded, and a video is minutes of
//! somebody else's work away from being watchable, with a state that somebody
//! has to be able to read while that happens.

use axum::Json;
use axum::extract::{Path, Query as HttpQuery, State as Injected};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::kernel::audit::{self, Actor, Auditable, Audited};
use crate::kernel::authz::{Access, Capability, Needs, Permit};
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::page::{Page, Query, older_than};
use crate::kernel::queue::{self, Task};
use crate::kernel::ratelimit::Limit;
use crate::kernel::say;
use crate::kernel::tenant::TenantId;
use crate::kernel::transcoder::Handing;
use crate::kernel::types::Title;

fn media(access: Access) -> Needs {
    Needs::new(Capability::Media, access)
}

/// The transcoder answers on the site's own address, so this is public and
/// counted. What makes it not a way in is the signature.
const ANSWER_LIMIT: Limit = Limit::new(120, 60);

pub(super) fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/videos",
            Guard {
                audience: Audience::User,
                needs: Some(media(Access::View)),
                rate: RatePolicy::None,
            },
            list,
        )
        .gives::<Page<Video>>(),
        Endpoint::get(
            "/api/videos/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(media(Access::View)),
                rate: RatePolicy::None,
            },
            read,
        )
        .gives::<Video>(),
        Endpoint::post(
            "/api/videos",
            Guard {
                audience: Audience::User,
                needs: Some(media(Access::Write)),
                rate: RatePolicy::None,
            },
            add,
        )
        .takes::<NewVideo>()
        .gives::<Video>(),
        Endpoint::delete(
            "/api/videos/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(media(Access::Delete)),
                rate: RatePolicy::None,
            },
            remove,
        ),
        Endpoint::post(
            "/api/sites/videos/callback",
            Guard {
                audience: Audience::Public,
                needs: None,
                rate: RatePolicy::Per(ANSWER_LIMIT),
            },
            answered,
        )
        .takes::<Finished>(),
    ]
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, utoipa::ToSchema,
)]
#[sqlx(type_name = "video_state", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum VideoState {
    Waiting,
    Working,
    Ready,
    Failed,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Video {
    pub id: Uuid,
    pub media_id: Option<Uuid>,
    pub title: String,
    pub state: VideoState,
    pub seconds: Option<i32>,
    /// Where it plays, and in what sizes. Whatever transcoded it decides the
    /// shape of this; nothing here reads into it.
    pub plays: serde_json::Value,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl Auditable for Video {
    const SUBJECT: &'static str = "video";

    fn subject_id(&self) -> String {
        self.id.to_string()
    }

    fn summary(&self) -> serde_json::Value {
        serde_json::json!({ "title": self.title, "state": self.state })
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewVideo {
    /// The file it is made from, already uploaded. A video that is not a file
    /// on this machine is not something this can hand to anybody.
    pub media_id: Uuid,
    pub title: Title,
}

/// What a transcoder says when it has finished, or given up.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Finished {
    pub reference: String,
    pub ok: bool,
    #[serde(default)]
    pub seconds: Option<i32>,
    #[serde(default)]
    pub plays: Option<serde_json::Value>,
    #[serde(default)]
    pub note: Option<String>,
}

/// Handing one over is work rather than a request: the transcoder may be busy,
/// down, or slow, and none of those is a reason for an upload to fail.
#[derive(Debug, Serialize, Deserialize)]
pub struct HandOver {
    pub video: Uuid,
}

impl Task for HandOver {
    const KIND: &'static str = "videos.hand-over";
}

#[must_use]
pub fn kinds() -> Vec<String> {
    vec![HandOver::KIND.to_owned()]
}

const COLUMNS: &str = "id, media_id, title, state, seconds, plays, note, created_at";

async fn list(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    HttpQuery(page): HttpQuery<Query>,
) -> Result<Json<Page<Video>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let rows: Vec<Video> = sqlx::query_as(&format!(
        "select {COLUMNS} from videos
          where deleted_at is null
            and ($1::timestamptz is null or created_at < $1)
          order by created_at desc
          limit $2"
    ))
    .bind(older_than(page.after.as_deref()))
    .bind(page.fetch())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Page::build(&page, rows, |video| {
        video.created_at.to_rfc3339()
    })))
}

async fn read(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
) -> Result<Json<Video>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let found: Option<Video> = sqlx::query_as(&format!(
        "select {COLUMNS} from videos where id = $1 and deleted_at is null"
    ))
    .bind(id)
    .fetch_optional(conn.conn())
    .await?;

    conn.commit().await?;

    found.map(Json).ok_or(AppError::NotFound("video"))
}

async fn add(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Json(body): Json<NewVideo>,
) -> Result<Audited<(StatusCode, Json<Video>)>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let file: Option<(String,)> =
        sqlx::query_as("select mime from media where id = $1 and deleted_at is null")
            .bind(body.media_id)
            .fetch_optional(conn.conn())
            .await?;

    let (mime,) = file.ok_or(AppError::NotFound("file"))?;

    if !mime.starts_with("video/") {
        return Err(AppError::Invalid(say::THAT_FILE_IS_NOT_A_VIDEO.into()));
    }

    let video: Video = sqlx::query_as(&format!(
        "insert into videos (tenant_id, media_id, title)
         values ($1, $2, $3)
         returning {COLUMNS}"
    ))
    .bind(caller.tenant().0)
    .bind(body.media_id)
    .bind(body.title.as_str())
    .fetch_one(conn.conn())
    .await?;

    queue::enqueue(&mut conn, &HandOver { video: video.id }, None).await?;

    let receipt = audit::record(&mut conn, Actor::of(&caller), "added", None, Some(&video)).await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, (StatusCode::CREATED, Json(video))))
}

async fn remove(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
) -> Result<Audited<StatusCode>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let gone: Option<Video> = sqlx::query_as(&format!(
        "update videos set deleted_at = now()
          where id = $1 and deleted_at is null
         returning {COLUMNS}"
    ))
    .bind(id)
    .fetch_optional(conn.conn())
    .await?;

    let gone = gone.ok_or(AppError::NotFound("video"))?;

    let receipt = audit::record(
        &mut conn,
        Actor::of(&caller),
        "threw away",
        Some(&gone),
        None,
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::NO_CONTENT))
}

/// Hands one over, and says what happened.
///
/// A machine with nothing to hand it to says the file that was uploaded is what
/// plays, which is true for an MP4 a browser can already read — better than a
/// video left saying "working" for ever on a machine that will never work on it.
pub async fn hand_over(state: &AppState, tenant: TenantId, job: &queue::Job) -> Result<()> {
    let asked: HandOver = job.task()?;
    let mut conn = state.db.tenant(tenant).await?;

    let found: Option<(Option<Uuid>, String)> = sqlx::query_as(
        "select media_id, state::text from videos where id = $1 and deleted_at is null",
    )
    .bind(asked.video)
    .fetch_optional(conn.conn())
    .await?;

    // Thrown away, or already taken: either way this is not the run that does
    // it, and doing it twice is a second bill from whoever transcodes.
    let Some((media_id, state_now)) = found.filter(|(_, state)| state == "waiting") else {
        conn.commit().await?;
        return Ok(());
    };

    let _ = state_now;

    if !state.transcoder.works_on_it() {
        sqlx::query(
            "update videos
                set state = 'ready',
                    plays = jsonb_build_object('as_uploaded', $2::text)
              where id = $1",
        )
        .bind(asked.video)
        .bind(media_id.map(|id| format!("/uploads/{id}")))
        .execute(conn.conn())
        .await?;

        conn.commit().await?;
        return Ok(());
    }

    let source = media_id
        .map(|id| format!("/uploads/{id}"))
        .ok_or(AppError::Bug("a video with nothing to transcode"))?;

    let handing = Handing {
        tenant: tenant.0,
        video: asked.video,
        source,
    };

    match state.transcoder.hand_over(&handing).await {
        Ok(taken) => {
            sqlx::query("update videos set state = 'working', reference = $2 where id = $1")
                .bind(asked.video)
                .bind(&taken.reference)
                .execute(conn.conn())
                .await?;

            conn.commit().await?;
            Ok(())
        }
        Err(why) => {
            // Left waiting rather than failed: the queue will try again, and a
            // transcoder that was down for a minute is not a broken video.
            conn.commit().await?;
            Err(why)
        }
    }
}

/// What the transcoder says when it is done.
///
/// Signed, because this is reached on the site's own address by something that
/// is not signed in — and matched by the reference it was given, so that an
/// answer about somebody else's video is an answer about nothing.
async fn answered(
    Injected(state): Injected<AppState>,
    caller: Caller,
    headers: axum::http::HeaderMap,
    body: String,
) -> Result<Audited<StatusCode>> {
    let signature = headers
        .get("webhook-signature")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();

    if !state.transcoder_signature_holds(&body, signature) {
        return Err(AppError::Forbidden);
    }

    let said: Finished = serde_json::from_str(&body)
        .map_err(|_| AppError::Invalid(say::NOT_SOMETHING_A_TRANSCODER_SAYS.into()))?;

    let mut conn = state.db.tenant(caller.tenant()).await?;

    let changed: Option<Video> = sqlx::query_as(&format!(
        "update videos
            set state = case when $2 then 'ready'::video_state else 'failed'::video_state end,
                seconds = coalesce($3, seconds),
                plays = coalesce($4, plays),
                note = $5
          where reference = $1 and deleted_at is null and state = 'working'
         returning {COLUMNS}"
    ))
    .bind(&said.reference)
    .bind(said.ok)
    .bind(said.seconds)
    .bind(said.plays)
    .bind(said.note.as_deref())
    .fetch_optional(conn.conn())
    .await?;

    // A video thrown away while it was being worked on is a thing that
    // happens, and there is nothing for whoever answered to retry — but the
    // answer is still written down, because an answer about nothing arriving
    // repeatedly is somebody's transcoder talking to the wrong site.
    let receipt = audit::record_raw(
        &mut conn,
        Actor {
            id: None,
            kind: audit::ActorKind::System,
            request_id: caller.request_id,
        },
        "finished a video",
        "video",
        changed
            .as_ref()
            .map(|video| video.id.to_string())
            .as_deref(),
        &serde_json::json!({ "ok": said.ok, "known": changed.is_some() }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::NO_CONTENT))
}
