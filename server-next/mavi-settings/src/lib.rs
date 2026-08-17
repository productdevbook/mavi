//! Site settings and language configuration.
//!
//! Settings belong to one Mavi site. The service accepts a scoped transaction
//! and keeps language-default transitions serialized by the site settings row;
//! the HTTP layer only maps these commands to routes.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_contract::{Api, Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Capability, Cursor, ErrorCode, MaviError, Page, PageRequest, Result, SiteContext,
    SiteId,
};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;

pub const SETTINGS_NOT_FOUND: &str = "settings_not_found";
pub const SETTINGS_PATCH_EMPTY: &str = "settings_patch_empty";
pub const SETTINGS_NAME_INVALID: &str = "settings_name_invalid";
pub const SETTINGS_TIMEZONE_INVALID: &str = "settings_timezone_invalid";
pub const LANGUAGE_TAG_INVALID: &str = "language_tag_invalid";
pub const LANGUAGE_NAME_INVALID: &str = "language_name_invalid";
pub const LANGUAGE_NOT_FOUND: &str = "language_not_found";
pub const LANGUAGE_ALREADY_EXISTS: &str = "language_already_exists";
pub const DEFAULT_LANGUAGE_REQUIRED: &str = "default_language_required";

#[allow(clippy::too_many_lines)]
#[must_use]
pub fn api() -> Api {
    Api::new([
        Endpoint::new(
            Method::Get,
            "/api/v1/settings",
            "settings.read",
            "Read site settings",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Settings,
            action: Action::View,
        })
        .returns(200, "SiteSettings")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Patch,
            "/api/v1/settings",
            "settings.update",
            "Update site settings",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Settings,
            action: Action::Write,
        })
        .takes("UpdateSiteSettings")
        .returns(200, "SiteSettings")
        .changes(true)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/languages",
            "languages.list",
            "List site languages",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Settings,
            action: Action::View,
        })
        .takes_query("LanguageListFilter")
        .returns(200, "LanguagePage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/languages",
            "languages.create",
            "Create a site language",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Settings,
            action: Action::Write,
        })
        .takes("CreateLanguage")
        .returns(201, "Language")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Patch,
            "/api/v1/languages/{tag}",
            "languages.update",
            "Update a site language",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Settings,
            action: Action::Write,
        })
        .takes("UpdateLanguage")
        .returns(200, "Language")
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
            "/api/v1/languages/{tag}",
            "languages.delete",
            "Delete a site language",
        )
        .account_or_assistant()
        .requires(Permission {
            capability: Capability::Settings,
            action: Action::Delete,
        })
        .returns(204, "Empty")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Conflict,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
    ])
    .with_shapes(settings_shapes())
}

#[allow(clippy::too_many_lines)]
fn settings_shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "SiteSettings",
            json!({
                "type": "object",
                "required": ["site_id", "name", "timezone", "updated_at"],
                "properties": {
                    "site_id": {"type": "string", "format": "uuid"},
                    "name": {"type": "string", "maxLength": 200},
                    "timezone": {"type": "string", "maxLength": 64},
                    "updated_at": {"type": "string", "format": "date-time"},
                },
            }),
        ),
        Shape::new(
            "UpdateSiteSettings",
            json!({
                "type": "object",
                "properties": {
                    "name": {"type": ["string", "null"], "maxLength": 200},
                    "timezone": {"type": ["string", "null"], "maxLength": 64},
                },
            }),
        ),
        Shape::new(
            "LanguageListFilter",
            json!({
                "type": "object",
                "properties": {
                    "after": {"type": ["string", "null"], "maxLength": 512},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                },
            }),
        ),
        Shape::new(
            "CreateLanguage",
            json!({
                "type": "object",
                "required": ["tag", "name"],
                "properties": {
                    "tag": {"type": "string", "pattern": "^[A-Za-z]{2,8}(-[A-Za-z0-9]{1,8})*$"},
                    "name": {"type": "string", "maxLength": 120},
                    "is_default": {"type": "boolean"},
                },
            }),
        ),
        Shape::new(
            "UpdateLanguage",
            json!({
                "type": "object",
                "properties": {
                    "name": {"type": ["string", "null"], "maxLength": 120},
                    "is_default": {"type": ["boolean", "null"]},
                },
            }),
        ),
        Shape::new(
            "Language",
            json!({
                "type": "object",
                "required": ["site_id", "tag", "name", "is_default", "created_at", "updated_at"],
                "properties": {
                    "site_id": {"type": "string", "format": "uuid"},
                    "tag": {"type": "string"},
                    "name": {"type": "string"},
                    "is_default": {"type": "boolean"},
                    "created_at": {"type": "string", "format": "date-time"},
                    "updated_at": {"type": "string", "format": "date-time"},
                },
            }),
        ),
        Shape::new(
            "LanguagePage",
            json!({
                "type": "object",
                "required": ["items", "next_cursor"],
                "properties": {
                    "items": {"type": "array", "items": {"$ref": "#/components/schemas/Language"}},
                    "next_cursor": {"type": ["string", "null"], "maxLength": 512},
                },
            }),
        ),
    ]
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct LanguageTag(String);

impl LanguageTag {
    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim();
        let valid = !value.is_empty()
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
            });
        if valid {
            Ok(Self(value.to_owned()))
        } else {
            Err(MaviError::validation(LANGUAGE_TAG_INVALID))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct LanguageName(String);

impl LanguageName {
    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim();
        if value.is_empty() || value.chars().count() > 120 {
            return Err(MaviError::validation(LANGUAGE_NAME_INVALID));
        }
        Ok(Self(value.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn timezone(value: &str) -> Result<String> {
    let value = value.trim();
    let valid = !value.is_empty()
        && value.len() <= 64
        && !value.contains(char::is_whitespace)
        && !value.contains("..")
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "/_+-".contains(character));
    if valid {
        Ok(value.to_owned())
    } else {
        Err(MaviError::validation(SETTINGS_TIMEZONE_INVALID))
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SiteSettings {
    pub site_id: SiteId,
    pub name: String,
    pub timezone: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UpdateSiteSettings {
    pub name: Option<String>,
    pub timezone: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct LanguageListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateLanguage {
    pub tag: String,
    pub name: String,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct UpdateLanguage {
    pub name: Option<String>,
    pub is_default: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Language {
    pub site_id: SiteId,
    pub tag: LanguageTag,
    pub name: LanguageName,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct LanguageCursor {
    created_at: DateTime<Utc>,
    tag: String,
}

fn encode_cursor(created_at: DateTime<Utc>, tag: &str) -> Result<Cursor> {
    let bytes = serde_json::to_vec(&LanguageCursor {
        created_at,
        tag: tag.to_owned(),
    })
    .map_err(|_| MaviError::Internal)?;
    Cursor::parse(URL_SAFE_NO_PAD.encode(bytes))
}

fn decode_cursor(cursor: &Cursor) -> Result<LanguageCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_str())
        .map_err(|_| MaviError::validation("invalid_cursor"))?;
    serde_json::from_slice(&bytes).map_err(|_| MaviError::validation("invalid_cursor"))
}

fn settings_from_row(row: &sqlx::postgres::PgRow) -> Result<SiteSettings> {
    Ok(SiteSettings {
        site_id: SiteId::from_uuid(row.try_get("site_id").map_err(|_| MaviError::Internal)?),
        name: row.try_get("name").map_err(|_| MaviError::Internal)?,
        timezone: row.try_get("timezone").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

fn language_from_row(row: &sqlx::postgres::PgRow) -> Result<Language> {
    Ok(Language {
        site_id: SiteId::from_uuid(row.try_get("site_id").map_err(|_| MaviError::Internal)?),
        tag: LanguageTag::parse(
            &row.try_get::<String, _>("tag")
                .map_err(|_| MaviError::Internal)?,
        )?,
        name: LanguageName::parse(
            &row.try_get::<String, _>("name")
                .map_err(|_| MaviError::Internal)?,
        )?,
        is_default: row.try_get("is_default").map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SettingsService;

impl SettingsService {
    /// Creates the settings rows required by a newly initialized site.
    ///
    /// Setup is orchestrated at the application boundary so identity does not
    /// write another domain's tables. The operation remains in the same
    /// transaction as owner creation and is therefore all-or-nothing.
    pub async fn initialize(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        site_name: &str,
    ) -> Result<()> {
        if !context.caller.is_public() {
            return Err(MaviError::Forbidden);
        }
        let name = setting_name(site_name)?;
        sqlx::query(
            "insert into site_settings (site_id, name) values ($1, $2)
             on conflict (site_id) do update set name = excluded.name, updated_at = now()",
        )
        .bind(context.site_id.into_uuid())
        .bind(name)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        sqlx::query(
            "insert into site_languages (site_id, tag, name, is_default)
             values ($1, 'en', 'English', true)
             on conflict (site_id, tag) do nothing",
        )
        .bind(context.site_id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        Ok(())
    }

    pub async fn get_settings(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
    ) -> Result<SiteSettings> {
        let row = sqlx::query(
            "select site_id, name, timezone, updated_at
               from site_settings where site_id = $1",
        )
        .bind(context.site_id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: SETTINGS_NOT_FOUND,
        })?;
        settings_from_row(&row)
    }

    pub async fn update_settings(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &UpdateSiteSettings,
    ) -> Result<SiteSettings> {
        if input.name.is_none() && input.timezone.is_none() {
            return Err(MaviError::validation(SETTINGS_PATCH_EMPTY));
        }
        let name = input.name.as_deref().map(setting_name).transpose()?;
        let timezone = input.timezone.as_deref().map(timezone).transpose()?;

        let row = sqlx::query(
            "update site_settings
                set name = coalesce($2, name), timezone = coalesce($3, timezone), updated_at = now()
              where site_id = $1
             returning site_id, name, timezone, updated_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(name)
        .bind(timezone)
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: SETTINGS_NOT_FOUND,
        })?;
        let settings = settings_from_row(&row)?;
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "settings.updated".to_owned(),
                    resource_type: "SiteSettings".to_owned(),
                    resource_id: Some(context.site_id.into_uuid()),
                    payload: json!({"name_changed": input.name.is_some(), "timezone_changed": input.timezone.is_some()}),
                },
            )
            .await?;
        Ok(settings)
    }

    pub async fn list_languages(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        filter: &LanguageListFilter,
    ) -> Result<Page<Language>> {
        let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select site_id, tag, name, is_default, created_at, updated_at
               from site_languages where site_id = ",
        );
        query.push_bind(context.site_id.into_uuid());
        if let Some(after) = after {
            query
                .push(" and (created_at, tag) > (")
                .push_bind(after.created_at)
                .push(", ")
                .push_bind(after.tag)
                .push(")");
        }
        query
            .push(" order by created_at asc, tag asc limit ")
            .push_bind(limit + 1);
        let rows = query
            .build()
            .fetch_all(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let mut items = rows
            .iter()
            .map(language_from_row)
            .collect::<Result<Vec<_>>>()?;
        let limit_usize = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if items.len() > limit_usize {
            let last = items
                .get(limit_usize.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_cursor(last.created_at, last.tag.as_str())?)
        } else {
            None
        };
        items.truncate(limit_usize);
        Ok(Page::new(items, next_cursor))
    }

    pub async fn create_language(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &CreateLanguage,
    ) -> Result<Language> {
        let tag = LanguageTag::parse(&input.tag)?;
        let name = LanguageName::parse(&input.name)?;
        lock_settings(tx, context.site_id).await?;
        let count: i64 =
            sqlx::query_scalar("select count(*) from site_languages where site_id = $1")
                .bind(context.site_id.into_uuid())
                .fetch_one(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?;
        let is_default = input.is_default || count == 0;
        if is_default {
            clear_default(tx, context.site_id).await?;
        }
        let row = sqlx::query(
            "insert into site_languages (site_id, tag, name, is_default)
             values ($1, $2, $3, $4)
             returning site_id, tag, name, is_default, created_at, updated_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(tag.as_str())
        .bind(name.as_str())
        .bind(is_default)
        .fetch_one(tx.conn())
        .await
        .map_err(map_language_write_error)?;
        let language = language_from_row(&row)?;
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "settings.language.created".to_owned(),
                    resource_type: "Language".to_owned(),
                    resource_id: None,
                    payload: json!({"tag": language.tag, "is_default": language.is_default}),
                },
            )
            .await?;
        Ok(language)
    }

    pub async fn update_language(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        tag: &str,
        input: &UpdateLanguage,
    ) -> Result<Language> {
        let tag = LanguageTag::parse(tag)?;
        if input.name.is_none() && input.is_default.is_none() {
            return Err(MaviError::validation(SETTINGS_PATCH_EMPTY));
        }
        let name = input.name.as_deref().map(LanguageName::parse).transpose()?;
        lock_settings(tx, context.site_id).await?;
        let current = language_by_tag(tx, context.site_id, tag.as_str()).await?;
        if current.is_default && input.is_default == Some(false) {
            return Err(MaviError::conflict(DEFAULT_LANGUAGE_REQUIRED));
        }
        let is_default = input.is_default.unwrap_or(current.is_default);
        if is_default {
            clear_default(tx, context.site_id).await?;
        }
        let row = sqlx::query(
            "update site_languages
                set name = coalesce($3, name), is_default = $4, updated_at = now()
              where site_id = $1 and tag = $2
             returning site_id, tag, name, is_default, created_at, updated_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(tag.as_str())
        .bind(name.map(|value| value.0))
        .bind(is_default)
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: LANGUAGE_NOT_FOUND,
        })?;
        let language = language_from_row(&row)?;
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "settings.language.updated".to_owned(),
                    resource_type: "Language".to_owned(),
                    resource_id: None,
                    payload: json!({"tag": language.tag, "is_default": language.is_default}),
                },
            )
            .await?;
        Ok(language)
    }

    pub async fn delete_language(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        tag: &str,
    ) -> Result<()> {
        let tag = LanguageTag::parse(tag)?;
        lock_settings(tx, context.site_id).await?;
        let language = language_by_tag(tx, context.site_id, tag.as_str()).await?;
        if language.is_default {
            return Err(MaviError::conflict(DEFAULT_LANGUAGE_REQUIRED));
        }
        sqlx::query("delete from site_languages where site_id = $1 and tag = $2")
            .bind(context.site_id.into_uuid())
            .bind(tag.as_str())
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "settings.language.deleted".to_owned(),
                    resource_type: "Language".to_owned(),
                    resource_id: None,
                    payload: json!({"tag": tag}),
                },
            )
            .await
    }
}

fn setting_name(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 200 {
        return Err(MaviError::validation(SETTINGS_NAME_INVALID));
    }
    Ok(value.to_owned())
}

async fn lock_settings(tx: &mut SiteTx, site_id: SiteId) -> Result<()> {
    sqlx::query("select site_id from site_settings where site_id = $1 for update")
        .bind(site_id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: SETTINGS_NOT_FOUND,
        })?;
    Ok(())
}

async fn clear_default(tx: &mut SiteTx, site_id: SiteId) -> Result<()> {
    sqlx::query("update site_languages set is_default = false, updated_at = now() where site_id = $1 and is_default")
        .bind(site_id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
    Ok(())
}

async fn language_by_tag(tx: &mut SiteTx, site_id: SiteId, tag: &str) -> Result<Language> {
    let row = sqlx::query(
        "select site_id, tag, name, is_default, created_at, updated_at
           from site_languages where site_id = $1 and tag = $2 for update",
    )
    .bind(site_id.into_uuid())
    .bind(tag)
    .fetch_optional(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?
    .ok_or(MaviError::NotFound {
        resource: LANGUAGE_NOT_FOUND,
    })?;
    language_from_row(&row)
}

fn map_language_write_error(error: sqlx::Error) -> MaviError {
    if let sqlx::Error::Database(database) = error
        && database.constraint() == Some("site_languages_pkey")
    {
        return MaviError::conflict(LANGUAGE_ALREADY_EXISTS);
    }
    MaviError::Internal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_and_timezone_values_are_validated_without_sql() {
        assert!(LanguageTag::parse("en").is_ok());
        assert!(LanguageTag::parse("pt-BR").is_ok());
        assert!(LanguageTag::parse("e").is_err());
        assert!(LanguageTag::parse("1n").is_err());
        assert!(LanguageTag::parse("en--US").is_err());
        assert!(timezone("Europe/Berlin").is_ok());
        assert!(timezone("Europe/../secret").is_err());
    }

    #[test]
    fn language_cursor_round_trips_and_rejects_corruption() {
        let created_at = Utc::now();
        let cursor = encode_cursor(created_at, "en").expect("cursor");
        let decoded = decode_cursor(&cursor).expect("decoded");
        assert_eq!(decoded.created_at, created_at);
        assert_eq!(decoded.tag, "en");
        assert!(decode_cursor(&Cursor::parse("not-a-cursor").expect("cursor")).is_err());
    }

    #[test]
    fn settings_contract_is_site_scoped_and_permissioned() {
        let catalog = api();
        catalog.validate().expect("settings API");
        let openapi = catalog.openapi("Mavi", "0.1.0").expect("OpenAPI");
        assert_eq!(
            openapi["paths"]["/api/v1/languages"]["get"]["parameters"][0]["name"],
            "after"
        );
        assert_eq!(
            catalog.endpoints[0].permission,
            Some(Permission {
                capability: Capability::Settings,
                action: Action::View,
            })
        );
    }
}
