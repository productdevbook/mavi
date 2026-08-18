//! A site-scoped, typed trash boundary.
//!
//! Trash is deliberately a small cross-domain application service. The
//! address may contain a kind from the HTTP path, but table names and labels
//! come only from [`TrashKind`]. Restoring keeps metadata and (for media)
//! bytes available; permanent deletion records a durable media cleanup task
//! before removing the metadata row.

use std::collections::BTreeSet;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Capability, Cursor, ErrorCode, MaviError, Page, PageRequest, Result, SiteContext,
    SiteId, ports::FileStore,
};
use mavi_media::{FileKind, MAX_FILE_BYTES, MAX_MEDIA_RELOCATION_BYTES};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

pub const TRASH_ITEM_NOT_FOUND: &str = "trash_item_not_found";
pub const TRASH_KIND_INVALID: &str = "trash_kind_invalid";
pub const TRASH_RESTORE_CONFLICT: &str = "trash_restore_conflict";
pub const TRASH_RELOCATION_FORMAT: &str = "mavi.trash.relocation";
pub const TRASH_RELOCATION_VERSION: u16 = 1;
pub const TRASH_RELOCATION_CONFLICT: &str = "trash_relocation_conflict";
pub const MAX_TRASH_RECORDS_PER_SECTION: usize = 10_000;
pub const MAX_TRASH_TOTAL_RECORDS: usize = 20_000;
pub const MAX_TRASH_RELOCATION_BYTES: usize = 256 * 1024 * 1024;

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

/// Complete soft-deleted state carried by the trusted shard relocation port.
///
/// The active portable bundle deliberately excludes these rows. Keeping the
/// trash adapter separate means a relocation can preserve restore semantics
/// without making public exports contain deleted content or file bytes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrashRelocation {
    pub format: String,
    pub version: u16,
    pub source_site_id: SiteId,
    pub content: Vec<TrashContentRelocation>,
    pub revisions: Vec<TrashRevisionRelocation>,
    pub slug_history: Vec<TrashSlugHistoryRelocation>,
    pub assignments: Vec<TrashAssignmentRelocation>,
    pub terms: Vec<TrashTermRelocation>,
    pub files: Vec<TrashFileRelocation>,
}

impl TrashRelocation {
    #[must_use]
    pub fn empty(source_site_id: SiteId) -> Self {
        Self {
            format: TRASH_RELOCATION_FORMAT.to_owned(),
            version: TRASH_RELOCATION_VERSION,
            source_site_id,
            content: Vec::new(),
            revisions: Vec::new(),
            slug_history: Vec::new(),
            assignments: Vec::new(),
            terms: Vec::new(),
            files: Vec::new(),
        }
    }

    #[allow(clippy::too_many_lines)]
    pub fn validate_for_relocation(&self, target_site: SiteId) -> Result<()> {
        if self.format != TRASH_RELOCATION_FORMAT {
            return Err(MaviError::validation("trash_relocation_format_invalid"));
        }
        if self.version != TRASH_RELOCATION_VERSION {
            return Err(MaviError::validation(
                "trash_relocation_version_unsupported",
            ));
        }
        if self.source_site_id != target_site || self.source_site_id.into_uuid().is_nil() {
            return Err(MaviError::conflict("trash_relocation_site_mismatch"));
        }

        let sections = [
            self.content.len(),
            self.revisions.len(),
            self.slug_history.len(),
            self.assignments.len(),
            self.terms.len(),
            self.files.len(),
        ];
        if sections
            .iter()
            .any(|count| *count > MAX_TRASH_RECORDS_PER_SECTION)
            || sections
                .iter()
                .try_fold(0usize, |total, count| total.checked_add(*count))
                .is_none_or(|total| total > MAX_TRASH_TOTAL_RECORDS)
        {
            return Err(MaviError::validation("trash_relocation_counts_invalid"));
        }

        let mut content_ids = BTreeSet::new();
        for content in &self.content {
            if !content_ids.insert(content.id)
                || !valid_identifier(&content.kind)
                || !valid_language(&content.language)
                || !valid_slug(&content.slug)
                || !valid_text(&content.title, 200)
                || content
                    .excerpt
                    .as_deref()
                    .is_some_and(|excerpt| excerpt.chars().count() > 2000)
                || !content.fields.is_object()
                || !valid_status(
                    &content.status,
                    content.scheduled_at.is_some(),
                    content.published_at.is_some(),
                )
                || content.revision == 0
            {
                return Err(MaviError::validation("trash_relocation_content_invalid"));
            }
        }

        let mut revision_ids = BTreeSet::new();
        for revision in &self.revisions {
            if !content_ids.contains(&revision.content_id)
                || revision.revision == 0
                || !revision_ids.insert((revision.content_id, revision.revision))
                || !valid_identifier(&revision.kind)
                || !valid_language(&revision.language)
                || !valid_slug(&revision.slug)
                || !valid_text(&revision.title, 200)
                || revision
                    .excerpt
                    .as_deref()
                    .is_some_and(|excerpt| excerpt.chars().count() > 2000)
                || !revision.fields.is_object()
                || !valid_status(
                    &revision.status,
                    revision.scheduled_at.is_some(),
                    revision.published_at.is_some(),
                )
            {
                return Err(MaviError::validation("trash_relocation_revision_invalid"));
            }
        }

        let mut slug_ids = BTreeSet::new();
        for history in &self.slug_history {
            if !content_ids.contains(&history.content_id)
                || !slug_ids.insert((
                    history.content_id,
                    history.language.as_str(),
                    history.slug.as_str(),
                ))
                || !valid_language(&history.language)
                || !valid_slug(&history.slug)
            {
                return Err(MaviError::validation(
                    "trash_relocation_slug_history_invalid",
                ));
            }
        }

        for assignment in &self.assignments {
            if !content_ids.contains(&assignment.content_id) || assignment.term_id.is_nil() {
                return Err(MaviError::validation("trash_relocation_assignment_invalid"));
            }
        }

        let mut term_ids = BTreeSet::new();
        for term in &self.terms {
            if !term_ids.insert(term.id)
                || !matches!(term.kind.as_str(), "category" | "tag")
                || !valid_language(&term.language)
                || !valid_slug(&term.slug)
                || !valid_text(&term.name, 100)
                || term.parent_id == Some(term.id)
            {
                return Err(MaviError::validation("trash_relocation_term_invalid"));
            }
        }

        let mut file_ids = BTreeSet::new();
        let mut storage_keys = BTreeSet::new();
        let mut raw_bytes = 0usize;
        for file in &self.files {
            if !file_ids.insert(file.id)
                || !storage_keys.insert(file.storage_key.as_str())
                || !valid_storage_key(&file.storage_key)
                || !valid_mime(&file.mime)
                || !valid_text(&file.name, 255)
                || file.bytes == 0
                || file.bytes > MAX_FILE_BYTES as u64
                || usize::try_from(file.bytes).ok() != Some(file.content.len())
                || file.sha256.len() != 64
                || !file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(MaviError::validation("trash_relocation_file_invalid"));
            }
            raw_bytes = raw_bytes
                .checked_add(file.content.len())
                .ok_or(MaviError::validation("trash_relocation_size_overflow"))?;
            if raw_bytes > MAX_MEDIA_RELOCATION_BYTES {
                return Err(MaviError::validation("trash_relocation_too_large"));
            }
            if hex_digest(&Sha256::digest(&file.content)) != file.sha256 {
                return Err(MaviError::validation("trash_relocation_digest_mismatch"));
            }
        }

        let bytes = serde_json::to_vec(self).map_err(|_| MaviError::Internal)?;
        if bytes.len() > MAX_TRASH_RELOCATION_BYTES {
            return Err(MaviError::validation("trash_relocation_too_large"));
        }
        Ok(())
    }

    pub fn record_count(&self) -> Result<i64> {
        let count = self
            .content
            .len()
            .checked_add(self.revisions.len())
            .and_then(|value| value.checked_add(self.slug_history.len()))
            .and_then(|value| value.checked_add(self.assignments.len()))
            .and_then(|value| value.checked_add(self.terms.len()))
            .and_then(|value| value.checked_add(self.files.len()))
            .ok_or(MaviError::validation(
                "trash_relocation_record_count_overflow",
            ))?;
        i64::try_from(count)
            .map_err(|_| MaviError::validation("trash_relocation_record_count_overflow"))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrashContentRelocation {
    pub id: Uuid,
    pub kind: String,
    pub language: String,
    pub slug: String,
    pub title: String,
    pub excerpt: Option<String>,
    pub body: String,
    pub fields: Value,
    pub status: String,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub published_at: Option<DateTime<Utc>>,
    pub revision: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrashRevisionRelocation {
    pub content_id: Uuid,
    pub revision: u32,
    pub kind: String,
    pub language: String,
    pub slug: String,
    pub title: String,
    pub excerpt: Option<String>,
    pub body: String,
    pub fields: Value,
    pub status: String,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub published_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrashSlugHistoryRelocation {
    pub content_id: Uuid,
    pub language: String,
    pub slug: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrashAssignmentRelocation {
    pub content_id: Uuid,
    pub term_id: Uuid,
    pub assigned_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrashTermRelocation {
    pub id: Uuid,
    pub kind: String,
    pub language: String,
    pub slug: String,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrashFileRelocation {
    pub id: Uuid,
    pub kind: FileKind,
    pub mime: String,
    pub name: String,
    pub storage_key: String,
    pub bytes: u64,
    pub sha256: String,
    pub created_at: DateTime<Utc>,
    pub deleted_at: DateTime<Utc>,
    #[serde(with = "base64_bytes")]
    pub content: Vec<u8>,
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

    /// Exports every restorable soft-deleted row and the retained media bytes
    /// owned by the trash boundary. Permanent deletions are intentionally not
    /// represented: once metadata is gone, there is nothing to restore.
    #[allow(clippy::similar_names, clippy::too_many_lines)]
    pub async fn export_for_relocation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        store: &dyn FileStore,
    ) -> Result<TrashRelocation> {
        let content_rows = sqlx::query(
            "select id, kind, language, slug, title, excerpt, body, fields, status,
                    scheduled_at, published_at, revision, created_at, updated_at, deleted_at
               from content_entries
              where site_id = $1 and deleted_at is not null
              order by deleted_at asc, id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(TrashContentRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                kind: row.try_get("kind").map_err(|_| MaviError::Internal)?,
                language: row.try_get("language").map_err(|_| MaviError::Internal)?,
                slug: row.try_get("slug").map_err(|_| MaviError::Internal)?,
                title: row.try_get("title").map_err(|_| MaviError::Internal)?,
                excerpt: row.try_get("excerpt").map_err(|_| MaviError::Internal)?,
                body: row.try_get("body").map_err(|_| MaviError::Internal)?,
                fields: row.try_get("fields").map_err(|_| MaviError::Internal)?,
                status: row.try_get("status").map_err(|_| MaviError::Internal)?,
                scheduled_at: row
                    .try_get("scheduled_at")
                    .map_err(|_| MaviError::Internal)?,
                published_at: row
                    .try_get("published_at")
                    .map_err(|_| MaviError::Internal)?,
                revision: u32::try_from(
                    row.try_get::<i32, _>("revision")
                        .map_err(|_| MaviError::Internal)?,
                )
                .map_err(|_| MaviError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
                deleted_at: row.try_get("deleted_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let revisions = sqlx::query(
            "select r.content_id, r.revision, r.kind, r.language, r.slug, r.title,
                    r.excerpt, r.body, r.fields, r.status, r.scheduled_at,
                    r.published_at, r.created_at
               from content_revisions r
               join content_entries c on c.site_id = r.site_id and c.id = r.content_id
              where r.site_id = $1 and c.deleted_at is not null
              order by r.content_id asc, r.revision asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(TrashRevisionRelocation {
                content_id: row.try_get("content_id").map_err(|_| MaviError::Internal)?,
                revision: u32::try_from(
                    row.try_get::<i32, _>("revision")
                        .map_err(|_| MaviError::Internal)?,
                )
                .map_err(|_| MaviError::Internal)?,
                kind: row.try_get("kind").map_err(|_| MaviError::Internal)?,
                language: row.try_get("language").map_err(|_| MaviError::Internal)?,
                slug: row.try_get("slug").map_err(|_| MaviError::Internal)?,
                title: row.try_get("title").map_err(|_| MaviError::Internal)?,
                excerpt: row.try_get("excerpt").map_err(|_| MaviError::Internal)?,
                body: row.try_get("body").map_err(|_| MaviError::Internal)?,
                fields: row.try_get("fields").map_err(|_| MaviError::Internal)?,
                status: row.try_get("status").map_err(|_| MaviError::Internal)?,
                scheduled_at: row
                    .try_get("scheduled_at")
                    .map_err(|_| MaviError::Internal)?,
                published_at: row
                    .try_get("published_at")
                    .map_err(|_| MaviError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let slug_history = sqlx::query(
            "select h.content_id, h.language, h.slug, h.created_at
               from content_slug_history h
               join content_entries c on c.site_id = h.site_id and c.id = h.content_id
              where h.site_id = $1 and c.deleted_at is not null
              order by h.content_id asc, h.created_at asc, h.slug asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(TrashSlugHistoryRelocation {
                content_id: row.try_get("content_id").map_err(|_| MaviError::Internal)?,
                language: row.try_get("language").map_err(|_| MaviError::Internal)?,
                slug: row.try_get("slug").map_err(|_| MaviError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let assignments = sqlx::query(
            "select a.content_id, a.term_id, a.assigned_at
               from content_term_assignments a
               left join content_entries c
                 on c.site_id = a.site_id and c.id = a.content_id
               left join taxonomy_terms t
                 on t.site_id = a.site_id and t.id = a.term_id
              where a.site_id = $1 and (c.deleted_at is not null or t.deleted_at is not null)
              order by a.content_id asc, a.term_id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(TrashAssignmentRelocation {
                content_id: row.try_get("content_id").map_err(|_| MaviError::Internal)?,
                term_id: row.try_get("term_id").map_err(|_| MaviError::Internal)?,
                assigned_at: row
                    .try_get("assigned_at")
                    .map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let terms = sqlx::query(
            "select id, kind, language, slug, name, parent_id, created_at, updated_at, deleted_at
               from taxonomy_terms
              where site_id = $1 and deleted_at is not null
              order by created_at asc, id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(TrashTermRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                kind: row.try_get("kind").map_err(|_| MaviError::Internal)?,
                language: row.try_get("language").map_err(|_| MaviError::Internal)?,
                slug: row.try_get("slug").map_err(|_| MaviError::Internal)?,
                name: row.try_get("name").map_err(|_| MaviError::Internal)?,
                parent_id: row.try_get("parent_id").map_err(|_| MaviError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
                deleted_at: row.try_get("deleted_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let rows = sqlx::query(
            "select id, kind, mime, name, storage_key, bytes, sha256, created_at, deleted_at
               from media_files
              where site_id = $1 and deleted_at is not null
              order by created_at asc, id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let mut files = Vec::with_capacity(rows.len());
        for row in &rows {
            let id: Uuid = row.try_get("id").map_err(|_| MaviError::Internal)?;
            let storage_key: String = row
                .try_get("storage_key")
                .map_err(|_| MaviError::Internal)?;
            let file_content = store.get(context, &storage_key).await?;
            let bytes = u64::try_from(
                row.try_get::<i64, _>("bytes")
                    .map_err(|_| MaviError::Internal)?,
            )
            .map_err(|_| MaviError::Internal)?;
            if file_content.len() != usize::try_from(bytes).map_err(|_| MaviError::Internal)?
                || hex_digest(&Sha256::digest(&file_content))
                    != row
                        .try_get::<String, _>("sha256")
                        .map_err(|_| MaviError::Internal)?
            {
                return Err(MaviError::validation("media_storage_integrity_failed"));
            }
            files.push(TrashFileRelocation {
                id,
                kind: parse_file_kind(
                    row.try_get::<String, _>("kind")
                        .map_err(|_| MaviError::Internal)?
                        .as_str(),
                )?,
                mime: row.try_get("mime").map_err(|_| MaviError::Internal)?,
                name: row.try_get("name").map_err(|_| MaviError::Internal)?,
                storage_key,
                bytes,
                sha256: row.try_get("sha256").map_err(|_| MaviError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                deleted_at: row.try_get("deleted_at").map_err(|_| MaviError::Internal)?,
                content: file_content,
            });
        }

        let relocation = TrashRelocation {
            format: TRASH_RELOCATION_FORMAT.to_owned(),
            version: TRASH_RELOCATION_VERSION,
            source_site_id: context.site_id,
            content: content_rows,
            revisions,
            slug_history,
            assignments,
            terms,
            files,
        };
        relocation.validate_for_relocation(context.site_id)?;
        Ok(relocation)
    }

    /// Restores the deleted state on the target without dropping any history.
    /// The caller must run this inside the same transaction as the other site
    /// relocation adapters.
    #[allow(clippy::too_many_lines)]
    pub async fn import_for_relocation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        store: &dyn FileStore,
        relocation: &TrashRelocation,
    ) -> Result<()> {
        relocation.validate_for_relocation(context.site_id)?;

        for term in ordered_terms(&relocation.terms)? {
            sqlx::query(
                "insert into taxonomy_terms
                    (site_id, id, kind, language, slug, name, parent_id,
                     created_at, updated_at, deleted_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                 on conflict (site_id, id) do update set
                    kind = excluded.kind, language = excluded.language,
                    slug = excluded.slug, name = excluded.name,
                    parent_id = excluded.parent_id, created_at = excluded.created_at,
                    updated_at = excluded.updated_at, deleted_at = excluded.deleted_at",
            )
            .bind(context.site_id.into_uuid())
            .bind(term.id)
            .bind(&term.kind)
            .bind(&term.language)
            .bind(&term.slug)
            .bind(&term.name)
            .bind(term.parent_id)
            .bind(term.created_at)
            .bind(term.updated_at)
            .bind(term.deleted_at)
            .execute(tx.conn())
            .await
            .map_err(map_relocation_write_error)?;
        }

        for content in &relocation.content {
            sqlx::query(
                "insert into content_entries
                    (site_id, id, kind, language, slug, title, excerpt, body, fields,
                     status, scheduled_at, published_at, revision, created_at,
                     updated_at, deleted_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                         $14, $15, $16)
                 on conflict (site_id, id) do update set
                    kind = excluded.kind, language = excluded.language,
                    slug = excluded.slug, title = excluded.title,
                    excerpt = excluded.excerpt, body = excluded.body,
                    fields = excluded.fields, status = excluded.status,
                    scheduled_at = excluded.scheduled_at,
                    published_at = excluded.published_at,
                    revision = excluded.revision, created_at = excluded.created_at,
                    updated_at = excluded.updated_at, deleted_at = excluded.deleted_at",
            )
            .bind(context.site_id.into_uuid())
            .bind(content.id)
            .bind(&content.kind)
            .bind(&content.language)
            .bind(&content.slug)
            .bind(&content.title)
            .bind(&content.excerpt)
            .bind(&content.body)
            .bind(&content.fields)
            .bind(&content.status)
            .bind(content.scheduled_at)
            .bind(content.published_at)
            .bind(
                i32::try_from(content.revision)
                    .map_err(|_| MaviError::validation("trash_relocation_revision_invalid"))?,
            )
            .bind(content.created_at)
            .bind(content.updated_at)
            .bind(content.deleted_at)
            .execute(tx.conn())
            .await
            .map_err(map_relocation_write_error)?;
        }

        for revision in &relocation.revisions {
            sqlx::query(
                "insert into content_revisions
                    (site_id, content_id, revision, kind, language, slug, title,
                     excerpt, body, fields, status, scheduled_at, published_at,
                     created_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                         $14)
                 on conflict (site_id, content_id, revision) do update set
                    kind = excluded.kind, language = excluded.language,
                    slug = excluded.slug, title = excluded.title,
                    excerpt = excluded.excerpt, body = excluded.body,
                    fields = excluded.fields, status = excluded.status,
                    scheduled_at = excluded.scheduled_at,
                    published_at = excluded.published_at,
                    created_at = excluded.created_at",
            )
            .bind(context.site_id.into_uuid())
            .bind(revision.content_id)
            .bind(
                i32::try_from(revision.revision)
                    .map_err(|_| MaviError::validation("trash_relocation_revision_invalid"))?,
            )
            .bind(&revision.kind)
            .bind(&revision.language)
            .bind(&revision.slug)
            .bind(&revision.title)
            .bind(&revision.excerpt)
            .bind(&revision.body)
            .bind(&revision.fields)
            .bind(&revision.status)
            .bind(revision.scheduled_at)
            .bind(revision.published_at)
            .bind(revision.created_at)
            .execute(tx.conn())
            .await
            .map_err(map_relocation_write_error)?;
        }

        for history in &relocation.slug_history {
            sqlx::query(
                "insert into content_slug_history
                    (site_id, content_id, language, slug, created_at)
                 values ($1, $2, $3, $4, $5)
                 on conflict (site_id, content_id, language, slug) do update set
                    created_at = excluded.created_at",
            )
            .bind(context.site_id.into_uuid())
            .bind(history.content_id)
            .bind(&history.language)
            .bind(&history.slug)
            .bind(history.created_at)
            .execute(tx.conn())
            .await
            .map_err(map_relocation_write_error)?;
        }

        for assignment in &relocation.assignments {
            sqlx::query(
                "insert into content_term_assignments
                    (site_id, content_id, term_id, assigned_at)
                 values ($1, $2, $3, $4)
                 on conflict (site_id, content_id, term_id) do update set
                    assigned_at = excluded.assigned_at",
            )
            .bind(context.site_id.into_uuid())
            .bind(assignment.content_id)
            .bind(assignment.term_id)
            .bind(assignment.assigned_at)
            .execute(tx.conn())
            .await
            .map_err(map_relocation_write_error)?;
        }

        for file in &relocation.files {
            store
                .put(context, &file.storage_key, file.content.clone())
                .await?;
            sqlx::query(
                "insert into media_files
                    (site_id, id, kind, mime, name, storage_key, bytes, sha256,
                     created_at, deleted_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                 on conflict (site_id, id) do update set
                    kind = excluded.kind, mime = excluded.mime,
                    name = excluded.name, storage_key = excluded.storage_key,
                    bytes = excluded.bytes, sha256 = excluded.sha256,
                    created_at = excluded.created_at, deleted_at = excluded.deleted_at",
            )
            .bind(context.site_id.into_uuid())
            .bind(file.id)
            .bind(file.kind.as_str())
            .bind(&file.mime)
            .bind(&file.name)
            .bind(&file.storage_key)
            .bind(
                i64::try_from(file.bytes)
                    .map_err(|_| MaviError::validation("trash_relocation_file_invalid"))?,
            )
            .bind(&file.sha256)
            .bind(file.created_at)
            .bind(file.deleted_at)
            .execute(tx.conn())
            .await
            .map_err(map_relocation_write_error)?;
        }

        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "portable.trash.relocated".to_owned(),
                    resource_type: "TrashSnapshot".to_owned(),
                    resource_id: None,
                    payload: json!({
                        "content": relocation.content.len(),
                        "revisions": relocation.revisions.len(),
                        "slug_history": relocation.slug_history.len(),
                        "assignments": relocation.assignments.len(),
                        "terms": relocation.terms.len(),
                        "files": relocation.files.len(),
                    }),
                },
            )
            .await
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

fn map_relocation_write_error(error: sqlx::Error) -> MaviError {
    if let sqlx::Error::Database(database) = error {
        if database.is_unique_violation() {
            return MaviError::conflict(TRASH_RELOCATION_CONFLICT);
        }
        if database.is_foreign_key_violation() || database.is_check_violation() {
            return MaviError::validation("trash_relocation_reference_invalid");
        }
    }
    MaviError::Internal
}

fn ordered_terms(terms: &[TrashTermRelocation]) -> Result<Vec<&TrashTermRelocation>> {
    let deleted_ids = terms.iter().map(|term| term.id).collect::<BTreeSet<_>>();
    let mut pending = terms.iter().collect::<Vec<_>>();
    let mut inserted = BTreeSet::new();
    let mut ordered = Vec::with_capacity(terms.len());
    while !pending.is_empty() {
        let position = pending.iter().position(|term| {
            term.parent_id.is_none_or(|parent_id| {
                !deleted_ids.contains(&parent_id) || inserted.contains(&parent_id)
            })
        });
        let Some(position) = position else {
            return Err(MaviError::validation("trash_relocation_term_cycle"));
        };
        let term = pending.remove(position);
        inserted.insert(term.id);
        ordered.push(term);
    }
    Ok(ordered)
}

fn parse_file_kind(value: &str) -> Result<FileKind> {
    match value {
        "image" => Ok(FileKind::Image),
        "video" => Ok(FileKind::Video),
        "audio" => Ok(FileKind::Audio),
        "document" => Ok(FileKind::Document),
        _ => Err(MaviError::validation("trash_relocation_file_invalid")),
    }
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

fn valid_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_lowercase())
        && value.chars().count() <= 31
        && chars.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
}

fn valid_language(value: &str) -> bool {
    (2..=35).contains(&value.chars().count())
        && !value.chars().any(char::is_control)
        && !value.is_empty()
}

fn valid_slug(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 255
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && (bytes[bytes.len() - 1].is_ascii_lowercase() || bytes[bytes.len() - 1].is_ascii_digit())
        && bytes
            .windows(2)
            .all(|pair| pair[0] != b'-' || pair[1] != b'-')
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

fn valid_text(value: &str, max_chars: usize) -> bool {
    !value.is_empty() && value.chars().count() <= max_chars && !value.chars().any(char::is_control)
}

fn valid_status(value: &str, scheduled: bool, published: bool) -> bool {
    match value {
        "draft" | "archived" => !scheduled && !published,
        "scheduled" => scheduled && !published,
        "published" => !scheduled && published,
        _ => false,
    }
}

fn valid_mime(value: &str) -> bool {
    let Some((kind, subtype)) = value.split_once('/') else {
        return false;
    };
    !kind.is_empty()
        && !subtype.is_empty()
        && kind.bytes().all(|byte| byte.is_ascii_lowercase())
        && subtype.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b".+-".contains(&byte)
        })
}

fn valid_storage_key(value: &str) -> bool {
    let Some((prefix, name)) = value.split_once('/') else {
        return false;
    };
    let Some((digest, extension)) = name.rsplit_once('.') else {
        return false;
    };
    prefix.len() == 2
        && prefix.bytes().all(|byte| byte.is_ascii_hexdigit())
        && digest.len() == 30
        && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        && (2..=5).contains(&extension.len())
        && extension
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

fn hex_digest(digest: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

mod base64_bytes {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _};

    pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&URL_SAFE_NO_PAD.encode(bytes))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        URL_SAFE_NO_PAD.decode(encoded).map_err(D::Error::custom)
    }
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

    #[test]
    fn relocation_is_site_bound_and_orders_deleted_term_parents() {
        let site = SiteId::new();
        let empty = TrashRelocation::empty(site);
        assert!(empty.validate_for_relocation(site).is_ok());
        assert!(empty.validate_for_relocation(SiteId::new()).is_err());

        let parent_id = Uuid::now_v7();
        let child_id = Uuid::now_v7();
        let now = Utc::now();
        let terms = vec![
            TrashTermRelocation {
                id: child_id,
                kind: "category".to_owned(),
                language: "en".to_owned(),
                slug: "child".to_owned(),
                name: "Child".to_owned(),
                parent_id: Some(parent_id),
                created_at: now,
                updated_at: now,
                deleted_at: now,
            },
            TrashTermRelocation {
                id: parent_id,
                kind: "category".to_owned(),
                language: "en".to_owned(),
                slug: "parent".to_owned(),
                name: "Parent".to_owned(),
                parent_id: None,
                created_at: now,
                updated_at: now,
                deleted_at: now,
            },
        ];
        let ordered = ordered_terms(&terms).expect("ordered terms");
        assert_eq!(ordered[0].id, parent_id);
        assert_eq!(ordered[1].id, child_id);
    }
}
