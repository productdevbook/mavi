use chrono::{DateTime, Utc};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Capability, Email, ErrorCode, MailListId, MailReaderId, MaviError, Page, PageRequest,
    Result, SiteContext,
};
use mavi_storage::SiteTx;
use rand::RngExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Row;

use crate::{MailService, decode_cursor, encode_cursor};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};

pub const MAIL_LIST_NOT_FOUND: &str = "mail_list_not_found";
pub const MAIL_READER_NOT_FOUND: &str = "mail_reader_not_found";
pub const MAIL_LIST_SLUG_INVALID: &str = "mail_list_slug_invalid";
pub const MAIL_LIST_NAME_INVALID: &str = "mail_list_name_invalid";
pub const MAIL_READER_NAME_INVALID: &str = "mail_reader_name_invalid";
pub const MAIL_READER_TAKEN: &str = "mail_reader_taken";
pub const MAIL_STANDING_INVALID: &str = "mail_standing_invalid";

const MAX_LIST_SLUG_CHARS: usize = 64;
const MAX_LIST_NAME_CHARS: usize = 200;
const MAX_READER_NAME_CHARS: usize = 200;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MailStanding {
    Subscribed,
    Unsubscribed,
    Bounced,
    Complained,
}

impl MailStanding {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Subscribed => "subscribed",
            Self::Unsubscribed => "unsubscribed",
            Self::Bounced => "bounced",
            Self::Complained => "complained",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "subscribed" => Ok(Self::Subscribed),
            "unsubscribed" => Ok(Self::Unsubscribed),
            "bounced" => Ok(Self::Bounced),
            "complained" => Ok(Self::Complained),
            _ => Err(MaviError::validation(MAIL_STANDING_INVALID)),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct MailListListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CreateMailList {
    pub slug: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UpdateMailList {
    pub name: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MailList {
    pub id: MailListId,
    pub slug: String,
    pub name: String,
    pub subscriber_count: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ReaderListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
    pub standing: Option<MailStanding>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AddReader {
    pub email: String,
    pub name: Option<String>,
    #[serde(default)]
    pub resubscribe: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct MailReader {
    pub id: MailReaderId,
    pub email: String,
    pub name: Option<String>,
    pub standing: MailStanding,
    pub added_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MailReaderCreated {
    pub reader: MailReader,
    /// Returned when a reader is added; only its hash is retained in storage.
    pub unsubscribe_token: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct UnsubscribeReceipt {
    pub unsubscribed: bool,
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
            "/api/v1/mail/lists",
            "mail.lists.list",
            "List site mailing lists with an opaque cursor",
        )
        .account_or_assistant()
        .requires(view)
        .takes_query("MailListListFilter")
        .returns(200, "MailListPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/mail/lists",
            "mail.lists.create",
            "Create a site mailing list",
        )
        .account_or_assistant()
        .requires(write)
        .takes("CreateMailList")
        .returns(201, "MailList")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/mail/lists/{id}",
            "mail.lists.read",
            "Read one site mailing list",
        )
        .account_or_assistant()
        .requires(view)
        .returns(200, "MailList")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Patch,
            "/api/v1/mail/lists/{id}",
            "mail.lists.update",
            "Rename a site mailing list",
        )
        .account_or_assistant()
        .requires(write)
        .takes("UpdateMailList")
        .returns(200, "MailList")
        .changes(true)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Delete,
            "/api/v1/mail/lists/{id}",
            "mail.lists.delete",
            "Remove a site mailing list from the active catalog",
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
            Method::Get,
            "/api/v1/mail/lists/{id}/readers",
            "mail.readers.list",
            "List one mailing list's readers with an opaque cursor",
        )
        .account_or_assistant()
        .requires(view)
        .takes_query("ReaderListFilter")
        .returns(200, "MailReaderPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/mail/lists/{id}/readers",
            "mail.readers.add",
            "Add or find a reader on a mailing list",
        )
        .account_or_assistant()
        .requires(write)
        .takes("AddReader")
        .returns(201, "MailReaderCreated")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Delete,
            "/api/v1/mail/readers/{id}",
            "mail.readers.delete",
            "Forget a site reader and their list memberships",
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
            "/public/v1/mail/unsubscribe/{token}",
            "mail.public.unsubscribe",
            "Unsubscribe a reader without revealing whether a token exists",
        )
        .public_mutation()
        .returns(200, "UnsubscribeReceipt")
        .refuses([ErrorCode::Validation, ErrorCode::Internal]),
    ]
}

fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "MailListListFilter",
            json!({"type": "object", "properties": {
                "after": {"type": ["string", "null"], "maxLength": 512},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100}
            }}),
        ),
        Shape::new(
            "CreateMailList",
            json!({"type": "object", "required": ["slug", "name"], "additionalProperties": false, "properties": {
                "slug": {"type": "string", "minLength": 1, "maxLength": MAX_LIST_SLUG_CHARS},
                "name": {"type": "string", "minLength": 1, "maxLength": MAX_LIST_NAME_CHARS}
            }}),
        ),
        Shape::new(
            "UpdateMailList",
            json!({"type": "object", "additionalProperties": false, "properties": {
                "name": {"type": ["string", "null"], "maxLength": MAX_LIST_NAME_CHARS}
            }}),
        ),
        Shape::new(
            "MailList",
            json!({"type": "object", "required": ["id", "slug", "name", "subscriber_count", "created_at", "updated_at"], "properties": {
                "id": {"type": "string", "format": "uuid"},
                "slug": {"type": "string"},
                "name": {"type": "string"},
                "subscriber_count": {"type": "integer", "format": "int64", "minimum": 0},
                "created_at": {"type": "string", "format": "date-time"},
                "updated_at": {"type": "string", "format": "date-time"}
            }}),
        ),
        Shape::new(
            "MailListPage",
            json!({"type": "object", "required": ["items", "next_cursor"], "properties": {
                "items": {"type": "array", "items": {"$ref": "#/components/schemas/MailList"}},
                "next_cursor": {"type": ["string", "null"], "maxLength": 512}
            }}),
        ),
        Shape::new(
            "MailStanding",
            json!({"type": "string", "enum": ["subscribed", "unsubscribed", "bounced", "complained"]}),
        ),
        Shape::new(
            "ReaderListFilter",
            json!({"type": "object", "properties": {
                "after": {"type": ["string", "null"], "maxLength": 512},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                "standing": {"$ref": "#/components/schemas/MailStanding"}
            }}),
        ),
        Shape::new(
            "AddReader",
            json!({"type": "object", "required": ["email"], "additionalProperties": false, "properties": {
                "email": {"type": "string", "format": "email"},
                "name": {"type": ["string", "null"], "maxLength": MAX_READER_NAME_CHARS},
                "resubscribe": {"type": "boolean"}
            }}),
        ),
        Shape::new(
            "MailReader",
            json!({"type": "object", "required": ["id", "email", "name", "standing", "added_at"], "properties": {
                "id": {"type": "string", "format": "uuid"},
                "email": {"type": "string", "format": "email"},
                "name": {"type": ["string", "null"]},
                "standing": {"$ref": "#/components/schemas/MailStanding"},
                "added_at": {"type": "string", "format": "date-time"}
            }}),
        ),
        Shape::new(
            "MailReaderCreated",
            json!({"type": "object", "required": ["reader", "unsubscribe_token"], "properties": {
                "reader": {"$ref": "#/components/schemas/MailReader"},
                "unsubscribe_token": {"type": "string", "minLength": 40, "maxLength": 64}
            }}),
        ),
        Shape::new(
            "MailReaderPage",
            json!({"type": "object", "required": ["items", "next_cursor"], "properties": {
                "items": {"type": "array", "items": {"$ref": "#/components/schemas/MailReader"}},
                "next_cursor": {"type": ["string", "null"], "maxLength": 512}
            }}),
        ),
        Shape::new(
            "UnsubscribeReceipt",
            json!({"type": "object", "required": ["unsubscribed"], "properties": {"unsubscribed": {"type": "boolean"}}}),
        ),
    ]
}

impl MailService {
    pub async fn list_lists(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        filter: &MailListListFilter,
    ) -> Result<Page<MailList>> {
        let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select l.id, l.slug, l.name, l.created_at, l.updated_at,
                    (select count(*) from mail_list_members m
                       join mail_readers r on r.site_id = m.site_id and r.id = m.reader_id
                      where m.site_id = l.site_id and m.list_id = l.id
                        and r.deleted_at is null and r.standing = 'subscribed') as subscriber_count
               from mail_lists l where l.site_id = ",
        );
        query.push_bind(context.site_id.into_uuid());
        query.push(" and l.deleted_at is null");
        if let Some(after) = after {
            query
                .push(" and (l.created_at, l.id) < (")
                .push_bind(after.created_at)
                .push(", ")
                .push_bind(after.id)
                .push(")");
        }
        let rows = query
            .push(" order by l.created_at desc, l.id desc limit ")
            .push_bind(limit + 1)
            .build()
            .fetch_all(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let mut items = rows.iter().map(from_list_row).collect::<Result<Vec<_>>>()?;
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

    pub async fn get_list(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: MailListId,
    ) -> Result<MailList> {
        let row = sqlx::query(
            "select l.id, l.slug, l.name, l.created_at, l.updated_at,
                    (select count(*) from mail_list_members m
                       join mail_readers r on r.site_id = m.site_id and r.id = m.reader_id
                      where m.site_id = l.site_id and m.list_id = l.id
                        and r.deleted_at is null and r.standing = 'subscribed') as subscriber_count
               from mail_lists l
              where l.site_id = $1 and l.id = $2 and l.deleted_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: MAIL_LIST_NOT_FOUND,
        })?;
        from_list_row(&row)
    }

    pub async fn create_list(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &CreateMailList,
    ) -> Result<MailList> {
        let slug = validate_slug(&input.slug)?;
        let name = validate_name(&input.name)?;
        let id = MailListId::new();
        sqlx::query("insert into mail_lists (site_id, id, slug, name) values ($1, $2, $3, $4)")
            .bind(context.site_id.into_uuid())
            .bind(id.into_uuid())
            .bind(&slug)
            .bind(&name)
            .execute(tx.conn())
            .await
            .map_err(|error| map_write_error(&error))?;
        let list = self.get_list(tx, context, id).await?;
        audit(
            tx,
            context,
            "mail.list.created",
            "MailList",
            id,
            json!({"slug": slug}),
        )
        .await?;
        Ok(list)
    }

    pub async fn update_list(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: MailListId,
        input: &UpdateMailList,
    ) -> Result<MailList> {
        let name = input.name.as_deref().map(validate_name).transpose()?;
        if name.is_none() {
            return self.get_list(tx, context, id).await;
        }
        let changed = sqlx::query(
            "update mail_lists set name = $3, updated_at = clock_timestamp()
              where site_id = $1 and id = $2 and deleted_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(name.as_deref())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if changed.rows_affected() == 0 {
            return Err(MaviError::NotFound {
                resource: MAIL_LIST_NOT_FOUND,
            });
        }
        let list = self.get_list(tx, context, id).await?;
        audit(tx, context, "mail.list.updated", "MailList", id, json!({})).await?;
        Ok(list)
    }

    pub async fn delete_list(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: MailListId,
    ) -> Result<()> {
        let changed = sqlx::query(
            "update mail_lists set deleted_at = clock_timestamp(), updated_at = clock_timestamp()
              where site_id = $1 and id = $2 and deleted_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if changed.rows_affected() == 0 {
            return Err(MaviError::NotFound {
                resource: MAIL_LIST_NOT_FOUND,
            });
        }
        audit(tx, context, "mail.list.deleted", "MailList", id, json!({})).await
    }

    pub async fn list_readers(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        list_id: MailListId,
        filter: &ReaderListFilter,
    ) -> Result<Page<MailReader>> {
        self.get_list(tx, context, list_id).await?;
        let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select r.id, r.email, r.name, r.standing, m.created_at as added_at
               from mail_list_members m
               join mail_readers r on r.site_id = m.site_id and r.id = m.reader_id
              where m.site_id = ",
        );
        query.push_bind(context.site_id.into_uuid());
        query.push(" and m.list_id = ");
        query.push_bind(list_id.into_uuid());
        query.push(" and r.deleted_at is null");
        if let Some(standing) = filter.standing {
            query.push(" and r.standing = ");
            query.push_bind(standing.as_str());
        }
        if let Some(after) = after {
            query
                .push(" and (m.created_at, r.id) < (")
                .push_bind(after.created_at)
                .push(", ")
                .push_bind(after.id)
                .push(")");
        }
        let rows = query
            .push(" order by m.created_at desc, r.id desc limit ")
            .push_bind(limit + 1)
            .build()
            .fetch_all(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let mut items = rows
            .iter()
            .map(from_reader_row)
            .collect::<Result<Vec<_>>>()?;
        let limit = usize::try_from(limit).map_err(|_| MaviError::Internal)?;
        let next_cursor = if items.len() > limit {
            let last = items
                .get(limit.saturating_sub(1))
                .ok_or(MaviError::Internal)?;
            Some(encode_cursor(last.added_at, last.id.into_uuid())?)
        } else {
            None
        };
        items.truncate(limit);
        Ok(Page::new(items, next_cursor))
    }

    pub async fn add_reader(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        list_id: MailListId,
        input: &AddReader,
    ) -> Result<MailReaderCreated> {
        self.get_list(tx, context, list_id).await?;
        let email = Email::parse(&input.email)
            .map_err(|_| MaviError::validation_field("invalid_email", "email"))?;
        let name = input
            .name
            .as_deref()
            .map(validate_reader_name)
            .transpose()?;
        let (token, token_hash) = mint_token();
        let existing = sqlx::query(
            "select id from mail_readers where site_id = $1 and email = $2 and deleted_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(email.as_str())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let reader_id = if let Some(row) = existing {
            let id: uuid::Uuid = row.try_get("id").map_err(|_| MaviError::Internal)?;
            sqlx::query(
                "update mail_readers
                    set name = coalesce($3, name),
                        standing = case when $4 then 'subscribed' else standing end,
                        unsubscribe_token_hash = $5,
                        updated_at = clock_timestamp()
                  where site_id = $1 and id = $2 and deleted_at is null",
            )
            .bind(context.site_id.into_uuid())
            .bind(id)
            .bind(name.as_deref())
            .bind(input.resubscribe)
            .bind(&token_hash)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
            MailReaderId::from_uuid(id)
        } else {
            let id = MailReaderId::new();
            sqlx::query(
                "insert into mail_readers
                    (site_id, id, email, name, unsubscribe_token_hash)
                 values ($1, $2, $3, $4, $5)",
            )
            .bind(context.site_id.into_uuid())
            .bind(id.into_uuid())
            .bind(email.as_str())
            .bind(name.as_deref())
            .bind(&token_hash)
            .execute(tx.conn())
            .await
            .map_err(|error| map_write_error(&error))?;
            id
        };
        sqlx::query(
            "insert into mail_list_members (site_id, list_id, reader_id)
             values ($1, $2, $3) on conflict do nothing",
        )
        .bind(context.site_id.into_uuid())
        .bind(list_id.into_uuid())
        .bind(reader_id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let reader = sqlx::query(
            "select r.id, r.email, r.name, r.standing, m.created_at as added_at
               from mail_list_members m
               join mail_readers r on r.site_id = m.site_id and r.id = m.reader_id
              where m.site_id = $1 and m.list_id = $2 and m.reader_id = $3 and r.deleted_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(list_id.into_uuid())
        .bind(reader_id.into_uuid())
        .fetch_one(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let reader = from_reader_row(&reader)?;
        audit(
            tx,
            context,
            "mail.reader.added",
            "MailReader",
            reader_id,
            json!({"list_id": list_id, "resubscribe": input.resubscribe}),
        )
        .await?;
        Ok(MailReaderCreated {
            reader,
            unsubscribe_token: token,
        })
    }

    pub async fn delete_reader(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: MailReaderId,
    ) -> Result<()> {
        let changed = sqlx::query(
            "update mail_readers set deleted_at = clock_timestamp(), updated_at = clock_timestamp()
              where site_id = $1 and id = $2 and deleted_at is null",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        if changed.rows_affected() == 0 {
            return Err(MaviError::NotFound {
                resource: MAIL_READER_NOT_FOUND,
            });
        }
        audit(
            tx,
            context,
            "mail.reader.deleted",
            "MailReader",
            id,
            json!({}),
        )
        .await
    }

    pub async fn unsubscribe(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        token: &str,
    ) -> Result<UnsubscribeReceipt> {
        let token_hash = hash_token(token)?;
        let row = sqlx::query(
            "update mail_readers
                set standing = 'unsubscribed', updated_at = clock_timestamp()
              where site_id = $1 and unsubscribe_token_hash = $2 and deleted_at is null
             returning id",
        )
        .bind(context.site_id.into_uuid())
        .bind(&token_hash)
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let reader_id = if let Some(row) = row {
            row.try_get::<uuid::Uuid, _>("id")
                .map_err(|_| MaviError::Internal)?
        } else {
            let token = sqlx::query(
                "select id, reader_id from mail_unsubscribe_tokens
                  where site_id = $1 and token_hash = $2 and used_at is null
                  limit 1",
            )
            .bind(context.site_id.into_uuid())
            .bind(&token_hash)
            .fetch_optional(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
            let Some(token) = token else {
                return Ok(UnsubscribeReceipt { unsubscribed: true });
            };
            let token_id: uuid::Uuid = token.try_get("id").map_err(|_| MaviError::Internal)?;
            let reader_id: uuid::Uuid = token
                .try_get("reader_id")
                .map_err(|_| MaviError::Internal)?;
            sqlx::query(
                "update mail_readers
                    set standing = 'unsubscribed', updated_at = clock_timestamp()
                  where site_id = $1 and id = $2 and deleted_at is null",
            )
            .bind(context.site_id.into_uuid())
            .bind(reader_id)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
            sqlx::query(
                "update mail_unsubscribe_tokens
                    set used_at = clock_timestamp()
                  where site_id = $1 and id = $2 and used_at is null",
            )
            .bind(context.site_id.into_uuid())
            .bind(token_id)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
            reader_id
        };
        {
            let id = reader_id;
            audit(
                tx,
                context,
                "mail.reader.unsubscribed",
                "MailReader",
                MailReaderId::from_uuid(id),
                json!({}),
            )
            .await?;
        }
        Ok(UnsubscribeReceipt { unsubscribed: true })
    }
}

fn validate_slug(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > MAX_LIST_SLUG_CHARS
        || value.starts_with('-')
        || value.ends_with('-')
        || !value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        return Err(MaviError::validation_field(MAIL_LIST_SLUG_INVALID, "slug"));
    }
    Ok(value.to_owned())
}

fn validate_name(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > MAX_LIST_NAME_CHARS
        || value.chars().any(char::is_control)
    {
        return Err(MaviError::validation_field(MAIL_LIST_NAME_INVALID, "name"));
    }
    Ok(value.to_owned())
}

fn validate_reader_name(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > MAX_READER_NAME_CHARS
        || value.chars().any(char::is_control)
    {
        return Err(MaviError::validation_field(
            MAIL_READER_NAME_INVALID,
            "name",
        ));
    }
    Ok(value.to_owned())
}

pub(crate) fn mint_token() -> (String, Vec<u8>) {
    let mut bytes = [0_u8; 32];
    rand::rng().fill(&mut bytes);
    let token = URL_SAFE_NO_PAD.encode(bytes);
    (token.clone(), hash_bytes(token.as_bytes()))
}

pub(crate) fn hash_token(token: &str) -> Result<Vec<u8>> {
    if token.is_empty() || token.len() > 128 {
        return Err(MaviError::validation("invalid_unsubscribe_token"));
    }
    Ok(hash_bytes(token.as_bytes()))
}

fn hash_bytes(value: &[u8]) -> Vec<u8> {
    Sha256::digest(value).to_vec()
}

fn from_list_row(row: &sqlx::postgres::PgRow) -> Result<MailList> {
    Ok(MailList {
        id: MailListId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        slug: row.try_get("slug").map_err(|_| MaviError::Internal)?,
        name: row.try_get("name").map_err(|_| MaviError::Internal)?,
        subscriber_count: row
            .try_get::<i64, _>("subscriber_count")
            .map_err(|_| MaviError::Internal)?
            .try_into()
            .map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

fn from_reader_row(row: &sqlx::postgres::PgRow) -> Result<MailReader> {
    Ok(MailReader {
        id: MailReaderId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        email: row.try_get("email").map_err(|_| MaviError::Internal)?,
        name: row.try_get("name").map_err(|_| MaviError::Internal)?,
        standing: MailStanding::parse(
            &row.try_get::<String, _>("standing")
                .map_err(|_| MaviError::Internal)?,
        )?,
        added_at: row.try_get("added_at").map_err(|_| MaviError::Internal)?,
    })
}

async fn audit(
    tx: &mut SiteTx,
    context: &SiteContext,
    action: &str,
    resource_type: &str,
    resource_id: impl Into<uuid::Uuid>,
    payload: serde_json::Value,
) -> Result<()> {
    mavi_audit::AuditService
        .record(
            tx,
            context,
            &mavi_audit::AuditEntry {
                action: action.to_owned(),
                resource_type: resource_type.to_owned(),
                resource_id: Some(resource_id.into()),
                payload,
            },
        )
        .await
}

fn map_write_error(error: &sqlx::Error) -> MaviError {
    if let sqlx::Error::Database(database) = error {
        match database.constraint() {
            Some("mail_lists_site_slug_active") => {
                return MaviError::conflict("mail_list_slug_taken");
            }
            Some("mail_readers_site_email_active") => {
                return MaviError::conflict(MAIL_READER_TAKEN);
            }
            _ => {}
        }
    }
    MaviError::Internal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_tokens_are_random_and_safe_to_put_in_a_url() {
        let (first, first_hash) = mint_token();
        let (second, second_hash) = mint_token();
        assert_ne!(first, second);
        assert_ne!(first_hash, second_hash);
        assert!(!first.contains('='));
        assert!(hash_token(&first).is_ok());
    }

    #[test]
    fn list_and_reader_filters_are_cursor_only() {
        let contract = serde_json::to_string(&api()).expect("contract");
        assert!(contract.contains("ReaderListFilter"));
        assert!(!contract.contains("offset"));
        assert!(!contract.contains("page_number"));
    }
}
