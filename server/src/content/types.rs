//! A site's own kinds of thing, and the fields they carry.
//!
//! What makes a recipe's cooking time worth storing separately from its body is
//! that a site can then be asked for every recipe under thirty minutes. So a
//! field is declared before it is written: what is not declared is refused, and
//! what is declared is what a filter may ask about.

use axum::Json;
use axum::extract::{Path, State as Injected};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::kernel::audit::{self, Actor, Audited};
use crate::kernel::authz::{Access, Capability, Needs, Permit};
use crate::kernel::db::TenantConn;
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::say::{self, Say};
use crate::kernel::types::Title;

fn needs(access: Access) -> Needs {
    Needs::new(Capability::Content, access)
}

pub(super) fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/content-types",
            Guard {
                audience: Audience::User,
                needs: Some(needs(Access::View)),
                rate: RatePolicy::None,
            },
            list,
        )
        .gives::<Vec<ContentType>>(),
        Endpoint::post(
            "/api/content-types",
            Guard {
                audience: Audience::User,
                needs: Some(needs(Access::Write)),
                rate: RatePolicy::None,
            },
            create,
        )
        .takes::<NewType>()
        .gives::<ContentType>(),
        Endpoint::put(
            "/api/content-types/{key}",
            Guard {
                audience: Audience::User,
                needs: Some(needs(Access::Write)),
                rate: RatePolicy::None,
            },
            replace,
        )
        .takes::<NewType>()
        .gives::<ContentType>(),
        Endpoint::delete(
            "/api/content-types/{key}",
            Guard {
                audience: Audience::User,
                needs: Some(needs(Access::Delete)),
                rate: RatePolicy::None,
            },
            remove,
        ),
    ]
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct ContentType {
    pub id: Uuid,
    pub key: String,
    pub name: String,
    /// What several of them are called. Null falls back to the name, which is
    /// what a language without a separate plural wants anyway.
    pub plural: Option<String>,
    /// What it is called in each language a site writes in. What is missing
    /// falls back to `name` and `plural`.
    pub names: Value,
    pub fields: Value,
    /// How much is written under it. What somebody wants to know before taking
    /// a kind away, since what was written under it stays behind.
    pub posts: i64,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewType {
    /// What several of them are called, where that is a different word.
    #[serde(default)]
    pub plural: Option<Title>,
    /// What it is called in each language a site writes in, as
    /// `{"tr": {"name": "Kitap", "plural": "Kitaplar"}}`. Left out, what is
    /// already written down stays.
    #[serde(default)]
    pub names: Option<Value>,
    /// Only where one is being made. Changing a type's name would change what
    /// every post of it is, so this is not a thing that can be edited.
    #[serde(default)]
    pub key: Option<String>,
    pub name: Title,
    pub fields: Vec<Field>,
}

/// One field a kind of thing carries.
#[derive(Clone, Debug, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = TypeField)]
pub struct Field {
    /// What the field is called in the data: the key a post's `fields` holds
    /// it under, and what a theme reads.
    pub name: String,
    /// What a person sees above the box. The name, where nothing else is said
    /// — which is what a site whose fields are already words wants.
    #[serde(default)]
    pub label: Option<String>,
    pub kind: Kind,
    #[serde(default)]
    pub required: bool,
    /// For a field that is one of a few things. Empty for the rest.
    #[serde(default)]
    pub choices: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = FieldKind)]
pub enum Kind {
    Text,
    Number,
    Boolean,
    /// A moment, as a string, because that is what a form sends and what a
    /// template shows. Compared as text, which for RFC 3339 is the same order.
    Moment,
    Choice,
}

async fn list(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
) -> Result<Json<Vec<ContentType>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;
    let rows = all(&mut conn).await?;
    conn.commit().await?;

    Ok(Json(rows))
}

pub(super) async fn all(conn: &mut TenantConn) -> Result<Vec<ContentType>> {
    Ok(sqlx::query_as(
        "select t.id, t.key, t.name, t.plural, t.names, t.fields,
                (select count(*) from posts p
                  where p.type_key = t.key and p.deleted_at is null) as posts
           from content_types t order by t.name",
    )
    .fetch_all(conn.conn())
    .await?)
}

async fn create(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Json(body): Json<NewType>,
) -> Result<Audited<(StatusCode, Json<ContentType>)>> {
    let key = body
        .key
        .as_deref()
        .ok_or_else(|| AppError::Invalid(say::A_KIND_OF_THING_IS_MADE_WITH_A_NAME.into()))?
        .to_owned();

    check(&body.fields)?;

    let mut conn = state.db.tenant(caller.tenant()).await?;

    let made: ContentType = sqlx::query_as(
        "insert into content_types (tenant_id, key, name, plural, names, fields)
         values ($1, $2, $3, $4, coalesce($5, '{}'::jsonb), $6)
         returning id, key, name, plural, names, fields, 0::bigint as posts",
    )
    .bind(caller.tenant().0)
    .bind(&key)
    .bind(body.name.as_str())
    .bind(body.plural.as_ref().map(Title::as_str))
    .bind(body.names.clone())
    .bind(serde_json::to_value(&body.fields).unwrap_or(Value::Null))
    .fetch_one(conn.conn())
    .await
    .map_err(named_wrongly)?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "made a kind of thing",
        "content_type",
        Some(&made.id.to_string()),
        &serde_json::json!({ "key": made.key }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, (StatusCode::CREATED, Json(made))))
}

/// The whole declaration at once rather than a field at a time: what a type is
/// is the list, and adding one field while somebody else removes another is a
/// list neither of them wrote.
async fn replace(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(key): Path<String>,
    Json(body): Json<NewType>,
) -> Result<Audited<Json<ContentType>>> {
    check(&body.fields)?;

    let mut conn = state.db.tenant(caller.tenant()).await?;

    let changed: Option<ContentType> = sqlx::query_as(
        "update content_types
            set name = $2, plural = $3, names = coalesce($4, names), fields = $5
          where key = $1
         returning id, key, name, plural, names, fields,
                   (select count(*) from posts p
                     where p.type_key = content_types.key and p.deleted_at is null)
                       as posts",
    )
    .bind(&key)
    .bind(body.name.as_str())
    .bind(body.plural.as_ref().map(Title::as_str))
    .bind(body.names.clone())
    .bind(serde_json::to_value(&body.fields).unwrap_or(Value::Null))
    .fetch_optional(conn.conn())
    .await?;

    let changed = changed.ok_or(AppError::NotFound("content type"))?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "changed a kind of thing",
        "content_type",
        Some(&changed.id.to_string()),
        &serde_json::json!({ "key": changed.key }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, Json(changed)))
}

/// What posts of it were is left alone: the type stops existing and what was
/// written under it keeps its fields, because throwing away a declaration is
/// not a reason to throw away a hundred pages.
async fn remove(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(key): Path<String>,
) -> Result<Audited<StatusCode>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let gone = sqlx::query("delete from content_types where key = $1")
        .bind(&key)
        .execute(conn.conn())
        .await?
        .rows_affected();

    if gone == 0 {
        return Err(AppError::NotFound("content type"));
    }

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "took away a kind of thing",
        "content_type",
        Some(&key),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::NO_CONTENT))
}

fn named_wrongly(error: sqlx::Error) -> AppError {
    match error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
    {
        Some(code) if code == "23505" => {
            AppError::Conflict(say::THERE_IS_ALREADY_A_KIND_OF_THING_BY_THAT_NAME.into())
        }
        Some(code) if code == "23514" => {
            AppError::Invalid(say::A_KIND_OF_THING_IS_NAMED_IN_LOWERCASE_AND_UNDERSCORES.into())
        }
        other => {
            let _ = other;
            AppError::Database(error)
        }
    }
}

/// A declaration that says the same thing twice, or a choice field with nothing
/// to choose from, is a form nobody can fill in.
fn check(fields: &[Field]) -> Result<()> {
    let mut seen: Vec<&str> = Vec::with_capacity(fields.len());

    for field in fields {
        let named = field.name.as_str();

        let sensible = !named.is_empty()
            && named.len() <= 40
            && named
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');

        if !sensible {
            return Err(AppError::Invalid(
                Say::of(say::A_FIELD_IS_NAMED_IN_LOWERCASE_AND_UNDERSCORES).naming("field", named),
            ));
        }

        if seen.contains(&named) {
            return Err(AppError::Invalid(
                Say::of(say::A_KIND_OF_THING_SAYS_THAT_FIELD_TWICE).naming("field", named),
            ));
        }

        if field.kind == Kind::Choice && field.choices.is_empty() {
            return Err(AppError::Invalid(
                Say::of(say::A_CHOICE_FIELD_HAS_NOTHING_TO_CHOOSE_FROM).naming("field", named),
            ));
        }

        seen.push(named);
    }

    Ok(())
}

/// Whether what was written fits what the type says it carries.
///
/// Checked here rather than trusted to whatever wrote it: a field nobody
/// declared is a typo that reads as an empty page, and a required one missing
/// is a template that renders nothing where a price should be.
pub(super) fn fits(declared: &[Field], written: &serde_json::Map<String, Value>) -> Result<()> {
    for (name, value) in written {
        let Some(field) = declared.iter().find(|field| field.name == *name) else {
            return Err(AppError::Invalid(
                Say::of(say::THAT_KIND_OF_THING_HAS_NO_SUCH_FIELD).naming("field", name),
            ));
        };

        let right = match field.kind {
            Kind::Text | Kind::Moment => value.is_string(),
            Kind::Number => value.is_number(),
            Kind::Boolean => value.is_boolean(),
            Kind::Choice => value
                .as_str()
                .is_some_and(|chosen| field.choices.iter().any(|choice| choice == chosen)),
        };

        // Null is how a form says "left empty", and a field that is not
        // required is allowed to be.
        let left_empty = value.is_null() && !field.required;

        if !right && !left_empty {
            return Err(AppError::Invalid(
                Say::of(say::THAT_IS_NOT_WHAT_THAT_FIELD_HOLDS).naming("field", name),
            ));
        }
    }

    for field in declared.iter().filter(|field| field.required) {
        let given = written
            .get(&field.name)
            .is_some_and(|value| !value.is_null());

        if !given {
            return Err(AppError::Invalid(
                Say::of(say::THAT_KIND_OF_THING_WANTS_THAT_FIELD).naming("field", &field.name),
            ));
        }
    }

    Ok(())
}

/// What a type declares, read back for checking or for filtering.
pub(super) async fn declared(conn: &mut TenantConn, key: &str) -> Result<Vec<Field>> {
    let found: Option<(Value,)> = sqlx::query_as("select fields from content_types where key = $1")
        .bind(key)
        .fetch_optional(conn.conn())
        .await?;

    let (fields,) = found.ok_or(AppError::NotFound("content type"))?;

    serde_json::from_value(fields).map_err(|_| AppError::Bug("a declaration that is not one"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_recipe() -> Vec<Field> {
        vec![
            Field {
                name: "minutes".to_owned(),
                label: None,
                kind: Kind::Number,
                required: true,
                choices: Vec::new(),
            },
            Field {
                name: "course".to_owned(),
                label: None,
                kind: Kind::Choice,
                required: false,
                choices: vec!["starter".to_owned(), "main".to_owned()],
            },
        ]
    }

    fn written(json: serde_json::Value) -> serde_json::Map<String, Value> {
        match json {
            Value::Object(map) => map,
            _ => panic!("a test wrote something that is not fields"),
        }
    }

    #[test]
    fn what_the_type_declares_is_what_can_be_written() {
        assert!(fits(&a_recipe(), &written(serde_json::json!({ "minutes": 25 }))).is_ok());

        assert!(
            fits(&a_recipe(), &written(serde_json::json!({ "mintues": 25 }))).is_err(),
            "a typo was stored as a field and would read as an empty page"
        );
    }

    #[test]
    fn a_field_holds_what_it_said_it_holds() {
        assert!(
            fits(
                &a_recipe(),
                &written(serde_json::json!({ "minutes": "25" }))
            )
            .is_err()
        );
        assert!(
            fits(
                &a_recipe(),
                &written(serde_json::json!({ "minutes": 25, "course": "pudding" }))
            )
            .is_err(),
            "something not on the list was chosen"
        );
    }

    #[test]
    fn what_is_wanted_is_wanted() {
        assert!(fits(&a_recipe(), &written(serde_json::json!({}))).is_err());
        assert!(
            fits(
                &a_recipe(),
                &written(serde_json::json!({ "minutes": null }))
            )
            .is_err()
        );
        assert!(
            fits(
                &a_recipe(),
                &written(serde_json::json!({ "minutes": 25, "course": null }))
            )
            .is_ok(),
            "a field that is not required was refused for being empty"
        );
    }

    #[test]
    fn a_declaration_that_cannot_be_filled_in_is_refused() {
        let twice = vec![
            Field {
                name: "minutes".to_owned(),
                label: None,
                kind: Kind::Number,
                required: false,
                choices: Vec::new(),
            },
            Field {
                name: "minutes".to_owned(),
                label: None,
                kind: Kind::Text,
                required: false,
                choices: Vec::new(),
            },
        ];

        assert!(check(&twice).is_err());

        let nothing_to_choose = vec![Field {
            name: "course".to_owned(),
            label: None,
            kind: Kind::Choice,
            required: false,
            choices: Vec::new(),
        }];

        assert!(check(&nothing_to_choose).is_err());

        let shouting = vec![Field {
            name: "Minutes".to_owned(),
            label: None,
            kind: Kind::Number,
            required: false,
            choices: Vec::new(),
        }];

        assert!(check(&shouting).is_err());
    }
}
