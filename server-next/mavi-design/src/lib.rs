//! Site-owned design source, immutable builds and publish activation.
//!
//! The domain stores source and build metadata in `PostgreSQL`. Build bytes are
//! written through the site-scoped [`FileStore`] port, so self-host uses a
//! directory adapter while cloud deployments can provide object storage. A
//! build is immutable once it becomes ready; publishing only switches which
//! ready build is live.

use std::{collections::BTreeSet, fmt::Debug};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Capability, Cursor, DesignBuildId, DesignChangeId, ErrorCode, MaviError, Page,
    PageRequest, Result, SiteContext,
    ports::{BoxFuture, FileStore},
};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

pub const DESIGN_CHANGE_NOT_FOUND: &str = "design_change_not_found";
pub const DESIGN_BUILD_NOT_FOUND: &str = "design_build_not_found";
pub const DESIGN_FILE_NOT_FOUND: &str = "design_file_not_found";
pub const DESIGN_FILE_PATH_INVALID: &str = "design_file_path_invalid";
pub const DESIGN_FILE_CONTENT_INVALID: &str = "design_file_content_invalid";
pub const DESIGN_NAME_INVALID: &str = "design_change_name_invalid";
pub const DESIGN_CHANGE_PUBLISHED: &str = "design_change_published";
pub const DESIGN_BUILD_IN_PROGRESS: &str = "design_build_in_progress";
pub const DESIGN_NOT_READY: &str = "design_change_not_ready";
pub const DESIGN_ENTRYPOINT_MISSING: &str = "design_entrypoint_missing";
pub const DESIGN_PUBLIC_ASSET_NOT_FOUND: &str = "design_public_asset_not_found";
pub const DESIGN_BUILD_FAILED: &str = "design_build_failed";

pub const MAX_DESIGN_FILE_BYTES: usize = 5 * 1024 * 1024;
/// Maximum raw design source and artifact bytes carried by one private shard
/// relocation. The portable coordinator applies the outer JSON envelope cap.
pub const MAX_DESIGN_RELOCATION_BYTES: usize = 128 * 1024 * 1024;
const MAX_DESIGN_NAME_CHARS: usize = 120;
const MAX_DESIGN_PATH_CHARS: usize = 200;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DesignState {
    Writing,
    Building,
    Ready,
    Failed,
    Published,
}

impl DesignState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Writing => "writing",
            Self::Building => "building",
            Self::Ready => "ready",
            Self::Failed => "failed",
            Self::Published => "published",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "writing" => Ok(Self::Writing),
            "building" => Ok(Self::Building),
            "ready" => Ok(Self::Ready),
            "failed" => Ok(Self::Failed),
            "published" => Ok(Self::Published),
            _ => Err(MaviError::Internal),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DesignBuildState {
    Queued,
    Ready,
    Failed,
}

impl DesignBuildState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "queued" => Ok(Self::Queued),
            "ready" => Ok(Self::Ready),
            "failed" => Ok(Self::Failed),
            _ => Err(MaviError::Internal),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StartDesignChange {
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DesignFileInput {
    pub path: String,
    pub contents: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct DesignChangeListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
    pub state: Option<DesignState>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct DesignFileListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DesignFileQuery {
    pub path: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct DesignBuildListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
}

#[derive(Clone, Debug, Serialize)]
pub struct DesignChange {
    pub id: DesignChangeId,
    pub name: String,
    pub state: DesignState,
    pub ready_build_id: Option<DesignBuildId>,
    pub published_build_id: Option<DesignBuildId>,
    pub last_error: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DesignBuild {
    pub id: DesignBuildId,
    pub change_id: DesignChangeId,
    pub state: DesignBuildState,
    pub error: Option<String>,
    pub preview_path: String,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DesignFile {
    pub path: String,
    pub contents: String,
    pub bytes: u64,
    pub sha256: String,
    pub removed: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DesignFileSummary {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub removed: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct BuildSourceFile {
    pub path: String,
    pub contents: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct BuildArtifact {
    pub path: String,
    pub mime: String,
    pub contents: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct StoredArtifact {
    pub path: String,
    pub storage_key: String,
    pub mime: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug)]
pub struct BuildRequest {
    pub build: DesignBuild,
    pub source: Vec<BuildSourceFile>,
}

#[derive(Clone, Debug)]
pub struct PublicArtifact {
    pub storage_key: String,
    pub mime: String,
}

/// Design source, build metadata and immutable artifact bytes used by the
/// authenticated shard relocation port. Public design APIs do not expose
/// this snapshot shape.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DesignRelocation {
    pub changes: Vec<DesignRelocationChange>,
    pub files: Vec<DesignRelocationFile>,
    pub builds: Vec<DesignRelocationBuild>,
    pub artifacts: Vec<DesignRelocationArtifact>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DesignRelocationChange {
    pub id: DesignChangeId,
    pub name: String,
    pub state: DesignState,
    pub ready_build_id: Option<DesignBuildId>,
    pub published_build_id: Option<DesignBuildId>,
    pub last_error: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DesignRelocationFile {
    pub change_id: DesignChangeId,
    pub path: String,
    #[serde(with = "base64_bytes")]
    pub contents: Vec<u8>,
    pub bytes: u64,
    pub sha256: String,
    pub removed: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DesignRelocationBuild {
    pub id: DesignBuildId,
    pub change_id: DesignChangeId,
    pub state: DesignBuildState,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DesignRelocationArtifact {
    pub build_id: DesignBuildId,
    pub path: String,
    pub storage_key: String,
    pub mime: String,
    pub bytes: u64,
    pub sha256: String,
    #[serde(with = "base64_bytes")]
    pub contents: Vec<u8>,
}

impl DesignRelocation {
    pub fn validate(&self) -> Result<()> {
        let change_ids = self
            .changes
            .iter()
            .map(|change| change.id)
            .collect::<BTreeSet<_>>();
        if change_ids.len() != self.changes.len()
            || self
                .changes
                .iter()
                .filter(|change| change.state == DesignState::Published)
                .count()
                > 1
        {
            return Err(MaviError::validation("design_relocation_changes_invalid"));
        }

        let build_ids = self
            .builds
            .iter()
            .map(|build| build.id)
            .collect::<BTreeSet<_>>();
        if build_ids.len() != self.builds.len() {
            return Err(MaviError::validation("design_relocation_builds_invalid"));
        }
        for change in &self.changes {
            if change.name != validate_name(&change.name)? {
                return Err(MaviError::validation(DESIGN_NAME_INVALID));
            }
            for build_id in [change.ready_build_id, change.published_build_id]
                .into_iter()
                .flatten()
            {
                if !self
                    .builds
                    .iter()
                    .any(|build| build.id == build_id && build.change_id == change.id)
                {
                    return Err(MaviError::validation(
                        "design_relocation_change_build_reference_invalid",
                    ));
                }
            }
            if change.state == DesignState::Published && change.published_build_id.is_none() {
                return Err(MaviError::validation(
                    "design_relocation_published_build_missing",
                ));
            }
        }
        for file in &self.files {
            if !change_ids.contains(&file.change_id)
                || validate_source_path(&file.path)? != file.path
            {
                return Err(MaviError::validation("design_relocation_file_invalid"));
            }
            validate_contents(
                std::str::from_utf8(&file.contents)
                    .map_err(|_| MaviError::validation(DESIGN_FILE_CONTENT_INVALID))?,
            )?;
            validate_digest_and_bytes(file.bytes, &file.sha256, &file.contents)?;
        }
        for build in &self.builds {
            if !change_ids.contains(&build.change_id) {
                return Err(MaviError::validation("design_relocation_build_invalid"));
            }
        }
        let mut artifact_keys = BTreeSet::new();
        for artifact in &self.artifacts {
            if !build_ids.contains(&artifact.build_id)
                || validate_artifact_path(&artifact.path)? != artifact.path
                || artifact.storage_key != artifact_storage_key(artifact.build_id, &artifact.path)
                || !artifact_keys.insert((artifact.build_id, artifact.path.clone()))
                || !valid_relocation_mime(&artifact.mime)
            {
                return Err(MaviError::validation("design_relocation_artifact_invalid"));
            }
            validate_digest_and_bytes(artifact.bytes, &artifact.sha256, &artifact.contents)?;
        }

        let total_bytes = self
            .files
            .iter()
            .map(|file| file.contents.len())
            .chain(
                self.artifacts
                    .iter()
                    .map(|artifact| artifact.contents.len()),
            )
            .try_fold(0usize, usize::checked_add)
            .ok_or_else(|| MaviError::validation("design_relocation_size_overflow"))?;
        if total_bytes > MAX_DESIGN_RELOCATION_BYTES {
            return Err(MaviError::validation("design_relocation_too_large"));
        }
        Ok(())
    }

    #[must_use]
    pub fn record_count(&self) -> usize {
        self.changes.len() + self.files.len() + self.builds.len() + self.artifacts.len()
    }
}

/// A runtime-specific design compiler.
pub trait BuildEngine: Debug + Send + Sync {
    fn build<'a>(
        &'a self,
        context: &'a SiteContext,
        build_id: DesignBuildId,
        source: &'a [BuildSourceFile],
    ) -> BoxFuture<'a, Result<Vec<BuildArtifact>>>;
}

/// The safe self-host baseline: publish files already placed below `public/`.
/// Application source below `src/` is kept in the design change but is not
/// executable by the server. Cloud deployments can replace this with a
/// sandboxed compiler adapter without changing the domain or HTTP contract.
#[derive(Clone, Copy, Debug, Default)]
pub struct StaticBuildEngine;

impl BuildEngine for StaticBuildEngine {
    fn build<'a>(
        &'a self,
        _context: &'a SiteContext,
        _build_id: DesignBuildId,
        source: &'a [BuildSourceFile],
    ) -> BoxFuture<'a, Result<Vec<BuildArtifact>>> {
        Box::pin(async move {
            let mut artifacts = Vec::new();
            let mut has_entrypoint = false;
            for file in source {
                let Some(path) = file.path.strip_prefix("public/") else {
                    continue;
                };
                let path = validate_artifact_path(path)?;
                if path == "index.html" {
                    has_entrypoint = true;
                }
                artifacts.push(BuildArtifact {
                    mime: mime_for_path(&path).to_owned(),
                    path,
                    contents: file.contents.clone(),
                });
            }
            if !has_entrypoint {
                return Err(MaviError::validation(DESIGN_ENTRYPOINT_MISSING));
            }
            Ok(artifacts)
        })
    }
}

#[must_use]
pub fn api() -> mavi_contract::Api {
    mavi_contract::Api::new(endpoints()).with_shapes(shapes())
}

#[must_use]
#[allow(clippy::too_many_lines)]
pub fn endpoints() -> Vec<Endpoint> {
    let design_view = Permission {
        capability: Capability::Design,
        action: Action::View,
    };
    let design_write = Permission {
        capability: Capability::Design,
        action: Action::Write,
    };
    let design_delete = Permission {
        capability: Capability::Design,
        action: Action::Delete,
    };
    let publish_write = Permission {
        capability: Capability::Publish,
        action: Action::Write,
    };

    vec![
        Endpoint::new(
            Method::Get,
            "/api/v1/design/changes",
            "design.changes.list",
            "List site design changes with an opaque cursor",
        )
        .account_or_assistant()
        .requires(design_view)
        .takes_query("DesignChangeListFilter")
        .returns(200, "DesignChangePage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/design/changes",
            "design.changes.start",
            "Start a site design change from the current published design",
        )
        .account_or_assistant()
        .requires(design_write)
        .takes("StartDesignChange")
        .returns(201, "DesignChange")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/design/changes/{id}",
            "design.changes.read",
            "Read one design change",
        )
        .account_or_assistant()
        .requires(design_view)
        .returns(200, "DesignChange")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/design/changes/{id}/files",
            "design.files.list",
            "List source files in a design change with an opaque cursor",
        )
        .account_or_assistant()
        .requires(design_view)
        .takes_query("DesignFileListFilter")
        .returns(200, "DesignFilePage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/design/changes/{id}/file",
            "design.files.read",
            "Read one source file",
        )
        .account_or_assistant()
        .requires(design_view)
        .with_query("DesignFileQuery")
        .returns(200, "DesignFile")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Put,
            "/api/v1/design/changes/{id}/file",
            "design.files.write",
            "Write one source file",
        )
        .account_or_assistant()
        .requires(design_write)
        .takes("DesignFileInput")
        .returns(200, "DesignFile")
        .changes(true)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Conflict,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Delete,
            "/api/v1/design/changes/{id}/file",
            "design.files.remove",
            "Remove one source file from a design change",
        )
        .account_or_assistant()
        .requires(design_delete)
        .with_query("DesignFileQuery")
        .returns(204, "Empty")
        .changes(true)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Conflict,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/design/changes/{id}/builds",
            "design.builds.create",
            "Build a design change into immutable public artifacts",
        )
        .account_or_assistant()
        .requires(design_write)
        .returns(201, "DesignBuild")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Conflict,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/design/changes/{id}/builds",
            "design.builds.list",
            "List immutable builds for a design change with an opaque cursor",
        )
        .account_or_assistant()
        .requires(design_view)
        .takes_query("DesignBuildListFilter")
        .returns(200, "DesignBuildPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/design/changes/{id}/publish",
            "design.changes.publish",
            "Make the latest ready build live",
        )
        .account_or_assistant()
        .requires(publish_write)
        .returns(200, "DesignChange")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Conflict,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/design/changes/{id}/rollback",
            "design.changes.rollback",
            "Make a previously published design build live again",
        )
        .account_or_assistant()
        .requires(publish_write)
        .returns(200, "DesignChange")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Conflict,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/preview/v1/design/{build_id}/{path}",
            "design.preview.asset",
            "Serve an immutable ready design build for preview",
        )
        .public()
        .returns(200, "DesignAsset")
        .refuses([
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/public/v1/site/{path}",
            "design.public.asset",
            "Serve the currently published design asset",
        )
        .public()
        .returns(200, "DesignAsset")
        .refuses([
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
    ]
}

#[must_use]
#[allow(clippy::too_many_lines)]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "DesignState",
            json!({"type": "string", "enum": ["writing", "building", "ready", "failed", "published"]}),
        ),
        Shape::new(
            "DesignBuildState",
            json!({"type": "string", "enum": ["queued", "ready", "failed"]}),
        ),
        Shape::new(
            "StartDesignChange",
            json!({
                "type": "object",
                "required": ["name"],
                "additionalProperties": false,
                "properties": {"name": {"type": "string", "minLength": 1, "maxLength": MAX_DESIGN_NAME_CHARS}}
            }),
        ),
        Shape::new(
            "DesignFileInput",
            json!({
                "type": "object",
                "required": ["path", "contents"],
                "additionalProperties": false,
                "properties": {
                    "path": {"type": "string", "minLength": 1, "maxLength": MAX_DESIGN_PATH_CHARS},
                    "contents": {"type": "string", "maxLength": MAX_DESIGN_FILE_BYTES}
                }
            }),
        ),
        Shape::new(
            "DesignChangeListFilter",
            json!({
                "type": "object",
                "properties": {
                    "after": {"type": ["string", "null"], "maxLength": 512},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                    "state": {"$ref": "#/components/schemas/DesignState"}
                }
            }),
        ),
        Shape::new(
            "DesignFileListFilter",
            json!({
                "type": "object",
                "properties": {
                    "after": {"type": ["string", "null"], "maxLength": 512},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100}
                }
            }),
        ),
        Shape::new(
            "DesignFileQuery",
            json!({
                "type": "object",
                "required": ["path"],
                "additionalProperties": false,
                "properties": {"path": {"type": "string", "minLength": 1, "maxLength": MAX_DESIGN_PATH_CHARS}}
            }),
        ),
        Shape::new(
            "DesignBuildListFilter",
            json!({
                "type": "object",
                "properties": {
                    "after": {"type": ["string", "null"], "maxLength": 512},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100}
                }
            }),
        ),
        Shape::new(
            "DesignChange",
            json!({
                "type": "object",
                "required": ["id", "name", "state", "ready_build_id", "published_build_id", "last_error", "published_at", "created_at", "updated_at"],
                "properties": {
                    "id": {"type": "string", "format": "uuid"},
                    "name": {"type": "string", "maxLength": MAX_DESIGN_NAME_CHARS},
                    "state": {"$ref": "#/components/schemas/DesignState"},
                    "ready_build_id": {"type": ["string", "null"], "format": "uuid"},
                    "published_build_id": {"type": ["string", "null"], "format": "uuid"},
                    "last_error": {"type": ["string", "null"]},
                    "published_at": {"type": ["string", "null"], "format": "date-time"},
                    "created_at": {"type": "string", "format": "date-time"},
                    "updated_at": {"type": "string", "format": "date-time"}
                }
            }),
        ),
        Shape::new(
            "DesignBuild",
            json!({
                "type": "object",
                "required": ["id", "change_id", "state", "error", "preview_path", "created_at", "completed_at"],
                "properties": {
                    "id": {"type": "string", "format": "uuid"},
                    "change_id": {"type": "string", "format": "uuid"},
                    "state": {"$ref": "#/components/schemas/DesignBuildState"},
                    "error": {"type": ["string", "null"]},
                    "preview_path": {"type": "string"},
                    "created_at": {"type": "string", "format": "date-time"},
                    "completed_at": {"type": ["string", "null"], "format": "date-time"}
                }
            }),
        ),
        Shape::new(
            "DesignFile",
            json!({
                "type": "object",
                "required": ["path", "contents", "bytes", "sha256", "removed", "updated_at"],
                "properties": {
                    "path": {"type": "string"},
                    "contents": {"type": "string"},
                    "bytes": {"type": "integer", "format": "int64"},
                    "sha256": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
                    "removed": {"type": "boolean"},
                    "updated_at": {"type": "string", "format": "date-time"}
                }
            }),
        ),
        Shape::new(
            "DesignFileSummary",
            json!({
                "type": "object",
                "required": ["path", "bytes", "sha256", "removed", "updated_at"],
                "properties": {
                    "path": {"type": "string"},
                    "bytes": {"type": "integer", "format": "int64"},
                    "sha256": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
                    "removed": {"type": "boolean"},
                    "updated_at": {"type": "string", "format": "date-time"}
                }
            }),
        ),
        Shape::new("DesignAsset", json!({"type": "string", "format": "binary"})),
        Shape::new(
            "DesignChangePage",
            json!({
                "type": "object",
                "required": ["items", "next_cursor"],
                "properties": {
                    "items": {"type": "array", "items": {"$ref": "#/components/schemas/DesignChange"}},
                    "next_cursor": {"type": ["string", "null"], "maxLength": 512}
                }
            }),
        ),
        Shape::new(
            "DesignFilePage",
            json!({
                "type": "object",
                "required": ["items", "next_cursor"],
                "properties": {
                    "items": {"type": "array", "items": {"$ref": "#/components/schemas/DesignFileSummary"}},
                    "next_cursor": {"type": ["string", "null"], "maxLength": 512}
                }
            }),
        ),
        Shape::new(
            "DesignBuildPage",
            json!({
                "type": "object",
                "required": ["items", "next_cursor"],
                "properties": {
                    "items": {"type": "array", "items": {"$ref": "#/components/schemas/DesignBuild"}},
                    "next_cursor": {"type": ["string", "null"], "maxLength": 512}
                }
            }),
        ),
    ]
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ChangeCursor {
    created_at: DateTime<Utc>,
    id: Uuid,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct FileCursor {
    path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct BuildCursor {
    created_at: DateTime<Utc>,
    id: Uuid,
}

fn encode_cursor<T: Serialize>(value: &T) -> Result<Cursor> {
    let bytes = serde_json::to_vec(value).map_err(|_| MaviError::Internal)?;
    Cursor::parse(URL_SAFE_NO_PAD.encode(bytes))
}

fn decode_cursor<T: for<'de> Deserialize<'de>>(cursor: &Cursor) -> Result<T> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_str())
        .map_err(|_| MaviError::validation("invalid_cursor"))?;
    serde_json::from_slice(&bytes).map_err(|_| MaviError::validation("invalid_cursor"))
}

/// Domain service. Every method takes an already site-scoped transaction.
#[derive(Clone, Copy, Debug, Default)]
pub struct DesignService;

impl DesignService {
    pub async fn start_change(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &StartDesignChange,
    ) -> Result<DesignChange> {
        let name = validate_name(&input.name)?;
        let id = DesignChangeId::new();
        let row = sqlx::query(
            "insert into design_changes (site_id, id, name)
             values ($1, $2, $3)
             returning id, name, state, ready_build_id, published_build_id,
                       last_error, published_at, created_at, updated_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(name)
        .fetch_one(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        sqlx::query(
            "insert into design_files
                (site_id, change_id, path, contents, bytes, sha256, removed)
             select site_id, $2, path, contents, bytes, sha256, false
               from design_files
              where site_id = $1
                and change_id = (select id from design_changes
                                  where site_id = $1 and state = 'published'
                                  order by published_at desc nulls last, created_at desc
                                  limit 1)
                and removed = false
             on conflict (site_id, change_id, path) do nothing",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        let change = from_change_row(&row)?;
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "design.change.started".to_owned(),
                    resource_type: "DesignChange".to_owned(),
                    resource_id: Some(id.into_uuid()),
                    payload: json!({"name": change.name}),
                },
            )
            .await?;
        Ok(change)
    }

    /// Exports all design history and immutable artifact bytes for the
    /// authenticated shard relocation coordinator.
    #[allow(clippy::too_many_lines)]
    pub async fn export_for_relocation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        store: &dyn FileStore,
    ) -> Result<DesignRelocation> {
        let change_rows = sqlx::query(
            "select id, name, state, ready_build_id, published_build_id,
                    last_error, published_at, created_at, updated_at
               from design_changes where site_id = $1 order by created_at asc, id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let changes = change_rows
            .iter()
            .map(|row| {
                Ok(DesignRelocationChange {
                    id: DesignChangeId::from_uuid(
                        row.try_get("id").map_err(|_| MaviError::Internal)?,
                    ),
                    name: row.try_get("name").map_err(|_| MaviError::Internal)?,
                    state: DesignState::parse(
                        &row.try_get::<String, _>("state")
                            .map_err(|_| MaviError::Internal)?,
                    )?,
                    ready_build_id: row
                        .try_get::<Option<Uuid>, _>("ready_build_id")
                        .map_err(|_| MaviError::Internal)?
                        .map(DesignBuildId::from_uuid),
                    published_build_id: row
                        .try_get::<Option<Uuid>, _>("published_build_id")
                        .map_err(|_| MaviError::Internal)?
                        .map(DesignBuildId::from_uuid),
                    last_error: row.try_get("last_error").map_err(|_| MaviError::Internal)?,
                    published_at: row
                        .try_get("published_at")
                        .map_err(|_| MaviError::Internal)?,
                    created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                    updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let file_rows = sqlx::query(
            "select change_id, path, contents, bytes, sha256, removed, created_at, updated_at
               from design_files where site_id = $1 order by change_id asc, path asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let files = file_rows
            .iter()
            .map(|row| {
                let contents: Vec<u8> = row.try_get("contents").map_err(|_| MaviError::Internal)?;
                Ok(DesignRelocationFile {
                    change_id: DesignChangeId::from_uuid(
                        row.try_get("change_id").map_err(|_| MaviError::Internal)?,
                    ),
                    path: row.try_get("path").map_err(|_| MaviError::Internal)?,
                    bytes: u64::try_from(
                        row.try_get::<i64, _>("bytes")
                            .map_err(|_| MaviError::Internal)?,
                    )
                    .map_err(|_| MaviError::Internal)?,
                    sha256: row.try_get("sha256").map_err(|_| MaviError::Internal)?,
                    removed: row.try_get("removed").map_err(|_| MaviError::Internal)?,
                    created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                    updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
                    contents,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let build_rows = sqlx::query(
            "select id, change_id, state, error, created_at, completed_at
               from design_builds where site_id = $1 order by created_at asc, id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let builds = build_rows
            .iter()
            .map(|row| {
                Ok(DesignRelocationBuild {
                    id: DesignBuildId::from_uuid(
                        row.try_get("id").map_err(|_| MaviError::Internal)?,
                    ),
                    change_id: DesignChangeId::from_uuid(
                        row.try_get("change_id").map_err(|_| MaviError::Internal)?,
                    ),
                    state: DesignBuildState::parse(
                        &row.try_get::<String, _>("state")
                            .map_err(|_| MaviError::Internal)?,
                    )?,
                    error: row.try_get("error").map_err(|_| MaviError::Internal)?,
                    created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
                    completed_at: row
                        .try_get("completed_at")
                        .map_err(|_| MaviError::Internal)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let artifact_rows = sqlx::query(
            "select build_id, path, storage_key, mime, bytes, sha256
               from design_build_artifacts where site_id = $1
              order by build_id asc, path asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let mut artifacts = Vec::with_capacity(artifact_rows.len());
        for row in &artifact_rows {
            let storage_key: String = row
                .try_get("storage_key")
                .map_err(|_| MaviError::Internal)?;
            let contents = store.get(context, &storage_key).await?;
            let bytes = u64::try_from(
                row.try_get::<i64, _>("bytes")
                    .map_err(|_| MaviError::Internal)?,
            )
            .map_err(|_| MaviError::Internal)?;
            let sha256: String = row.try_get("sha256").map_err(|_| MaviError::Internal)?;
            if usize::try_from(bytes).map_err(|_| MaviError::Internal)? != contents.len()
                || sha256_hex(&contents) != sha256
            {
                return Err(MaviError::validation("design_storage_integrity_failed"));
            }
            artifacts.push(DesignRelocationArtifact {
                build_id: DesignBuildId::from_uuid(
                    row.try_get("build_id").map_err(|_| MaviError::Internal)?,
                ),
                path: row.try_get("path").map_err(|_| MaviError::Internal)?,
                storage_key,
                mime: row.try_get("mime").map_err(|_| MaviError::Internal)?,
                bytes,
                sha256,
                contents,
            });
        }

        let relocation = DesignRelocation {
            changes,
            files,
            builds,
            artifacts,
        };
        relocation.validate()?;
        Ok(relocation)
    }

    /// Imports a validated design snapshot with idempotent metadata upserts
    /// and replace-safe artifact writes. Pointers are applied last so all
    /// composite foreign keys are present before a publish state is restored.
    #[allow(clippy::too_many_lines)]
    pub async fn import_for_relocation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        store: &dyn FileStore,
        relocation: &DesignRelocation,
    ) -> Result<()> {
        relocation.validate()?;
        for artifact in &relocation.artifacts {
            store
                .put(context, &artifact.storage_key, artifact.contents.clone())
                .await?;
        }

        sqlx::query(
            "update design_changes set state = 'ready'
              where site_id = $1 and state = 'published'",
        )
        .bind(context.site_id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        for change in &relocation.changes {
            let staged_state = if change.state == DesignState::Published {
                DesignState::Ready
            } else {
                change.state
            };
            sqlx::query(
                "insert into design_changes
                    (site_id, id, name, state, ready_build_id, published_build_id,
                     last_error, published_at, created_at, updated_at)
                 values ($1, $2, $3, $4, null, null, $5, $6, $7, $8)
                 on conflict (site_id, id) do update set
                    name = excluded.name, state = excluded.state,
                    ready_build_id = null, published_build_id = null,
                    last_error = excluded.last_error, published_at = excluded.published_at,
                    created_at = excluded.created_at, updated_at = excluded.updated_at",
            )
            .bind(context.site_id.into_uuid())
            .bind(change.id.into_uuid())
            .bind(&change.name)
            .bind(staged_state.as_str())
            .bind(&change.last_error)
            .bind(change.published_at)
            .bind(change.created_at)
            .bind(change.updated_at)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }

        for file in &relocation.files {
            sqlx::query(
                "insert into design_files
                    (site_id, change_id, path, contents, bytes, sha256, removed, created_at, updated_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 on conflict (site_id, change_id, path) do update set
                    contents = excluded.contents, bytes = excluded.bytes,
                    sha256 = excluded.sha256, removed = excluded.removed,
                    created_at = excluded.created_at, updated_at = excluded.updated_at",
            )
            .bind(context.site_id.into_uuid())
            .bind(file.change_id.into_uuid())
            .bind(&file.path)
            .bind(&file.contents)
            .bind(i64::try_from(file.bytes).map_err(|_| MaviError::Internal)?)
            .bind(&file.sha256)
            .bind(file.removed)
            .bind(file.created_at)
            .bind(file.updated_at)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }

        for build in &relocation.builds {
            sqlx::query(
                "insert into design_builds
                    (site_id, id, change_id, state, error, created_at, completed_at)
                 values ($1, $2, $3, $4, $5, $6, $7)
                 on conflict (site_id, id) do update set
                    change_id = excluded.change_id, state = excluded.state,
                    error = excluded.error, created_at = excluded.created_at,
                    completed_at = excluded.completed_at",
            )
            .bind(context.site_id.into_uuid())
            .bind(build.id.into_uuid())
            .bind(build.change_id.into_uuid())
            .bind(build.state.as_str())
            .bind(&build.error)
            .bind(build.created_at)
            .bind(build.completed_at)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }

        for artifact in &relocation.artifacts {
            sqlx::query(
                "insert into design_build_artifacts
                    (site_id, build_id, path, storage_key, mime, bytes, sha256)
                 values ($1, $2, $3, $4, $5, $6, $7)
                 on conflict (site_id, build_id, path) do update set
                    storage_key = excluded.storage_key, mime = excluded.mime,
                    bytes = excluded.bytes, sha256 = excluded.sha256",
            )
            .bind(context.site_id.into_uuid())
            .bind(artifact.build_id.into_uuid())
            .bind(&artifact.path)
            .bind(&artifact.storage_key)
            .bind(&artifact.mime)
            .bind(i64::try_from(artifact.bytes).map_err(|_| MaviError::Internal)?)
            .bind(&artifact.sha256)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }

        for change in &relocation.changes {
            sqlx::query(
                "update design_changes
                    set state = $3, ready_build_id = $4, published_build_id = $5,
                        updated_at = $6
                  where site_id = $1 and id = $2",
            )
            .bind(context.site_id.into_uuid())
            .bind(change.id.into_uuid())
            .bind(change.state.as_str())
            .bind(change.ready_build_id.map(DesignBuildId::into_uuid))
            .bind(change.published_build_id.map(DesignBuildId::into_uuid))
            .bind(change.updated_at)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }

        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "portable.design.relocated".to_owned(),
                    resource_type: "DesignSnapshot".to_owned(),
                    resource_id: None,
                    payload: json!({
                        "changes": relocation.changes.len(),
                        "files": relocation.files.len(),
                        "builds": relocation.builds.len(),
                        "artifacts": relocation.artifacts.len(),
                    }),
                },
            )
            .await
    }

    pub async fn list_changes(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        filter: &DesignChangeListFilter,
    ) -> Result<Page<DesignChange>> {
        let after: Option<ChangeCursor> =
            filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select id, name, state, ready_build_id, published_build_id,
                    last_error, published_at, created_at, updated_at
               from design_changes where site_id = ",
        );
        query.push_bind(context.site_id.into_uuid());
        if let Some(state) = filter.state {
            query.push(" and state = ").push_bind(state.as_str());
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
        let mut items = rows
            .iter()
            .map(from_change_row)
            .collect::<Result<Vec<_>>>()?;
        let limit = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if items.len() > limit {
            let last = items
                .get(limit.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_cursor(&ChangeCursor {
                created_at: last.created_at,
                id: last.id.into_uuid(),
            })?)
        } else {
            None
        };
        items.truncate(limit);
        Ok(Page::new(items, next_cursor))
    }

    pub async fn get_change(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: DesignChangeId,
    ) -> Result<DesignChange> {
        let row = sqlx::query(
            "select id, name, state, ready_build_id, published_build_id,
                    last_error, published_at, created_at, updated_at
               from design_changes where site_id = $1 and id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: DESIGN_CHANGE_NOT_FOUND,
        })?;
        from_change_row(&row)
    }

    pub async fn list_files(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        change_id: DesignChangeId,
        filter: &DesignFileListFilter,
    ) -> Result<Page<DesignFileSummary>> {
        self.ensure_change(tx, context, change_id).await?;
        let after: Option<FileCursor> =
            filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select path, bytes, sha256, removed, updated_at
               from design_files where site_id = ",
        );
        query
            .push_bind(context.site_id.into_uuid())
            .push(" and change_id = ")
            .push_bind(change_id.into_uuid());
        if let Some(after) = after {
            query.push(" and path > ").push_bind(after.path);
        }
        let rows = query
            .push(" order by path asc limit ")
            .push_bind(limit + 1)
            .build()
            .fetch_all(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let mut items = rows
            .iter()
            .map(from_file_summary_row)
            .collect::<Result<Vec<_>>>()?;
        let limit = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if items.len() > limit {
            let last = items
                .get(limit.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_cursor(&FileCursor {
                path: last.path.clone(),
            })?)
        } else {
            None
        };
        items.truncate(limit);
        Ok(Page::new(items, next_cursor))
    }

    pub async fn read_file(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        change_id: DesignChangeId,
        path: &str,
    ) -> Result<DesignFile> {
        let path = validate_source_path(path)?;
        let row = sqlx::query(
            "select path, contents, bytes, sha256, removed, updated_at
               from design_files
              where site_id = $1 and change_id = $2 and path = $3 and removed = false",
        )
        .bind(context.site_id.into_uuid())
        .bind(change_id.into_uuid())
        .bind(path)
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: DESIGN_FILE_NOT_FOUND,
        })?;
        from_file_row(&row)
    }

    pub async fn write_file(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        change_id: DesignChangeId,
        input: &DesignFileInput,
    ) -> Result<DesignFile> {
        let path = validate_source_path(&input.path)?;
        validate_contents(&input.contents)?;
        self.ensure_editable_change(tx, context, change_id).await?;
        let contents = input.contents.as_bytes();
        let bytes = i64::try_from(contents.len()).map_err(|_| MaviError::Internal)?;
        let sha256 = sha256_hex(contents);
        sqlx::query(
            "insert into design_files
                (site_id, change_id, path, contents, bytes, sha256, removed)
             values ($1, $2, $3, $4, $5, $6, false)
             on conflict (site_id, change_id, path) do update set
                contents = excluded.contents,
                bytes = excluded.bytes,
                sha256 = excluded.sha256,
                removed = false,
                updated_at = clock_timestamp()",
        )
        .bind(context.site_id.into_uuid())
        .bind(change_id.into_uuid())
        .bind(&path)
        .bind(contents)
        .bind(bytes)
        .bind(&sha256)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        self.mark_writing(tx, context, change_id).await?;
        let file = self.read_file(tx, context, change_id, &path).await?;
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "design.file.written".to_owned(),
                    resource_type: "DesignChange".to_owned(),
                    resource_id: Some(change_id.into_uuid()),
                    payload: json!({"path": file.path, "bytes": file.bytes}),
                },
            )
            .await?;
        Ok(file)
    }

    pub async fn remove_file(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        change_id: DesignChangeId,
        path: &str,
    ) -> Result<()> {
        let path = validate_source_path(path)?;
        self.ensure_editable_change(tx, context, change_id).await?;
        let result = sqlx::query(
            "update design_files
                set removed = true, updated_at = clock_timestamp()
              where site_id = $1 and change_id = $2 and path = $3 and removed = false",
        )
        .bind(context.site_id.into_uuid())
        .bind(change_id.into_uuid())
        .bind(&path)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if result.rows_affected() == 0 {
            return Err(MaviError::NotFound {
                resource: DESIGN_FILE_NOT_FOUND,
            });
        }
        self.mark_writing(tx, context, change_id).await?;
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "design.file.removed".to_owned(),
                    resource_type: "DesignChange".to_owned(),
                    resource_id: Some(change_id.into_uuid()),
                    payload: json!({"path": path}),
                },
            )
            .await
    }

    pub async fn start_build(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        change_id: DesignChangeId,
    ) -> Result<BuildRequest> {
        let row = sqlx::query(
            "select state from design_changes
              where site_id = $1 and id = $2 for update",
        )
        .bind(context.site_id.into_uuid())
        .bind(change_id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: DESIGN_CHANGE_NOT_FOUND,
        })?;
        match DesignState::parse(
            &row.try_get::<String, _>("state")
                .map_err(|_| MaviError::Internal)?,
        )? {
            DesignState::Published => return Err(MaviError::conflict(DESIGN_CHANGE_PUBLISHED)),
            DesignState::Building => return Err(MaviError::conflict(DESIGN_BUILD_IN_PROGRESS)),
            DesignState::Writing | DesignState::Ready | DesignState::Failed => {}
        }

        let build_id = DesignBuildId::new();
        let row = sqlx::query(
            "insert into design_builds (site_id, id, change_id)
             values ($1, $2, $3)
             returning id, change_id, state, error, created_at, completed_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(build_id.into_uuid())
        .bind(change_id.into_uuid())
        .fetch_one(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        sqlx::query(
            "update design_changes
                set state = 'building', ready_build_id = null,
                    last_error = null, updated_at = clock_timestamp()
              where site_id = $1 and id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(change_id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let source_rows = sqlx::query(
            "select path, contents from design_files
              where site_id = $1 and change_id = $2 and removed = false
              order by path asc",
        )
        .bind(context.site_id.into_uuid())
        .bind(change_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let source = source_rows
            .iter()
            .map(|row| {
                Ok(BuildSourceFile {
                    path: row.try_get("path").map_err(|_| MaviError::Internal)?,
                    contents: row.try_get("contents").map_err(|_| MaviError::Internal)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let build = from_build_row(&row)?;
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "design.build.started".to_owned(),
                    resource_type: "DesignBuild".to_owned(),
                    resource_id: Some(build_id.into_uuid()),
                    payload: json!({"change_id": change_id}),
                },
            )
            .await?;
        Ok(BuildRequest { build, source })
    }

    pub async fn persist_artifacts(
        &self,
        context: &SiteContext,
        store: &dyn FileStore,
        build_id: DesignBuildId,
        artifacts: Vec<BuildArtifact>,
    ) -> Result<Vec<StoredArtifact>> {
        let mut paths = BTreeSet::new();
        let mut stored = Vec::with_capacity(artifacts.len());
        for artifact in artifacts {
            let path = match validate_artifact_path(&artifact.path) {
                Ok(path) => path,
                Err(error) => {
                    remove_artifacts(context, store, &stored).await;
                    return Err(error);
                }
            };
            if !paths.insert(path.clone()) || artifact.contents.is_empty() {
                remove_artifacts(context, store, &stored).await;
                return Err(MaviError::validation(DESIGN_FILE_CONTENT_INVALID));
            }
            let bytes = u64::try_from(artifact.contents.len()).map_err(|_| MaviError::Internal)?;
            let sha256 = sha256_hex(&artifact.contents);
            let storage_key = artifact_storage_key(build_id, &path);
            if let Err(error) = store.put(context, &storage_key, artifact.contents).await {
                remove_artifacts(context, store, &stored).await;
                return Err(error);
            }
            stored.push(StoredArtifact {
                path,
                storage_key,
                mime: artifact.mime,
                bytes,
                sha256,
            });
        }
        if !paths.contains("index.html") {
            remove_artifacts(context, store, &stored).await;
            return Err(MaviError::validation(DESIGN_ENTRYPOINT_MISSING));
        }
        Ok(stored)
    }

    pub async fn finish_build_success(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        build_id: DesignBuildId,
        artifacts: &[StoredArtifact],
    ) -> Result<DesignBuild> {
        if artifacts.is_empty()
            || !artifacts
                .iter()
                .any(|artifact| artifact.path == "index.html")
        {
            return Err(MaviError::validation(DESIGN_ENTRYPOINT_MISSING));
        }
        let row = sqlx::query(
            "select id, change_id, state, error, created_at, completed_at
               from design_builds
              where site_id = $1 and id = $2 for update",
        )
        .bind(context.site_id.into_uuid())
        .bind(build_id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: DESIGN_BUILD_NOT_FOUND,
        })?;
        let build = from_build_row(&row)?;
        if build.state != DesignBuildState::Queued {
            return Err(MaviError::conflict(DESIGN_BUILD_IN_PROGRESS));
        }
        let mut paths = BTreeSet::new();
        for artifact in artifacts {
            if !paths.insert(artifact.path.clone()) {
                return Err(MaviError::validation(DESIGN_FILE_PATH_INVALID));
            }
            sqlx::query(
                "insert into design_build_artifacts
                    (site_id, build_id, path, storage_key, mime, bytes, sha256)
                 values ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(context.site_id.into_uuid())
            .bind(build_id.into_uuid())
            .bind(&artifact.path)
            .bind(&artifact.storage_key)
            .bind(&artifact.mime)
            .bind(i64::try_from(artifact.bytes).map_err(|_| MaviError::Internal)?)
            .bind(&artifact.sha256)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }
        sqlx::query(
            "update design_builds
                set state = 'ready', completed_at = clock_timestamp(), error = null
              where site_id = $1 and id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(build_id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        sqlx::query(
            "update design_changes
                set state = 'ready', ready_build_id = $3,
                    last_error = null, updated_at = clock_timestamp()
              where site_id = $1 and id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(build.change_id.into_uuid())
        .bind(build_id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "design.build.ready".to_owned(),
                    resource_type: "DesignBuild".to_owned(),
                    resource_id: Some(build_id.into_uuid()),
                    payload: json!({"change_id": build.change_id, "artifacts": artifacts.len()}),
                },
            )
            .await?;
        self.get_build(tx, context, build_id).await
    }

    pub async fn finish_build_failed(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        build_id: DesignBuildId,
        error: &str,
    ) -> Result<DesignBuild> {
        let error = validate_build_error(error);
        let row = sqlx::query(
            "select id, change_id, state, error, created_at, completed_at
               from design_builds
              where site_id = $1 and id = $2 for update",
        )
        .bind(context.site_id.into_uuid())
        .bind(build_id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: DESIGN_BUILD_NOT_FOUND,
        })?;
        let build = from_build_row(&row)?;
        if build.state != DesignBuildState::Queued {
            return Err(MaviError::conflict(DESIGN_BUILD_IN_PROGRESS));
        }
        sqlx::query(
            "update design_builds
                set state = 'failed', error = $3, completed_at = clock_timestamp()
              where site_id = $1 and id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(build_id.into_uuid())
        .bind(&error)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        sqlx::query(
            "update design_changes
                set state = 'failed', ready_build_id = null, last_error = $3,
                    updated_at = clock_timestamp()
              where site_id = $1 and id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(build.change_id.into_uuid())
        .bind(&error)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "design.build.failed".to_owned(),
                    resource_type: "DesignBuild".to_owned(),
                    resource_id: Some(build_id.into_uuid()),
                    payload: json!({"change_id": build.change_id, "error": error}),
                },
            )
            .await?;
        self.get_build(tx, context, build_id).await
    }

    pub async fn list_builds(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        change_id: DesignChangeId,
        filter: &DesignBuildListFilter,
    ) -> Result<Page<DesignBuild>> {
        self.ensure_change(tx, context, change_id).await?;
        let after: Option<BuildCursor> =
            filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select id, change_id, state, error, created_at, completed_at
               from design_builds where site_id = ",
        );
        query
            .push_bind(context.site_id.into_uuid())
            .push(" and change_id = ")
            .push_bind(change_id.into_uuid());
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
        let mut items = rows
            .iter()
            .map(from_build_row)
            .collect::<Result<Vec<_>>>()?;
        let limit = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if items.len() > limit {
            let last = items
                .get(limit.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_cursor(&BuildCursor {
                created_at: last.created_at,
                id: last.id.into_uuid(),
            })?)
        } else {
            None
        };
        items.truncate(limit);
        Ok(Page::new(items, next_cursor))
    }

    pub async fn get_build(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        build_id: DesignBuildId,
    ) -> Result<DesignBuild> {
        let row = sqlx::query(
            "select id, change_id, state, error, created_at, completed_at
               from design_builds where site_id = $1 and id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(build_id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: DESIGN_BUILD_NOT_FOUND,
        })?;
        from_build_row(&row)
    }

    pub async fn publish(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        change_id: DesignChangeId,
    ) -> Result<DesignChange> {
        self.activate(tx, context, change_id, false).await
    }

    pub async fn rollback(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        change_id: DesignChangeId,
    ) -> Result<DesignChange> {
        self.activate(tx, context, change_id, true).await
    }

    pub async fn preview_artifact(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        build_id: DesignBuildId,
        path: &str,
    ) -> Result<PublicArtifact> {
        let path = normalize_public_path(path)?;
        let row = sqlx::query(
            "select a.storage_key, a.mime
               from design_build_artifacts a
               join design_builds b on b.site_id = a.site_id and b.id = a.build_id
              where a.site_id = $1 and a.build_id = $2 and a.path = $3 and b.state = 'ready'",
        )
        .bind(context.site_id.into_uuid())
        .bind(build_id.into_uuid())
        .bind(path)
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: DESIGN_PUBLIC_ASSET_NOT_FOUND,
        })?;
        public_artifact_from_row(&row)
    }

    pub async fn live_artifact(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        path: &str,
    ) -> Result<PublicArtifact> {
        let path = normalize_public_path(path)?;
        let row = sqlx::query(
            "select a.storage_key, a.mime
               from design_changes c
               join design_build_artifacts a
                 on a.site_id = c.site_id and a.build_id = c.published_build_id
              where c.site_id = $1 and c.state = 'published' and a.path = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(path)
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: DESIGN_PUBLIC_ASSET_NOT_FOUND,
        })?;
        public_artifact_from_row(&row)
    }

    async fn activate(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        target_id: DesignChangeId,
        rollback: bool,
    ) -> Result<DesignChange> {
        let row = sqlx::query(
            "select id, state, ready_build_id, published_build_id
               from design_changes where site_id = $1 and id = $2 for update",
        )
        .bind(context.site_id.into_uuid())
        .bind(target_id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: DESIGN_CHANGE_NOT_FOUND,
        })?;
        let target_state = DesignState::parse(
            &row.try_get::<String, _>("state")
                .map_err(|_| MaviError::Internal)?,
        )?;
        let selected_build = if rollback {
            if target_state != DesignState::Ready {
                return Err(MaviError::conflict(DESIGN_NOT_READY));
            }
            row.try_get::<Option<Uuid>, _>("published_build_id")
                .map_err(|_| MaviError::Internal)?
        } else {
            if target_state != DesignState::Ready {
                return Err(MaviError::conflict(DESIGN_NOT_READY));
            }
            row.try_get::<Option<Uuid>, _>("ready_build_id")
                .map_err(|_| MaviError::Internal)?
        }
        .ok_or(MaviError::conflict(DESIGN_NOT_READY))?;

        let current = sqlx::query(
            "select id from design_changes
              where site_id = $1 and state = 'published' for update",
        )
        .bind(context.site_id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .map(|current| current.try_get::<Uuid, _>("id"))
        .transpose()
        .map_err(|_| MaviError::Internal)?;
        if current == Some(target_id.into_uuid()) {
            return Err(MaviError::conflict(DESIGN_CHANGE_PUBLISHED));
        }
        if let Some(current_id) = current {
            sqlx::query(
                "update design_changes set state = 'ready', updated_at = clock_timestamp()
                  where site_id = $1 and id = $2",
            )
            .bind(context.site_id.into_uuid())
            .bind(current_id)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }
        sqlx::query(
            "update design_changes
                set state = 'published', published_build_id = $3,
                    published_at = clock_timestamp(), last_error = null,
                    updated_at = clock_timestamp()
              where site_id = $1 and id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(target_id.into_uuid())
        .bind(selected_build)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: if rollback {
                        "design.change.rolled_back".to_owned()
                    } else {
                        "design.change.published".to_owned()
                    },
                    resource_type: "DesignChange".to_owned(),
                    resource_id: Some(target_id.into_uuid()),
                    payload: json!({"build_id": selected_build, "rollback": rollback}),
                },
            )
            .await?;
        self.get_change(tx, context, target_id).await
    }

    async fn ensure_change(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: DesignChangeId,
    ) -> Result<()> {
        sqlx::query("select 1 from design_changes where site_id = $1 and id = $2")
            .bind(context.site_id.into_uuid())
            .bind(id.into_uuid())
            .fetch_optional(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?
            .ok_or(MaviError::NotFound {
                resource: DESIGN_CHANGE_NOT_FOUND,
            })?;
        Ok(())
    }

    async fn ensure_editable_change(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: DesignChangeId,
    ) -> Result<()> {
        let row = sqlx::query(
            "select state from design_changes where site_id = $1 and id = $2 for update",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: DESIGN_CHANGE_NOT_FOUND,
        })?;
        if DesignState::parse(
            &row.try_get::<String, _>("state")
                .map_err(|_| MaviError::Internal)?,
        )? == DesignState::Published
        {
            return Err(MaviError::conflict(DESIGN_CHANGE_PUBLISHED));
        }
        Ok(())
    }

    async fn mark_writing(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: DesignChangeId,
    ) -> Result<()> {
        sqlx::query(
            "update design_changes
                set state = 'writing', ready_build_id = null,
                    last_error = null, updated_at = clock_timestamp()
              where site_id = $1 and id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        Ok(())
    }
}

async fn remove_artifacts(
    context: &SiteContext,
    store: &dyn FileStore,
    artifacts: &[StoredArtifact],
) {
    for artifact in artifacts {
        let _ = store.remove(context, &artifact.storage_key).await;
    }
}

fn from_change_row(row: &sqlx::postgres::PgRow) -> Result<DesignChange> {
    Ok(DesignChange {
        id: DesignChangeId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        name: row.try_get("name").map_err(|_| MaviError::Internal)?,
        state: DesignState::parse(
            &row.try_get::<String, _>("state")
                .map_err(|_| MaviError::Internal)?,
        )?,
        ready_build_id: row
            .try_get::<Option<Uuid>, _>("ready_build_id")
            .map_err(|_| MaviError::Internal)?
            .map(DesignBuildId::from_uuid),
        published_build_id: row
            .try_get::<Option<Uuid>, _>("published_build_id")
            .map_err(|_| MaviError::Internal)?
            .map(DesignBuildId::from_uuid),
        last_error: row.try_get("last_error").map_err(|_| MaviError::Internal)?,
        published_at: row
            .try_get("published_at")
            .map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

fn from_build_row(row: &sqlx::postgres::PgRow) -> Result<DesignBuild> {
    let id = DesignBuildId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?);
    let change_id =
        DesignChangeId::from_uuid(row.try_get("change_id").map_err(|_| MaviError::Internal)?);
    Ok(DesignBuild {
        id,
        change_id,
        state: DesignBuildState::parse(
            &row.try_get::<String, _>("state")
                .map_err(|_| MaviError::Internal)?,
        )?,
        error: row.try_get("error").map_err(|_| MaviError::Internal)?,
        preview_path: preview_path(id),
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        completed_at: row
            .try_get("completed_at")
            .map_err(|_| MaviError::Internal)?,
    })
}

fn from_file_summary_row(row: &sqlx::postgres::PgRow) -> Result<DesignFileSummary> {
    let bytes: i64 = row.try_get("bytes").map_err(|_| MaviError::Internal)?;
    Ok(DesignFileSummary {
        path: row.try_get("path").map_err(|_| MaviError::Internal)?,
        bytes: u64::try_from(bytes).map_err(|_| MaviError::Internal)?,
        sha256: row.try_get("sha256").map_err(|_| MaviError::Internal)?,
        removed: row.try_get("removed").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

fn from_file_row(row: &sqlx::postgres::PgRow) -> Result<DesignFile> {
    let bytes: i64 = row.try_get("bytes").map_err(|_| MaviError::Internal)?;
    let contents: Vec<u8> = row.try_get("contents").map_err(|_| MaviError::Internal)?;
    Ok(DesignFile {
        path: row.try_get("path").map_err(|_| MaviError::Internal)?,
        contents: String::from_utf8(contents).map_err(|_| MaviError::Internal)?,
        bytes: u64::try_from(bytes).map_err(|_| MaviError::Internal)?,
        sha256: row.try_get("sha256").map_err(|_| MaviError::Internal)?,
        removed: row.try_get("removed").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

fn public_artifact_from_row(row: &sqlx::postgres::PgRow) -> Result<PublicArtifact> {
    Ok(PublicArtifact {
        storage_key: row
            .try_get("storage_key")
            .map_err(|_| MaviError::Internal)?,
        mime: row.try_get("mime").map_err(|_| MaviError::Internal)?,
    })
}

fn validate_name(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > MAX_DESIGN_NAME_CHARS
        || value.chars().any(char::is_control)
    {
        return Err(MaviError::validation_field(DESIGN_NAME_INVALID, "name"));
    }
    Ok(value.to_owned())
}

fn validate_contents(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > MAX_DESIGN_FILE_BYTES
        || value.chars().any(|character| {
            character == '\0'
                || (character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        })
    {
        return Err(MaviError::validation_field(
            DESIGN_FILE_CONTENT_INVALID,
            "contents",
        ));
    }
    Ok(())
}

fn validate_digest_and_bytes(bytes: u64, sha256: &str, contents: &[u8]) -> Result<()> {
    if bytes == 0
        || bytes > MAX_DESIGN_FILE_BYTES as u64
        || usize::try_from(bytes).map_err(|_| MaviError::Internal)? != contents.len()
        || sha256.len() != 64
        || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
        || sha256_hex(contents) != sha256
    {
        return Err(MaviError::validation("design_relocation_content_invalid"));
    }
    Ok(())
}

fn valid_relocation_mime(mime: &str) -> bool {
    let Some((kind, subtype)) = mime.split_once('/') else {
        return false;
    };
    !kind.is_empty()
        && !subtype.is_empty()
        && kind.bytes().all(|byte| byte.is_ascii_lowercase())
        && subtype.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b".+-".contains(&byte)
        })
}

fn validate_source_path(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > MAX_DESIGN_PATH_CHARS
        || value.starts_with('/')
        || value.contains('\\')
        || value.contains('\0')
    {
        return Err(MaviError::validation_field(
            DESIGN_FILE_PATH_INVALID,
            "path",
        ));
    }
    let mut components = value.split('/');
    let root = components.next().unwrap_or_default();
    if root != "src" && root != "public" {
        return Err(MaviError::validation_field(
            DESIGN_FILE_PATH_INVALID,
            "path",
        ));
    }
    if components.clone().next().is_none()
        || components.any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Err(MaviError::validation_field(
            DESIGN_FILE_PATH_INVALID,
            "path",
        ));
    }
    if [
        "package.json",
        "Dockerfile",
        ".env",
        "vite.config.js",
        "vite.config.ts",
        "astro.config.js",
        "astro.config.ts",
    ]
    .iter()
    .any(|forbidden| value == *forbidden || value.ends_with(&format!("/{forbidden}")))
        || value.starts_with("scripts/")
        || value.starts_with(".github/")
    {
        return Err(MaviError::validation_field(
            DESIGN_FILE_PATH_INVALID,
            "path",
        ));
    }
    Ok(value.to_owned())
}

fn validate_artifact_path(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > MAX_DESIGN_PATH_CHARS
        || value.starts_with('/')
        || value.contains('\\')
        || value.contains('\0')
        || value
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Err(MaviError::validation(DESIGN_FILE_PATH_INVALID));
    }
    Ok(value.to_owned())
}

fn normalize_public_path(value: &str) -> Result<String> {
    let value = value.trim_start_matches('/');
    if value.is_empty() {
        return Ok("index.html".to_owned());
    }
    validate_artifact_path(value)
}

fn validate_build_error(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        return DESIGN_BUILD_FAILED.to_owned();
    }
    value.chars().take(500).collect()
}

#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut value = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(value, "{byte:02x}").expect("writing a digest to String cannot fail");
    }
    value
}

#[must_use]
pub fn artifact_storage_key(build_id: DesignBuildId, path: &str) -> String {
    format!("design/builds/{build_id}/{path}")
}

#[must_use]
pub fn preview_path(build_id: DesignBuildId) -> String {
    format!("/preview/v1/design/{build_id}/index.html")
}

fn mime_for_path(path: &str) -> &'static str {
    match path
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "css" => "text/css",
        "js" | "mjs" => "text/javascript",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "html" => "text/html",
        _ => "application/octet-stream",
    }
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
    fn source_paths_are_explicit_and_non_traversable() {
        assert!(validate_source_path("public/index.html").is_ok());
        assert!(validate_source_path("src/styles.css").is_ok());
        assert!(validate_source_path("../public/index.html").is_err());
        assert!(validate_source_path("public/../secret").is_err());
        assert!(validate_source_path("public/package.json").is_err());
        assert!(validate_source_path("public").is_err());
    }

    #[test]
    fn artifact_keys_are_build_scoped_and_cursors_are_opaque() {
        let build_id = DesignBuildId::new();
        let key = artifact_storage_key(build_id, "index.html");
        assert!(key.contains(&build_id.to_string()));
        assert_eq!(preview_path(build_id).matches("index.html").count(), 1);
        let cursor = encode_cursor(&FileCursor {
            path: "index.html".to_owned(),
        })
        .expect("cursor");
        assert!(!cursor.as_str().contains("index.html"));
        assert_eq!(
            decode_cursor::<FileCursor>(&cursor).expect("decoded").path,
            "index.html"
        );
    }

    #[test]
    fn design_list_contracts_are_cursor_only() {
        let api = api();
        api.validate().expect("design API contract");
        for name in [
            "DesignChangeListFilter",
            "DesignFileListFilter",
            "DesignBuildListFilter",
        ] {
            let shape = shapes()
                .into_iter()
                .find(|shape| shape.name == name)
                .expect("list shape");
            let properties = shape.schema["properties"].as_object().expect("properties");
            assert!(properties.contains_key("after"));
            assert!(properties.contains_key("limit"));
            assert!(!properties.contains_key("page"));
            assert!(!properties.contains_key("offset"));
        }
    }

    #[tokio::test]
    async fn static_engine_only_exposes_public_files() {
        let context = SiteContext::public(mavi_core::SiteId::new());
        let source = vec![
            BuildSourceFile {
                path: "src/main.ts".to_owned(),
                contents: b"secret".to_vec(),
            },
            BuildSourceFile {
                path: "public/index.html".to_owned(),
                contents: b"<h1>ok</h1>".to_vec(),
            },
        ];
        let artifacts = StaticBuildEngine
            .build(&context, DesignBuildId::new(), &source)
            .await
            .expect("build");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].path, "index.html");
    }
}
