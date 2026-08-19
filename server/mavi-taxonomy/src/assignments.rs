//! Atomic content-to-term assignment commands and filtered membership reads.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Capability, ContentId, Cursor, ErrorCode, MaviError, Page, PageRequest, Result,
    SiteContext, TermId,
};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use super::terms::{self, TERM_NOT_FOUND, Term};

pub const CONTENT_NOT_FOUND: &str = "taxonomy_content_not_found";

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::new(
            Method::Get,
            "/api/v1/content/{id}/terms",
            "taxonomy.content_terms.list",
            "List terms assigned to content",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Taxonomy,
            action: Action::View,
        })
        .returns(200, "TermList")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Put,
            "/api/v1/content/{id}/terms",
            "taxonomy.content_terms.replace",
            "Replace terms assigned to content",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Taxonomy,
            action: Action::Write,
        })
        .takes("ReplaceContentTerms")
        .returns(200, "TermList")
        .changes(true)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/terms/{id}/content",
            "taxonomy.term_content.list",
            "List content assigned to a term",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Taxonomy,
            action: Action::View,
        })
        .takes_query("ContentTermAssignmentListFilter")
        .returns(200, "ContentTermAssignmentPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
    ]
}

#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "TermList",
            json!({
                "type": "array",
                "items": {"$ref": "#/components/schemas/Term"},
            }),
        ),
        Shape::new(
            "ReplaceContentTerms",
            json!({
                "type": "object",
                "required": ["term_ids"],
                "properties": {
                    "term_ids": {"type": "array", "maxItems": 100, "items": {"type": "string", "format": "uuid"}},
                },
            }),
        ),
        Shape::new(
            "ContentTermAssignmentListFilter",
            json!({
                "type": "object",
                "properties": {
                    "after": {"type": ["string", "null"], "maxLength": 512},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                },
            }),
        ),
        Shape::new(
            "ContentTermAssignment",
            json!({
                "type": "object",
                "required": ["content_id", "assigned_at"],
                "properties": {
                    "content_id": {"type": "string", "format": "uuid"},
                    "assigned_at": {"type": "string", "format": "date-time"},
                },
            }),
        ),
        Shape::new(
            "ContentTermAssignmentPage",
            json!({
                "type": "object",
                "required": ["items", "next_cursor"],
                "properties": {
                    "items": {"type": "array", "items": {"$ref": "#/components/schemas/ContentTermAssignment"}},
                    "next_cursor": {"type": ["string", "null"], "maxLength": 512},
                },
            }),
        ),
    ]
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ReplaceContentTerms {
    pub term_ids: Vec<TermId>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContentTermAssignmentListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
}

#[derive(Clone, Debug, Serialize)]
pub struct ContentTermAssignment {
    pub content_id: ContentId,
    pub assigned_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct AssignmentCursor {
    assigned_at: DateTime<Utc>,
    content_id: Uuid,
}

fn encode_cursor(assigned_at: DateTime<Utc>, content_id: Uuid) -> Result<Cursor> {
    let bytes = serde_json::to_vec(&AssignmentCursor {
        assigned_at,
        content_id,
    })
    .map_err(|_| MaviError::Internal)?;
    Cursor::parse(URL_SAFE_NO_PAD.encode(bytes))
}

fn decode_cursor(cursor: &Cursor) -> Result<AssignmentCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_str())
        .map_err(|_| MaviError::validation("invalid_cursor"))?;
    serde_json::from_slice(&bytes).map_err(|_| MaviError::validation("invalid_cursor"))
}

async fn ensure_content(
    tx: &mut SiteTx,
    context: &SiteContext,
    content_id: ContentId,
) -> Result<()> {
    let exists: bool = sqlx::query_scalar(
        "select exists(select 1 from content_entries
          where site_id = $1 and id = $2 and deleted_at is null)",
    )
    .bind(context.site_id.into_uuid())
    .bind(content_id.into_uuid())
    .fetch_one(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    if exists {
        Ok(())
    } else {
        Err(MaviError::NotFound {
            resource: CONTENT_NOT_FOUND,
        })
    }
}

async fn find_terms(
    tx: &mut SiteTx,
    context: &SiteContext,
    term_ids: &[TermId],
) -> Result<Vec<Term>> {
    if term_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "select id, site_id, kind, language, slug, name, parent_id, created_at, updated_at
           from taxonomy_terms where site_id = ",
    );
    query
        .push_bind(context.site_id.into_uuid())
        .push(" and deleted_at is null and id in (");
    let mut separated = query.separated(", ");
    for id in term_ids {
        separated.push_bind(id.into_uuid());
    }
    separated.push_unseparated(") order by name asc, id asc");
    let rows = query
        .build()
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
    if rows.len() != term_ids.len() {
        return Err(MaviError::NotFound {
            resource: TERM_NOT_FOUND,
        });
    }
    rows.iter().map(terms::from_row).collect()
}

pub(super) async fn list_for_content(
    tx: &mut SiteTx,
    context: &SiteContext,
    content_id: ContentId,
) -> Result<Vec<Term>> {
    ensure_content(tx, context, content_id).await?;
    let rows = sqlx::query(
        "select t.id, t.site_id, t.kind, t.language, t.slug, t.name, t.parent_id, t.created_at, t.updated_at
           from content_term_assignments a
           join taxonomy_terms t on t.site_id = a.site_id and t.id = a.term_id
          where a.site_id = $1 and a.content_id = $2 and t.deleted_at is null
          order by t.name asc, t.id asc",
    )
    .bind(context.site_id.into_uuid())
    .bind(content_id.into_uuid())
    .fetch_all(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    rows.iter().map(terms::from_row).collect()
}

pub(super) async fn replace_for_content(
    tx: &mut SiteTx,
    context: &SiteContext,
    content_id: ContentId,
    input: &ReplaceContentTerms,
) -> Result<Vec<Term>> {
    ensure_content(tx, context, content_id).await?;
    let term_ids = terms::validate_assignment_ids(&input.term_ids)?;
    let terms = find_terms(tx, context, &term_ids).await?;
    sqlx::query("delete from content_term_assignments where site_id = $1 and content_id = $2")
        .bind(context.site_id.into_uuid())
        .bind(content_id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
    for term_id in &term_ids {
        sqlx::query(
            "insert into content_term_assignments (site_id, content_id, term_id)
             values ($1, $2, $3)",
        )
        .bind(context.site_id.into_uuid())
        .bind(content_id.into_uuid())
        .bind(term_id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
    }
    AuditService
        .record(
            tx,
            context,
            &AuditEntry {
                action: "taxonomy.content_terms.replaced".to_owned(),
                resource_type: "Content".to_owned(),
                resource_id: Some(content_id.into_uuid()),
                payload: json!({"term_count": term_ids.len()}),
            },
        )
        .await?;
    Ok(terms)
}

pub(super) async fn list_content_for_term(
    tx: &mut SiteTx,
    context: &SiteContext,
    term_id: TermId,
    filter: &ContentTermAssignmentListFilter,
) -> Result<Page<ContentTermAssignment>> {
    terms::get(tx, context, term_id).await?;
    let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
    let limit = i64::from(filter.page.effective_limit());
    let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "select a.content_id, a.assigned_at
           from content_term_assignments a
           join content_entries c on c.site_id = a.site_id and c.id = a.content_id
          where a.site_id = ",
    );
    query
        .push_bind(context.site_id.into_uuid())
        .push(" and a.term_id = ")
        .push_bind(term_id.into_uuid())
        .push(" and c.deleted_at is null");
    if let Some(after) = after {
        query
            .push(" and (a.assigned_at, a.content_id) < (")
            .push_bind(after.assigned_at)
            .push(", ")
            .push_bind(after.content_id)
            .push(")");
    }
    let rows = query
        .push(" order by a.assigned_at desc, a.content_id desc limit ")
        .push_bind(limit + 1)
        .build()
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
    let mut items = rows
        .iter()
        .map(|row| {
            Ok(ContentTermAssignment {
                content_id: ContentId::from_uuid(
                    row.try_get("content_id").map_err(|_| MaviError::Internal)?,
                ),
                assigned_at: row
                    .try_get("assigned_at")
                    .map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let limit_usize = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
    let next_cursor = if items.len() > limit_usize {
        let last = items
            .get(limit_usize.saturating_sub(1))
            .ok_or(MaviError::Internal)?;
        Some(encode_cursor(
            last.assigned_at,
            last.content_id.into_uuid(),
        )?)
    } else {
        None
    };
    items.truncate(limit_usize);
    Ok(Page::new(items, next_cursor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignment_cursor_round_trips_and_contract_is_valid() {
        let assigned_at = Utc::now();
        let content_id = Uuid::now_v7();
        let cursor = encode_cursor(assigned_at, content_id).expect("cursor");
        let decoded = decode_cursor(&cursor).expect("decoded cursor");
        assert_eq!(decoded.assigned_at, assigned_at);
        assert_eq!(decoded.content_id, content_id);
        let api = mavi_contract::Api::new(endpoints()).with_shapes(shapes());
        api.validate().expect("assignment API contract");
    }
}
