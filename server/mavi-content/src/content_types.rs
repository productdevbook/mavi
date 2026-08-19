//! Site-declared content types and validation for their custom fields.
//!
//! A content kind is intentionally not an enum in Rust: each site may invent
//! its own kind. When a site declares a schema for that kind, content writes
//! are checked against it in the same scoped transaction. Removing a
//! declaration never removes content; undeclared kinds keep their flexible
//! JSON fields.

use std::collections::BTreeSet;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Capability, Cursor, ErrorCode, MaviError, Page, PageRequest, Result, SiteContext,
    SiteId,
};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;

use super::ContentKind;

pub const CONTENT_TYPE_NOT_FOUND: &str = "content_type_not_found";
pub const CONTENT_TYPE_NAME_INVALID: &str = "content_type_name_invalid";
pub const CONTENT_TYPE_FIELDS_INVALID: &str = "content_type_fields_invalid";
pub const CONTENT_FIELD_KEY_INVALID: &str = "content_field_key_invalid";
pub const CONTENT_FIELD_KEY_DUPLICATE: &str = "content_field_key_duplicate";
pub const CONTENT_FIELD_LABEL_INVALID: &str = "content_field_label_invalid";
pub const CONTENT_FIELD_OPTIONS_INVALID: &str = "content_field_options_invalid";
pub const CONTENT_FIELD_REQUIRED: &str = "content_field_required";
pub const CONTENT_FIELD_VALUE_INVALID: &str = "content_field_value_invalid";
pub const CONTENT_FIELD_UNKNOWN: &str = "content_field_unknown";
pub const CONTENT_FIELD_VALUE_TOO_LARGE: &str = "content_field_value_too_large";

const MAX_FIELDS: usize = 50;
const MAX_OPTIONS: usize = 100;
const MAX_FIELD_KEY: usize = 31;
const MAX_FIELD_LABEL: usize = 200;
const MAX_OPTION: usize = 200;
const MAX_CONTENT_FIELDS_BYTES: usize = 64 * 1024;

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::new(
            Method::Get,
            "/api/v1/content-types",
            "content_types.list",
            "List site content types",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Content,
            action: Action::View,
        })
        .takes_query("ContentTypeListFilter")
        .returns(200, "ContentTypePage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Put,
            "/api/v1/content-types/{kind}",
            "content_types.upsert",
            "Create or update a site content type",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Content,
            action: Action::Write,
        })
        .takes("DeclareContentType")
        .returns(200, "ContentType")
        .changes(true)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Delete,
            "/api/v1/content-types/{kind}",
            "content_types.delete",
            "Delete a site content type declaration",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Content,
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

#[allow(clippy::too_many_lines)]
#[must_use]
pub fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "ContentFieldKind",
            json!({
                "type": "string",
                "enum": ["text", "long", "email", "number", "choice", "boolean"]
            }),
        ),
        Shape::new(
            "ContentTypeField",
            json!({
                "type": "object",
                "required": ["key", "label", "required", "kind", "options"],
                "properties": {
                    "key": {"type": "string", "maxLength": 31},
                    "label": {"type": "string", "maxLength": 200},
                    "required": {"type": "boolean"},
                    "kind": {"$ref": "#/components/schemas/ContentFieldKind"},
                    "options": {"type": "array", "items": {"type": "string", "maxLength": 200}},
                },
            }),
        ),
        Shape::new(
            "ContentTypeListFilter",
            json!({
                "type": "object",
                "properties": {
                    "after": {"type": ["string", "null"], "maxLength": 512},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                },
            }),
        ),
        Shape::new(
            "DeclareContentType",
            json!({
                "type": "object",
                "required": ["name"],
                "properties": {
                    "name": {"type": "string", "maxLength": 100},
                    "fields": {"type": "array", "maxItems": 50, "items": {"$ref": "#/components/schemas/ContentTypeField"}},
                },
            }),
        ),
        Shape::new(
            "ContentType",
            json!({
                "type": "object",
                "required": ["site_id", "kind", "name", "fields", "created_at", "updated_at"],
                "properties": {
                    "site_id": {"type": "string", "format": "uuid"},
                    "kind": {"type": "string", "maxLength": 31},
                    "name": {"type": "string", "maxLength": 100},
                    "fields": {"type": "array", "items": {"$ref": "#/components/schemas/ContentTypeField"}},
                    "created_at": {"type": "string", "format": "date-time"},
                    "updated_at": {"type": "string", "format": "date-time"},
                },
            }),
        ),
        Shape::new(
            "ContentTypePage",
            json!({
                "type": "object",
                "required": ["items", "next_cursor"],
                "properties": {
                    "items": {"type": "array", "items": {"$ref": "#/components/schemas/ContentType"}},
                    "next_cursor": {"type": ["string", "null"], "maxLength": 512},
                },
            }),
        ),
    ]
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    #[default]
    Text,
    Long,
    Email,
    Number,
    Choice,
    Boolean,
}

impl FieldKind {
    fn accepts(self, value: &Value, options: &[String]) -> bool {
        match self {
            Self::Text | Self::Long => value.is_string(),
            Self::Email => value.as_str().is_some_and(looks_like_email),
            Self::Number => value.is_number(),
            Self::Choice => value
                .as_str()
                .is_some_and(|value| options.iter().any(|option| option == value)),
            Self::Boolean => value.is_boolean(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContentTypeField {
    pub key: String,
    pub label: String,
    pub required: bool,
    #[serde(default)]
    pub kind: FieldKind,
    #[serde(default)]
    pub options: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DeclareContentType {
    pub name: String,
    #[serde(default)]
    pub fields: Vec<ContentTypeField>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ContentTypeListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
}

#[derive(Clone, Debug, Serialize)]
pub struct ContentType {
    pub site_id: SiteId,
    pub kind: ContentKind,
    pub name: String,
    pub fields: Vec<ContentTypeField>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ContentTypeCursor {
    created_at: DateTime<Utc>,
    kind: String,
}

fn encode_cursor(created_at: DateTime<Utc>, kind: &str) -> Result<Cursor> {
    let bytes = serde_json::to_vec(&ContentTypeCursor {
        created_at,
        kind: kind.to_owned(),
    })
    .map_err(|_| MaviError::Internal)?;
    Cursor::parse(URL_SAFE_NO_PAD.encode(bytes))
}

fn decode_cursor(cursor: &Cursor) -> Result<ContentTypeCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_str())
        .map_err(|_| MaviError::validation("invalid_cursor"))?;
    serde_json::from_slice(&bytes).map_err(|_| MaviError::validation("invalid_cursor"))
}

fn content_type_name(value: &str) -> Result<String> {
    let value = value.trim();
    if !(1..=100).contains(&value.chars().count()) {
        return Err(MaviError::validation(CONTENT_TYPE_NAME_INVALID));
    }
    Ok(value.to_owned())
}

fn normalize_fields(fields: &[ContentTypeField]) -> Result<Vec<ContentTypeField>> {
    if fields.len() > MAX_FIELDS {
        return Err(MaviError::validation(CONTENT_TYPE_FIELDS_INVALID));
    }

    let mut keys = BTreeSet::new();
    let mut normalized = Vec::with_capacity(fields.len());
    for field in fields {
        let key = field.key.trim();
        if key.is_empty()
            || key.len() > MAX_FIELD_KEY
            || !key.starts_with(|character: char| character.is_ascii_lowercase())
            || !key.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
            })
        {
            return Err(MaviError::validation_field(CONTENT_FIELD_KEY_INVALID, key));
        }
        if !keys.insert(key.to_owned()) {
            return Err(MaviError::validation_field(
                CONTENT_FIELD_KEY_DUPLICATE,
                key,
            ));
        }

        let label = field.label.trim();
        if !(1..=MAX_FIELD_LABEL).contains(&label.chars().count()) {
            return Err(MaviError::validation_field(
                CONTENT_FIELD_LABEL_INVALID,
                key,
            ));
        }

        let mut options = Vec::with_capacity(field.options.len());
        let mut option_set = BTreeSet::new();
        for option in &field.options {
            let option = option.trim();
            if option.is_empty()
                || option.chars().count() > MAX_OPTION
                || !option_set.insert(option.to_owned())
            {
                return Err(MaviError::validation_field(
                    CONTENT_FIELD_OPTIONS_INVALID,
                    key,
                ));
            }
            options.push(option.to_owned());
        }

        if field.kind == FieldKind::Choice {
            if options.is_empty() || options.len() > MAX_OPTIONS {
                return Err(MaviError::validation_field(
                    CONTENT_FIELD_OPTIONS_INVALID,
                    key,
                ));
            }
        } else if !options.is_empty() {
            return Err(MaviError::validation_field(
                CONTENT_FIELD_OPTIONS_INVALID,
                key,
            ));
        }

        normalized.push(ContentTypeField {
            key: key.to_owned(),
            label: label.to_owned(),
            required: field.required,
            kind: field.kind,
            options,
        });
    }

    Ok(normalized)
}

fn decode_fields(value: Value) -> Result<Vec<ContentTypeField>> {
    let fields: Vec<ContentTypeField> =
        serde_json::from_value(value).map_err(|_| MaviError::Internal)?;
    normalize_fields(&fields).map_err(|_| MaviError::Internal)
}

fn content_type_from_row(row: &sqlx::postgres::PgRow) -> Result<ContentType> {
    let fields: Value = row.try_get("fields").map_err(|_| MaviError::Internal)?;
    Ok(ContentType {
        site_id: SiteId::from_uuid(row.try_get("site_id").map_err(|_| MaviError::Internal)?),
        kind: ContentKind::parse(
            &row.try_get::<String, _>("kind")
                .map_err(|_| MaviError::Internal)?,
        )?,
        name: row.try_get("name").map_err(|_| MaviError::Internal)?,
        fields: decode_fields(fields)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

pub(super) async fn initialize(tx: &mut SiteTx, context: &SiteContext) -> Result<()> {
    if !context.caller.is_public() {
        return Err(MaviError::Forbidden);
    }

    for (kind, name) in [("post", "Post"), ("page", "Page")] {
        sqlx::query(
            "insert into content_types (site_id, kind, name, fields)
             values ($1, $2, $3, '[]'::jsonb)
             on conflict (site_id, kind) do nothing",
        )
        .bind(context.site_id.into_uuid())
        .bind(kind)
        .bind(name)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
    }
    Ok(())
}

pub(super) async fn list(
    tx: &mut SiteTx,
    context: &SiteContext,
    filter: &ContentTypeListFilter,
) -> Result<Page<ContentType>> {
    let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
    let limit = i64::from(filter.page.effective_limit());
    let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "select site_id, kind, name, fields, created_at, updated_at
           from content_types where site_id = ",
    );
    query.push_bind(context.site_id.into_uuid());
    if let Some(after) = after {
        query
            .push(" and (created_at, kind) > (")
            .push_bind(after.created_at)
            .push(", ")
            .push_bind(after.kind)
            .push(")");
    }
    let rows = query
        .push(" order by created_at asc, kind asc limit ")
        .push_bind(limit + 1)
        .build()
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
    let mut items = rows
        .iter()
        .map(content_type_from_row)
        .collect::<Result<Vec<_>>>()?;
    let limit_usize = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
    let next_cursor = if items.len() > limit_usize {
        let last = items
            .get(limit_usize.saturating_sub(1))
            .ok_or(MaviError::Internal)?;
        Some(encode_cursor(last.created_at, last.kind.as_str())?)
    } else {
        None
    };
    items.truncate(limit_usize);
    Ok(Page::new(items, next_cursor))
}

pub(super) async fn upsert(
    tx: &mut SiteTx,
    context: &SiteContext,
    kind: &str,
    input: &DeclareContentType,
) -> Result<ContentType> {
    let kind = ContentKind::parse(kind)?;
    let name = content_type_name(&input.name)?;
    let fields = normalize_fields(&input.fields)?;
    let fields_value = serde_json::to_value(&fields).map_err(|_| MaviError::Internal)?;
    let row = sqlx::query(
        "insert into content_types (site_id, kind, name, fields)
         values ($1, $2, $3, $4)
         on conflict (site_id, kind) do update
             set name = excluded.name, fields = excluded.fields, updated_at = now()
         returning site_id, kind, name, fields, created_at, updated_at",
    )
    .bind(context.site_id.into_uuid())
    .bind(kind.as_str())
    .bind(name)
    .bind(fields_value)
    .fetch_one(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    let content_type = content_type_from_row(&row)?;
    AuditService
        .record(
            tx,
            context,
            &AuditEntry {
                action: "content.type.upserted".to_owned(),
                resource_type: "ContentType".to_owned(),
                resource_id: None,
                payload: json!({"kind": content_type.kind, "field_count": content_type.fields.len()}),
            },
        )
        .await?;
    Ok(content_type)
}

pub(super) async fn delete(tx: &mut SiteTx, context: &SiteContext, kind: &str) -> Result<()> {
    let kind = ContentKind::parse(kind)?;
    let result = sqlx::query("delete from content_types where site_id = $1 and kind = $2")
        .bind(context.site_id.into_uuid())
        .bind(kind.as_str())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
    if result.rows_affected() == 0 {
        return Err(MaviError::NotFound {
            resource: CONTENT_TYPE_NOT_FOUND,
        });
    }
    AuditService
        .record(
            tx,
            context,
            &AuditEntry {
                action: "content.type.deleted".to_owned(),
                resource_type: "ContentType".to_owned(),
                resource_id: None,
                payload: json!({"kind": kind}),
            },
        )
        .await
}

pub(super) async fn validate_content_fields(
    tx: &mut SiteTx,
    context: &SiteContext,
    kind: &ContentKind,
    fields: &Value,
) -> Result<()> {
    let Some(object) = fields.as_object() else {
        return Err(MaviError::validation(super::CONTENT_FIELDS_INVALID));
    };
    if serde_json::to_vec(fields)
        .map_err(|_| MaviError::Internal)?
        .len()
        > MAX_CONTENT_FIELDS_BYTES
    {
        return Err(MaviError::validation(CONTENT_FIELD_VALUE_TOO_LARGE));
    }

    let row = sqlx::query("select fields from content_types where site_id = $1 and kind = $2")
        .bind(context.site_id.into_uuid())
        .bind(kind.as_str())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
    let Some(row) = row else {
        return Ok(());
    };
    let declared_value: Value = row.try_get("fields").map_err(|_| MaviError::Internal)?;
    let declared = decode_fields(declared_value)?;

    for field in &declared {
        let value = object.get(&field.key);
        let empty = match value {
            None | Some(Value::Null) => true,
            Some(Value::String(value)) => value.trim().is_empty(),
            Some(_) => false,
        };
        if empty {
            if field.required {
                return Err(MaviError::validation_field(
                    CONTENT_FIELD_REQUIRED,
                    &field.key,
                ));
            }
            continue;
        }

        let Some(value) = value else { continue };
        if !field.kind.accepts(value, &field.options) {
            return Err(MaviError::validation_field(
                CONTENT_FIELD_VALUE_INVALID,
                &field.key,
            ));
        }
    }

    if let Some(unknown) = object
        .keys()
        .find(|key| !declared.iter().any(|field| field.key == **key))
    {
        return Err(MaviError::validation_field(CONTENT_FIELD_UNKNOWN, unknown));
    }
    Ok(())
}

fn looks_like_email(value: &str) -> bool {
    let value = value.trim();
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
}

#[cfg(test)]
mod tests {
    use super::*;
    use mavi_contract::Api;

    fn field(key: &str, kind: FieldKind, required: bool, options: &[&str]) -> ContentTypeField {
        ContentTypeField {
            key: key.to_owned(),
            label: key.to_owned(),
            required,
            kind,
            options: options.iter().map(|option| (*option).to_owned()).collect(),
        }
    }

    #[test]
    fn content_type_fields_are_normalized_and_checked() {
        let fields =
            normalize_fields(&[field("  title ", FieldKind::Text, true, &[])]).expect("field");
        assert_eq!(fields[0].key, "title");
        assert!(normalize_fields(&[field("Title", FieldKind::Text, false, &[])]).is_err());
        assert!(normalize_fields(&[field("status", FieldKind::Choice, false, &[])]).is_err());
        assert!(
            normalize_fields(&[
                field("title", FieldKind::Text, false, &[]),
                field("title", FieldKind::Text, false, &[]),
            ])
            .is_err()
        );
    }

    #[test]
    fn content_field_values_follow_the_declared_kind() {
        let declared = [
            field("title", FieldKind::Text, true, &[]),
            field("status", FieldKind::Choice, false, &["draft", "ready"]),
            field("published", FieldKind::Boolean, false, &[]),
        ];
        let object = json!({"title": "Hello", "status": "ready", "published": true});
        assert!(object.as_object().is_some());
        assert!(
            declared[1]
                .kind
                .accepts(&json!("ready"), &declared[1].options)
        );
        assert!(
            !declared[1]
                .kind
                .accepts(&json!("other"), &declared[1].options)
        );
        assert!(!declared[2].kind.accepts(&json!("true"), &[]));
    }

    #[test]
    fn content_type_contract_is_self_consistent() {
        let catalog = Api::new(endpoints()).with_shapes(shapes());
        catalog.validate().expect("content type API");
        assert_eq!(
            catalog.endpoints[0]
                .request
                .as_ref()
                .map(|request| request.shape.as_str()),
            Some("ContentTypeListFilter")
        );
    }
}
