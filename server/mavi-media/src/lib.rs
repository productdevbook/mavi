//! Site-scoped media metadata and binary storage orchestration.
//!
//! `PostgreSQL` owns the tenant-scoped metadata, while a [`FileStore`] owns the
//! bytes. Uploads write bytes before metadata; trashing tombstones metadata
//! while retaining bytes for restore, and permanent trash deletion removes
//! both. The two systems cannot share one transaction, so cleanup remains an
//! explicit retryable adapter operation.

use std::{collections::BTreeSet, io::Cursor as IoCursor};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use image::{ImageReader, Limits, codecs::jpeg::JpegEncoder, imageops::FilterType};
use mavi_audit::{AuditEntry, AuditService};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Capability, Cursor, ErrorCode, FileId, JobId, MaviError, MediaVariantId, Page,
    PageRequest, Result, SiteContext, ports::FileStore,
};
use mavi_jobs::{JobKind, JobState, JobsService};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

pub const FILE_NOT_FOUND: &str = "media_file_not_found";
pub const FILE_EMPTY: &str = "media_file_empty";
pub const FILE_TOO_LARGE: &str = "media_file_too_large";
pub const FILE_KIND_UNSUPPORTED: &str = "media_file_kind_unsupported";
pub const FILE_NAME_INVALID: &str = "media_file_name_invalid";
pub const FILE_NAME_TOO_LONG: &str = "media_file_name_too_long";
pub const FILE_VISIBILITY_INVALID: &str = "media_file_visibility_invalid";
pub const MEDIA_CLEANUP_JOB: JobKind = JobKind::new("media.cleanup", 8);
pub const MEDIA_VARIANT_JOB: JobKind = JobKind::new("media.variant_generate", 5);
pub const MEDIA_ORPHAN_CLEANUP_JOB: JobKind = JobKind::new("media.orphan_cleanup", 5);
pub const MEDIA_ORPHAN_BUCKET_SECONDS: i64 = 60 * 60;
pub const MAX_VARIANT_SOURCE_DIMENSION: u32 = 10_000;
pub const MAX_VARIANT_SOURCE_ALLOC: u64 = 256 * 1024 * 1024;
const VARIANT_JPEG_QUALITY: u8 = 82;

pub const MAX_FILE_BYTES: usize = 100 * 1024 * 1024;
/// Maximum raw binary payload carried by one private shard relocation.
///
/// The wire envelope is JSON and therefore larger than this value after
/// base64 encoding. The portable coordinator applies the larger envelope
/// limit around this domain limit.
pub const MAX_MEDIA_RELOCATION_BYTES: usize = 192 * 1024 * 1024;
const MAX_FILE_NAME_CHARS: usize = 255;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Image,
    Video,
    Audio,
    Document,
}

impl FileKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Document => "document",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "image" => Ok(Self::Image),
            "video" => Ok(Self::Video),
            "audio" => Ok(Self::Audio),
            "document" => Ok(Self::Document),
            _ => Err(MaviError::Internal),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileVisibility {
    #[default]
    Private,
    Public,
}

impl FileVisibility {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Public => "public",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "private" => Ok(Self::Private),
            "public" => Ok(Self::Public),
            _ => Err(MaviError::Internal),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct FileListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
    pub kind: Option<FileKind>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct FileVariantListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VariantPreset {
    Thumbnail,
    Medium,
    Large,
}

impl VariantPreset {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Thumbnail => "thumbnail",
            Self::Medium => "medium",
            Self::Large => "large",
        }
    }

    #[must_use]
    pub const fn max_dimension(self) -> u32 {
        match self {
            Self::Thumbnail => 320,
            Self::Medium => 1024,
            Self::Large => 2048,
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "thumbnail" => Ok(Self::Thumbnail),
            "medium" => Ok(Self::Medium),
            "large" => Ok(Self::Large),
            _ => Err(MaviError::validation("media_variant_preset_invalid")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FileVariant {
    pub id: MediaVariantId,
    pub source_file_id: FileId,
    pub preset: VariantPreset,
    pub mime: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub sha256: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MediaVariantJob {
    pub source_file_id: FileId,
    pub variant_id: MediaVariantId,
    pub preset: VariantPreset,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedVariant {
    pub content: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaVariantSource {
    pub storage_key: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UploadFileQuery {
    pub name: String,
    #[serde(default)]
    pub visibility: FileVisibility,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FileRecord {
    pub id: FileId,
    pub kind: FileKind,
    pub visibility: FileVisibility,
    pub mime: String,
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
    pub created_at: DateTime<Utc>,
}

/// Payload for the durable binary cleanup job.
///
/// Metadata deletion and object-store deletion cannot share one transaction.
/// The payload therefore carries only the site-scoped file identity and the
/// already-validated storage key; the worker still completes the cleanup task
/// through this domain service before acknowledging the job.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MediaCleanupJob {
    pub file_id: FileId,
    pub storage_key: String,
    #[serde(default)]
    pub additional_storage_keys: Vec<String>,
}

/// Payload for the periodic, site-scoped storage reconciliation job.
///
/// The bucket is part of the idempotency key so one worker process can poll
/// frequently without creating duplicate scans while another worker may
/// safely claim the same site after a restart.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MediaOrphanCleanupJob {
    pub bucket: i64,
}

/// Site media transferred only by the authenticated shard relocation port.
///
/// Public portable exports intentionally do not include this type. Binary
/// data is encoded as URL-safe base64 so the operator can use one typed JSON
/// request while retaining the existing HTTP transfer boundary.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MediaRelocation {
    pub files: Vec<MediaRelocationFile>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MediaRelocationFile {
    pub id: FileId,
    pub kind: FileKind,
    pub visibility: FileVisibility,
    pub mime: String,
    pub name: String,
    pub storage_key: String,
    pub bytes: u64,
    pub sha256: String,
    pub created_at: DateTime<Utc>,
    #[serde(with = "base64_bytes")]
    pub content: Vec<u8>,
}

impl MediaRelocation {
    pub fn validate(&self) -> Result<()> {
        let mut ids = BTreeSet::new();
        let mut storage_keys = BTreeSet::new();
        let mut total_bytes = 0usize;

        for file in &self.files {
            if !ids.insert(file.id) {
                return Err(MaviError::validation("media_relocation_duplicate_file"));
            }
            if !storage_keys.insert(file.storage_key.as_str()) {
                return Err(MaviError::validation(
                    "media_relocation_duplicate_storage_key",
                ));
            }
            validate_storage_key(file.id, &file.storage_key)?;
            if validate_name(&file.name)? != file.name {
                return Err(MaviError::validation(FILE_NAME_INVALID));
            }
            if !valid_mime(&file.mime) {
                return Err(MaviError::validation("media_relocation_mime_invalid"));
            }
            if file.bytes == 0 || file.bytes > MAX_FILE_BYTES as u64 {
                return Err(MaviError::validation(FILE_TOO_LARGE));
            }
            let content_bytes = file.content.len();
            if content_bytes
                != usize::try_from(file.bytes)
                    .map_err(|_| MaviError::validation("media_relocation_byte_count_invalid"))?
            {
                return Err(MaviError::validation("media_relocation_byte_count_invalid"));
            }
            total_bytes = total_bytes
                .checked_add(content_bytes)
                .ok_or_else(|| MaviError::validation("media_relocation_size_overflow"))?;
            if total_bytes > MAX_MEDIA_RELOCATION_BYTES {
                return Err(MaviError::validation("media_relocation_too_large"));
            }
            if file.sha256.len() != 64 || !file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(MaviError::validation("media_relocation_digest_invalid"));
            }
            if hex_digest(&Sha256::digest(&file.content)) != file.sha256 {
                return Err(MaviError::validation("media_relocation_digest_mismatch"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MediaService;

#[must_use]
pub fn api() -> mavi_contract::Api {
    mavi_contract::Api::new(endpoints()).with_shapes(shapes())
}

#[must_use]
#[allow(clippy::too_many_lines)]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::new(
            Method::Get,
            "/api/v1/files",
            "media.files.list",
            "List site files with an opaque cursor",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Media,
            action: Action::View,
        })
        .takes_query("FileListFilter")
        .returns(200, "FilePage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/files",
            "media.files.upload",
            "Upload a file whose kind is detected from its bytes",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Media,
            action: Action::Write,
        })
        .takes_raw("FileBytes")
        .with_query("UploadFileQuery")
        .returns(201, "File")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/files/{id}",
            "media.files.read",
            "Read file metadata",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Media,
            action: Action::View,
        })
        .returns(200, "File")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/files/{id}/content",
            "media.files.download",
            "Download file bytes for an authorized site caller",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Media,
            action: Action::View,
        })
        .returns_raw(200, "FileBytes")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/files/{id}/variants",
            "media.files.variants.list",
            "List generated image variants for a file",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Media,
            action: Action::View,
        })
        .takes_query("FileVariantListFilter")
        .returns(200, "FileVariantPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/files/{id}/variants/{preset}/content",
            "media.files.variants.download",
            "Download a generated image variant for an authorized site caller",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Media,
            action: Action::View,
        })
        .returns_raw(200, "FileBytes")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/public/v1/files/{id}",
            "media.files.public_download",
            "Download a file explicitly marked public",
        )
        .public()
        .returns_raw(200, "FileBytes")
        .refuses([ErrorCode::NotFound, ErrorCode::Internal]),
        Endpoint::new(
            Method::Get,
            "/public/v1/files/{id}/variants/{preset}",
            "media.files.variants.public_download",
            "Download a generated variant of an explicitly public file",
        )
        .public()
        .returns_raw(200, "FileBytes")
        .refuses([
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Delete,
            "/api/v1/files/{id}",
            "media.files.trash",
            "Move a file to trash while retaining its binary for restore",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Media,
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
#[allow(clippy::too_many_lines)]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "FileKind",
            json!({"type": "string", "enum": ["image", "video", "audio", "document"]}),
        ),
        Shape::new(
            "FileVisibility",
            json!({"type": "string", "enum": ["private", "public"]}),
        ),
        Shape::new(
            "FileListFilter",
            json!({
                "type": "object",
                "properties": {
                    "after": {"type": ["string", "null"], "maxLength": 512},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                    "kind": {"$ref": "#/components/schemas/FileKind"},
                },
            }),
        ),
        Shape::new(
            "FileVariantListFilter",
            json!({
                "type": "object",
                "properties": {
                    "after": {"type": ["string", "null"], "maxLength": 512},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                },
            }),
        ),
        Shape::new(
            "VariantPreset",
            json!({"type": "string", "enum": ["thumbnail", "medium", "large"]}),
        ),
        Shape::new(
            "UploadFileQuery",
            json!({
                "type": "object",
                "required": ["name"],
                "properties": {
                    "name": {"type": "string", "minLength": 1, "maxLength": 255},
                    "visibility": {"$ref": "#/components/schemas/FileVisibility"},
                },
            }),
        ),
        Shape::new("FileBytes", json!({"type": "string", "format": "binary"})),
        Shape::new(
            "File",
            json!({
                "type": "object",
                "required": ["id", "kind", "visibility", "mime", "name", "bytes", "sha256", "created_at"],
                "properties": {
                    "id": {"type": "string", "format": "uuid"},
                    "kind": {"$ref": "#/components/schemas/FileKind"},
                    "visibility": {"$ref": "#/components/schemas/FileVisibility"},
                    "mime": {"type": "string", "maxLength": 127},
                    "name": {"type": "string", "maxLength": 255},
                    "bytes": {"type": "integer", "format": "int64", "minimum": 1},
                    "sha256": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
                    "created_at": {"type": "string", "format": "date-time"},
                },
            }),
        ),
        Shape::new(
            "FilePage",
            json!({
                "type": "object",
                "required": ["items", "next_cursor"],
                "properties": {
                    "items": {"type": "array", "items": {"$ref": "#/components/schemas/File"}},
                    "next_cursor": {"type": ["string", "null"], "maxLength": 512},
                },
            }),
        ),
        Shape::new(
            "FileVariant",
            json!({
                "type": "object",
                "required": ["id", "source_file_id", "preset", "mime", "width", "height", "bytes", "sha256", "created_at"],
                "properties": {
                    "id": {"type": "string", "format": "uuid"},
                    "source_file_id": {"type": "string", "format": "uuid"},
                    "preset": {"$ref": "#/components/schemas/VariantPreset"},
                    "mime": {"type": "string", "const": "image/jpeg"},
                    "width": {"type": "integer", "minimum": 1},
                    "height": {"type": "integer", "minimum": 1},
                    "bytes": {"type": "integer", "format": "int64", "minimum": 1},
                    "sha256": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
                    "created_at": {"type": "string", "format": "date-time"},
                },
            }),
        ),
        Shape::new(
            "FileVariantPage",
            json!({
                "type": "object",
                "required": ["items", "next_cursor"],
                "properties": {
                    "items": {"type": "array", "items": {"$ref": "#/components/schemas/FileVariant"}},
                    "next_cursor": {"type": ["string", "null"], "maxLength": 512},
                },
            }),
        ),
    ]
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct FileCursor {
    created_at: DateTime<Utc>,
    id: Uuid,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct VariantCursor {
    created_at: DateTime<Utc>,
    id: Uuid,
}

#[derive(Clone, Copy, Debug)]
struct DetectedFile {
    kind: FileKind,
    mime: &'static str,
    extension: &'static str,
}

const SIGNATURES: &[(&[u8], usize, FileKind, &str, &str)] = &[
    (b"\x89PNG\r\n\x1a\n", 0, FileKind::Image, "image/png", "png"),
    (b"\xff\xd8\xff", 0, FileKind::Image, "image/jpeg", "jpg"),
    (b"GIF8", 0, FileKind::Image, "image/gif", "gif"),
    (b"WEBP", 8, FileKind::Image, "image/webp", "webp"),
    (b"ftyp", 4, FileKind::Video, "video/mp4", "mp4"),
    (b"ID3", 0, FileKind::Audio, "audio/mpeg", "mp3"),
    (b"%PDF-", 0, FileKind::Document, "application/pdf", "pdf"),
];

impl MediaService {
    pub async fn upload(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        store: &dyn FileStore,
        name: &str,
        visibility: FileVisibility,
        bytes: Vec<u8>,
    ) -> Result<FileRecord> {
        let detected = detect(&bytes)?;
        let name = validate_name(name)?;
        let id = FileId::new();
        let storage_key = storage_key(id, detected.extension);
        let byte_count =
            i64::try_from(bytes.len()).map_err(|_| MaviError::validation(FILE_TOO_LARGE))?;
        let sha256 = hex_digest(&Sha256::digest(&bytes));

        store.put(context, &storage_key, bytes).await?;

        let Ok(row) = sqlx::query(
            "insert into media_files
                (site_id, id, kind, visibility, mime, name, storage_key, bytes, sha256)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             returning id, kind, visibility, mime, name, storage_key, bytes, sha256, created_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(detected.kind.as_str())
        .bind(visibility.as_str())
        .bind(detected.mime)
        .bind(&name)
        .bind(&storage_key)
        .bind(byte_count)
        .bind(&sha256)
        .fetch_one(tx.conn())
        .await
        else {
            let _ = store.remove(context, &storage_key).await;
            return Err(MaviError::Internal);
        };

        let file = from_row(&row)?;
        if let Err(error) = AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "media.file.uploaded".to_owned(),
                    resource_type: "File".to_owned(),
                    resource_id: Some(id.into_uuid()),
                    payload: json!({"kind": file.record.kind, "visibility": file.record.visibility, "bytes": file.record.bytes, "mime": file.record.mime}),
                },
            )
            .await
        {
            let _ = store.remove(context, &storage_key).await;
            return Err(error);
        }

        Ok(file.record)
    }

    /// Reads live media metadata and bytes for the authenticated shard
    /// relocation coordinator. Deleted files remain owned by the trash
    /// domain and are not part of the active site snapshot.
    pub async fn export_for_relocation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        store: &dyn FileStore,
    ) -> Result<MediaRelocation> {
        let rows = sqlx::query(
            "select id, kind, visibility, mime, name, storage_key, bytes, sha256, created_at
               from media_files
              where site_id = $1 and deleted_at is null
              order by created_at asc, id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        let mut files = Vec::with_capacity(rows.len());
        for row in &rows {
            let stored = from_row(row)?;
            let bytes = store.get(context, &stored.storage_key).await?;
            let expected_bytes = usize::try_from(stored.record.bytes)
                .map_err(|_| MaviError::validation("media_relocation_byte_count_invalid"))?;
            if bytes.len() != expected_bytes
                || hex_digest(&Sha256::digest(&bytes)) != stored.record.sha256
            {
                return Err(MaviError::validation("media_storage_integrity_failed"));
            }
            files.push(MediaRelocationFile {
                id: stored.record.id,
                kind: stored.record.kind,
                visibility: stored.record.visibility,
                mime: stored.record.mime,
                name: stored.record.name,
                storage_key: stored.storage_key,
                bytes: stored.record.bytes,
                sha256: stored.record.sha256,
                created_at: stored.record.created_at,
                content: bytes,
            });
        }

        let relocation = MediaRelocation { files };
        relocation.validate()?;
        Ok(relocation)
    }

    /// Copies a validated media snapshot into the target site's storage and
    /// upserts its metadata. `FileStore::put` is intentionally replace-safe,
    /// which makes retries after a network failure idempotent.
    pub async fn import_for_relocation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        store: &dyn FileStore,
        relocation: &MediaRelocation,
    ) -> Result<()> {
        relocation.validate()?;
        for file in &relocation.files {
            store
                .put(context, &file.storage_key, file.content.clone())
                .await?;
            sqlx::query(
                "insert into media_files
                    (site_id, id, kind, visibility, mime, name, storage_key, bytes, sha256, created_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                 on conflict (site_id, id) do update set
                    kind = excluded.kind, visibility = excluded.visibility,
                    mime = excluded.mime, name = excluded.name,
                    storage_key = excluded.storage_key, bytes = excluded.bytes,
                    sha256 = excluded.sha256, created_at = excluded.created_at,
                    deleted_at = null",
            )
            .bind(context.site_id.into_uuid())
            .bind(file.id.into_uuid())
            .bind(file.kind.as_str())
            .bind(file.visibility.as_str())
            .bind(&file.mime)
            .bind(&file.name)
            .bind(&file.storage_key)
            .bind(
                i64::try_from(file.bytes)
                    .map_err(|_| MaviError::validation("media_relocation_byte_count_invalid"))?,
            )
            .bind(&file.sha256)
            .bind(file.created_at)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }

        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "portable.media.relocated".to_owned(),
                    resource_type: "MediaSnapshot".to_owned(),
                    resource_id: None,
                    payload: json!({
                        "files": relocation.files.len(),
                        "bytes": relocation.files.iter().map(|file| file.bytes).sum::<u64>(),
                    }),
                },
            )
            .await
    }

    pub async fn list(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        filter: &FileListFilter,
    ) -> Result<Page<FileRecord>> {
        let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select id, kind, visibility, mime, name, storage_key, bytes, sha256, created_at
               from media_files where site_id = ",
        );
        query
            .push_bind(context.site_id.into_uuid())
            .push(" and deleted_at is null");
        if let Some(kind) = filter.kind {
            query.push(" and kind = ").push_bind(kind.as_str());
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
        let mut files = rows
            .iter()
            .map(from_row)
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(|file| file.record)
            .collect::<Vec<_>>();
        let limit_usize = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if files.len() > limit_usize {
            let last = files
                .get(limit_usize.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_cursor(last.created_at, last.id.into_uuid())?)
        } else {
            None
        };
        files.truncate(limit_usize);
        Ok(Page::new(files, next_cursor))
    }

    pub async fn get(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: FileId,
    ) -> Result<FileRecord> {
        let row = sqlx::query(
            "select id, kind, visibility, mime, name, storage_key, bytes, sha256, created_at
               from media_files
              where site_id = $1 and id = $2 and deleted_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: FILE_NOT_FOUND,
        })?;
        Ok(from_row(&row)?.record)
    }

    pub async fn list_variants(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        source_file_id: FileId,
        filter: &FileVariantListFilter,
    ) -> Result<Page<FileVariant>> {
        self.get(tx, context, source_file_id).await?;
        let after = filter
            .page
            .after
            .as_ref()
            .map(decode_variant_cursor)
            .transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let rows = match after {
            Some(after) => sqlx::query(
                "select id, source_file_id, preset, mime, width, height, bytes, sha256, created_at
                       from media_variants
                      where site_id = $1 and source_file_id = $2
                        and (created_at, id) > ($3, $4)
                      order by created_at asc, id asc
                      limit $5",
            )
            .bind(context.site_id.into_uuid())
            .bind(source_file_id.into_uuid())
            .bind(after.created_at)
            .bind(after.id)
            .bind(limit + 1)
            .fetch_all(tx.conn())
            .await,
            None => sqlx::query(
                "select id, source_file_id, preset, mime, width, height, bytes, sha256, created_at
                       from media_variants
                      where site_id = $1 and source_file_id = $2
                      order by created_at asc, id asc
                      limit $3",
            )
            .bind(context.site_id.into_uuid())
            .bind(source_file_id.into_uuid())
            .bind(limit + 1)
            .fetch_all(tx.conn())
            .await,
        }
        .map_err(|_| MaviError::Internal)?;

        let mut variants = rows
            .iter()
            .map(variant_from_row)
            .collect::<Result<Vec<_>>>()?;
        let limit_usize = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if variants.len() > limit_usize {
            let last = variants
                .get(limit_usize.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_variant_cursor(last.created_at, last.id.into_uuid())?)
        } else {
            None
        };
        variants.truncate(limit_usize);
        Ok(Page::new(variants, next_cursor))
    }

    pub async fn read_variant_bytes(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        store: &dyn FileStore,
        source_file_id: FileId,
        preset: VariantPreset,
        public: bool,
    ) -> Result<(FileVariant, Vec<u8>)> {
        let row = if public {
            sqlx::query(
                "select v.id, v.source_file_id, v.preset, v.mime, v.storage_key,
                        v.width, v.height, v.bytes, v.sha256, v.created_at
                   from media_variants v
                   join media_files f on f.site_id = v.site_id and f.id = v.source_file_id
                  where v.site_id = $1 and v.source_file_id = $2 and v.preset = $3
                    and f.visibility = 'public' and f.deleted_at is null",
            )
            .bind(context.site_id.into_uuid())
            .bind(source_file_id.into_uuid())
            .bind(preset.as_str())
            .fetch_optional(tx.conn())
            .await
        } else {
            sqlx::query(
                "select v.id, v.source_file_id, v.preset, v.mime, v.storage_key,
                        v.width, v.height, v.bytes, v.sha256, v.created_at
                   from media_variants v
                   join media_files f on f.site_id = v.site_id and f.id = v.source_file_id
                  where v.site_id = $1 and v.source_file_id = $2 and v.preset = $3
                    and f.deleted_at is null",
            )
            .bind(context.site_id.into_uuid())
            .bind(source_file_id.into_uuid())
            .bind(preset.as_str())
            .fetch_optional(tx.conn())
            .await
        }
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: "media_variant_not_found",
        })?;
        let stored = stored_variant_from_row(&row)?;
        let bytes = store.get(context, &stored.storage_key).await?;
        let expected_bytes = usize::try_from(stored.variant.bytes)
            .map_err(|_| MaviError::validation("media_variant_integrity_failed"))?;
        if bytes.len() != expected_bytes
            || hex_digest(&Sha256::digest(&bytes)) != stored.variant.sha256
        {
            return Err(MaviError::validation("media_variant_integrity_failed"));
        }
        Ok((stored.variant, bytes))
    }

    /// Returns the live source receipt without exposing storage metadata to HTTP.
    pub async fn variant_source(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        source_file_id: FileId,
    ) -> Result<Option<MediaVariantSource>> {
        let row = sqlx::query(
            "select storage_key, bytes, sha256 from media_files
              where site_id = $1 and id = $2 and kind = 'image' and deleted_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(source_file_id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let bytes: i64 = row.try_get("bytes").map_err(|_| MaviError::Internal)?;
        Ok(Some(MediaVariantSource {
            storage_key: row
                .try_get("storage_key")
                .map_err(|_| MaviError::Internal)?,
            bytes: u64::try_from(bytes).map_err(|_| MaviError::Internal)?,
            sha256: row.try_get("sha256").map_err(|_| MaviError::Internal)?,
        }))
    }

    /// Finalizes a rendered variant only while its source is still live.
    /// Returning the owned key makes a racing or repeated job safe to clean up
    /// its candidate object without ever deleting an existing variant.
    pub async fn finalize_variant(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        job: &MediaVariantJob,
        storage_key: &str,
        rendered: &RenderedVariant,
    ) -> Result<Option<String>> {
        let source_exists = self
            .variant_source(tx, context, job.source_file_id)
            .await?
            .is_some();
        if !source_exists {
            return Ok(None);
        }
        let bytes = i64::try_from(rendered.content.len())
            .map_err(|_| MaviError::validation("media_variant_too_large"))?;
        let sha256 = hex_digest(&Sha256::digest(&rendered.content));
        let inserted: Option<Uuid> = sqlx::query_scalar(
            "insert into media_variants
                (site_id, id, source_file_id, preset, mime, storage_key, width, height, bytes, sha256)
             values ($1, $2, $3, $4, 'image/jpeg', $5, $6, $7, $8, $9)
             on conflict (site_id, source_file_id, preset) do nothing
             returning id",
        )
        .bind(context.site_id.into_uuid())
        .bind(job.variant_id.into_uuid())
        .bind(job.source_file_id.into_uuid())
        .bind(job.preset.as_str())
        .bind(storage_key)
        .bind(i32::try_from(rendered.width).map_err(|_| MaviError::Internal)?)
        .bind(i32::try_from(rendered.height).map_err(|_| MaviError::Internal)?)
        .bind(bytes)
        .bind(&sha256)
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        if inserted.is_some() {
            AuditService
                .record(
                    tx,
                    context,
                    &AuditEntry {
                        action: "media.variant.generated".to_owned(),
                        resource_type: "MediaVariant".to_owned(),
                        resource_id: Some(job.variant_id.into_uuid()),
                        payload: json!({
                            "source_file_id": job.source_file_id,
                            "preset": job.preset,
                            "width": rendered.width,
                            "height": rendered.height,
                            "bytes": bytes,
                        }),
                    },
                )
                .await?;
            return Ok(Some(storage_key.to_owned()));
        }

        sqlx::query_scalar(
            "select storage_key from media_variants
              where site_id = $1 and source_file_id = $2 and preset = $3",
        )
        .bind(context.site_id.into_uuid())
        .bind(job.source_file_id.into_uuid())
        .bind(job.preset.as_str())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)
    }

    /// Reads a live file's metadata and bytes for an already-authorized caller.
    ///
    /// The storage key never leaves this adapter. Callers must perform their
    /// own domain-level authorization before asking for the binary.
    pub async fn read_bytes(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        store: &dyn FileStore,
        id: FileId,
    ) -> Result<(FileRecord, Vec<u8>)> {
        self.read_bytes_for_visibility(tx, context, store, id, None)
            .await
    }

    /// Reads bytes only when the file was explicitly marked public. Public
    /// delivery never treats the absence of a visibility flag as permission;
    /// the database default is private and this predicate stays at the
    /// repository boundary.
    pub async fn read_public_bytes(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        store: &dyn FileStore,
        id: FileId,
    ) -> Result<(FileRecord, Vec<u8>)> {
        self.read_bytes_for_visibility(tx, context, store, id, Some(FileVisibility::Public))
            .await
    }

    async fn read_bytes_for_visibility(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        store: &dyn FileStore,
        id: FileId,
        visibility: Option<FileVisibility>,
    ) -> Result<(FileRecord, Vec<u8>)> {
        let row = match visibility {
            Some(visibility) => sqlx::query(
                "select id, kind, visibility, mime, name, storage_key, bytes, sha256, created_at
                       from media_files
                      where site_id = $1 and id = $2 and visibility = $3 and deleted_at is null",
            )
            .bind(context.site_id.into_uuid())
            .bind(id.into_uuid())
            .bind(visibility.as_str())
            .fetch_optional(tx.conn())
            .await,
            None => sqlx::query(
                "select id, kind, visibility, mime, name, storage_key, bytes, sha256, created_at
                       from media_files
                      where site_id = $1 and id = $2 and deleted_at is null",
            )
            .bind(context.site_id.into_uuid())
            .bind(id.into_uuid())
            .fetch_optional(tx.conn())
            .await,
        }
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: FILE_NOT_FOUND,
        })?;
        let stored = from_row(&row)?;
        let bytes = store.get(context, &stored.storage_key).await?;
        let expected_bytes = usize::try_from(stored.record.bytes)
            .map_err(|_| MaviError::validation("media_storage_integrity_failed"))?;
        if bytes.len() != expected_bytes
            || hex_digest(&Sha256::digest(&bytes)) != stored.record.sha256
        {
            return Err(MaviError::validation("media_storage_integrity_failed"));
        }
        Ok((stored.record, bytes))
    }

    /// Tombstones metadata while retaining the binary for trash restore.
    pub async fn trash(&self, tx: &mut SiteTx, context: &SiteContext, id: FileId) -> Result<()> {
        let row = sqlx::query(
            "select id, kind, visibility, mime, name, storage_key, bytes, sha256, created_at
               from media_files
              where site_id = $1 and id = $2 and deleted_at is null
              for update",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: FILE_NOT_FOUND,
        })?;
        let stored = from_row(&row)?;
        sqlx::query(
            "update media_files set deleted_at = clock_timestamp()
              where site_id = $1 and id = $2",
        )
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
                    action: "media.file.trashed".to_owned(),
                    resource_type: "File".to_owned(),
                    resource_id: Some(id.into_uuid()),
                    payload: json!({"storage_key": stored.storage_key, "bytes_retained": true}),
                },
            )
            .await?;

        Ok(())
    }

    /// Ensures that one pending cleanup task has a durable, retryable job.
    ///
    /// The idempotency key is the file identity, not a process attempt. A
    /// dead job is explicitly reopened so a transient object-store outage
    /// cannot strand a task forever after the job's bounded retry count.
    pub async fn enqueue_next_cleanup(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        jobs: &JobsService,
    ) -> Result<Option<JobId>> {
        let row = sqlx::query(
            "select file_id, storage_key, storage_keys
               from media_cleanup_tasks
              where site_id = $1 and completed_at is null
              order by created_at asc, file_id asc
              limit 1",
        )
        .bind(context.site_id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let Some(row) = row else {
            return Ok(None);
        };

        let file_id = FileId::from_uuid(row.try_get("file_id").map_err(|_| MaviError::Internal)?);
        let storage_key: String = row
            .try_get("storage_key")
            .map_err(|_| MaviError::Internal)?;
        let additional_storage_keys: Vec<String> = row
            .try_get("storage_keys")
            .map_err(|_| MaviError::Internal)?;
        let job_id = self
            .enqueue_cleanup_job_with_variants(
                tx,
                context,
                jobs,
                file_id,
                &storage_key,
                additional_storage_keys,
            )
            .await?;
        if jobs.get(tx, job_id).await?.state == JobState::Dead {
            jobs.retry_at(tx, context, job_id, Utc::now() + Duration::minutes(1))
                .await?;
        }
        Ok(Some(job_id))
    }

    /// Creates the idempotent cleanup job for a metadata deletion.
    pub async fn enqueue_cleanup_job(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        jobs: &JobsService,
        file_id: FileId,
        storage_key: &str,
    ) -> Result<JobId> {
        let additional_storage_keys: Vec<String> = sqlx::query_scalar(
            "select storage_keys from media_cleanup_tasks
              where site_id = $1 and file_id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(file_id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .unwrap_or_default();
        self.enqueue_cleanup_job_with_variants(
            tx,
            context,
            jobs,
            file_id,
            storage_key,
            additional_storage_keys,
        )
        .await
    }

    async fn enqueue_cleanup_job_with_variants(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        jobs: &JobsService,
        file_id: FileId,
        storage_key: &str,
        additional_storage_keys: Vec<String>,
    ) -> Result<JobId> {
        let payload = serde_json::to_value(MediaCleanupJob {
            file_id,
            storage_key: storage_key.to_owned(),
            additional_storage_keys,
        })
        .map_err(|_| MaviError::Internal)?;
        jobs.enqueue(
            tx,
            context,
            MEDIA_CLEANUP_JOB.name,
            &payload,
            None,
            Some(&format!("media-cleanup:{file_id}")),
        )
        .await
    }

    /// Enqueues one missing preset for the oldest live image. Variants are
    /// derived data: relocation and older uploads do not need to carry them,
    /// because this discovery converges every image to the same preset set.
    pub async fn enqueue_next_variant_job(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        jobs: &JobsService,
    ) -> Result<Option<JobId>> {
        let row = sqlx::query(
            "with presets(preset) as (
                 values ('thumbnail'::text), ('medium'::text), ('large'::text)
             )
             select f.id as source_file_id, p.preset
               from media_files f
               cross join presets p
               left join media_variants v
                 on v.site_id = f.site_id
                and v.source_file_id = f.id
                and v.preset = p.preset
              where f.site_id = $1 and f.kind = 'image' and f.deleted_at is null
                and v.id is null
              order by f.created_at asc, f.id asc, p.preset asc
              limit 1",
        )
        .bind(context.site_id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let source_file_id = FileId::from_uuid(
            row.try_get("source_file_id")
                .map_err(|_| MaviError::Internal)?,
        );
        let preset = VariantPreset::parse(
            row.try_get::<String, _>("preset")
                .map_err(|_| MaviError::Internal)?
                .as_str(),
        )?;
        let job = MediaVariantJob {
            source_file_id,
            variant_id: deterministic_variant_id(source_file_id, preset),
            preset,
        };
        let payload = serde_json::to_value(&job).map_err(|_| MaviError::Internal)?;
        let job_id = jobs
            .enqueue(
                tx,
                context,
                MEDIA_VARIANT_JOB.name,
                &payload,
                None,
                Some(&format!(
                    "media-variant:{source_file_id}:{}",
                    preset.as_str()
                )),
            )
            .await?;
        if jobs.get(tx, job_id).await?.state == JobState::Dead {
            jobs.retry_at(tx, context, job_id, Utc::now() + Duration::minutes(1))
                .await?;
        }
        Ok(Some(job_id))
    }

    /// Enqueues one immediate storage reconciliation job for the current
    /// time bucket. Repeated polls in the same bucket are idempotent.
    pub async fn enqueue_orphan_cleanup_job(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        jobs: &JobsService,
        now: DateTime<Utc>,
    ) -> Result<JobId> {
        let bucket = now.timestamp().div_euclid(MEDIA_ORPHAN_BUCKET_SECONDS);
        let payload = serde_json::to_value(MediaOrphanCleanupJob { bucket })
            .map_err(|_| MaviError::Internal)?;
        jobs.enqueue(
            tx,
            context,
            MEDIA_ORPHAN_CLEANUP_JOB.name,
            &payload,
            None,
            Some(&format!("media-orphan:{bucket}")),
        )
        .await
    }

    /// Returns every metadata-owned key plus pending permanent-deletion keys.
    /// The latter keeps an object from being mistaken for an orphan between
    /// metadata deletion and the external cleanup receipt.
    pub async fn known_storage_keys(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
    ) -> Result<BTreeSet<String>> {
        let rows = sqlx::query(
            "select storage_key from media_files where site_id = $1
             union
             select storage_key from media_variants where site_id = $1
             union
             select storage_key from media_cleanup_tasks
              where site_id = $1 and completed_at is null",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        let mut keys = rows
            .into_iter()
            .map(|row| row.try_get("storage_key").map_err(|_| MaviError::Internal))
            .collect::<Result<BTreeSet<_>>>()?;
        let additional = sqlx::query(
            "select unnest(storage_keys) as storage_key
               from media_cleanup_tasks
              where site_id = $1 and completed_at is null",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        for row in additional {
            keys.insert(
                row.try_get("storage_key")
                    .map_err(|_| MaviError::Internal)?,
            );
        }
        let pending_variants = sqlx::query(
            "select payload->>'variant_id' as variant_id
               from jobs
              where site_id = $1 and kind = $2 and state in ('ready', 'running')",
        )
        .bind(context.site_id.into_uuid())
        .bind(MEDIA_VARIANT_JOB.name)
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        for row in pending_variants {
            let variant_id = row
                .try_get::<Option<String>, _>("variant_id")
                .map_err(|_| MaviError::Internal)?
                .and_then(|value| Uuid::parse_str(&value).ok())
                .map(MediaVariantId::from_uuid);
            if let Some(variant_id) = variant_id {
                keys.insert(variant_storage_key(variant_id));
            }
        }
        Ok(keys)
    }

    /// Records one immutable receipt when orphan bytes were actually removed.
    pub async fn record_orphan_cleanup(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        count: usize,
        bucket: i64,
    ) -> Result<()> {
        if count == 0 {
            return Ok(());
        }
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "media.orphans.cleaned".to_owned(),
                    resource_type: "MediaStorage".to_owned(),
                    resource_id: None,
                    payload: json!({"bucket": bucket, "count": count}),
                },
            )
            .await
    }

    /// Marks a durable cleanup task complete after the binary adapter confirms
    /// removal. The row remains as a receipt that the external deletion was
    /// attempted, which lets a future worker distinguish pending work.
    pub async fn complete_cleanup(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        file_id: FileId,
        storage_key: &str,
    ) -> Result<()> {
        let result = sqlx::query(
            "update media_cleanup_tasks
                set attempts = attempts + 1, completed_at = clock_timestamp()
              where site_id = $1 and file_id = $2 and storage_key = $3
                and completed_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(file_id.into_uuid())
        .bind(storage_key)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if result.rows_affected() == 1 {
            AuditService
                .record(
                    tx,
                    context,
                    &AuditEntry {
                        action: "media.file.cleanup_completed".to_owned(),
                        resource_type: "File".to_owned(),
                        resource_id: Some(file_id.into_uuid()),
                        payload: json!({"storage_key": storage_key}),
                    },
                )
                .await?;
            return Ok(());
        }

        let already_completed = sqlx::query_scalar::<_, bool>(
            "select exists(
                select 1 from media_cleanup_tasks
                 where site_id = $1 and file_id = $2
                   and storage_key = $3 and completed_at is not null
            )",
        )
        .bind(context.site_id.into_uuid())
        .bind(file_id.into_uuid())
        .bind(storage_key)
        .fetch_one(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if already_completed {
            Ok(())
        } else {
            Err(MaviError::NotFound {
                resource: "media_cleanup_task",
            })
        }
    }
}

/// Decodes one uploaded image under bounded resource limits and produces a
/// deterministic JPEG variant. The worker calls this in a blocking task so a
/// malformed or unusually large image cannot block the async runtime.
pub fn render_variant(source: &[u8], preset: VariantPreset) -> Result<RenderedVariant> {
    let mut reader = ImageReader::new(IoCursor::new(source))
        .with_guessed_format()
        .map_err(|_| MaviError::validation("media_variant_image_invalid"))?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_VARIANT_SOURCE_DIMENSION);
    limits.max_image_height = Some(MAX_VARIANT_SOURCE_DIMENSION);
    limits.max_alloc = Some(MAX_VARIANT_SOURCE_ALLOC);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|_| MaviError::validation("media_variant_image_invalid"))?;
    let (source_width, source_height) = (image.width(), image.height());
    let max_dimension = preset.max_dimension();
    let (width, height) = if source_width >= source_height && source_width > max_dimension {
        (
            max_dimension,
            u32::try_from(
                u64::from(source_height) * u64::from(max_dimension) / u64::from(source_width),
            )
            .map_err(|_| MaviError::validation("media_variant_dimensions_invalid"))?
            .max(1),
        )
    } else if source_height > max_dimension {
        (
            u32::try_from(
                u64::from(source_width) * u64::from(max_dimension) / u64::from(source_height),
            )
            .map_err(|_| MaviError::validation("media_variant_dimensions_invalid"))?
            .max(1),
            max_dimension,
        )
    } else {
        (source_width, source_height)
    };
    let resized = image
        .resize_exact(width, height, FilterType::Lanczos3)
        .to_rgb8();
    let mut content = Vec::new();
    JpegEncoder::new_with_quality(&mut content, VARIANT_JPEG_QUALITY)
        .encode_image(&resized)
        .map_err(|_| MaviError::validation("media_variant_encode_failed"))?;
    Ok(RenderedVariant {
        content,
        width,
        height,
    })
}

#[derive(Clone, Debug)]
struct StoredFile {
    record: FileRecord,
    storage_key: String,
}

#[derive(Clone, Debug)]
struct StoredVariant {
    variant: FileVariant,
    storage_key: String,
}

fn from_row(row: &sqlx::postgres::PgRow) -> Result<StoredFile> {
    let kind = FileKind::parse(row.try_get("kind").map_err(|_| MaviError::Internal)?)?;
    let visibility =
        FileVisibility::parse(row.try_get("visibility").map_err(|_| MaviError::Internal)?)?;
    let bytes: i64 = row.try_get("bytes").map_err(|_| MaviError::Internal)?;
    let bytes = u64::try_from(bytes).map_err(|_| MaviError::Internal)?;
    Ok(StoredFile {
        record: FileRecord {
            id: FileId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
            kind,
            visibility,
            mime: row.try_get("mime").map_err(|_| MaviError::Internal)?,
            name: row.try_get("name").map_err(|_| MaviError::Internal)?,
            bytes,
            sha256: row.try_get("sha256").map_err(|_| MaviError::Internal)?,
            created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        },
        storage_key: row
            .try_get("storage_key")
            .map_err(|_| MaviError::Internal)?,
    })
}

fn variant_from_row(row: &sqlx::postgres::PgRow) -> Result<FileVariant> {
    let width: i32 = row.try_get("width").map_err(|_| MaviError::Internal)?;
    let height: i32 = row.try_get("height").map_err(|_| MaviError::Internal)?;
    let bytes: i64 = row.try_get("bytes").map_err(|_| MaviError::Internal)?;
    Ok(FileVariant {
        id: MediaVariantId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        source_file_id: FileId::from_uuid(
            row.try_get("source_file_id")
                .map_err(|_| MaviError::Internal)?,
        ),
        preset: VariantPreset::parse(
            row.try_get::<String, _>("preset")
                .map_err(|_| MaviError::Internal)?
                .as_str(),
        )?,
        mime: row.try_get("mime").map_err(|_| MaviError::Internal)?,
        width: u32::try_from(width).map_err(|_| MaviError::Internal)?,
        height: u32::try_from(height).map_err(|_| MaviError::Internal)?,
        bytes: u64::try_from(bytes).map_err(|_| MaviError::Internal)?,
        sha256: row.try_get("sha256").map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
    })
}

fn stored_variant_from_row(row: &sqlx::postgres::PgRow) -> Result<StoredVariant> {
    Ok(StoredVariant {
        variant: variant_from_row(row)?,
        storage_key: row
            .try_get("storage_key")
            .map_err(|_| MaviError::Internal)?,
    })
}

fn encode_cursor(created_at: DateTime<Utc>, id: Uuid) -> Result<Cursor> {
    let bytes =
        serde_json::to_vec(&FileCursor { created_at, id }).map_err(|_| MaviError::Internal)?;
    Cursor::parse(URL_SAFE_NO_PAD.encode(bytes))
}

fn decode_cursor(cursor: &Cursor) -> Result<FileCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_str())
        .map_err(|_| MaviError::validation("invalid_cursor"))?;
    serde_json::from_slice(&bytes).map_err(|_| MaviError::validation("invalid_cursor"))
}

fn encode_variant_cursor(created_at: DateTime<Utc>, id: Uuid) -> Result<Cursor> {
    let bytes =
        serde_json::to_vec(&VariantCursor { created_at, id }).map_err(|_| MaviError::Internal)?;
    Cursor::parse(URL_SAFE_NO_PAD.encode(bytes))
}

fn decode_variant_cursor(cursor: &Cursor) -> Result<VariantCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_str())
        .map_err(|_| MaviError::validation("invalid_cursor"))?;
    serde_json::from_slice(&bytes).map_err(|_| MaviError::validation("invalid_cursor"))
}

fn storage_key(id: FileId, extension: &str) -> String {
    storage_key_uuid(id.into_uuid(), extension)
}

fn deterministic_variant_id(source_file_id: FileId, preset: VariantPreset) -> MediaVariantId {
    let digest = Sha256::digest(format!(
        "mavi-media-variant:{source_file_id}:{}",
        preset.as_str()
    ));
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    MediaVariantId::from_uuid(Uuid::from_bytes(bytes))
}

fn storage_key_uuid(id: Uuid, extension: &str) -> String {
    let flat = id.to_string().replace('-', "");
    let (front, back) = flat.split_at(2);
    format!("{front}/{back}.{extension}")
}

#[must_use]
pub fn variant_storage_key(id: MediaVariantId) -> String {
    storage_key_uuid(id.into_uuid(), "jpg")
}

/// Returns whether a storage key has Mavi's generated media shape.
///
/// This intentionally does not accept arbitrary site files, design sources,
/// build artifacts or temporary files. The worker must call this allowlist
/// before it considers a listed object for deletion.
#[must_use]
pub fn is_generated_media_storage_key(key: &str) -> bool {
    let mut parts = key.split('/');
    let Some(prefix) = parts.next() else {
        return false;
    };
    let Some(filename) = parts.next() else {
        return false;
    };
    if parts.next().is_some() || prefix.len() != 2 {
        return false;
    }

    let Some((stem, extension)) = filename.rsplit_once('.') else {
        return false;
    };
    if stem.len() != 30
        || !is_lower_hex(prefix)
        || !is_lower_hex(stem)
        || !SIGNATURES
            .iter()
            .any(|(_, _, _, _, known)| *known == extension)
    {
        return false;
    }

    Uuid::parse_str(&format!("{prefix}{stem}")).is_ok()
}

fn is_lower_hex(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn hex_digest(digest: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

#[must_use]
pub fn sha256_digest(bytes: &[u8]) -> String {
    hex_digest(&Sha256::digest(bytes))
}

fn validate_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Ok("A file".to_owned());
    }
    if name.chars().count() > MAX_FILE_NAME_CHARS {
        return Err(MaviError::validation(FILE_NAME_TOO_LONG));
    }
    if name.chars().any(char::is_control) {
        return Err(MaviError::validation(FILE_NAME_INVALID));
    }
    Ok(name.to_owned())
}

fn valid_mime(mime: &str) -> bool {
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

fn validate_storage_key(id: FileId, key: &str) -> Result<()> {
    let Some((_, extension)) = key.rsplit_once('.') else {
        return Err(MaviError::validation(
            "media_relocation_storage_key_invalid",
        ));
    };
    if !(2..=5).contains(&extension.len())
        || !extension
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        || storage_key(id, extension) != key
    {
        return Err(MaviError::validation(
            "media_relocation_storage_key_invalid",
        ));
    }
    Ok(())
}

fn detect(bytes: &[u8]) -> Result<DetectedFile> {
    if bytes.is_empty() {
        return Err(MaviError::validation(FILE_EMPTY));
    }
    if bytes.len() > MAX_FILE_BYTES {
        return Err(MaviError::validation(FILE_TOO_LARGE));
    }

    SIGNATURES
        .iter()
        .find(|(signature, offset, ..)| {
            bytes
                .get(*offset..offset.saturating_add(signature.len()))
                .is_some_and(|value| value == *signature)
        })
        .map(|(_, _, kind, mime, extension)| DetectedFile {
            kind: *kind,
            mime,
            extension,
        })
        .ok_or_else(|| MaviError::validation(FILE_KIND_UNSUPPORTED))
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

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00";

    #[test]
    fn detection_ignores_the_filename_and_uses_an_allowlist() {
        assert_eq!(detect(PNG).expect("png").kind, FileKind::Image);
        assert_eq!(detect(b"%PDF-1.7").expect("pdf").mime, "application/pdf");
        assert!(detect(b"<!doctype html>").is_err());
    }

    #[test]
    fn storage_key_contains_only_the_generated_id_and_earned_extension() {
        let key = storage_key(
            FileId::from_uuid(
                Uuid::parse_str("018f1f27-7f2d-7c2e-8c3d-0123456789ab").expect("uuid"),
            ),
            "png",
        );
        assert_eq!(key, "01/8f1f277f2d7c2e8c3d0123456789ab.png");
        assert!(!key.contains("holiday"));
    }

    #[test]
    fn orphan_allowlist_accepts_only_generated_media_keys() {
        let id = FileId::from_uuid(
            Uuid::parse_str("018f1f27-7f2d-7c2e-8c3d-0123456789ab").expect("uuid"),
        );
        assert!(is_generated_media_storage_key(&storage_key(id, "png")));
        assert!(!is_generated_media_storage_key("src/index.html"));
        assert!(!is_generated_media_storage_key("public/index.html"));
        assert!(!is_generated_media_storage_key(
            "ab/0123456789abcdef0123456789abcd.tmp"
        ));
        assert!(!is_generated_media_storage_key(
            "AB/8f1f277f2d7c2e8c3d0123456789ab.png"
        ));
        assert!(!is_generated_media_storage_key(
            "ab/8f1f277f2d7c2e8c3d0123456789ab.png/extra"
        ));
        assert!(is_generated_media_storage_key(&variant_storage_key(
            MediaVariantId::from_uuid(
                Uuid::parse_str("018f1f27-7f2d-7c2e-8c3d-0123456789ac").expect("uuid"),
            ),
        )));
    }

    #[test]
    fn image_variants_are_bounded_and_keep_aspect_ratio() {
        let source = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            640,
            320,
            image::Rgb([12, 34, 56]),
        ));
        let mut encoded = IoCursor::new(Vec::new());
        source
            .write_to(&mut encoded, image::ImageFormat::Png)
            .expect("encode source");

        let rendered = render_variant(&encoded.into_inner(), VariantPreset::Thumbnail)
            .expect("render variant");
        assert_eq!((rendered.width, rendered.height), (320, 160));
        assert!(rendered.content.starts_with(&[0xff, 0xd8, 0xff]));
        assert!(render_variant(b"not-an-image", VariantPreset::Thumbnail).is_err());
    }

    #[test]
    fn cursors_are_opaque_keysets() {
        let cursor = encode_cursor(Utc::now(), Uuid::now_v7()).expect("cursor");
        assert!(decode_cursor(&cursor).is_ok());
        assert!(decode_cursor(&Cursor::parse("not-a-cursor").expect("cursor")).is_err());
    }

    #[test]
    fn relocation_validates_site_key_metadata_and_content_integrity() {
        let id = FileId::from_uuid(
            Uuid::parse_str("018f1f27-7f2d-7c2e-8c3d-0123456789ab").expect("uuid"),
        );
        let content = b"binary".to_vec();
        let snapshot = MediaRelocation {
            files: vec![MediaRelocationFile {
                id,
                kind: FileKind::Document,
                visibility: FileVisibility::Private,
                mime: "application/pdf".to_owned(),
                name: "document.pdf".to_owned(),
                storage_key: storage_key(id, "pdf"),
                bytes: content.len() as u64,
                sha256: hex_digest(&Sha256::digest(&content)),
                created_at: Utc::now(),
                content,
            }],
        };
        snapshot.validate().expect("valid relocation");

        let mut invalid = snapshot;
        invalid.files[0].content[0] ^= 1;
        let error = invalid.validate().expect_err("digest mismatch");
        assert!(matches!(
            error,
            MaviError::Validation { ref code, .. }
                if code == "media_relocation_digest_mismatch"
        ));
    }

    #[test]
    fn relocation_binary_uses_a_string_wire_shape() {
        let id = FileId::new();
        let content = vec![0, 1, 2, 255];
        let snapshot = MediaRelocation {
            files: vec![MediaRelocationFile {
                id,
                kind: FileKind::Image,
                visibility: FileVisibility::Public,
                mime: "image/png".to_owned(),
                name: "image.png".to_owned(),
                storage_key: storage_key(id, "png"),
                bytes: content.len() as u64,
                sha256: hex_digest(&Sha256::digest(&content)),
                created_at: Utc::now(),
                content,
            }],
        };
        let value = serde_json::to_value(&snapshot).expect("serialize");
        assert!(value["files"][0]["content"].is_string());
        assert_eq!(
            serde_json::from_value::<MediaRelocation>(value).expect("deserialize"),
            snapshot
        );
    }

    #[test]
    fn media_contract_is_permissioned_and_has_no_offset_pagination() {
        let catalog = api();
        assert!(catalog.validate().is_ok());
        let list = catalog
            .endpoints
            .iter()
            .find(|endpoint| endpoint.operation_id == "media.files.list")
            .expect("media list endpoint");
        assert_eq!(
            list.request.as_ref().expect("query input").shape,
            "FileListFilter"
        );
        let filter = catalog
            .shapes
            .iter()
            .find(|shape| shape.name == "FileListFilter")
            .expect("file filter shape");
        let properties = filter.schema["properties"].as_object().expect("properties");
        assert!(properties.contains_key("after"));
        assert!(properties.contains_key("limit"));
        assert!(!properties.contains_key("page"));
        assert!(!properties.contains_key("offset"));
    }
}
