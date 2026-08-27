//! A site-scoped, typed trash boundary.
//!
//! Trash is deliberately a small cross-domain application service. The
//! address may contain a kind from the HTTP path, but table names and labels
//! come only from [`TrashKind`]. Restoring keeps metadata and (for media)
//! bytes available; permanent deletion records a durable media cleanup task
//! before removing the metadata row.

use super::{JobKind, WorkflowScheduler};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Capability, Cursor, ErrorCode, JobId, MaviError, Page, PageRequest, Result, SiteContext,
};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

pub const TRASH_ITEM_NOT_FOUND: &str = "trash_item_not_found";
pub const TRASH_KIND_INVALID: &str = "trash_kind_invalid";
pub const TRASH_RESTORE_CONFLICT: &str = "trash_restore_conflict";
pub const TRASH_SHOP_PRODUCT_ACTIVE_HOLD: &str = "trash_shop_product_active_hold";
pub const TRASH_FLOW_ACTIVE_WORK: &str = "trash_flow_active_work";
pub const TRASH_RETENTION_JOB: JobKind = JobKind::new("trash.retention", 5);
pub const TRASH_RETENTION_BUCKET_SECONDS: i64 = 24 * 60 * 60;
pub const MAX_TRASH_RETENTION_BATCH: i64 = 100;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrashKind {
    Board,
    Flow,
    Course,
    Student,
    Form,
    Product,
    Coupon,
    Content,
    File,
    Term,
}

impl TrashKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Board => "board",
            Self::Flow => "flow",
            Self::Course => "course",
            Self::Student => "student",
            Self::Form => "form",
            Self::Product => "product",
            Self::Coupon => "coupon",
            Self::Content => "content",
            Self::File => "file",
            Self::Term => "term",
        }
    }

    #[must_use]
    pub const fn resource_type(self) -> &'static str {
        match self {
            Self::Board => "Board",
            Self::Flow => "AutomationFlow",
            Self::Course => "Course",
            Self::Student => "CourseStudent",
            Self::Form => "Form",
            Self::Product => "ShopProduct",
            Self::Coupon => "ShopCoupon",
            Self::Content => "Content",
            Self::File => "File",
            Self::Term => "TaxonomyTerm",
        }
    }

    #[must_use]
    const fn rank(self) -> i32 {
        match self {
            Self::Board => 10,
            Self::Flow => 9,
            Self::Course => 8,
            Self::Student => 7,
            Self::Product => 6,
            Self::Coupon => 5,
            Self::Form => 4,
            Self::Content => 3,
            Self::File => 2,
            Self::Term => 1,
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "board" => Ok(Self::Board),
            "flow" => Ok(Self::Flow),
            "course" => Ok(Self::Course),
            "student" => Ok(Self::Student),
            "form" => Ok(Self::Form),
            "product" => Ok(Self::Product),
            "coupon" => Ok(Self::Coupon),
            "content" => Ok(Self::Content),
            "file" => Ok(Self::File),
            "term" => Ok(Self::Term),
            _ => Err(MaviError::validation(TRASH_KIND_INVALID)),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TrashListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
    pub kind: Option<TrashKind>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TrashItem {
    pub kind: TrashKind,
    pub id: Uuid,
    pub label: String,
    pub deleted_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Default)]
pub struct PermanentDeletion {
    pub file_id: Option<Uuid>,
    pub file_storage_key: Option<String>,
}

/// Payload for the idempotent daily trash retention job.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrashRetentionJob {
    pub bucket: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExpiredTrashItem {
    pub kind: TrashKind,
    pub id: Uuid,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TrashCursor {
    deleted_at: DateTime<Utc>,
    kind_rank: i32,
    id: Uuid,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TrashService;

#[must_use]
pub fn api() -> mavi_contract::Api {
    mavi_contract::Api::new(endpoints()).with_shapes(shapes())
}

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::new(
            Method::Get,
            "/api/v1/trash",
            "trash.items.list",
            "List restorable site trash items with an opaque cursor",
        )
        .account_or_assistant()
        .requires(Permission::from_legacy(Capability::Trash, Action::View))
        .takes_query("TrashListFilter")
        .returns(200, "TrashPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/trash/{kind}/{id}/restore",
            "trash.items.restore",
            "Restore one item from site trash",
        )
        .account_or_assistant()
        .requires(Permission::from_legacy(Capability::Trash, Action::Write))
        .returns(204, "Empty")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Conflict,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Delete,
            "/api/v1/trash/{kind}/{id}",
            "trash.items.delete_permanently",
            "Permanently delete one item from site trash",
        )
        .account_or_assistant()
        .requires(Permission::from_legacy(Capability::Trash, Action::Delete))
        .returns(204, "Empty")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Conflict,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
    ]
}

#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "TrashKind",
            json!({"type": "string", "enum": ["board", "flow", "course", "student", "form", "product", "coupon", "content", "file", "term"]}),
        ),
        Shape::new(
            "TrashListFilter",
            json!({
                "type": "object",
                "properties": {
                    "after": {"type": ["string", "null"], "maxLength": 512},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                    "kind": {"$ref": "#/components/schemas/TrashKind"},
                },
            }),
        ),
        Shape::new(
            "TrashItem",
            json!({
                "type": "object",
                "required": ["kind", "id", "label", "deleted_at"],
                "properties": {
                    "kind": {"$ref": "#/components/schemas/TrashKind"},
                    "id": {"type": "string", "format": "uuid"},
                    "label": {"type": "string", "maxLength": 255},
                    "deleted_at": {"type": "string", "format": "date-time"},
                },
            }),
        ),
        Shape::new(
            "TrashPage",
            json!({
                "type": "object",
                "required": ["items", "next_cursor"],
                "properties": {
                    "items": {"type": "array", "items": {"$ref": "#/components/schemas/TrashItem"}},
                    "next_cursor": {"type": ["string", "null"], "maxLength": 512},
                },
            }),
        ),
    ]
}

impl TrashService {
    /// Enqueues one retention pass for the current UTC day. Discovery is
    /// idempotent, so the fixed-site worker can safely discover it again.
    pub async fn enqueue_retention_job(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        jobs: &WorkflowScheduler,
        now: DateTime<Utc>,
    ) -> Result<JobId> {
        let bucket = now.timestamp().div_euclid(TRASH_RETENTION_BUCKET_SECONDS);
        let payload =
            serde_json::to_value(TrashRetentionJob { bucket }).map_err(|_| MaviError::Internal)?;
        let idempotency_key = format!("trash:retention:{}:{}", context.site_id, bucket);
        jobs.enqueue(
            tx,
            context,
            TRASH_RETENTION_JOB.name,
            &payload,
            None,
            Some(&idempotency_key),
        )
        .await
    }

    /// Schedules the next bounded batch after a retention job consumed its
    /// full page. The claim id makes the continuation unique even when two
    /// workers discover the same daily job around a lease hand-off.
    pub async fn enqueue_retention_continuation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        jobs: &WorkflowScheduler,
        bucket: i64,
        parent_job: JobId,
    ) -> Result<JobId> {
        let payload =
            serde_json::to_value(TrashRetentionJob { bucket }).map_err(|_| MaviError::Internal)?;
        let idempotency_key = format!(
            "trash:retention:{}:{}:continuation:{}",
            context.site_id, bucket, parent_job
        );
        jobs.enqueue(
            tx,
            context,
            TRASH_RETENTION_JOB.name,
            &payload,
            None,
            Some(&idempotency_key),
        )
        .await
    }

    /// Returns a bounded, oldest-first batch of soft-deleted core records.
    /// Each candidate is locked again by [`Self::permanently_delete`]. This
    /// keeps discovery bounded while making the final delete the authority if
    /// a manual restore races with a worker poll.
    pub async fn expired(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        before: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<ExpiredTrashItem>> {
        if !(1..=MAX_TRASH_RETENTION_BATCH).contains(&limit) {
            return Err(MaviError::validation("trash_retention_batch_invalid"));
        }
        let rows = sqlx::query(
            "select kind, id
                 from (
                 select site_id, 'board'::text as kind, id, deleted_at,
                        10::int as kind_rank
                 from boards where deleted_at is not null
                 union all
                 select site_id, 'flow'::text as kind, id, deleted_at,
                        9::int as kind_rank
                 from automation_flows where deleted_at is not null
                 union all
                 select site_id, 'course'::text as kind, id, deleted_at,
                        8::int as kind_rank
                 from courses where deleted_at is not null
                 union all
                 select site_id, 'student'::text as kind, id, deleted_at,
                        7::int as kind_rank
                 from course_students where deleted_at is not null
                 union all
                 select site_id, 'form'::text as kind, id, deleted_at,
                        4::int as kind_rank
                 from forms where deleted_at is not null
                 union all
                 select site_id, 'product'::text as kind, id, deleted_at,
                        6::int as kind_rank
                 from shop_products where deleted_at is not null
                 union all
                 select site_id, 'coupon'::text as kind, id, deleted_at,
                        5::int as kind_rank
                   from shop_coupons where deleted_at is not null
                 union all
                 select site_id, 'content'::text as kind, id, deleted_at,
                        3::int as kind_rank
                   from content_entries where deleted_at is not null
                 union all
                 select site_id, 'file'::text as kind, id, deleted_at,
                        2::int as kind_rank
                   from media_files where deleted_at is not null
                 union all
                 select site_id, 'term'::text as kind, id, deleted_at,
                        1::int as kind_rank
                   from taxonomy_terms where deleted_at is not null
               ) as trashed
              where site_id = $1 and deleted_at < $2
              order by deleted_at asc, kind_rank asc, id asc
              limit $3",
        )
        .bind(context.site_id.into_uuid())
        .bind(before)
        .bind(limit)
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        rows.iter()
            .map(|row| {
                Ok(ExpiredTrashItem {
                    kind: TrashKind::parse(
                        &row.try_get::<String, _>("kind")
                            .map_err(|_| MaviError::Internal)?,
                    )?,
                    id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                })
            })
            .collect()
    }

    pub async fn list(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        filter: &TrashListFilter,
    ) -> Result<Page<TrashItem>> {
        let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select kind, id, label, deleted_at, kind_rank
               from (
                 select site_id, 'board'::text as kind, id, name as label,
                        deleted_at, 10::int as kind_rank
                 from boards where deleted_at is not null
                 union all
                 select site_id, 'flow'::text as kind, id, name as label,
                        deleted_at, 9::int as kind_rank
                 from automation_flows where deleted_at is not null
                 union all
                 select site_id, 'course'::text as kind, id, title as label,
                        deleted_at, 8::int as kind_rank
                 from courses where deleted_at is not null
                 union all
                 select site_id, 'student'::text as kind, id,
                        concat(name, ' <', email, '>') as label,
                        deleted_at, 7::int as kind_rank
                 from course_students where deleted_at is not null
                 union all
                 select site_id, 'form'::text as kind, id, name as label,
                        deleted_at, 4::int as kind_rank
                 from forms where deleted_at is not null
                 union all
                 select site_id, 'product'::text as kind, id, name as label,
                        deleted_at, 6::int as kind_rank
                 from shop_products where deleted_at is not null
                 union all
                 select site_id, 'coupon'::text as kind, id, code as label,
                        deleted_at, 5::int as kind_rank
                   from shop_coupons where deleted_at is not null
                 union all
                 select site_id, 'content'::text as kind, id, title as label,
                        deleted_at, 3::int as kind_rank
                   from content_entries where deleted_at is not null
                 union all
                 select site_id, 'file'::text as kind, id, name as label,
                        deleted_at, 2::int as kind_rank
                   from media_files where deleted_at is not null
                 union all
                 select site_id, 'term'::text as kind, id, name as label,
                        deleted_at, 1::int as kind_rank
                   from taxonomy_terms where deleted_at is not null
               ) as trashed
              where site_id = ",
        );
        query.push_bind(context.site_id.into_uuid());
        if let Some(kind) = filter.kind {
            query.push(" and kind = ").push_bind(kind.as_str());
        }
        if let Some(after) = after {
            query
                .push(" and (deleted_at < ")
                .push_bind(after.deleted_at)
                .push(" or (deleted_at = ")
                .push_bind(after.deleted_at)
                .push(" and kind_rank < ")
                .push_bind(after.kind_rank)
                .push(") or (deleted_at = ")
                .push_bind(after.deleted_at)
                .push(" and kind_rank = ")
                .push_bind(after.kind_rank)
                .push(" and id < ")
                .push_bind(after.id)
                .push("))");
        }
        let rows = query
            .push(" order by deleted_at desc, kind_rank desc, id desc limit ")
            .push_bind(limit + 1)
            .build()
            .fetch_all(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let mut items = rows.iter().map(from_row).collect::<Result<Vec<_>>>()?;
        let limit_usize = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if items.len() > limit_usize {
            let last = items
                .get(limit_usize.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_cursor(last.deleted_at, last.kind.rank(), last.id)?)
        } else {
            None
        };
        items.truncate(limit_usize);
        Ok(Page::new(items, next_cursor))
    }

    #[allow(clippy::too_many_lines)]
    pub async fn restore(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        kind: TrashKind,
        id: Uuid,
    ) -> Result<()> {
        let rows_affected = match kind {
            TrashKind::Board => {
                let result = sqlx::query(
                    "update boards
                            set deleted_at = null,
                                archived = trash_archived,
                                trash_archived = false,
                                updated_at = clock_timestamp()
                          where site_id = $1 and id = $2 and deleted_at is not null",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .execute(tx.conn())
                .await;
                let result = result.map_err(map_write_error)?;
                if result.rows_affected() > 0 {
                    sqlx::query(
                        "update board_lists
                                set deleted_at = null, updated_at = clock_timestamp()
                              where site_id = $1 and board_id = $2 and deleted_at is not null",
                    )
                    .bind(context.site_id.into_uuid())
                    .bind(id)
                    .execute(tx.conn())
                    .await
                    .map_err(map_write_error)?;
                    sqlx::query(
                        "update board_cards
                                set deleted_at = null, updated_at = clock_timestamp()
                              where site_id = $1 and board_id = $2 and deleted_at is not null",
                    )
                    .bind(context.site_id.into_uuid())
                    .bind(id)
                    .execute(tx.conn())
                    .await
                    .map_err(map_write_error)?;
                }
                result.rows_affected()
            }
            TrashKind::Flow => sqlx::query(
                "update automation_flows
                        set deleted_at = null,
                            enabled = trash_enabled,
                            trash_enabled = false,
                            updated_at = clock_timestamp()
                      where site_id = $1 and id = $2 and deleted_at is not null",
            )
            .bind(context.site_id.into_uuid())
            .bind(id)
            .execute(tx.conn())
            .await
            .map_err(map_write_error)?
            .rows_affected(),
            TrashKind::Course => sqlx::query(
                "update courses set deleted_at = null, updated_at = clock_timestamp()
                      where site_id = $1 and id = $2 and deleted_at is not null",
            )
            .bind(context.site_id.into_uuid())
            .bind(id)
            .execute(tx.conn())
            .await
            .map_err(map_write_error)?
            .rows_affected(),
            TrashKind::Student => sqlx::query(
                "update course_students set deleted_at = null, updated_at = clock_timestamp()
                      where site_id = $1 and id = $2 and deleted_at is not null",
            )
            .bind(context.site_id.into_uuid())
            .bind(id)
            .execute(tx.conn())
            .await
            .map_err(map_write_error)?
            .rows_affected(),
            TrashKind::Form => sqlx::query(
                "update forms set deleted_at = null, updated_at = clock_timestamp()
                      where site_id = $1 and id = $2 and deleted_at is not null",
            )
            .bind(context.site_id.into_uuid())
            .bind(id)
            .execute(tx.conn())
            .await
            .map_err(map_write_error)?
            .rows_affected(),
            TrashKind::Product => sqlx::query(
                "update shop_products set deleted_at = null, updated_at = clock_timestamp()
                      where site_id = $1 and id = $2 and deleted_at is not null",
            )
            .bind(context.site_id.into_uuid())
            .bind(id)
            .execute(tx.conn())
            .await
            .map_err(map_write_error)?
            .rows_affected(),
            TrashKind::Coupon => sqlx::query(
                "update shop_coupons set deleted_at = null, updated_at = clock_timestamp()
                      where site_id = $1 and id = $2 and deleted_at is not null",
            )
            .bind(context.site_id.into_uuid())
            .bind(id)
            .execute(tx.conn())
            .await
            .map_err(map_write_error)?
            .rows_affected(),
            TrashKind::Content => sqlx::query(
                "update content_entries set deleted_at = null, updated_at = clock_timestamp()
                      where site_id = $1 and id = $2 and deleted_at is not null",
            )
            .bind(context.site_id.into_uuid())
            .bind(id)
            .execute(tx.conn())
            .await
            .map_err(map_write_error)?
            .rows_affected(),
            TrashKind::File => sqlx::query(
                "update media_files set deleted_at = null
                      where site_id = $1 and id = $2 and deleted_at is not null",
            )
            .bind(context.site_id.into_uuid())
            .bind(id)
            .execute(tx.conn())
            .await
            .map_err(map_write_error)?
            .rows_affected(),
            TrashKind::Term => sqlx::query(
                "update taxonomy_terms set deleted_at = null, updated_at = clock_timestamp()
                      where site_id = $1 and id = $2 and deleted_at is not null",
            )
            .bind(context.site_id.into_uuid())
            .bind(id)
            .execute(tx.conn())
            .await
            .map_err(map_write_error)?
            .rows_affected(),
        };
        if rows_affected == 0 {
            return Err(MaviError::NotFound {
                resource: TRASH_ITEM_NOT_FOUND,
            });
        }
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "trash.item.restored".to_owned(),
                    resource_type: kind.resource_type().to_owned(),
                    resource_id: Some(id),
                    payload: json!({"kind": kind}),
                },
            )
            .await
    }

    #[allow(clippy::too_many_lines)]
    pub async fn permanently_delete(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        kind: TrashKind,
        id: Uuid,
    ) -> Result<PermanentDeletion> {
        let mut deletion = PermanentDeletion::default();
        ensure_trashed(tx, context, kind, id).await?;
        let payload = match kind {
            TrashKind::Board => {
                let list_count: i64 = sqlx::query_scalar(
                    "select count(*) from board_lists where site_id = $1 and board_id = $2",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .fetch_one(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                let card_count: i64 = sqlx::query_scalar(
                    "select count(*) from board_cards where site_id = $1 and board_id = $2",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .fetch_one(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                let comment_count: i64 = sqlx::query_scalar(
                    "select count(*) from board_comments where site_id = $1 and board_id = $2",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .fetch_one(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                let activity_count: i64 = sqlx::query_scalar(
                    "select count(*) from board_activity where site_id = $1 and board_id = $2",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .fetch_one(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                json!({
                    "kind": kind,
                    "list_count": list_count,
                    "card_count": card_count,
                    "comment_count": comment_count,
                    "activity_count": activity_count,
                })
            }
            TrashKind::Flow => {
                let active_run: bool = sqlx::query_scalar(
                    "select exists(
                         select 1 from automation_runs
                          where site_id = $1 and flow_id = $2
                            and state not in ('succeeded', 'failed')
                     )",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .fetch_one(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                if active_run {
                    return Err(MaviError::conflict(TRASH_FLOW_ACTIVE_WORK));
                }
                let running_job: bool = sqlx::query_scalar(
                    "select exists(
                         select 1
                           from workflow_outbox o
                           join workflow_runs r on r.site_id = o.site_id
                                                and r.idempotency_key = o.idempotency_key
                          where o.site_id = $1 and r.status = 'running'
                            and (
                                (o.workflow = 'automation.flow.start' and
                                 coalesce(o.payload->'job_payload'->>'flow_id', o.payload->>'flow_id') = $2)
                                or (o.workflow = 'automation.flow.step' and
                                    coalesce(o.payload->'job_payload'->>'run_id', o.payload->>'run_id') in (
                                    select id::text from automation_runs
                                     where site_id = $1 and flow_id = $3
                                ))
                            )
                     )",
                )
                .bind(context.site_id.into_uuid())
                .bind(id.to_string())
                .bind(id)
                .fetch_one(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                if running_job {
                    return Err(MaviError::conflict(TRASH_FLOW_ACTIVE_WORK));
                }
                let flow_step_count: i64 = sqlx::query_scalar(
                    "select count(*) from automation_flow_steps
                      where site_id = $1 and flow_id = $2",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .fetch_one(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                let run_count: i64 = sqlx::query_scalar(
                    "select count(*) from automation_runs
                      where site_id = $1 and flow_id = $2",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .fetch_one(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                let run_step_count: i64 = sqlx::query_scalar(
                    "select count(*) from automation_run_steps
                      where site_id = $1 and run_id in (
                          select id from automation_runs
                           where site_id = $1 and flow_id = $2
                      )",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .fetch_one(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                let job_count: i64 = sqlx::query_scalar(
                    "select count(*)
                       from workflow_outbox o
                       join workflow_runs r on r.site_id = o.site_id
                                            and r.idempotency_key = o.idempotency_key
                      where o.site_id = $1 and r.status <> 'running'
                        and (
                            (o.workflow = 'automation.flow.start' and
                             coalesce(o.payload->'job_payload'->>'flow_id', o.payload->>'flow_id') = $2)
                            or (o.workflow = 'automation.flow.step' and
                                coalesce(o.payload->'job_payload'->>'run_id', o.payload->>'run_id') in (
                                select id::text from automation_runs
                                 where site_id = $1 and flow_id = $3
                            ))
                        )",
                )
                .bind(context.site_id.into_uuid())
                .bind(id.to_string())
                .bind(id)
                .fetch_one(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                sqlx::query(
                    "delete from workflow_runs r
                      using workflow_outbox o
                      where r.site_id = $1 and r.site_id = o.site_id
                        and r.idempotency_key = o.idempotency_key
                        and r.status <> 'running'
                        and (
                            (o.workflow = 'automation.flow.start' and
                             coalesce(o.payload->'job_payload'->>'flow_id', o.payload->>'flow_id') = $2)
                            or (o.workflow = 'automation.flow.step' and
                                coalesce(o.payload->'job_payload'->>'run_id', o.payload->>'run_id') in (
                                select id::text from automation_runs
                                 where site_id = $1 and flow_id = $3
                            ))
                        )",
                )
                .bind(context.site_id.into_uuid())
                .bind(id.to_string())
                .bind(id)
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                sqlx::query(
                    "delete from workflow_outbox o
                      where o.site_id = $1
                        and not exists (
                            select 1 from workflow_runs r
                             where r.site_id = o.site_id
                               and r.idempotency_key = o.idempotency_key
                        )
                        and (
                            (o.workflow = 'automation.flow.start' and
                             coalesce(o.payload->'job_payload'->>'flow_id', o.payload->>'flow_id') = $2)
                            or (o.workflow = 'automation.flow.step' and
                                coalesce(o.payload->'job_payload'->>'run_id', o.payload->>'run_id') in (
                                select id::text from automation_runs
                                 where site_id = $1 and flow_id = $3
                            ))
                        )",
                )
                .bind(context.site_id.into_uuid())
                .bind(id.to_string())
                .bind(id)
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                json!({
                    "kind": kind,
                    "flow_step_count": flow_step_count,
                    "run_count": run_count,
                    "run_step_count": run_step_count,
                    "job_count": job_count,
                })
            }
            TrashKind::Form | TrashKind::Content | TrashKind::Term => json!({"kind": kind}),
            TrashKind::Course => {
                let module_count: i64 = sqlx::query_scalar(
                    "select count(*) from course_modules where site_id = $1 and course_id = $2",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .fetch_one(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                let lesson_count: i64 = sqlx::query_scalar(
                    "select count(*) from course_lessons l
                      join course_modules m on m.site_id = l.site_id and m.id = l.module_id
                      where l.site_id = $1 and m.course_id = $2",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .fetch_one(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                let enrollment_count: i64 = sqlx::query_scalar(
                    "select count(*) from course_enrollments where site_id = $1 and course_id = $2",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .fetch_one(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                json!({
                    "kind": kind,
                    "module_count": module_count,
                    "lesson_count": lesson_count,
                    "enrollment_count": enrollment_count,
                })
            }
            TrashKind::Student => {
                let enrollment_count: i64 = sqlx::query_scalar(
                    "select count(*) from course_enrollments where site_id = $1 and student_id = $2",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .fetch_one(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                let progress_count: i64 = sqlx::query_scalar(
                    "select count(*) from course_progress where site_id = $1 and student_id = $2",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .fetch_one(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                let session_count: i64 = sqlx::query_scalar(
                    "select count(*) from course_student_sessions where site_id = $1 and student_id = $2",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .fetch_one(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                json!({
                    "kind": kind,
                    "enrollment_count": enrollment_count,
                    "progress_count": progress_count,
                    "session_count": session_count,
                })
            }
            TrashKind::Product => {
                let active_hold: bool = sqlx::query_scalar(
                    "select exists(
                         select 1 from shop_stock_holds
                          where site_id = $1 and product_id = $2 and status = 'held'
                     )",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .fetch_one(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                if active_hold {
                    return Err(MaviError::conflict(TRASH_SHOP_PRODUCT_ACTIVE_HOLD));
                }
                let order_line_count: i64 = sqlx::query_scalar(
                    "select count(*) from shop_order_lines
                      where site_id = $1 and product_id = $2",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .fetch_one(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                let settled_hold_count: i64 = sqlx::query_scalar(
                    "select count(*) from shop_stock_holds
                      where site_id = $1 and product_id = $2 and status <> 'held'",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .fetch_one(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                json!({
                    "kind": kind,
                    "order_line_count": order_line_count,
                    "settled_hold_count": settled_hold_count,
                })
            }
            TrashKind::Coupon => {
                let use_count: i64 = sqlx::query_scalar(
                    "select count(*) from shop_coupon_uses
                      where site_id = $1 and coupon_id = $2",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .fetch_one(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                json!({"kind": kind, "use_count": use_count})
            }
            TrashKind::File => {
                let storage_key: String = sqlx::query_scalar(
                    "select storage_key from media_files
                      where site_id = $1 and id = $2 and deleted_at is not null
                      for update",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .fetch_optional(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?
                .ok_or(MaviError::NotFound {
                    resource: TRASH_ITEM_NOT_FOUND,
                })?;
                let variant_storage_keys: Vec<String> = sqlx::query_scalar(
                    "select storage_key from media_variants
                      where site_id = $1 and source_file_id = $2
                      order by id asc",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .fetch_all(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                sqlx::query(
                    "insert into media_cleanup_tasks
                        (site_id, file_id, storage_key, storage_keys)
                     values ($1, $2, $3, $4)
                     on conflict (site_id, file_id) do update
                       set storage_key = excluded.storage_key,
                           storage_keys = excluded.storage_keys,
                           completed_at = null",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .bind(&storage_key)
                .bind(&variant_storage_keys)
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                sqlx::query(
                    "delete from media_variants
                      where site_id = $1 and source_file_id = $2",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                deletion.file_id = Some(id);
                deletion.file_storage_key = Some(storage_key.clone());
                json!({
                    "kind": kind,
                    "storage_key": storage_key,
                    "variant_count": variant_storage_keys.len(),
                })
            }
        };

        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "trash.item.permanently_deleted".to_owned(),
                    resource_type: kind.resource_type().to_owned(),
                    resource_id: Some(id),
                    payload,
                },
            )
            .await?;

        match kind {
            TrashKind::Board => {
                sqlx::query(
                    "delete from boards
                      where site_id = $1 and id = $2 and deleted_at is not null",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
            }
            TrashKind::Flow => {
                sqlx::query(
                    "delete from automation_run_steps
                      where site_id = $1 and run_id in (
                          select id from automation_runs
                           where site_id = $1 and flow_id = $2
                      )",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                sqlx::query(
                    "delete from automation_runs
                      where site_id = $1 and flow_id = $2",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                sqlx::query(
                    "delete from automation_flow_steps
                      where site_id = $1 and flow_id = $2",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                sqlx::query(
                    "delete from automation_flows
                      where site_id = $1 and id = $2 and deleted_at is not null",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
            }
            TrashKind::Course => {
                sqlx::query(
                    "delete from courses where site_id = $1 and id = $2 and deleted_at is not null",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
            }
            TrashKind::Student => {
                sqlx::query(
                    "delete from course_students where site_id = $1 and id = $2 and deleted_at is not null",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
            }
            TrashKind::Form => {
                sqlx::query(
                    "delete from forms where site_id = $1 and id = $2 and deleted_at is not null",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
            }
            TrashKind::Product => {
                sqlx::query(
                    "update shop_order_lines set product_id = null
                      where site_id = $1 and product_id = $2",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                sqlx::query(
                    "delete from shop_stock_holds
                      where site_id = $1 and product_id = $2 and status <> 'held'",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                sqlx::query(
                    "delete from shop_products
                      where site_id = $1 and id = $2 and deleted_at is not null",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
            }
            TrashKind::Coupon => {
                sqlx::query(
                    "delete from shop_coupon_uses
                      where site_id = $1 and coupon_id = $2",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                sqlx::query(
                    "delete from shop_coupons
                      where site_id = $1 and id = $2 and deleted_at is not null",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
            }
            TrashKind::Content => {
                sqlx::query(
                    "delete from content_slug_history where site_id = $1 and content_id = $2",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                sqlx::query("delete from content_revisions where site_id = $1 and content_id = $2")
                    .bind(context.site_id.into_uuid())
                    .bind(id)
                    .execute(tx.conn())
                    .await
                    .map_err(|_| MaviError::Internal)?;
                sqlx::query("delete from content_entries where site_id = $1 and id = $2 and deleted_at is not null")
                    .bind(context.site_id.into_uuid())
                    .bind(id)
                    .execute(tx.conn())
                    .await
                    .map_err(|_| MaviError::Internal)?;
            }
            TrashKind::File => {
                sqlx::query("delete from media_files where site_id = $1 and id = $2 and deleted_at is not null")
                    .bind(context.site_id.into_uuid())
                    .bind(id)
                    .execute(tx.conn())
                    .await
                    .map_err(|_| MaviError::Internal)?;
            }
            TrashKind::Term => {
                sqlx::query("delete from taxonomy_terms where site_id = $1 and id = $2 and deleted_at is not null")
                    .bind(context.site_id.into_uuid())
                    .bind(id)
                    .execute(tx.conn())
                    .await
                    .map_err(|_| MaviError::Internal)?;
            }
        }
        Ok(deletion)
    }
}
#[allow(clippy::too_many_lines)]
async fn ensure_trashed(
    tx: &mut SiteTx,
    context: &SiteContext,
    kind: TrashKind,
    id: Uuid,
) -> Result<()> {
    let exists = match kind {
        TrashKind::Board => {
            sqlx::query_scalar::<_, Uuid>(
                "select id from boards
                  where site_id = $1 and id = $2 and deleted_at is not null
                  for update",
            )
            .bind(context.site_id.into_uuid())
            .bind(id)
            .fetch_optional(tx.conn())
            .await
        }
        TrashKind::Flow => {
            sqlx::query_scalar::<_, Uuid>(
                "select id from automation_flows
                  where site_id = $1 and id = $2 and deleted_at is not null
                  for update",
            )
            .bind(context.site_id.into_uuid())
            .bind(id)
            .fetch_optional(tx.conn())
            .await
        }
        TrashKind::Course => {
            sqlx::query_scalar::<_, Uuid>(
                "select id from courses
              where site_id = $1 and id = $2 and deleted_at is not null
              for update",
            )
            .bind(context.site_id.into_uuid())
            .bind(id)
            .fetch_optional(tx.conn())
            .await
        }
        TrashKind::Student => {
            sqlx::query_scalar::<_, Uuid>(
                "select id from course_students
              where site_id = $1 and id = $2 and deleted_at is not null
              for update",
            )
            .bind(context.site_id.into_uuid())
            .bind(id)
            .fetch_optional(tx.conn())
            .await
        }
        TrashKind::Form => {
            sqlx::query_scalar::<_, Uuid>(
                "select id from forms
              where site_id = $1 and id = $2 and deleted_at is not null
              for update",
            )
            .bind(context.site_id.into_uuid())
            .bind(id)
            .fetch_optional(tx.conn())
            .await
        }
        TrashKind::Product => {
            sqlx::query_scalar::<_, Uuid>(
                "select id from shop_products
              where site_id = $1 and id = $2 and deleted_at is not null
              for update",
            )
            .bind(context.site_id.into_uuid())
            .bind(id)
            .fetch_optional(tx.conn())
            .await
        }
        TrashKind::Coupon => {
            sqlx::query_scalar::<_, Uuid>(
                "select id from shop_coupons
              where site_id = $1 and id = $2 and deleted_at is not null
              for update",
            )
            .bind(context.site_id.into_uuid())
            .bind(id)
            .fetch_optional(tx.conn())
            .await
        }
        TrashKind::Content => {
            sqlx::query_scalar::<_, Uuid>(
                "select id from content_entries
              where site_id = $1 and id = $2 and deleted_at is not null
              for update",
            )
            .bind(context.site_id.into_uuid())
            .bind(id)
            .fetch_optional(tx.conn())
            .await
        }
        TrashKind::Term => {
            sqlx::query_scalar::<_, Uuid>(
                "select id from taxonomy_terms
              where site_id = $1 and id = $2 and deleted_at is not null
              for update",
            )
            .bind(context.site_id.into_uuid())
            .bind(id)
            .fetch_optional(tx.conn())
            .await
        }
        TrashKind::File => {
            sqlx::query_scalar::<_, Uuid>(
                "select id from media_files
              where site_id = $1 and id = $2 and deleted_at is not null
              for update",
            )
            .bind(context.site_id.into_uuid())
            .bind(id)
            .fetch_optional(tx.conn())
            .await
        }
    }
    .map_err(|_| MaviError::Internal)?;
    if exists.is_some() {
        Ok(())
    } else {
        Err(MaviError::NotFound {
            resource: TRASH_ITEM_NOT_FOUND,
        })
    }
}

fn from_row(row: &sqlx::postgres::PgRow) -> Result<TrashItem> {
    let kind = TrashKind::parse(row.try_get("kind").map_err(|_| MaviError::Internal)?)?;
    Ok(TrashItem {
        kind,
        id: row.try_get("id").map_err(|_| MaviError::Internal)?,
        label: row.try_get("label").map_err(|_| MaviError::Internal)?,
        deleted_at: row.try_get("deleted_at").map_err(|_| MaviError::Internal)?,
    })
}

fn map_write_error(error: sqlx::Error) -> MaviError {
    if let sqlx::Error::Database(database) = error
        && database.is_unique_violation()
    {
        return MaviError::conflict(TRASH_RESTORE_CONFLICT);
    }
    MaviError::Internal
}
fn encode_cursor(deleted_at: DateTime<Utc>, kind_rank: i32, id: Uuid) -> Result<Cursor> {
    let bytes = serde_json::to_vec(&TrashCursor {
        deleted_at,
        kind_rank,
        id,
    })
    .map_err(|_| MaviError::Internal)?;
    Cursor::parse(URL_SAFE_NO_PAD.encode(bytes))
}

fn decode_cursor(cursor: &Cursor) -> Result<TrashCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_str())
        .map_err(|_| MaviError::validation("invalid_cursor"))?;
    serde_json::from_slice(&bytes).map_err(|_| MaviError::validation("invalid_cursor"))
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_never_becomes_a_table_name_from_the_url() {
        assert!(TrashKind::parse("content; drop table content_entries").is_err());
        assert_eq!(
            TrashKind::parse("form").expect("form kind"),
            TrashKind::Form
        );
        assert_eq!(
            TrashKind::parse("product").expect("product kind"),
            TrashKind::Product
        );
        assert_eq!(
            TrashKind::parse("coupon").expect("coupon kind"),
            TrashKind::Coupon
        );
        assert_eq!(TrashKind::Content.resource_type(), "Content");
    }

    #[test]
    fn trash_cursor_and_contract_are_keyset_only() {
        let cursor =
            encode_cursor(Utc::now(), TrashKind::File.rank(), Uuid::now_v7()).expect("cursor");
        assert!(decode_cursor(&cursor).is_ok());
        let filter = shapes()
            .into_iter()
            .find(|shape| shape.name == "TrashListFilter")
            .expect("trash filter");
        let properties = filter.schema["properties"].as_object().expect("properties");
        assert!(properties.contains_key("after"));
        assert!(properties.contains_key("limit"));
        assert!(!properties.contains_key("offset"));
        assert!(!properties.contains_key("page"));
        assert_eq!(
            filter.schema["properties"],
            serde_json::json!({
                "after": {"type": ["string", "null"], "maxLength": 512},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                "kind": {"$ref": "#/components/schemas/TrashKind"}
            })
        );
        assert!(api().validate().is_ok());
    }
}
