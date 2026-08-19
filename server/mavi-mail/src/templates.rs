use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Capability, ErrorCode, MailTemplateId, MaviError, Page, PageRequest, Result,
    SiteContext, ports::MailMessage,
};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sqlx::Row;

use crate::{MailService, decode_cursor, encode_cursor};

pub use mavi_core::ports::MailContentType;

pub const MAIL_TEMPLATE_NOT_FOUND: &str = "mail_template_not_found";
pub const MAIL_TEMPLATE_KEY_INVALID: &str = "mail_template_key_invalid";
pub const MAIL_TEMPLATE_LANGUAGE_INVALID: &str = "mail_template_language_invalid";
pub const MAIL_TEMPLATE_SUBJECT_INVALID: &str = "mail_template_subject_invalid";
pub const MAIL_TEMPLATE_BODY_INVALID: &str = "mail_template_body_invalid";
pub const MAIL_TEMPLATE_PLACEHOLDER_INVALID: &str = "mail_template_placeholder_invalid";
pub const MAIL_TEMPLATE_VARIABLE_MISSING: &str = "mail_template_variable_missing";
pub const MAIL_TEMPLATE_VARIABLE_VALUE_INVALID: &str = "mail_template_variable_value_invalid";

const MAX_TEMPLATE_KEY_CHARS: usize = 64;
const MAX_LANGUAGE_CHARS: usize = 35;
const MAX_SUBJECT_CHARS: usize = 300;
const MAX_BODY_CHARS: usize = 100_000;
const MAX_VARIABLES: usize = 64;
const MAX_VARIABLE_NAME_CHARS: usize = 64;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreateMailTemplate {
    pub key: String,
    pub language: String,
    pub subject: String,
    pub body: String,
    #[serde(default)]
    pub content_type: MailContentType,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UpdateMailTemplate {
    pub subject: Option<String>,
    pub body: Option<String>,
    pub content_type: Option<MailContentType>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MailTemplateListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
}

#[derive(Clone, Debug, Serialize)]
pub struct MailTemplate {
    pub id: MailTemplateId,
    pub key: String,
    pub language: String,
    pub subject: String,
    pub body: String,
    pub content_type: MailContentType,
    pub variables: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MailTemplatePreview {
    #[serde(default)]
    pub variables: Map<String, Value>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RenderedMail {
    pub subject: String,
    pub body: String,
    pub content_type: MailContentType,
}

pub fn api() -> mavi_contract::Api {
    mavi_contract::Api::new(endpoints()).with_shapes(shapes())
}

#[allow(clippy::too_many_lines)]
fn endpoints() -> Vec<Endpoint> {
    let view = Permission {
        capability: Capability::Mail,
        action: Action::View,
    };
    let write = Permission {
        capability: Capability::Mail,
        action: Action::Write,
    };
    let delete = Permission {
        capability: Capability::Mail,
        action: Action::Delete,
    };
    vec![
        Endpoint::new(
            Method::Get,
            "/api/v1/mail/templates",
            "mail.templates.list",
            "List site mail templates with an opaque cursor",
        )
        .account_or_assistant()
        .requires(view)
        .takes_query("MailTemplateListFilter")
        .returns(200, "MailTemplatePage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/mail/templates",
            "mail.templates.create",
            "Create a validated site mail template",
        )
        .account_or_assistant()
        .requires(write)
        .takes("CreateMailTemplate")
        .returns(201, "MailTemplate")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/mail/templates/{id}",
            "mail.templates.read",
            "Read one site mail template",
        )
        .account_or_assistant()
        .requires(view)
        .returns(200, "MailTemplate")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Patch,
            "/api/v1/mail/templates/{id}",
            "mail.templates.update",
            "Update mail template wording without changing its identity",
        )
        .account_or_assistant()
        .requires(write)
        .takes("UpdateMailTemplate")
        .returns(200, "MailTemplate")
        .changes(true)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Delete,
            "/api/v1/mail/templates/{id}",
            "mail.templates.delete",
            "Remove a site mail template from the active catalog",
        )
        .account_or_assistant()
        .requires(delete)
        .returns(204, "Empty")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/mail/templates/{id}/preview",
            "mail.templates.preview",
            "Render a mail template without enqueueing or sending it",
        )
        .account_or_assistant()
        .requires(view)
        .takes("MailTemplatePreview")
        .returns(200, "RenderedMail")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
    ]
}

fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "MailContentType",
            json!({"type": "string", "enum": ["plain", "html"]}),
        ),
        Shape::new(
            "CreateMailTemplate",
            json!({
                "type": "object",
                "required": ["key", "language", "subject", "body"],
                "additionalProperties": false,
                "properties": {
                    "key": {"type": "string", "minLength": 1, "maxLength": MAX_TEMPLATE_KEY_CHARS},
                    "language": {"type": "string", "minLength": 2, "maxLength": MAX_LANGUAGE_CHARS},
                    "subject": {"type": "string", "minLength": 1, "maxLength": MAX_SUBJECT_CHARS},
                    "body": {"type": "string", "minLength": 1, "maxLength": MAX_BODY_CHARS},
                    "content_type": {"$ref": "#/components/schemas/MailContentType"}
                }
            }),
        ),
        Shape::new(
            "UpdateMailTemplate",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "subject": {"type": ["string", "null"], "maxLength": MAX_SUBJECT_CHARS},
                    "body": {"type": ["string", "null"], "maxLength": MAX_BODY_CHARS},
                    "content_type": {"oneOf": [{"$ref": "#/components/schemas/MailContentType"}, {"type": "null"}]}
                }
            }),
        ),
        Shape::new(
            "MailTemplateListFilter",
            json!({"type": "object", "properties": {
                "after": {"type": ["string", "null"], "maxLength": 512},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100}
            }}),
        ),
        Shape::new(
            "MailTemplate",
            json!({
                "type": "object",
                "required": ["id", "key", "language", "subject", "body", "content_type", "variables", "created_at", "updated_at"],
                "properties": {
                    "id": {"type": "string", "format": "uuid"},
                    "key": {"type": "string"},
                    "language": {"type": "string"},
                    "subject": {"type": "string"},
                    "body": {"type": "string"},
                    "content_type": {"$ref": "#/components/schemas/MailContentType"},
                    "variables": {"type": "array", "items": {"type": "string"}},
                    "created_at": {"type": "string", "format": "date-time"},
                    "updated_at": {"type": "string", "format": "date-time"}
                }
            }),
        ),
        Shape::new(
            "MailTemplatePreview",
            json!({"type": "object", "additionalProperties": false, "properties": {
                "variables": {"type": "object", "additionalProperties": true}
            }}),
        ),
        Shape::new(
            "RenderedMail",
            json!({
                "type": "object",
                "required": ["subject", "body", "content_type"],
                "properties": {
                    "subject": {"type": "string"},
                    "body": {"type": "string"},
                    "content_type": {"$ref": "#/components/schemas/MailContentType"}
                }
            }),
        ),
        Shape::new(
            "MailTemplatePage",
            json!({"type": "object", "required": ["items", "next_cursor"], "properties": {
                "items": {"type": "array", "items": {"$ref": "#/components/schemas/MailTemplate"}},
                "next_cursor": {"type": ["string", "null"], "maxLength": 512}
            }}),
        ),
    ]
}

impl MailService {
    pub async fn list_templates(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        filter: &MailTemplateListFilter,
    ) -> Result<Page<MailTemplate>> {
        let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select id, template_key, language, subject, body, content_type, created_at, updated_at
               from mail_templates where site_id = ",
        );
        query.push_bind(context.site_id.into_uuid());
        query.push(" and deleted_at is null");
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
        let limit = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if items.len() > limit {
            let last = items
                .get(limit.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_cursor(last.created_at, last.id.into_uuid())?)
        } else {
            None
        };
        items.truncate(limit);
        Ok(Page::new(items, next_cursor))
    }

    pub async fn get_template(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: MailTemplateId,
    ) -> Result<MailTemplate> {
        let row = sqlx::query(
            "select id, template_key, language, subject, body, content_type, created_at, updated_at
               from mail_templates where site_id = $1 and id = $2 and deleted_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: MAIL_TEMPLATE_NOT_FOUND,
        })?;
        from_row(&row)
    }

    pub async fn create_template(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &CreateMailTemplate,
    ) -> Result<MailTemplate> {
        let (key, language, subject, body, content_type, variables) = validate_template(input)?;
        let id = MailTemplateId::new();
        let row = sqlx::query(
            "insert into mail_templates
                (site_id, id, template_key, language, subject, body, content_type)
             values ($1, $2, $3, $4, $5, $6, $7)
             returning id, template_key, language, subject, body, content_type, created_at, updated_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(&key)
        .bind(&language)
        .bind(&subject)
        .bind(&body)
        .bind(content_type.as_str())
        .fetch_one(tx.conn())
        .await
        .map_err(|error| map_write_error(&error))?;
        let template = from_row(&row)?;
        debug_assert_eq!(template.variables, variables);
        mavi_audit::AuditService
            .record(
                tx,
                context,
                &mavi_audit::AuditEntry {
                    action: "mail.template.created".to_owned(),
                    resource_type: "MailTemplate".to_owned(),
                    resource_id: Some(id.into_uuid()),
                    payload: json!({"key": template.key, "language": template.language}),
                },
            )
            .await?;
        Ok(template)
    }

    pub async fn update_template(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: MailTemplateId,
        input: &UpdateMailTemplate,
    ) -> Result<MailTemplate> {
        let subject = input.subject.as_deref().map(validate_subject).transpose()?;
        let body = input.body.as_deref().map(validate_body).transpose()?;
        let content_type = input.content_type.map(MailContentType::as_str);
        if subject.is_none() && body.is_none() && content_type.is_none() {
            return self.get_template(tx, context, id).await;
        }
        if let Some(body) = body.as_deref() {
            extract_variables(subject.as_deref().unwrap_or(""))?;
            extract_variables(body)?;
        }
        if let Some(subject) = subject.as_deref() {
            extract_variables(subject)?;
        }
        let row = sqlx::query(
            "update mail_templates
                set subject = coalesce($3, subject),
                    body = coalesce($4, body),
                    content_type = coalesce($5, content_type),
                    updated_at = clock_timestamp()
              where site_id = $1 and id = $2 and deleted_at is null
             returning id, template_key, language, subject, body, content_type, created_at, updated_at",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(subject.as_deref())
        .bind(body.as_deref())
        .bind(content_type)
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: MAIL_TEMPLATE_NOT_FOUND,
        })?;
        let template = from_row(&row)?;
        mavi_audit::AuditService
            .record(
                tx,
                context,
                &mavi_audit::AuditEntry {
                    action: "mail.template.updated".to_owned(),
                    resource_type: "MailTemplate".to_owned(),
                    resource_id: Some(id.into_uuid()),
                    payload: json!({"key": template.key, "language": template.language}),
                },
            )
            .await?;
        Ok(template)
    }

    pub async fn delete_template(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: MailTemplateId,
    ) -> Result<()> {
        let result = sqlx::query(
            "update mail_templates
                set deleted_at = clock_timestamp(), updated_at = clock_timestamp()
              where site_id = $1 and id = $2 and deleted_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if result.rows_affected() == 0 {
            return Err(MaviError::NotFound {
                resource: MAIL_TEMPLATE_NOT_FOUND,
            });
        }
        mavi_audit::AuditService
            .record(
                tx,
                context,
                &mavi_audit::AuditEntry {
                    action: "mail.template.deleted".to_owned(),
                    resource_type: "MailTemplate".to_owned(),
                    resource_id: Some(id.into_uuid()),
                    payload: json!({}),
                },
            )
            .await
    }

    pub async fn preview_template(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: MailTemplateId,
        input: &MailTemplatePreview,
    ) -> Result<RenderedMail> {
        let template = self.get_template(tx, context, id).await?;
        render(
            &template.subject,
            &template.body,
            template.content_type,
            &input.variables,
        )
    }

    pub(crate) async fn render_for_delivery(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: MailTemplateId,
        recipient: &str,
        variables: &Map<String, Value>,
    ) -> Result<MailMessage> {
        let recipient = mavi_core::Email::parse(recipient)
            .map_err(|_| MaviError::validation_field("invalid_email", "recipient"))?;
        let template = self.get_template(tx, context, id).await?;
        let rendered = render(
            &template.subject,
            &template.body,
            template.content_type,
            variables,
        )?;
        Ok(MailMessage {
            recipient: recipient.to_string(),
            subject: rendered.subject,
            body: rendered.body,
            content_type: rendered.content_type,
            unsubscribe_url: None,
        })
    }
}

fn validate_template(
    input: &CreateMailTemplate,
) -> Result<(String, String, String, String, MailContentType, Vec<String>)> {
    let key = input.key.trim();
    if key.is_empty()
        || key.chars().count() > MAX_TEMPLATE_KEY_CHARS
        || !key.chars().enumerate().all(|(index, character)| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || (index > 0 && character == '_')
        })
    {
        return Err(MaviError::validation_field(
            MAIL_TEMPLATE_KEY_INVALID,
            "key",
        ));
    }
    let language = validate_language(&input.language)?;
    let subject = validate_subject(&input.subject)?;
    let body = validate_body(&input.body)?;
    let subject_variables = extract_variables(&subject)?;
    let body_variables = extract_variables(&body)?;
    let variables = subject_variables
        .into_iter()
        .chain(body_variables)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok((
        key.to_owned(),
        language,
        subject,
        body,
        input.content_type,
        variables,
    ))
}

fn validate_language(value: &str) -> Result<String> {
    let value = value.trim();
    let valid = !value.is_empty()
        && value.chars().count() <= MAX_LANGUAGE_CHARS
        && value.split('-').all(|part| {
            (2..=8).contains(&part.chars().count())
                && part
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
        });
    if !valid {
        return Err(MaviError::validation_field(
            MAIL_TEMPLATE_LANGUAGE_INVALID,
            "language",
        ));
    }
    Ok(value.to_owned())
}

fn validate_subject(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > MAX_SUBJECT_CHARS
        || value.chars().any(char::is_control)
    {
        return Err(MaviError::validation_field(
            MAIL_TEMPLATE_SUBJECT_INVALID,
            "subject",
        ));
    }
    Ok(value.to_owned())
}

fn validate_body(value: &str) -> Result<String> {
    if value.is_empty()
        || value.chars().count() > MAX_BODY_CHARS
        || value.chars().any(|character| character == '\0')
    {
        return Err(MaviError::validation_field(
            MAIL_TEMPLATE_BODY_INVALID,
            "body",
        ));
    }
    Ok(value.to_owned())
}

fn extract_variables(value: &str) -> Result<Vec<String>> {
    let mut names = BTreeSet::new();
    let mut cursor = 0;
    while let Some(relative_open) = value[cursor..].find("{{") {
        let open = cursor + relative_open;
        if value[cursor..open].contains("}}") {
            return Err(MaviError::validation(MAIL_TEMPLATE_PLACEHOLDER_INVALID));
        }
        let Some(relative_close) = value[open + 2..].find("}}") else {
            return Err(MaviError::validation(MAIL_TEMPLATE_PLACEHOLDER_INVALID));
        };
        let close = open + 2 + relative_close;
        let name = value[open + 2..close].trim();
        if name.is_empty()
            || name.chars().count() > MAX_VARIABLE_NAME_CHARS
            || !name.chars().enumerate().all(|(index, character)| {
                character.is_ascii_lowercase()
                    || character.is_ascii_digit()
                    || (index > 0 && character == '_')
            })
            || {
                names.insert(name.to_owned());
                names.len() > MAX_VARIABLES
            }
        {
            return Err(MaviError::validation(MAIL_TEMPLATE_PLACEHOLDER_INVALID));
        }
        cursor = close + 2;
    }
    if value[cursor..].contains("}}") {
        return Err(MaviError::validation(MAIL_TEMPLATE_PLACEHOLDER_INVALID));
    }
    Ok(names.into_iter().collect())
}

fn render(
    subject: &str,
    body: &str,
    content_type: MailContentType,
    variables: &Map<String, Value>,
) -> Result<RenderedMail> {
    Ok(RenderedMail {
        subject: render_text(subject, variables)?,
        body: render_text(body, variables)?,
        content_type,
    })
}

fn render_text(value: &str, variables: &Map<String, Value>) -> Result<String> {
    let names = extract_variables(value)?;
    let mut rendered = String::with_capacity(value.len());
    let mut cursor = 0;
    while let Some(relative_open) = value[cursor..].find("{{") {
        let open = cursor + relative_open;
        rendered.push_str(&value[cursor..open]);
        let close = open
            + 2
            + value[open + 2..]
                .find("}}")
                .ok_or_else(|| MaviError::validation(MAIL_TEMPLATE_PLACEHOLDER_INVALID))?;
        let name = value[open + 2..close].trim();
        let value = variables.get(name).ok_or_else(|| {
            MaviError::validation_field(MAIL_TEMPLATE_VARIABLE_MISSING, format!("variables.{name}"))
        })?;
        match value {
            Value::String(value) => rendered.push_str(value),
            Value::Number(value) => rendered.push_str(&value.to_string()),
            Value::Bool(value) => rendered.push_str(if *value { "true" } else { "false" }),
            Value::Null | Value::Array(_) | Value::Object(_) => {
                return Err(MaviError::validation_field(
                    MAIL_TEMPLATE_VARIABLE_VALUE_INVALID,
                    format!("variables.{name}"),
                ));
            }
        }
        cursor = close + 2;
    }
    rendered.push_str(&value[cursor..]);
    if names.iter().any(|name| !variables.contains_key(name)) {
        let missing = names
            .into_iter()
            .find(|name| !variables.contains_key(name))
            .ok_or(MaviError::Internal)?;
        return Err(MaviError::validation_field(
            MAIL_TEMPLATE_VARIABLE_MISSING,
            format!("variables.{missing}"),
        ));
    }
    Ok(rendered)
}

pub(crate) fn parse_content_type(value: &str) -> Result<MailContentType> {
    match value {
        "plain" => Ok(MailContentType::Plain),
        "html" => Ok(MailContentType::Html),
        _ => Err(MaviError::Internal),
    }
}

fn from_row(row: &sqlx::postgres::PgRow) -> Result<MailTemplate> {
    let subject: String = row.try_get("subject").map_err(|_| MaviError::Internal)?;
    let body: String = row.try_get("body").map_err(|_| MaviError::Internal)?;
    let subject_variables = extract_variables(&subject).map_err(|_| MaviError::Internal)?;
    let body_variables = extract_variables(&body).map_err(|_| MaviError::Internal)?;
    let variables = subject_variables
        .into_iter()
        .chain(body_variables)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(MailTemplate {
        id: MailTemplateId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        key: row
            .try_get("template_key")
            .map_err(|_| MaviError::Internal)?,
        language: row.try_get("language").map_err(|_| MaviError::Internal)?,
        subject,
        body,
        content_type: parse_content_type(
            &row.try_get::<String, _>("content_type")
                .map_err(|_| MaviError::Internal)?,
        )?,
        variables,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

fn map_write_error(error: &sqlx::Error) -> MaviError {
    if let sqlx::Error::Database(database) = error
        && database.constraint() == Some("mail_templates_site_key_language_active")
    {
        return MaviError::conflict("mail_template_taken");
    }
    MaviError::Internal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_have_strict_placeholders_and_safe_subjects() {
        let input = CreateMailTemplate {
            key: "welcome_email".to_owned(),
            language: "en".to_owned(),
            subject: "Welcome {{name}}".to_owned(),
            body: "Hello {{name}}, see {{count}} items.".to_owned(),
            content_type: MailContentType::Plain,
        };
        let (_, _, _, _, _, variables) = validate_template(&input).expect("template");
        assert_eq!(variables, vec!["count", "name"]);
        assert!(validate_subject("Subject\nBcc: attacker@example.test").is_err());
        assert!(extract_variables("broken {{name").is_err());
        assert!(extract_variables("{{Name}}").is_err());
    }

    #[test]
    fn rendering_rejects_missing_and_structured_values() {
        let variables = serde_json::from_value::<Map<String, Value>>(json!({"name": "Ada"}))
            .expect("variables");
        let rendered = render(
            "Hi {{name}}",
            "Hello {{name}}",
            MailContentType::Plain,
            &variables,
        )
        .expect("rendered");
        assert_eq!(rendered.subject, "Hi Ada");
        assert!(render("Hi {{missing}}", "Body", MailContentType::Plain, &variables).is_err());
        let structured = serde_json::from_value::<Map<String, Value>>(json!({"name": {"x": 1}}))
            .expect("variables");
        assert!(render("Hi {{name}}", "Body", MailContentType::Plain, &structured).is_err());
    }

    #[test]
    fn template_lists_are_cursor_only() {
        let contract = serde_json::to_string(&api()).expect("contract");
        assert!(contract.contains("MailTemplateListFilter"));
        assert!(!contract.contains("offset"));
        assert!(!contract.contains("page_number"));
    }
}
