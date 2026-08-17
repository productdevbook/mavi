//! Versioned, validated site export/import bundles.
//!
//! A portable bundle is an application-level snapshot, not a `PostgreSQL` dump.
//! It contains only explicitly supported records, carries a schema hash and
//! source-site provenance, and is validated before any import write starts.
//! Import runs in the caller's transaction, so a failed reference or conflict
//! cannot leave a partially migrated site behind.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use chrono::{DateTime, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{Action, Capability, ErrorCode, MaviError, Result, SiteContext, SiteId};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

pub const FORMAT: &str = "mavi.portable";
pub const VERSION: u16 = 1;
pub const MAX_RECORDS_PER_SECTION: usize = 10_000;
pub const MAX_TOTAL_RECORDS: usize = 20_000;
pub const MAX_BUNDLE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_FIELDS_BYTES: usize = 64 * 1024;

const SCHEMA_DESCRIPTOR: &str = concat!(
    "mavi.portable:v1:",
    "site,languages,content_types,terms,content,revisions,slug_history,assignments"
);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PortableBundle {
    pub manifest: PortableManifest,
    pub site: PortableSite,
    pub languages: Vec<PortableLanguage>,
    pub content_types: Vec<PortableContentType>,
    pub terms: Vec<PortableTerm>,
    pub content: Vec<PortableContent>,
    pub revisions: Vec<PortableRevision>,
    pub slug_history: Vec<PortableSlugHistory>,
    pub assignments: Vec<PortableAssignment>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PortableManifest {
    pub format: String,
    pub version: u16,
    pub source_site_id: SiteId,
    pub exported_at: DateTime<Utc>,
    pub schema_hash: String,
    pub counts: PortableCounts,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PortableCounts {
    pub languages: usize,
    pub content_types: usize,
    pub terms: usize,
    pub content: usize,
    pub revisions: usize,
    pub slug_history: usize,
    pub assignments: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PortableSite {
    pub name: String,
    pub timezone: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PortableLanguage {
    pub tag: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PortableContentType {
    pub kind: String,
    pub name: String,
    pub fields: Value,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PortableTerm {
    pub id: Uuid,
    pub kind: String,
    pub language: String,
    pub slug: String,
    pub name: String,
    pub parent_id: Option<Uuid>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PortableContent {
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
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PortableRevision {
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
pub struct PortableSlugHistory {
    pub content_id: Uuid,
    pub language: String,
    pub slug: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PortableAssignment {
    pub content_id: Uuid,
    pub term_id: Uuid,
    pub assigned_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportStrategy {
    ValidateOnly,
    CreateOnly,
    Upsert,
}

impl ImportStrategy {
    const fn writes(self) -> bool {
        !matches!(self, Self::ValidateOnly)
    }

    const fn upsert(self) -> bool {
        matches!(self, Self::Upsert)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PortableImportRequest {
    pub bundle: PortableBundle,
    pub strategy: ImportStrategy,
}

#[derive(Clone, Debug, Serialize)]
pub struct ImportReceipt {
    pub strategy: ImportStrategy,
    pub languages: u32,
    pub content_types: u32,
    pub terms: u32,
    pub content: u32,
    pub revisions: u32,
    pub slug_history: u32,
    pub assignments: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PortableService;

#[must_use]
#[allow(clippy::too_many_lines)]
pub fn api() -> mavi_contract::Api {
    let view = Permission {
        capability: Capability::Portable,
        action: Action::View,
    };
    let write = Permission {
        capability: Capability::Portable,
        action: Action::Write,
    };
    mavi_contract::Api::new([
        Endpoint::new(
            Method::Get,
            "/api/v1/portable/export",
            "portable.export",
            "Export an explicit versioned site bundle",
        )
        .account_or_assistant()
        .requires(view)
        .returns(200, "PortableBundle")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/portable/import",
            "portable.import",
            "Validate or atomically import a versioned site bundle",
        )
        .account_or_assistant()
        .requires(write)
        .takes("PortableImportRequest")
        .returns(200, "ImportReceipt")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Conflict,
            ErrorCode::Validation,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
    ])
    .with_shapes(shapes())
}

#[must_use]
#[allow(clippy::too_many_lines)]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "PortableCounts",
            json!({
                "type":"object",
                "required":["languages","content_types","terms","content","revisions","slug_history","assignments"],
                "properties":{"languages":{"type":"integer","minimum":0},"content_types":{"type":"integer","minimum":0},"terms":{"type":"integer","minimum":0},"content":{"type":"integer","minimum":0},"revisions":{"type":"integer","minimum":0},"slug_history":{"type":"integer","minimum":0},"assignments":{"type":"integer","minimum":0}}
            }),
        ),
        Shape::new(
            "PortableManifest",
            json!({
                "type":"object",
                "required":["format","version","source_site_id","exported_at","schema_hash","counts"],
                "properties":{"format":{"type":"string","const":"mavi.portable"},"version":{"type":"integer","const":1},"source_site_id":{"type":"string","format":"uuid"},"exported_at":{"type":"string","format":"date-time"},"schema_hash":{"type":"string"},"counts":{"$ref":"#/components/schemas/PortableCounts"}}
            }),
        ),
        Shape::new(
            "PortableSite",
            json!({"type":"object","required":["name","timezone"],"properties":{"name":{"type":"string","maxLength":200},"timezone":{"type":"string","maxLength":64}}}),
        ),
        Shape::new(
            "PortableLanguage",
            json!({"type":"object","required":["tag","name","is_default"],"properties":{"tag":{"type":"string","maxLength":35},"name":{"type":"string","maxLength":120},"is_default":{"type":"boolean"}}}),
        ),
        Shape::new(
            "PortableContentType",
            json!({"type":"object","required":["kind","name","fields"],"properties":{"kind":{"type":"string","maxLength":31},"name":{"type":"string","maxLength":100},"fields":{"type":"array"}}}),
        ),
        Shape::new(
            "PortableTerm",
            json!({"type":"object","required":["id","kind","language","slug","name","parent_id"],"properties":{"id":{"type":"string","format":"uuid"},"kind":{"type":"string","enum":["category","tag"]},"language":{"type":"string","maxLength":35},"slug":{"type":"string","maxLength":160},"name":{"type":"string","maxLength":100},"parent_id":{"type":["string","null"],"format":"uuid"}}}),
        ),
        Shape::new(
            "PortableContent",
            json!({"type":"object","required":["id","kind","language","slug","title","excerpt","body","fields","status","scheduled_at","published_at","revision","created_at","updated_at"],"properties":{"id":{"type":"string","format":"uuid"},"kind":{"type":"string"},"language":{"type":"string"},"slug":{"type":"string"},"title":{"type":"string","maxLength":200},"excerpt":{"type":["string","null"]},"body":{"type":"string"},"fields":{"type":"object"},"status":{"type":"string","enum":["draft","scheduled","published","archived"]},"scheduled_at":{"type":["string","null"],"format":"date-time"},"published_at":{"type":["string","null"],"format":"date-time"},"revision":{"type":"integer","minimum":1},"created_at":{"type":"string","format":"date-time"},"updated_at":{"type":"string","format":"date-time"}}}),
        ),
        Shape::new(
            "PortableRevision",
            json!({"type":"object","required":["content_id","revision","kind","language","slug","title","excerpt","body","fields","status","scheduled_at","published_at","created_at"],"properties":{"content_id":{"type":"string","format":"uuid"},"revision":{"type":"integer","minimum":1},"kind":{"type":"string"},"language":{"type":"string"},"slug":{"type":"string"},"title":{"type":"string"},"excerpt":{"type":["string","null"]},"body":{"type":"string"},"fields":{"type":"object"},"status":{"type":"string"},"scheduled_at":{"type":["string","null"],"format":"date-time"},"published_at":{"type":["string","null"],"format":"date-time"},"created_at":{"type":"string","format":"date-time"}}}),
        ),
        Shape::new(
            "PortableSlugHistory",
            json!({"type":"object","required":["content_id","language","slug","created_at"],"properties":{"content_id":{"type":"string","format":"uuid"},"language":{"type":"string"},"slug":{"type":"string"},"created_at":{"type":"string","format":"date-time"}}}),
        ),
        Shape::new(
            "PortableAssignment",
            json!({"type":"object","required":["content_id","term_id","assigned_at"],"properties":{"content_id":{"type":"string","format":"uuid"},"term_id":{"type":"string","format":"uuid"},"assigned_at":{"type":"string","format":"date-time"}}}),
        ),
        Shape::new(
            "PortableBundle",
            json!({
                "type":"object",
                "required":["manifest","site","languages","content_types","terms","content","revisions","slug_history","assignments"],
                "properties":{"manifest":{"$ref":"#/components/schemas/PortableManifest"},"site":{"$ref":"#/components/schemas/PortableSite"},"languages":{"type":"array","items":{"$ref":"#/components/schemas/PortableLanguage"}},"content_types":{"type":"array","items":{"$ref":"#/components/schemas/PortableContentType"}},"terms":{"type":"array","items":{"$ref":"#/components/schemas/PortableTerm"}},"content":{"type":"array","items":{"$ref":"#/components/schemas/PortableContent"}},"revisions":{"type":"array","items":{"$ref":"#/components/schemas/PortableRevision"}},"slug_history":{"type":"array","items":{"$ref":"#/components/schemas/PortableSlugHistory"}},"assignments":{"type":"array","items":{"$ref":"#/components/schemas/PortableAssignment"}}}
            }),
        ),
        Shape::new(
            "ImportStrategy",
            json!({"type":"string","enum":["validate_only","create_only","upsert"]}),
        ),
        Shape::new(
            "PortableImportRequest",
            json!({"type":"object","required":["bundle","strategy"],"properties":{"bundle":{"$ref":"#/components/schemas/PortableBundle"},"strategy":{"$ref":"#/components/schemas/ImportStrategy"}}}),
        ),
        Shape::new(
            "ImportReceipt",
            json!({"type":"object","required":["strategy","languages","content_types","terms","content","revisions","slug_history","assignments"],"properties":{"strategy":{"$ref":"#/components/schemas/ImportStrategy"},"languages":{"type":"integer","minimum":0},"content_types":{"type":"integer","minimum":0},"terms":{"type":"integer","minimum":0},"content":{"type":"integer","minimum":0},"revisions":{"type":"integer","minimum":0},"slug_history":{"type":"integer","minimum":0},"assignments":{"type":"integer","minimum":0}}}),
        ),
    ]
}

impl PortableBundle {
    pub fn validate_for_site(&self, target_site: SiteId) -> Result<()> {
        self.validate_for_target(target_site, false)
    }

    /// Validate a bundle for an internal shard relocation.
    ///
    /// A relocation keeps the logical `SiteId` stable, so it is intentionally
    /// different from a user-requested import. The caller must still provide
    /// the same source and target site, and the service only exposes this
    /// path as an internal application port; there is no public HTTP endpoint
    /// for it.
    pub fn validate_for_relocation(&self, target_site: SiteId) -> Result<()> {
        if self.manifest.source_site_id != target_site {
            return Err(MaviError::conflict("portable_relocation_site_mismatch"));
        }
        self.validate_for_target(target_site, true)
    }

    fn validate_for_target(&self, target_site: SiteId, allow_same_site: bool) -> Result<()> {
        if self.manifest.format != FORMAT {
            return Err(MaviError::validation("portable_format_invalid"));
        }
        if self.manifest.version != VERSION {
            return Err(MaviError::validation("portable_version_unsupported"));
        }
        if self.manifest.schema_hash != schema_hash() {
            return Err(MaviError::validation("portable_schema_hash_invalid"));
        }
        if self.manifest.source_site_id.into_uuid().is_nil() {
            return Err(MaviError::validation("portable_source_site_invalid"));
        }
        if self.manifest.source_site_id == target_site && !allow_same_site {
            return Err(MaviError::conflict("portable_self_import_forbidden"));
        }

        let counts = PortableCounts {
            languages: self.languages.len(),
            content_types: self.content_types.len(),
            terms: self.terms.len(),
            content: self.content.len(),
            revisions: self.revisions.len(),
            slug_history: self.slug_history.len(),
            assignments: self.assignments.len(),
        };
        if self.manifest.counts != counts
            || counts.languages > MAX_RECORDS_PER_SECTION
            || counts.content_types > MAX_RECORDS_PER_SECTION
            || counts.terms > MAX_RECORDS_PER_SECTION
            || counts.content > MAX_RECORDS_PER_SECTION
            || counts.revisions > MAX_RECORDS_PER_SECTION
            || counts.slug_history > MAX_RECORDS_PER_SECTION
            || counts.assignments > MAX_RECORDS_PER_SECTION
            || total_records(&counts) > MAX_TOTAL_RECORDS
        {
            return Err(MaviError::validation("portable_counts_invalid"));
        }

        let bytes = serde_json::to_vec(self).map_err(|_| MaviError::Internal)?;
        if bytes.len() > MAX_BUNDLE_BYTES {
            return Err(MaviError::validation("portable_bundle_too_large"));
        }
        validate_site(&self.site)?;
        validate_languages(&self.languages)?;
        validate_content_types(&self.content_types)?;
        validate_terms(&self.terms)?;
        validate_content(&self.languages, &self.content, &self.revisions)?;
        validate_slug_history(&self.content, &self.slug_history)?;
        validate_assignments(&self.content, &self.terms, &self.assignments)?;
        Ok(())
    }
}

impl PortableService {
    #[allow(clippy::too_many_lines, clippy::similar_names)]
    pub async fn export(&self, tx: &mut SiteTx, context: &SiteContext) -> Result<PortableBundle> {
        let site = sqlx::query("select name, timezone from site_settings where site_id = $1")
            .bind(context.site_id.into_uuid())
            .fetch_optional(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?
            .ok_or(MaviError::NotFound {
                resource: "site_settings",
            })?;
        let site = PortableSite {
            name: site.try_get("name").map_err(|_| MaviError::Internal)?,
            timezone: site.try_get("timezone").map_err(|_| MaviError::Internal)?,
        };

        let languages = sqlx::query(
            "select tag, name, is_default from site_languages
             where site_id = $1 order by created_at asc, tag asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(PortableLanguage {
                tag: row.try_get("tag").map_err(|_| MaviError::Internal)?,
                name: row.try_get("name").map_err(|_| MaviError::Internal)?,
                is_default: row.try_get("is_default").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let content_types = sqlx::query(
            "select kind, name, fields from content_types
             where site_id = $1 order by created_at asc, kind asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(PortableContentType {
                kind: row.try_get("kind").map_err(|_| MaviError::Internal)?,
                name: row.try_get("name").map_err(|_| MaviError::Internal)?,
                fields: row.try_get("fields").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let terms = sqlx::query(
            "select id, kind, language, slug, name, parent_id from taxonomy_terms
             where site_id = $1 and deleted_at is null order by created_at asc, id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(PortableTerm {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                kind: row.try_get("kind").map_err(|_| MaviError::Internal)?,
                language: row.try_get("language").map_err(|_| MaviError::Internal)?,
                slug: row.try_get("slug").map_err(|_| MaviError::Internal)?,
                name: row.try_get("name").map_err(|_| MaviError::Internal)?,
                parent_id: row.try_get("parent_id").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let content_rows = sqlx::query(
            "select id, kind, language, slug, title, excerpt, body, fields, status,
                    scheduled_at, published_at, revision, created_at, updated_at
             from content_entries where site_id = $1 and deleted_at is null
             order by created_at asc, id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(content_row)
        .collect::<Result<Vec<_>>>()?;
        let content_ids = content_rows.iter().map(|item| item.id).collect::<Vec<_>>();

        let revisions = sqlx::query(
            "select r.content_id, r.revision, r.kind, r.language, r.slug, r.title,
                    r.excerpt, r.body, r.fields, r.status, r.scheduled_at,
                    r.published_at, r.created_at
             from content_revisions r
             join content_entries c on c.site_id = r.site_id and c.id = r.content_id
             where r.site_id = $1 and c.deleted_at is null
             order by r.content_id asc, r.revision asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(revision_row)
        .collect::<Result<Vec<_>>>()?;

        let slug_history = sqlx::query(
            "select h.content_id, h.language, h.slug, h.created_at
             from content_slug_history h
             join content_entries c on c.site_id = h.site_id and c.id = h.content_id
             where h.site_id = $1 and c.deleted_at is null
             order by h.content_id asc, h.created_at asc, h.slug asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(PortableSlugHistory {
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
             join content_entries c on c.site_id = a.site_id and c.id = a.content_id
             join taxonomy_terms t on t.site_id = a.site_id and t.id = a.term_id
             where a.site_id = $1 and c.deleted_at is null and t.deleted_at is null
             order by a.content_id asc, a.term_id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(PortableAssignment {
                content_id: row.try_get("content_id").map_err(|_| MaviError::Internal)?,
                term_id: row.try_get("term_id").map_err(|_| MaviError::Internal)?,
                assigned_at: row
                    .try_get("assigned_at")
                    .map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let counts = PortableCounts {
            languages: languages.len(),
            content_types: content_types.len(),
            terms: terms.len(),
            content: content_rows.len(),
            revisions: revisions.len(),
            slug_history: slug_history.len(),
            assignments: assignments.len(),
        };
        let bundle = PortableBundle {
            manifest: PortableManifest {
                format: FORMAT.to_owned(),
                version: VERSION,
                source_site_id: context.site_id,
                exported_at: Utc::now(),
                schema_hash: schema_hash(),
                counts,
            },
            site,
            languages,
            content_types,
            terms,
            content: content_rows,
            revisions,
            slug_history,
            assignments,
        };
        bundle.validate_for_site(SiteId::new())?;
        if bundle.manifest.source_site_id != context.site_id {
            return Err(MaviError::Internal);
        }
        // `content_ids` is intentionally computed above to make the export
        // boundary explicit; the validation below also guards future query
        // changes from returning orphaned revisions.
        if bundle
            .revisions
            .iter()
            .any(|revision| !content_ids.contains(&revision.content_id))
        {
            return Err(MaviError::Internal);
        }
        Ok(bundle)
    }

    #[allow(clippy::too_many_lines)]
    pub async fn import(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        request: &PortableImportRequest,
    ) -> Result<ImportReceipt> {
        request.bundle.validate_for_site(context.site_id)?;
        self.import_validated(tx, context, request, "portable.bundle.imported", false)
            .await
    }

    /// Relocate a site bundle into its existing logical site on another shard.
    ///
    /// This is deliberately restricted to an upsert because relocation is a
    /// retryable worker operation and must be safe after a partial network
    /// failure. The caller owns the transaction and must commit it only after
    /// this method succeeds.
    pub async fn relocate(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        request: &PortableImportRequest,
    ) -> Result<ImportReceipt> {
        if request.strategy != ImportStrategy::Upsert {
            return Err(MaviError::validation(
                "portable_relocation_strategy_invalid",
            ));
        }
        request.bundle.validate_for_relocation(context.site_id)?;
        self.import_validated(tx, context, request, "portable.bundle.relocated", true)
            .await
    }

    #[allow(clippy::too_many_lines)]
    async fn import_validated(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        request: &PortableImportRequest,
        audit_action: &str,
        initialize_site_settings: bool,
    ) -> Result<ImportReceipt> {
        let receipt = receipt_for(request.strategy, &request.bundle)?;
        if !request.strategy.writes() {
            return Ok(receipt);
        }

        validate_external_parents(tx, &request.bundle.terms).await?;
        if !request.strategy.upsert() {
            ensure_create_only(tx, &request.bundle).await?;
        }

        let settings = if initialize_site_settings {
            sqlx::query(
                "insert into site_settings (site_id, name, timezone)
                 values ($1, $2, $3)
                 on conflict (site_id) do update set
                    name = excluded.name, timezone = excluded.timezone, updated_at = now()",
            )
        } else {
            sqlx::query(
                "update site_settings set name = $2, timezone = $3, updated_at = now()
                 where site_id = $1",
            )
        };
        settings
            .bind(context.site_id.into_uuid())
            .bind(&request.bundle.site.name)
            .bind(&request.bundle.site.timezone)
            .execute(tx.conn())
            .await
            .map_err(map_import_write_error)?;

        sqlx::query(
            "update site_languages set is_default = false, updated_at = now() where site_id = $1",
        )
        .bind(context.site_id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(map_import_write_error)?;
        for language in &request.bundle.languages {
            if request.strategy.upsert() {
                sqlx::query(
                    "insert into site_languages (site_id, tag, name, is_default)
                     values ($1, $2, $3, $4)
                     on conflict (site_id, tag) do update set
                        name = excluded.name, is_default = excluded.is_default, updated_at = now()",
                )
            } else {
                sqlx::query(
                    "insert into site_languages (site_id, tag, name, is_default)
                     values ($1, $2, $3, $4)",
                )
            }
            .bind(context.site_id.into_uuid())
            .bind(&language.tag)
            .bind(&language.name)
            .bind(language.is_default)
            .execute(tx.conn())
            .await
            .map_err(map_import_write_error)?;
        }

        for content_type in &request.bundle.content_types {
            if request.strategy.upsert() {
                sqlx::query(
                    "insert into content_types (site_id, kind, name, fields)
                     values ($1, $2, $3, $4)
                     on conflict (site_id, kind) do update set
                        name = excluded.name, fields = excluded.fields, updated_at = now()",
                )
            } else {
                sqlx::query(
                    "insert into content_types (site_id, kind, name, fields)
                     values ($1, $2, $3, $4)",
                )
            }
            .bind(context.site_id.into_uuid())
            .bind(&content_type.kind)
            .bind(&content_type.name)
            .bind(&content_type.fields)
            .execute(tx.conn())
            .await
            .map_err(map_import_write_error)?;
        }

        for term in ordered_terms(&request.bundle.terms)? {
            if request.strategy.upsert() {
                sqlx::query(
                    "insert into taxonomy_terms
                        (site_id, id, kind, language, slug, name, parent_id)
                     values ($1, $2, $3, $4, $5, $6, $7)
                     on conflict (site_id, id) do update set
                        kind = excluded.kind, language = excluded.language,
                        slug = excluded.slug, name = excluded.name,
                        parent_id = excluded.parent_id, deleted_at = null, updated_at = now()",
                )
            } else {
                sqlx::query(
                    "insert into taxonomy_terms
                        (site_id, id, kind, language, slug, name, parent_id)
                     values ($1, $2, $3, $4, $5, $6, $7)",
                )
            }
            .bind(context.site_id.into_uuid())
            .bind(term.id)
            .bind(&term.kind)
            .bind(&term.language)
            .bind(&term.slug)
            .bind(&term.name)
            .bind(term.parent_id)
            .execute(tx.conn())
            .await
            .map_err(map_import_write_error)?;
        }

        for item in &request.bundle.content {
            if request.strategy.upsert() {
                sqlx::query(
                    "insert into content_entries
                        (site_id, id, kind, language, slug, title, excerpt, body,
                         fields, status, scheduled_at, published_at, revision,
                         created_at, updated_at)
                     values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                             $13, $14, $15)
                     on conflict (site_id, id) do update set
                        kind = excluded.kind, language = excluded.language,
                        slug = excluded.slug, title = excluded.title,
                        excerpt = excluded.excerpt, body = excluded.body,
                        fields = excluded.fields, status = excluded.status,
                        scheduled_at = excluded.scheduled_at,
                        published_at = excluded.published_at,
                        revision = excluded.revision, deleted_at = null,
                        updated_at = excluded.updated_at",
                )
            } else {
                sqlx::query(
                    "insert into content_entries
                        (site_id, id, kind, language, slug, title, excerpt, body,
                         fields, status, scheduled_at, published_at, revision,
                         created_at, updated_at)
                     values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                             $13, $14, $15)",
                )
            }
            .bind(context.site_id.into_uuid())
            .bind(item.id)
            .bind(&item.kind)
            .bind(&item.language)
            .bind(&item.slug)
            .bind(&item.title)
            .bind(&item.excerpt)
            .bind(&item.body)
            .bind(&item.fields)
            .bind(&item.status)
            .bind(item.scheduled_at)
            .bind(item.published_at)
            .bind(
                i32::try_from(item.revision)
                    .map_err(|_| MaviError::validation("portable_revision_invalid"))?,
            )
            .bind(item.created_at)
            .bind(item.updated_at)
            .execute(tx.conn())
            .await
            .map_err(map_import_write_error)?;
        }

        for revision in &request.bundle.revisions {
            if request.strategy.upsert() {
                sqlx::query(
                    "insert into content_revisions
                        (site_id, content_id, revision, kind, language, slug, title,
                         excerpt, body, fields, status, scheduled_at, published_at,
                         created_at)
                     values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                             $13, $14)
                     on conflict (site_id, content_id, revision) do update set
                        kind = excluded.kind, language = excluded.language,
                        slug = excluded.slug, title = excluded.title,
                        excerpt = excluded.excerpt, body = excluded.body,
                        fields = excluded.fields, status = excluded.status,
                        scheduled_at = excluded.scheduled_at,
                        published_at = excluded.published_at,
                        created_at = excluded.created_at",
                )
            } else {
                sqlx::query(
                    "insert into content_revisions
                        (site_id, content_id, revision, kind, language, slug, title,
                         excerpt, body, fields, status, scheduled_at, published_at,
                         created_at)
                     values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                             $13, $14)",
                )
            }
            .bind(context.site_id.into_uuid())
            .bind(revision.content_id)
            .bind(
                i32::try_from(revision.revision)
                    .map_err(|_| MaviError::validation("portable_revision_invalid"))?,
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
            .map_err(map_import_write_error)?;
        }

        for history in &request.bundle.slug_history {
            if request.strategy.upsert() {
                sqlx::query(
                    "insert into content_slug_history
                        (site_id, content_id, language, slug, created_at)
                     values ($1, $2, $3, $4, $5)
                     on conflict (site_id, content_id, language, slug) do nothing",
                )
            } else {
                sqlx::query(
                    "insert into content_slug_history
                        (site_id, content_id, language, slug, created_at)
                     values ($1, $2, $3, $4, $5)",
                )
            }
            .bind(context.site_id.into_uuid())
            .bind(history.content_id)
            .bind(&history.language)
            .bind(&history.slug)
            .bind(history.created_at)
            .execute(tx.conn())
            .await
            .map_err(map_import_write_error)?;
        }

        for assignment in &request.bundle.assignments {
            if request.strategy.upsert() {
                sqlx::query(
                    "insert into content_term_assignments
                        (site_id, content_id, term_id, assigned_at)
                     values ($1, $2, $3, $4)
                     on conflict (site_id, content_id, term_id) do update set
                        assigned_at = excluded.assigned_at",
                )
            } else {
                sqlx::query(
                    "insert into content_term_assignments
                        (site_id, content_id, term_id, assigned_at)
                     values ($1, $2, $3, $4)",
                )
            }
            .bind(context.site_id.into_uuid())
            .bind(assignment.content_id)
            .bind(assignment.term_id)
            .bind(assignment.assigned_at)
            .execute(tx.conn())
            .await
            .map_err(map_import_write_error)?;
        }

        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: audit_action.to_owned(),
                    resource_type: "PortableBundle".to_owned(),
                    resource_id: None,
                    payload: json!({
                        "source_site_id": request.bundle.manifest.source_site_id,
                        "version": request.bundle.manifest.version,
                        "strategy": request.strategy,
                        "counts": request.bundle.manifest.counts,
                    }),
                },
            )
            .await?;
        Ok(receipt)
    }
}

fn schema_hash() -> String {
    let digest = Sha256::digest(SCHEMA_DESCRIPTOR.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut hex, "{byte:02x}").expect("writing to a String cannot fail");
    }
    format!("sha256:{hex}")
}

fn total_records(counts: &PortableCounts) -> usize {
    counts.languages
        + counts.content_types
        + counts.terms
        + counts.content
        + counts.revisions
        + counts.slug_history
        + counts.assignments
}

fn validate_site(site: &PortableSite) -> Result<()> {
    if site.name.trim().is_empty() || site.name.chars().count() > 200 {
        return Err(MaviError::validation("portable_site_name_invalid"));
    }
    if site.timezone.trim().is_empty()
        || site.timezone.len() > 64
        || site.timezone.chars().any(char::is_whitespace)
    {
        return Err(MaviError::validation("portable_timezone_invalid"));
    }
    Ok(())
}

fn validate_languages(languages: &[PortableLanguage]) -> Result<()> {
    let mut tags = BTreeSet::new();
    let mut defaults = 0;
    for language in languages {
        if !tags.insert(language.tag.clone()) || !valid_language(&language.tag) {
            return Err(MaviError::validation("portable_language_invalid"));
        }
        if language.name.trim().is_empty() || language.name.chars().count() > 120 {
            return Err(MaviError::validation("portable_language_invalid"));
        }
        defaults += usize::from(language.is_default);
    }
    if languages.is_empty() || defaults != 1 {
        return Err(MaviError::validation("portable_default_language_invalid"));
    }
    Ok(())
}

fn validate_content_types(content_types: &[PortableContentType]) -> Result<()> {
    let mut kinds = BTreeSet::new();
    for content_type in content_types {
        if !kinds.insert(content_type.kind.clone())
            || !valid_kind(&content_type.kind)
            || content_type.name.trim().is_empty()
            || content_type.name.chars().count() > 100
            || !content_type.fields.is_array()
            || serialized_size(&content_type.fields)? > MAX_FIELDS_BYTES
        {
            return Err(MaviError::validation("portable_content_type_invalid"));
        }
    }
    Ok(())
}

fn validate_terms(terms: &[PortableTerm]) -> Result<()> {
    let mut ids = BTreeSet::new();
    let mut slugs = BTreeSet::new();
    for term in terms {
        if term.id.is_nil()
            || !ids.insert(term.id)
            || !matches!(term.kind.as_str(), "category" | "tag")
            || !valid_language(&term.language)
            || !valid_slug(&term.slug)
            || term.name.trim().is_empty()
            || term.name.chars().count() > 100
            || !slugs.insert((term.kind.clone(), term.language.clone(), term.slug.clone()))
        {
            return Err(MaviError::validation("portable_term_invalid"));
        }
        if term.kind == "tag" && term.parent_id.is_some() {
            return Err(MaviError::validation("portable_term_parent_invalid"));
        }
    }
    for term in terms {
        if let Some(parent_id) = term.parent_id {
            if parent_id == term.id {
                return Err(MaviError::validation("portable_term_cycle"));
            }
            if let Some(parent) = terms.iter().find(|candidate| candidate.id == parent_id)
                && (parent.kind != "category" || parent.language != term.language)
            {
                return Err(MaviError::validation("portable_term_parent_invalid"));
            }
        }
    }
    for term in terms {
        let mut seen = BTreeSet::new();
        let mut current = term.parent_id;
        while let Some(parent_id) = current {
            if !seen.insert(parent_id) {
                return Err(MaviError::validation("portable_term_cycle"));
            }
            current = terms
                .iter()
                .find(|candidate| candidate.id == parent_id)
                .and_then(|parent| parent.parent_id);
        }
    }
    Ok(())
}

fn validate_content(
    languages: &[PortableLanguage],
    content: &[PortableContent],
    revisions: &[PortableRevision],
) -> Result<()> {
    let language_tags = languages
        .iter()
        .map(|language| language.tag.as_str())
        .collect::<BTreeSet<_>>();
    let mut ids = BTreeSet::new();
    let mut slugs = BTreeSet::new();
    for item in content {
        if item.id.is_nil()
            || !ids.insert(item.id)
            || !valid_kind(&item.kind)
            || !language_tags.contains(item.language.as_str())
            || !valid_language(&item.language)
            || !valid_slug(&item.slug)
            || !slugs.insert((item.language.clone(), item.slug.clone()))
            || item.title.trim().is_empty()
            || item.title.chars().count() > 200
            || !item.fields.is_object()
            || serialized_size(&item.fields)? > MAX_FIELDS_BYTES
            || item.revision == 0
            || !valid_publication(&item.status, item.scheduled_at, item.published_at)
        {
            return Err(MaviError::validation("portable_content_invalid"));
        }
    }
    let content_ids = content.iter().map(|item| item.id).collect::<BTreeSet<_>>();
    let mut revisions_seen = BTreeSet::new();
    for revision in revisions {
        if !content_ids.contains(&revision.content_id)
            || revision.revision == 0
            || !revisions_seen.insert((revision.content_id, revision.revision))
            || !valid_kind(&revision.kind)
            || !language_tags.contains(revision.language.as_str())
            || !valid_language(&revision.language)
            || !valid_slug(&revision.slug)
            || revision.title.trim().is_empty()
            || revision.title.chars().count() > 200
            || !revision.fields.is_object()
            || serialized_size(&revision.fields)? > MAX_FIELDS_BYTES
            || !valid_publication(
                &revision.status,
                revision.scheduled_at,
                revision.published_at,
            )
        {
            return Err(MaviError::validation("portable_revision_invalid"));
        }
    }
    if content
        .iter()
        .any(|item| !revisions_seen.contains(&(item.id, item.revision)))
    {
        return Err(MaviError::validation("portable_current_revision_missing"));
    }
    Ok(())
}

fn validate_slug_history(
    content: &[PortableContent],
    history: &[PortableSlugHistory],
) -> Result<()> {
    let content_ids = content.iter().map(|item| item.id).collect::<BTreeSet<_>>();
    let languages = content
        .iter()
        .map(|item| item.language.clone())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    for item in history {
        if !content_ids.contains(&item.content_id)
            || !languages.contains(&item.language)
            || !valid_language(&item.language)
            || !valid_slug(&item.slug)
            || !seen.insert((item.content_id, item.language.clone(), item.slug.clone()))
        {
            return Err(MaviError::validation("portable_slug_history_invalid"));
        }
    }
    Ok(())
}

fn validate_assignments(
    content: &[PortableContent],
    terms: &[PortableTerm],
    assignments: &[PortableAssignment],
) -> Result<()> {
    let content_ids = content.iter().map(|item| item.id).collect::<BTreeSet<_>>();
    let term_ids = terms.iter().map(|item| item.id).collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    for assignment in assignments {
        if !content_ids.contains(&assignment.content_id)
            || !term_ids.contains(&assignment.term_id)
            || !seen.insert((assignment.content_id, assignment.term_id))
        {
            return Err(MaviError::validation("portable_assignment_invalid"));
        }
    }
    Ok(())
}

fn valid_language(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 35
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
        })
}

fn valid_kind(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 31
        && value.as_bytes()[0].is_ascii_lowercase()
        && value.bytes().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == b'_'
        })
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        })
}

fn valid_publication(
    status: &str,
    scheduled_at: Option<DateTime<Utc>>,
    published_at: Option<DateTime<Utc>>,
) -> bool {
    match status {
        "draft" | "archived" => scheduled_at.is_none() && published_at.is_none(),
        "scheduled" => scheduled_at.is_some() && published_at.is_none(),
        "published" => scheduled_at.is_none() && published_at.is_some(),
        _ => false,
    }
}

fn serialized_size(value: &Value) -> Result<usize> {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .map_err(|_| MaviError::Internal)
}

fn content_row(row: &sqlx::postgres::PgRow) -> Result<PortableContent> {
    Ok(PortableContent {
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
    })
}

fn revision_row(row: &sqlx::postgres::PgRow) -> Result<PortableRevision> {
    Ok(PortableRevision {
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
}

fn receipt_for(strategy: ImportStrategy, bundle: &PortableBundle) -> Result<ImportReceipt> {
    Ok(ImportReceipt {
        strategy,
        languages: u32::try_from(bundle.languages.len()).map_err(|_| MaviError::Internal)?,
        content_types: u32::try_from(bundle.content_types.len())
            .map_err(|_| MaviError::Internal)?,
        terms: u32::try_from(bundle.terms.len()).map_err(|_| MaviError::Internal)?,
        content: u32::try_from(bundle.content.len()).map_err(|_| MaviError::Internal)?,
        revisions: u32::try_from(bundle.revisions.len()).map_err(|_| MaviError::Internal)?,
        slug_history: u32::try_from(bundle.slug_history.len()).map_err(|_| MaviError::Internal)?,
        assignments: u32::try_from(bundle.assignments.len()).map_err(|_| MaviError::Internal)?,
    })
}

async fn validate_external_parents(tx: &mut SiteTx, terms: &[PortableTerm]) -> Result<()> {
    let bundled = terms.iter().map(|term| term.id).collect::<BTreeSet<_>>();
    for term in terms {
        if let Some(parent_id) = term.parent_id {
            if bundled.contains(&parent_id) {
                continue;
            }
            let exists: bool = sqlx::query_scalar(
                "select exists(select 1 from taxonomy_terms where id = $1 and deleted_at is null)",
            )
            .bind(parent_id)
            .fetch_one(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
            if !exists {
                return Err(MaviError::validation("portable_term_parent_missing"));
            }
        }
    }
    Ok(())
}

async fn ensure_create_only(tx: &mut SiteTx, bundle: &PortableBundle) -> Result<()> {
    for language in &bundle.languages {
        if exists(
            tx,
            "select exists(select 1 from site_languages where tag = $1)",
            &language.tag,
        )
        .await?
        {
            return Err(MaviError::conflict("portable_language_conflict"));
        }
    }
    for content_type in &bundle.content_types {
        if exists(
            tx,
            "select exists(select 1 from content_types where kind = $1)",
            &content_type.kind,
        )
        .await?
        {
            return Err(MaviError::conflict("portable_content_type_conflict"));
        }
    }
    for term in &bundle.terms {
        if exists_uuid(tx, "select exists(select 1 from taxonomy_terms where id = $1)", term.id).await?
            || exists_three(tx, "select exists(select 1 from taxonomy_terms where kind = $1 and language = $2 and slug = $3)", &term.kind, &term.language, &term.slug).await?
        {
            return Err(MaviError::conflict("portable_term_conflict"));
        }
    }
    for item in &bundle.content {
        if exists_uuid(tx, "select exists(select 1 from content_entries where id = $1)", item.id).await?
            || exists_two(tx, "select exists(select 1 from content_entries where language = $1 and slug = $2 and deleted_at is null)", &item.language, &item.slug).await?
        {
            return Err(MaviError::conflict("portable_content_conflict"));
        }
    }
    Ok(())
}

async fn exists(tx: &mut SiteTx, query: &'static str, value: &str) -> Result<bool> {
    sqlx::query_scalar(query)
        .bind(value)
        .fetch_one(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)
}

async fn exists_uuid(tx: &mut SiteTx, query: &'static str, value: Uuid) -> Result<bool> {
    sqlx::query_scalar(query)
        .bind(value)
        .fetch_one(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)
}

async fn exists_two(
    tx: &mut SiteTx,
    query: &'static str,
    first: &str,
    second: &str,
) -> Result<bool> {
    sqlx::query_scalar(query)
        .bind(first)
        .bind(second)
        .fetch_one(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)
}

async fn exists_three(
    tx: &mut SiteTx,
    query: &'static str,
    first: &str,
    second: &str,
    third: &str,
) -> Result<bool> {
    sqlx::query_scalar(query)
        .bind(first)
        .bind(second)
        .bind(third)
        .fetch_one(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)
}

fn ordered_terms(terms: &[PortableTerm]) -> Result<Vec<&PortableTerm>> {
    let mut remaining = terms.iter().collect::<Vec<_>>();
    let mut inserted = BTreeSet::new();
    let mut ordered = Vec::with_capacity(terms.len());
    while !remaining.is_empty() {
        let before = remaining.len();
        let mut next = Vec::with_capacity(remaining.len());
        for term in remaining {
            if term
                .parent_id
                .is_none_or(|parent_id| inserted.contains(&parent_id))
            {
                inserted.insert(term.id);
                ordered.push(term);
            } else {
                next.push(term);
            }
        }
        if next.len() == before {
            return Err(MaviError::validation("portable_term_cycle"));
        }
        remaining = next;
    }
    Ok(ordered)
}

#[allow(clippy::needless_pass_by_value)]
fn map_import_write_error(error: sqlx::Error) -> MaviError {
    let Some(database_error) = error.as_database_error() else {
        return MaviError::Internal;
    };
    match database_error.code().as_deref() {
        Some("23505") => MaviError::conflict("portable_record_conflict"),
        Some("23503" | "23514") => MaviError::validation("portable_record_invalid"),
        _ => MaviError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle() -> PortableBundle {
        let source = SiteId::new();
        let content_id = Uuid::now_v7();
        let term_id = Uuid::now_v7();
        let revision_time = Utc::now();
        let content = PortableContent {
            id: content_id,
            kind: "post".to_owned(),
            language: "en".to_owned(),
            slug: "hello-world".to_owned(),
            title: "Hello".to_owned(),
            excerpt: None,
            body: "Body".to_owned(),
            fields: json!({}),
            status: "draft".to_owned(),
            scheduled_at: None,
            published_at: None,
            revision: 1,
            created_at: revision_time,
            updated_at: revision_time,
        };
        let revisions = vec![PortableRevision {
            content_id,
            revision: 1,
            kind: "post".to_owned(),
            language: "en".to_owned(),
            slug: "hello-world".to_owned(),
            title: "Hello".to_owned(),
            excerpt: None,
            body: "Body".to_owned(),
            fields: json!({}),
            status: "draft".to_owned(),
            scheduled_at: None,
            published_at: None,
            created_at: revision_time,
        }];
        let terms = vec![PortableTerm {
            id: term_id,
            kind: "tag".to_owned(),
            language: "en".to_owned(),
            slug: "intro".to_owned(),
            name: "Intro".to_owned(),
            parent_id: None,
        }];
        PortableBundle {
            manifest: PortableManifest {
                format: FORMAT.to_owned(),
                version: VERSION,
                source_site_id: source,
                exported_at: revision_time,
                schema_hash: schema_hash(),
                counts: PortableCounts {
                    languages: 1,
                    content_types: 0,
                    terms: 1,
                    content: 1,
                    revisions: 1,
                    slug_history: 0,
                    assignments: 1,
                },
            },
            site: PortableSite {
                name: "Demo".to_owned(),
                timezone: "UTC".to_owned(),
            },
            languages: vec![PortableLanguage {
                tag: "en".to_owned(),
                name: "English".to_owned(),
                is_default: true,
            }],
            content_types: vec![],
            terms,
            content: vec![content],
            revisions,
            slug_history: vec![],
            assignments: vec![PortableAssignment {
                content_id,
                term_id,
                assigned_at: revision_time,
            }],
        }
    }

    #[test]
    fn bundle_validation_rejects_self_import_and_accepts_cross_site_bundle() {
        let mut bundle = bundle();
        assert!(
            bundle
                .validate_for_site(bundle.manifest.source_site_id)
                .is_err()
        );
        assert!(
            bundle
                .validate_for_relocation(bundle.manifest.source_site_id)
                .is_ok()
        );
        assert!(bundle.validate_for_relocation(SiteId::new()).is_err());
        assert!(bundle.validate_for_site(SiteId::new()).is_ok());
        bundle.manifest.schema_hash = "bad".to_owned();
        assert!(bundle.validate_for_site(SiteId::new()).is_err());
    }

    #[test]
    fn terms_are_ordered_parent_before_child() {
        let parent = Uuid::now_v7();
        let child = Uuid::now_v7();
        let terms = vec![
            PortableTerm {
                id: child,
                kind: "category".to_owned(),
                language: "en".to_owned(),
                slug: "child".to_owned(),
                name: "Child".to_owned(),
                parent_id: Some(parent),
            },
            PortableTerm {
                id: parent,
                kind: "category".to_owned(),
                language: "en".to_owned(),
                slug: "parent".to_owned(),
                name: "Parent".to_owned(),
                parent_id: None,
            },
        ];
        let ordered = ordered_terms(&terms).expect("order");
        assert_eq!(ordered[0].id, parent);
        assert_eq!(ordered[1].id, child);
    }

    #[test]
    fn portable_contract_is_versioned_and_not_paginated() {
        let api = api();
        assert!(
            api.endpoints
                .iter()
                .any(|endpoint| endpoint.operation_id == "portable.export")
        );
        assert!(
            api.endpoints
                .iter()
                .any(|endpoint| endpoint.operation_id == "portable.import")
        );
        assert!(
            shapes()
                .iter()
                .all(|shape| !shape.schema.to_string().contains("offset"))
        );
        api.validate().expect("portable API");
    }
}
