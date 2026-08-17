//! A site-scoped, typed trash boundary.
//!
//! Trash is deliberately a small cross-domain application service. The
//! address may contain a kind from the HTTP path, but table names and labels
//! come only from [`TrashKind`]. Restoring keeps metadata and (for media)
//! bytes available; permanent deletion records a durable media cleanup task
//! before removing the metadata row.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Capability, Cursor, ErrorCode, MaviError, Page, PageRequest, Result, SiteContext,
};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

pub const TRASH_ITEM_NOT_FOUND: &str = "trash_item_not_found";
pub const TRASH_KIND_INVALID: &str = "trash_kind_invalid";
pub const TRASH_RESTORE_CONFLICT: &str = "trash_restore_conflict";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrashKind {
    Content,
    File,
    Term,
}

impl TrashKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Content => "content",
            Self::File => "file",
            Self::Term => "term",
        }
    }

    #[must_use]
    pub const fn resource_type(self) -> &'static str {
        match self {
            Self::Content => "Content",
            Self::File => "File",
            Self::Term => "TaxonomyTerm",
        }
    }

    #[must_use]
    const fn rank(self) -> i32 {
        match self {
            Self::Content => 3,
            Self::File => 2,
            Self::Term => 1,
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
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
        .requires(Permission {
            capability: Capability::Trash,
            action: Action::View,
        })
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
        .requires(Permission {
            capability: Capability::Trash,
            action: Action::Write,
        })
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
        .requires(Permission {
            capability: Capability::Trash,
            action: Action::Delete,
        })
        .returns(204, "Empty")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
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
            json!({"type": "string", "enum": ["content", "file", "term"]}),
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

    pub async fn restore(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        kind: TrashKind,
        id: Uuid,
    ) -> Result<()> {
        let result =
            match kind {
                TrashKind::Content => sqlx::query(
                    "update content_entries set deleted_at = null, updated_at = clock_timestamp()
                      where site_id = $1 and id = $2 and deleted_at is not null",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .execute(tx.conn())
                .await,
                TrashKind::File => {
                    sqlx::query(
                        "update media_files set deleted_at = null
                      where site_id = $1 and id = $2 and deleted_at is not null",
                    )
                    .bind(context.site_id.into_uuid())
                    .bind(id)
                    .execute(tx.conn())
                    .await
                }
                TrashKind::Term => sqlx::query(
                    "update taxonomy_terms set deleted_at = null, updated_at = clock_timestamp()
                      where site_id = $1 and id = $2 and deleted_at is not null",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .execute(tx.conn())
                .await,
            };
        let result = result.map_err(map_write_error)?;
        if result.rows_affected() == 0 {
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

    pub async fn permanently_delete(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        kind: TrashKind,
        id: Uuid,
    ) -> Result<PermanentDeletion> {
        let mut deletion = PermanentDeletion::default();
        let payload = match kind {
            TrashKind::Content | TrashKind::Term => json!({"kind": kind}),
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
                sqlx::query(
                    "insert into media_cleanup_tasks (site_id, file_id, storage_key)
                     values ($1, $2, $3)
                     on conflict (site_id, file_id) do update
                       set storage_key = excluded.storage_key,
                           completed_at = null",
                )
                .bind(context.site_id.into_uuid())
                .bind(id)
                .bind(&storage_key)
                .execute(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
                deletion.file_id = Some(id);
                deletion.file_storage_key = Some(storage_key.clone());
                json!({"kind": kind, "storage_key": storage_key})
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
            TrashKind::Content => {
                ensure_trashed(tx, context, TrashKind::Content, id).await?;
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
                ensure_trashed(tx, context, TrashKind::Term, id).await?;
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

async fn ensure_trashed(
    tx: &mut SiteTx,
    context: &SiteContext,
    kind: TrashKind,
    id: Uuid,
) -> Result<()> {
    let exists = match kind {
        TrashKind::Content => {
            sqlx::query_scalar::<_, bool>(
                "select exists(select 1 from content_entries
                    where site_id = $1 and id = $2 and deleted_at is not null)",
            )
            .bind(context.site_id.into_uuid())
            .bind(id)
            .fetch_one(tx.conn())
            .await
        }
        TrashKind::File => {
            sqlx::query_scalar::<_, bool>(
                "select exists(select 1 from media_files
                    where site_id = $1 and id = $2 and deleted_at is not null)",
            )
            .bind(context.site_id.into_uuid())
            .bind(id)
            .fetch_one(tx.conn())
            .await
        }
        TrashKind::Term => {
            sqlx::query_scalar::<_, bool>(
                "select exists(select 1 from taxonomy_terms
                    where site_id = $1 and id = $2 and deleted_at is not null)",
            )
            .bind(context.site_id.into_uuid())
            .bind(id)
            .fetch_one(tx.conn())
            .await
        }
    }
    .map_err(|_| MaviError::Internal)?;
    if exists {
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
        assert!(api().validate().is_ok());
    }
}
