//! Term values and the site-scoped term repository.

use std::collections::BTreeSet;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Capability, Cursor, ErrorCode, MaviError, Page, PageRequest, Result, SiteContext,
    SiteId, TermId,
};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

pub const TERM_NOT_FOUND: &str = "taxonomy_term_not_found";
pub const TERM_KIND_INVALID: &str = "taxonomy_term_kind_invalid";
pub const TERM_LANGUAGE_INVALID: &str = "taxonomy_term_language_invalid";
pub const TERM_SLUG_INVALID: &str = "taxonomy_term_slug_invalid";
pub const TERM_NAME_INVALID: &str = "taxonomy_term_name_invalid";
pub const TERM_PARENT_INVALID: &str = "taxonomy_term_parent_invalid";
pub const TERM_PARENT_NOT_FOUND: &str = "taxonomy_term_parent_not_found";
pub const TERM_PARENT_LANGUAGE_INVALID: &str = "taxonomy_term_parent_language_invalid";
pub const TERM_CYCLE: &str = "taxonomy_term_cycle";
pub const TERM_SLUG_TAKEN: &str = "taxonomy_term_slug_taken";
pub const TERM_ASSIGNMENT_LIMIT: &str = "taxonomy_term_assignment_limit";

const LANGUAGE_MAX: usize = 35;
const SLUG_MAX: usize = 160;
const NAME_MAX: usize = 100;
const MAX_ASSIGNMENTS: usize = 100;

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::new(
            Method::Get,
            "/api/v1/terms",
            "taxonomy.terms.list",
            "List site taxonomy terms",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Taxonomy,
            action: Action::View,
        })
        .takes_query("TermListFilter")
        .returns(200, "TermPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/terms",
            "taxonomy.terms.create",
            "Create a site taxonomy term",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Taxonomy,
            action: Action::Write,
        })
        .takes("CreateTerm")
        .returns(201, "Term")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/terms/{id}",
            "taxonomy.terms.read",
            "Read one site taxonomy term",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Taxonomy,
            action: Action::View,
        })
        .returns(200, "Term")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Patch,
            "/api/v1/terms/{id}",
            "taxonomy.terms.update",
            "Update a site taxonomy term",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Taxonomy,
            action: Action::Write,
        })
        .takes("UpdateTerm")
        .returns(200, "Term")
        .changes(true)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Delete,
            "/api/v1/terms/{id}",
            "taxonomy.terms.delete",
            "Move a site taxonomy term to trash",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Taxonomy,
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
            "TermKind",
            json!({"type": "string", "enum": ["category", "tag"]}),
        ),
        Shape::new(
            "TermListFilter",
            json!({
                "type": "object",
                "properties": {
                    "after": {"type": ["string", "null"], "maxLength": 512},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                    "kind": {"$ref": "#/components/schemas/TermKind"},
                    "language": {"type": ["string", "null"], "maxLength": 35},
                    "parent_id": {"type": ["string", "null"], "format": "uuid"},
                    "roots": {"type": "boolean"},
                },
            }),
        ),
        Shape::new(
            "Term",
            json!({
                "type": "object",
                "required": ["id", "site_id", "kind", "language", "slug", "name", "parent_id", "created_at", "updated_at"],
                "properties": {
                    "id": {"type": "string", "format": "uuid"},
                    "site_id": {"type": "string", "format": "uuid"},
                    "kind": {"$ref": "#/components/schemas/TermKind"},
                    "language": {"type": "string", "maxLength": 35},
                    "slug": {"type": "string", "maxLength": 160},
                    "name": {"type": "string", "maxLength": 100},
                    "parent_id": {"type": ["string", "null"], "format": "uuid"},
                    "created_at": {"type": "string", "format": "date-time"},
                    "updated_at": {"type": "string", "format": "date-time"},
                },
            }),
        ),
        Shape::new(
            "TermPage",
            json!({
                "type": "object",
                "required": ["items", "next_cursor"],
                "properties": {
                    "items": {"type": "array", "items": {"$ref": "#/components/schemas/Term"}},
                    "next_cursor": {"type": ["string", "null"], "maxLength": 512},
                },
            }),
        ),
        Shape::new(
            "CreateTerm",
            json!({
                "type": "object",
                "required": ["kind", "language", "slug", "name"],
                "properties": {
                    "kind": {"$ref": "#/components/schemas/TermKind"},
                    "language": {"type": "string", "maxLength": 35},
                    "slug": {"type": "string", "maxLength": 160},
                    "name": {"type": "string", "maxLength": 100},
                    "parent_id": {"type": ["string", "null"], "format": "uuid"},
                },
            }),
        ),
        Shape::new(
            "UpdateTerm",
            json!({
                "type": "object",
                "properties": {
                    "name": {"type": ["string", "null"], "maxLength": 100},
                    "parent_id": {"type": ["string", "null"], "format": "uuid"},
                },
            }),
        ),
    ]
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TermKind {
    Category,
    Tag,
}

impl TermKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Category => "category",
            Self::Tag => "tag",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "category" => Ok(Self::Category),
            "tag" => Ok(Self::Tag),
            _ => Err(MaviError::validation(TERM_KIND_INVALID)),
        }
    }

    const fn may_have_parent(self) -> bool {
        matches!(self, Self::Category)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Term {
    pub id: TermId,
    pub site_id: SiteId,
    pub kind: TermKind,
    pub language: String,
    pub slug: String,
    pub name: String,
    pub parent_id: Option<TermId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreateTerm {
    pub kind: TermKind,
    pub language: String,
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub parent_id: Option<TermId>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UpdateTerm {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_parent_id")]
    pub parent_id: Option<Option<TermId>>,
}

#[allow(clippy::option_option)]
fn deserialize_parent_id<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<Option<TermId>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<TermId>::deserialize(deserializer)?))
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TermListFilter {
    pub kind: Option<TermKind>,
    pub language: Option<String>,
    pub parent_id: Option<TermId>,
    #[serde(default)]
    pub roots: bool,
    #[serde(flatten)]
    pub page: PageRequest,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TermCursor {
    created_at: DateTime<Utc>,
    id: Uuid,
}

fn encode_cursor(created_at: DateTime<Utc>, id: Uuid) -> Result<Cursor> {
    let bytes =
        serde_json::to_vec(&TermCursor { created_at, id }).map_err(|_| MaviError::Internal)?;
    Cursor::parse(URL_SAFE_NO_PAD.encode(bytes))
}

fn decode_cursor(cursor: &Cursor) -> Result<TermCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_str())
        .map_err(|_| MaviError::validation("invalid_cursor"))?;
    serde_json::from_slice(&bytes).map_err(|_| MaviError::validation("invalid_cursor"))
}

fn parse_language(value: &str) -> Result<String> {
    let valid = !value.is_empty()
        && value.len() <= LANGUAGE_MAX
        && value.split('-').enumerate().all(|(index, part)| {
            let length = part.len();
            let size_ok = if index == 0 {
                (2..=8).contains(&length)
            } else {
                (1..=8).contains(&length)
            };
            size_ok
                && part.chars().all(|character| {
                    if index == 0 {
                        character.is_ascii_alphabetic()
                    } else {
                        character.is_ascii_alphanumeric()
                    }
                })
        });
    if valid {
        Ok(value.to_owned())
    } else {
        Err(MaviError::validation(TERM_LANGUAGE_INVALID))
    }
}

fn parse_slug(value: &str) -> Result<String> {
    let valid = !value.is_empty()
        && value.len() <= SLUG_MAX
        && value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--");
    if valid {
        Ok(value.to_owned())
    } else {
        Err(MaviError::validation(TERM_SLUG_INVALID))
    }
}

fn parse_name(value: &str) -> Result<String> {
    let value = value.trim();
    if (1..=NAME_MAX).contains(&value.chars().count()) {
        Ok(value.to_owned())
    } else {
        Err(MaviError::validation(TERM_NAME_INVALID))
    }
}

pub(super) fn from_row(row: &sqlx::postgres::PgRow) -> Result<Term> {
    Ok(Term {
        id: TermId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        site_id: SiteId::from_uuid(row.try_get("site_id").map_err(|_| MaviError::Internal)?),
        kind: TermKind::parse(
            &row.try_get::<String, _>("kind")
                .map_err(|_| MaviError::Internal)?,
        )?,
        language: row.try_get("language").map_err(|_| MaviError::Internal)?,
        slug: row.try_get("slug").map_err(|_| MaviError::Internal)?,
        name: row.try_get("name").map_err(|_| MaviError::Internal)?,
        parent_id: row
            .try_get::<Option<Uuid>, _>("parent_id")
            .map_err(|_| MaviError::Internal)?
            .map(TermId::from_uuid),
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

fn map_write_error(error: &sqlx::Error) -> MaviError {
    if let sqlx::Error::Database(database) = error
        && database.constraint() == Some("taxonomy_terms_site_kind_language_slug")
    {
        return MaviError::conflict(TERM_SLUG_TAKEN);
    }
    MaviError::Internal
}

async fn read_parent(
    tx: &mut SiteTx,
    context: &SiteContext,
    kind: TermKind,
    language: &str,
    term_id: TermId,
    parent_id: Option<TermId>,
) -> Result<Option<Uuid>> {
    let Some(parent_id) = parent_id else {
        return Ok(None);
    };
    if !kind.may_have_parent() {
        return Err(MaviError::validation(TERM_PARENT_INVALID));
    }
    if parent_id == term_id {
        return Err(MaviError::validation(TERM_CYCLE));
    }

    let parent = sqlx::query(
        "select id, kind, language from taxonomy_terms
          where site_id = $1 and id = $2 and deleted_at is null",
    )
    .bind(context.site_id.into_uuid())
    .bind(parent_id.into_uuid())
    .fetch_optional(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?
    .ok_or(MaviError::NotFound {
        resource: TERM_PARENT_NOT_FOUND,
    })?;
    let parent_kind = TermKind::parse(
        &parent
            .try_get::<String, _>("kind")
            .map_err(|_| MaviError::Internal)?,
    )?;
    if !parent_kind.may_have_parent() {
        return Err(MaviError::validation(TERM_PARENT_INVALID));
    }
    if parent
        .try_get::<String, _>("language")
        .map_err(|_| MaviError::Internal)?
        != language
    {
        return Err(MaviError::validation(TERM_PARENT_LANGUAGE_INVALID));
    }

    let creates_cycle: bool = sqlx::query_scalar(
        "with recursive ancestors(id) as (
             select parent_id from taxonomy_terms where site_id = $1 and id = $2
             union
             select t.parent_id
               from taxonomy_terms t
               join ancestors a on a.id = t.id
              where t.site_id = $1 and t.parent_id is not null
         )
         select exists(select 1 from ancestors where id = $3)",
    )
    .bind(context.site_id.into_uuid())
    .bind(parent_id.into_uuid())
    .bind(term_id.into_uuid())
    .fetch_one(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    if creates_cycle {
        return Err(MaviError::validation(TERM_CYCLE));
    }

    Ok(Some(parent_id.into_uuid()))
}

pub(super) async fn list(
    tx: &mut SiteTx,
    context: &SiteContext,
    filter: &TermListFilter,
) -> Result<Page<Term>> {
    let language = filter.language.as_deref().map(parse_language).transpose()?;
    let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
    let limit = i64::from(filter.page.effective_limit());
    let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "select id, site_id, kind, language, slug, name, parent_id, created_at, updated_at
           from taxonomy_terms where site_id = ",
    );
    query
        .push_bind(context.site_id.into_uuid())
        .push(" and deleted_at is null");
    if let Some(kind) = filter.kind {
        query.push(" and kind = ").push_bind(kind.as_str());
    }
    if let Some(language) = language {
        query.push(" and language = ").push_bind(language);
    }
    if let Some(parent_id) = filter.parent_id {
        query
            .push(" and parent_id = ")
            .push_bind(parent_id.into_uuid());
    } else if filter.roots {
        query.push(" and parent_id is null");
    }
    if let Some(after) = after {
        query
            .push(" and (created_at, id) < (")
            .push_bind(after.created_at)
            .push(", ")
            .push_bind(after.id)
            .push(")");
    }
    let rows = query
        .push(" order by created_at desc, id desc limit ")
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
        Some(encode_cursor(last.created_at, last.id.into_uuid())?)
    } else {
        None
    };
    items.truncate(limit_usize);
    Ok(Page::new(items, next_cursor))
}

pub(super) async fn get(tx: &mut SiteTx, context: &SiteContext, id: TermId) -> Result<Term> {
    let row = sqlx::query(
        "select id, site_id, kind, language, slug, name, parent_id, created_at, updated_at
           from taxonomy_terms where site_id = $1 and id = $2 and deleted_at is null",
    )
    .bind(context.site_id.into_uuid())
    .bind(id.into_uuid())
    .fetch_optional(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?
    .ok_or(MaviError::NotFound {
        resource: TERM_NOT_FOUND,
    })?;
    from_row(&row)
}

pub(super) async fn create(
    tx: &mut SiteTx,
    context: &SiteContext,
    input: &CreateTerm,
) -> Result<Term> {
    let language = parse_language(&input.language)?;
    let slug = parse_slug(&input.slug)?;
    let name = parse_name(&input.name)?;
    let id = TermId::new();
    let parent_id = read_parent(tx, context, input.kind, &language, id, input.parent_id).await?;
    let row = sqlx::query(
        "insert into taxonomy_terms
            (site_id, id, kind, language, slug, name, parent_id)
         values ($1, $2, $3, $4, $5, $6, $7)
         returning id, site_id, kind, language, slug, name, parent_id, created_at, updated_at",
    )
    .bind(context.site_id.into_uuid())
    .bind(id.into_uuid())
    .bind(input.kind.as_str())
    .bind(language)
    .bind(slug)
    .bind(name)
    .bind(parent_id)
    .fetch_one(tx.conn())
    .await
    .map_err(|error| map_write_error(&error))?;
    let term = from_row(&row)?;
    AuditService
        .record(
            tx,
            context,
            &AuditEntry {
                action: "taxonomy.term.created".to_owned(),
                resource_type: "TaxonomyTerm".to_owned(),
                resource_id: Some(term.id.into_uuid()),
                payload: json!({"kind": term.kind, "language": term.language}),
            },
        )
        .await?;
    Ok(term)
}

pub(super) async fn update(
    tx: &mut SiteTx,
    context: &SiteContext,
    id: TermId,
    input: &UpdateTerm,
) -> Result<Term> {
    let current = get(tx, context, id).await?;
    let name = input
        .name
        .as_deref()
        .map(parse_name)
        .transpose()?
        .unwrap_or(current.name);
    let parent_id = match input.parent_id {
        Some(parent_id) => parent_id,
        None => current.parent_id,
    };
    let parent_id =
        read_parent(tx, context, current.kind, &current.language, id, parent_id).await?;
    let row = sqlx::query(
        "update taxonomy_terms set name = $3, parent_id = $4, updated_at = now()
           where site_id = $1 and id = $2 and deleted_at is null
         returning id, site_id, kind, language, slug, name, parent_id, created_at, updated_at",
    )
    .bind(context.site_id.into_uuid())
    .bind(id.into_uuid())
    .bind(name)
    .bind(parent_id)
    .fetch_optional(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?
    .ok_or(MaviError::NotFound {
        resource: TERM_NOT_FOUND,
    })?;
    let term = from_row(&row)?;
    AuditService
        .record(
            tx,
            context,
            &AuditEntry {
                action: "taxonomy.term.updated".to_owned(),
                resource_type: "TaxonomyTerm".to_owned(),
                resource_id: Some(term.id.into_uuid()),
                payload: json!({"parent_id": term.parent_id}),
            },
        )
        .await?;
    Ok(term)
}

pub(super) async fn delete(tx: &mut SiteTx, context: &SiteContext, id: TermId) -> Result<()> {
    let deleted = sqlx::query(
        "update taxonomy_terms set deleted_at = now(), updated_at = now()
           where site_id = $1 and id = $2 and deleted_at is null",
    )
    .bind(context.site_id.into_uuid())
    .bind(id.into_uuid())
    .execute(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    if deleted.rows_affected() == 0 {
        return Err(MaviError::NotFound {
            resource: TERM_NOT_FOUND,
        });
    }
    sqlx::query(
        "update taxonomy_terms set parent_id = null, updated_at = now()
           where site_id = $1 and parent_id = $2 and deleted_at is null",
    )
    .bind(context.site_id.into_uuid())
    .bind(id.into_uuid())
    .execute(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    sqlx::query("delete from content_term_assignments where site_id = $1 and term_id = $2")
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
    AuditService
        .record(
            tx,
            context,
            &AuditEntry {
                action: "taxonomy.term.deleted".to_owned(),
                resource_type: "TaxonomyTerm".to_owned(),
                resource_id: Some(id.into_uuid()),
                payload: json!({}),
            },
        )
        .await
}

pub(super) fn validate_assignment_ids(term_ids: &[TermId]) -> Result<Vec<TermId>> {
    if term_ids.len() > MAX_ASSIGNMENTS {
        return Err(MaviError::validation(TERM_ASSIGNMENT_LIMIT));
    }
    let unique = term_ids.iter().copied().collect::<BTreeSet<_>>();
    Ok(unique.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn term_values_enforce_language_slug_and_name_rules() {
        assert!(parse_language("en-US").is_ok());
        assert!(parse_language("1n").is_err());
        assert!(parse_slug("news").is_ok());
        assert!(parse_slug("news--today").is_err());
        assert_eq!(parse_name(" News ").expect("name"), "News");
        assert!(parse_name(" ").is_err());
    }

    #[test]
    fn updating_parent_distinguishes_omitted_from_root() {
        let omitted = serde_json::from_str::<UpdateTerm>("{}").expect("omitted parent");
        assert_eq!(omitted.parent_id, None);
        let root =
            serde_json::from_str::<UpdateTerm>(r#"{"parent_id":null}"#).expect("root parent");
        assert_eq!(root.parent_id, Some(None));
    }

    #[test]
    fn term_contract_is_self_consistent() {
        let api = mavi_contract::Api::new(endpoints()).with_shapes(shapes());
        api.validate().expect("taxonomy API contract");
    }
}
