//! Read-only revision history for site content.
//!
//! Revisions are written by the content application service in the same
//! transaction as the current row. This module only exposes that immutable
//! history; restoring an old revision is an explicit update command, not a
//! hidden side effect of reading it.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Capability, ContentId, Cursor, ErrorCode, MaviError, Page, PageRequest, Result,
    SiteContext,
};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;

use super::{ContentKind, LanguageTag, Publication, Slug, status};

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::new(
            Method::Get,
            "/api/v1/content/{id}/revisions",
            "content.revisions.list",
            "List content revisions",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Content,
            action: Action::View,
        })
        .takes_query("ContentRevisionListFilter")
        .returns(200, "ContentRevisionPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/content/{id}/revisions/{revision}",
            "content.revisions.read",
            "Read one content revision",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Content,
            action: Action::View,
        })
        .returns(200, "ContentRevision")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/content/{id}/revisions/{revision}/restore",
            "content.revisions.restore",
            "Restore a content revision as a new draft",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Content,
            action: Action::Write,
        })
        .returns(200, "Content")
        .changes(true)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::NotFound,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
    ]
}

#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "ContentRevisionListFilter",
            json!({
                "type": "object",
                "properties": {
                    "after": {"type": ["string", "null"], "maxLength": 512},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                },
            }),
        ),
        Shape::new(
            "ContentRevision",
            json!({
                "type": "object",
                "required": ["content_id", "revision", "kind", "language", "slug", "title", "excerpt", "body", "fields", "publication", "created_at"],
                "properties": {
                    "content_id": {"type": "string", "format": "uuid"},
                    "revision": {"type": "integer", "minimum": 1},
                    "kind": {"type": "string", "maxLength": 31},
                    "language": {"type": "string", "maxLength": 35},
                    "slug": {"type": "string", "maxLength": 160},
                    "title": {"type": "string", "maxLength": 200},
                    "excerpt": {"type": ["string", "null"]},
                    "body": {"type": "string"},
                    "fields": {"type": "object", "additionalProperties": true},
                    "publication": {"$ref": "#/components/schemas/Publication"},
                    "created_at": {"type": "string", "format": "date-time"},
                },
            }),
        ),
        Shape::new(
            "ContentRevisionPage",
            json!({
                "type": "object",
                "required": ["items", "next_cursor"],
                "properties": {
                    "items": {"type": "array", "items": {"$ref": "#/components/schemas/ContentRevision"}},
                    "next_cursor": {"type": ["string", "null"], "maxLength": 512},
                },
            }),
        ),
    ]
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContentRevisionListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
}

#[derive(Clone, Debug, Serialize)]
pub struct ContentRevision {
    pub content_id: ContentId,
    pub revision: u32,
    pub kind: ContentKind,
    pub language: LanguageTag,
    pub slug: Slug,
    pub title: String,
    pub excerpt: Option<String>,
    pub body: String,
    pub fields: Value,
    pub publication: Publication,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RevisionCursor {
    created_at: DateTime<Utc>,
    revision: i32,
}

fn encode_cursor(created_at: DateTime<Utc>, revision: i32) -> Result<Cursor> {
    let bytes = serde_json::to_vec(&RevisionCursor {
        created_at,
        revision,
    })
    .map_err(|_| MaviError::Internal)?;
    Cursor::parse(URL_SAFE_NO_PAD.encode(bytes))
}

fn decode_cursor(cursor: &Cursor) -> Result<RevisionCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_str())
        .map_err(|_| MaviError::validation("invalid_cursor"))?;
    serde_json::from_slice(&bytes).map_err(|_| MaviError::validation("invalid_cursor"))
}

fn from_row(row: &sqlx::postgres::PgRow) -> Result<ContentRevision> {
    let status_value: String = row.try_get("status").map_err(|_| MaviError::Internal)?;
    let scheduled_at = row
        .try_get("scheduled_at")
        .map_err(|_| MaviError::Internal)?;
    let published_at = row
        .try_get("published_at")
        .map_err(|_| MaviError::Internal)?;
    let revision: i32 = row.try_get("revision").map_err(|_| MaviError::Internal)?;
    let fields: Value = row.try_get("fields").map_err(|_| MaviError::Internal)?;
    Ok(ContentRevision {
        content_id: ContentId::from_uuid(
            row.try_get("content_id").map_err(|_| MaviError::Internal)?,
        ),
        revision: u32::try_from(revision).map_err(|_| MaviError::Internal)?,
        kind: ContentKind::parse(
            &row.try_get::<String, _>("kind")
                .map_err(|_| MaviError::Internal)?,
        )?,
        language: LanguageTag::parse(
            &row.try_get::<String, _>("language")
                .map_err(|_| MaviError::Internal)?,
        )?,
        slug: Slug::parse(
            &row.try_get::<String, _>("slug")
                .map_err(|_| MaviError::Internal)?,
        )?,
        title: row.try_get("title").map_err(|_| MaviError::Internal)?,
        excerpt: row.try_get("excerpt").map_err(|_| MaviError::Internal)?,
        body: row.try_get("body").map_err(|_| MaviError::Internal)?,
        fields,
        publication: status(&status_value, scheduled_at, published_at)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
    })
}

async fn ensure_content(tx: &mut SiteTx, context: &SiteContext, id: ContentId) -> Result<()> {
    let exists: bool = sqlx::query_scalar(
        "select exists(select 1 from content_entries where site_id = $1 and id = $2 and deleted_at is null)",
    )
    .bind(context.site_id.into_uuid())
    .bind(id.into_uuid())
    .fetch_one(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    if exists {
        Ok(())
    } else {
        Err(MaviError::NotFound {
            resource: super::CONTENT_NOT_FOUND,
        })
    }
}

pub(super) async fn list(
    tx: &mut SiteTx,
    context: &SiteContext,
    id: ContentId,
    filter: &ContentRevisionListFilter,
) -> Result<Page<ContentRevision>> {
    ensure_content(tx, context, id).await?;
    let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
    let limit = i64::from(filter.page.effective_limit());
    let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "select content_id, revision, kind, language, slug, title, excerpt, body, fields, status, scheduled_at, published_at, created_at
           from content_revisions where site_id = ",
    );
    query.push_bind(context.site_id.into_uuid());
    query.push(" and content_id = ").push_bind(id.into_uuid());
    if let Some(after) = after {
        query
            .push(" and (created_at, revision) < (")
            .push_bind(after.created_at)
            .push(", ")
            .push_bind(after.revision)
            .push(")");
    }
    let rows = query
        .push(" order by created_at desc, revision desc limit ")
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
        Some(encode_cursor(
            last.created_at,
            i32::try_from(last.revision).map_err(|_| MaviError::Internal)?,
        )?)
    } else {
        None
    };
    items.truncate(limit_usize);
    Ok(Page::new(items, next_cursor))
}

pub(super) async fn read(
    tx: &mut SiteTx,
    context: &SiteContext,
    id: ContentId,
    revision: u32,
) -> Result<ContentRevision> {
    ensure_content(tx, context, id).await?;
    let revision =
        i32::try_from(revision).map_err(|_| MaviError::validation("invalid_revision"))?;
    let row = sqlx::query(
        "select content_id, revision, kind, language, slug, title, excerpt, body, fields, status, scheduled_at, published_at, created_at
           from content_revisions where site_id = $1 and content_id = $2 and revision = $3",
    )
    .bind(context.site_id.into_uuid())
    .bind(id.into_uuid())
    .bind(revision)
    .fetch_optional(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?
    .ok_or(MaviError::NotFound {
        resource: "content_revision_not_found",
    })?;
    from_row(&row)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_cursor_round_trips_and_contract_is_valid() {
        let created_at = Utc::now();
        let cursor = encode_cursor(created_at, 3).expect("cursor");
        let decoded = decode_cursor(&cursor).expect("decoded");
        assert_eq!(decoded.created_at, created_at);
        assert_eq!(decoded.revision, 3);
        assert!(decode_cursor(&Cursor::parse("bad").expect("cursor")).is_err());
        assert!(endpoints()[0].request.is_some());
    }
}
