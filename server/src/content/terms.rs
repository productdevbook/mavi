//! What a post is filed under.
//!
//! Categories and tags in one table with a kind on it: a post's relationship
//! to either is the same relationship, and two tables would be two things that
//! can disagree. A category sits under another; a tag sits under nothing.
use axum::Json;
use axum::extract::{Path, Query as HttpQuery, State as Injected};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::taxonomy;
use crate::kernel::audit::{self, Actor, Auditable, Audited};
use crate::kernel::authz::{self, Access};
use crate::kernel::db::Tx;
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::page::{Page, Query};
use crate::kernel::say;
use crate::kernel::types::{Slug, Title};

/// A category nests and a tag does not, and that is the whole of the
/// difference. Two tables is how the two came to disagree about a post.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, utoipa::ToSchema,
)]
#[sqlx(type_name = "term_kind", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum TermKind {
    Category,
    Tag,
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Term {
    pub id: Uuid,
    pub kind: TermKind,
    pub language: String,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub parent_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

impl Auditable for Term {
    const SUBJECT: &'static str = "term";

    fn subject_id(&self) -> String {
        self.id.to_string()
    }

    fn summary(&self) -> serde_json::Value {
        serde_json::json!({
            "kind": self.kind,
            "language": self.language,
            "slug": self.slug,
            "name": self.name,
        })
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewTerm {
    pub kind: TermKind,
    pub language: String,
    pub name: Title,
    #[serde(default)]
    pub slug: Option<Slug>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub parent_id: Option<Uuid>,
}

/// What may be changed about one. The kind and the language are not here: a
/// category that becomes a tag is a different thing under the same posts, and
/// making one is how a site says that.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct TermChanges {
    pub name: Option<Title>,
    pub slug: Option<Slug>,
    pub description: Option<String>,
    /// The nil uuid means "under nothing", because a missing field means
    /// "leave it alone" and both have to be sayable.
    pub parent_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct Filter {
    #[serde(flatten)]
    pub page: Query,
    pub kind: Option<TermKind>,
    pub language: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Attachment {
    pub term_ids: Vec<Uuid>,
}

pub(super) fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/terms",
            Guard {
                audience: Audience::User,
                needs: Some(taxonomy(Access::View)),
                rate: RatePolicy::None,
            },
            list,
        )
        .gives::<Page<Term>>(),
        Endpoint::post(
            "/api/terms",
            Guard {
                audience: Audience::User,
                needs: Some(taxonomy(Access::Write)),
                rate: RatePolicy::None,
            },
            create,
        )
        .takes::<NewTerm>()
        .gives::<Term>(),
        Endpoint::patch(
            "/api/terms/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(taxonomy(Access::Write)),
                rate: RatePolicy::None,
            },
            change,
        )
        .takes::<TermChanges>()
        .gives::<Term>(),
        Endpoint::delete(
            "/api/terms/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(taxonomy(Access::Delete)),
                rate: RatePolicy::None,
            },
            remove,
        ),
        Endpoint::put(
            "/api/posts/{id}/terms",
            Guard {
                audience: Audience::User,
                needs: Some(super::needs(Access::Write)),
                rate: RatePolicy::None,
            },
            attach,
        )
        .takes::<Attachment>()
        .gives::<Vec<Term>>(),
    ]
}

async fn list(
    Injected(state): Injected<AppState>,
    _caller: Caller,
    _permit: authz::Permit,
    HttpQuery(filter): HttpQuery<Filter>,
) -> Result<Json<Page<Term>>> {
    let mut conn = state.db.begin().await?;

    let rows: Vec<Term> = sqlx::query_as(
        "select id, kind, language, slug, name, description, parent_id, created_at
           from terms
          where ($1::term_kind is null or kind = $1)
            and ($2::text is null or language = $2)
            and ($3::timestamptz is null or created_at < $3)
          order by created_at desc, id desc
          limit $4",
    )
    .bind(filter.kind)
    .bind(filter.language.as_deref())
    .bind(
        filter
            .page
            .after
            .as_deref()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(DateTime::<Utc>::from),
    )
    .bind(filter.page.fetch())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Page::build(&filter.page, rows, |term| {
        term.created_at.to_rfc3339()
    })))
}

async fn create(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: authz::Permit,
    Json(body): Json<NewTerm>,
) -> Result<Audited<(StatusCode, Json<Term>)>> {
    let mut conn = state.db.begin().await?;

    if body.parent_id.is_some() && body.kind == TermKind::Tag {
        return Err(AppError::Invalid(say::TAG_NOT_SIT_UNDER_ANOTHER.into()));
    }

    let slug = match body.slug {
        Some(slug) => slug,
        None => Slug::parse(Slug::from_title(body.name.as_str()).as_str())
            .map_err(|_| AppError::Invalid(say::NAME_MAKES_NO_ADDRESS.into()))?,
    };

    let term: Term = sqlx::query_as(
        "insert into terms (parent_id, kind, language, slug, name, description)
         values ($1, $2, $3, $4, $5, $6)
         returning id, kind, language, slug, name, description, parent_id, created_at",
    )
    .bind(body.parent_id)
    .bind(body.kind)
    .bind(&body.language)
    .bind(slug.as_str())
    .bind(body.name.as_str())
    .bind(body.description.as_deref())
    .fetch_one(conn.conn())
    .await
    .map_err(|error| {
        match error
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
        {
            Some(code) if code == "23505" => AppError::Conflict(say::NAME_ALREADY_USE.into()),
            _ => AppError::Database(error),
        }
    })?;

    let receipt = audit::record(&mut conn, Actor::of(&caller), "made", None, Some(&term)).await?;
    conn.commit().await?;

    Ok(Audited::new(receipt, (StatusCode::CREATED, Json(term))))
}

/// Renaming one, moving it under another, or writing what it is for.
///
/// The address is a separate decision from the name: a category renamed keeps
/// answering on the address people have linked to unless somebody says
/// otherwise, because a link that stops working is worse than a stale slug.
async fn change(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: authz::Permit,
    Path(id): Path<Uuid>,
    Json(wanted): Json<TermChanges>,
) -> Result<Audited<Json<Term>>> {
    let mut conn = state.db.begin().await?;
    let before = one(&mut conn, id).await?;

    if wanted.parent_id.is_some() && before.kind == TermKind::Tag {
        return Err(AppError::Invalid(say::TAG_NOT_SIT_UNDER_ANOTHER.into()));
    }

    // Under itself is a tree with a loop in it, which is a listing that never
    // ends rather than a mistake anybody sees.
    if wanted.parent_id == Some(id) {
        return Err(AppError::Invalid(
            say::A_THING_DOES_NOT_SIT_UNDER_ITSELF.into(),
        ));
    }

    let after: Term = sqlx::query_as(
        "update terms
            set name = coalesce($2, name),
                slug = coalesce($3, slug),
                description = coalesce($4, description),
                parent_id = case when $5 then null else coalesce($6, parent_id) end
          where id = $1
         returning id, kind, language, slug, name, description, parent_id, created_at",
    )
    .bind(id)
    .bind(wanted.name.as_ref().map(Title::as_str))
    .bind(wanted.slug.as_ref().map(Slug::as_str))
    .bind(wanted.description.as_deref())
    .bind(wanted.parent_id.is_some() && wanted.parent_id == Some(Uuid::nil()))
    .bind(wanted.parent_id.filter(|parent| !parent.is_nil()))
    .fetch_one(conn.conn())
    .await
    .map_err(|error| {
        match error
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
        {
            Some(code) if code == "23505" => AppError::Conflict(say::NAME_ALREADY_USE.into()),
            _ => AppError::Database(error),
        }
    })?;

    let receipt = audit::record(
        &mut conn,
        Actor::of(&caller),
        "changed",
        Some(&before),
        Some(&after),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, Json(after)))
}

async fn remove(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: authz::Permit,
    Path(id): Path<Uuid>,
) -> Result<Audited<StatusCode>> {
    let mut conn = state.db.begin().await?;
    let before = one(&mut conn, id).await?;

    // Hard, not soft: what a term means is the posts under it, and a term
    // nobody can see is not a thing a site restores. The posts stay.
    sqlx::query("delete from terms where id = $1")
        .bind(id)
        .execute(conn.conn())
        .await?;

    let receipt = audit::record(
        &mut conn,
        Actor::of(&caller),
        "removed",
        Some(&before),
        None,
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::NO_CONTENT))
}

async fn one(conn: &mut Tx, id: Uuid) -> Result<Term> {
    sqlx::query_as(
        "select id, kind, language, slug, name, description, parent_id, created_at
           from terms where id = $1",
    )
    .bind(id)
    .fetch_optional(conn.conn())
    .await?
    .ok_or(AppError::NotFound("term"))
}

/// The whole set at once, rather than one at a time: what a post is filed
/// under is one decision, and doing it in pieces leaves it half-made when
/// something goes wrong halfway.
async fn attach(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: authz::Permit,
    Path(id): Path<Uuid>,
    Json(body): Json<Attachment>,
) -> Result<Audited<Json<Vec<Term>>>> {
    let mut conn = state.db.begin().await?;

    let post: Option<(Uuid, Option<Uuid>)> =
        sqlx::query_as("select id, author_id from posts where id = $1 and deleted_at is null")
            .bind(id)
            .fetch_optional(conn.conn())
            .await?;

    let Some((post_id, author_id)) = post else {
        return Err(AppError::NotFound("post"));
    };

    caller.may(
        crate::kernel::authz::Needs::new(crate::kernel::authz::Capability::Content, Access::Write),
        author_id,
    )?;

    sqlx::query("delete from post_terms where post_id = $1")
        .bind(post_id)
        .execute(conn.conn())
        .await?;

    // One statement rather than one per term: a post with forty tags is one
    // round trip either way.
    sqlx::query(
        "insert into post_terms (post_id, term_id)
         select $1, t.id from terms t where t.id = any($2)",
    )
    .bind(post_id)
    .bind(&body.term_ids)
    .execute(conn.conn())
    .await?;

    let now: Vec<Term> = sqlx::query_as(
        "select t.id, t.kind, t.language, t.slug, t.name, t.description, t.parent_id, t.created_at
           from terms t join post_terms pt on pt.term_id = t.id
          where pt.post_id = $1
          order by t.name",
    )
    .bind(post_id)
    .fetch_all(conn.conn())
    .await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "filed",
        "post",
        Some(&post_id.to_string()),
        &serde_json::json!({ "terms": body.term_ids }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, Json(now)))
}
