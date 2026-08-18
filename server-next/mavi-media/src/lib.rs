//! Site-scoped media metadata and binary storage orchestration.
//!
//! `PostgreSQL` owns the tenant-scoped metadata, while a [`FileStore`] owns the
//! bytes. Uploads write bytes before metadata; trashing tombstones metadata
//! while retaining bytes for restore, and permanent trash deletion removes
//! both. The two systems cannot share one transaction, so cleanup remains an
//! explicit retryable adapter operation.

use std::collections::BTreeSet;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Capability, Cursor, ErrorCode, FileId, MaviError, Page, PageRequest, Result,
    SiteContext, ports::FileStore,
};
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

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct FileListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
    pub kind: Option<FileKind>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UploadFileQuery {
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FileRecord {
    pub id: FileId,
    pub kind: FileKind,
    pub mime: String,
    pub name: String,
    pub bytes: u64,
    pub sha256: String,
    pub created_at: DateTime<Utc>,
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
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "FileKind",
            json!({"type": "string", "enum": ["image", "video", "audio", "document"]}),
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
            "UploadFileQuery",
            json!({
                "type": "object",
                "required": ["name"],
                "properties": {"name": {"type": "string", "minLength": 1, "maxLength": 255}},
            }),
        ),
        Shape::new("FileBytes", json!({"type": "string", "format": "binary"})),
        Shape::new(
            "File",
            json!({
                "type": "object",
                "required": ["id", "kind", "mime", "name", "bytes", "sha256", "created_at"],
                "properties": {
                    "id": {"type": "string", "format": "uuid"},
                    "kind": {"$ref": "#/components/schemas/FileKind"},
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
    ]
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct FileCursor {
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
                (site_id, id, kind, mime, name, storage_key, bytes, sha256)
             values ($1, $2, $3, $4, $5, $6, $7, $8)
             returning id, kind, mime, name, storage_key, bytes, sha256, created_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(detected.kind.as_str())
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
                    payload: json!({"kind": file.record.kind, "bytes": file.record.bytes, "mime": file.record.mime}),
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
            "select id, kind, mime, name, storage_key, bytes, sha256, created_at
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
                    (site_id, id, kind, mime, name, storage_key, bytes, sha256, created_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 on conflict (site_id, id) do update set
                    kind = excluded.kind, mime = excluded.mime, name = excluded.name,
                    storage_key = excluded.storage_key, bytes = excluded.bytes,
                    sha256 = excluded.sha256, created_at = excluded.created_at,
                    deleted_at = null",
            )
            .bind(context.site_id.into_uuid())
            .bind(file.id.into_uuid())
            .bind(file.kind.as_str())
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
            "select id, kind, mime, name, storage_key, bytes, sha256, created_at
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
            "select id, kind, mime, name, storage_key, bytes, sha256, created_at
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
        let row = sqlx::query(
            "select id, kind, mime, name, storage_key, bytes, sha256, created_at
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
        let stored = from_row(&row)?;
        let bytes = store.get(context, &stored.storage_key).await?;
        Ok((stored.record, bytes))
    }

    /// Tombstones metadata while retaining the binary for trash restore.
    pub async fn trash(&self, tx: &mut SiteTx, context: &SiteContext, id: FileId) -> Result<()> {
        let row = sqlx::query(
            "select id, kind, mime, name, storage_key, bytes, sha256, created_at
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

    /// Marks a durable cleanup task complete after the binary adapter confirms
    /// removal. The row remains as a receipt that the external deletion was
    /// attempted, which lets a future worker distinguish pending work.
    pub async fn complete_cleanup(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        file_id: FileId,
    ) -> Result<()> {
        let result = sqlx::query(
            "update media_cleanup_tasks
                set attempts = attempts + 1, completed_at = clock_timestamp()
              where site_id = $1 and file_id = $2 and completed_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(file_id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if result.rows_affected() == 0 {
            return Err(MaviError::NotFound {
                resource: "media_cleanup_task",
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct StoredFile {
    record: FileRecord,
    storage_key: String,
}

fn from_row(row: &sqlx::postgres::PgRow) -> Result<StoredFile> {
    let kind = FileKind::parse(row.try_get("kind").map_err(|_| MaviError::Internal)?)?;
    let bytes: i64 = row.try_get("bytes").map_err(|_| MaviError::Internal)?;
    let bytes = u64::try_from(bytes).map_err(|_| MaviError::Internal)?;
    Ok(StoredFile {
        record: FileRecord {
            id: FileId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
            kind,
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

fn storage_key(id: FileId, extension: &str) -> String {
    let flat = id.to_string().replace('-', "");
    let (front, back) = flat.split_at(2);
    format!("{front}/{back}.{extension}")
}

fn hex_digest(digest: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
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
