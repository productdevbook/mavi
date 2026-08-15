//! Posts and pages.
//!
//! One table: a post is in the feed and a page is not, and everything else
//! about them is the same. A site's own kind of thing is a post with a `type`
//! on it, and what that kind declares is what may be written in its fields.
//!
//! The same writing in two languages is one group rather than two unrelated
//! rows, one post per language.
use axum::Json;
use axum::extract::{Path, Query as HttpQuery, State as Injected};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use super::needs;
use crate::kernel::audit::{self, Actor, Auditable, Audited};
use crate::kernel::authz::{self, Access, Capability, Needs};
use crate::kernel::db::TenantConn;
use crate::kernel::error::{AppError, Result};
use crate::kernel::events::{self, EmitsEvents};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::page::{Page, Query};
use crate::kernel::say::{self, Say};
use crate::kernel::types::{Slug, Title};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, utoipa::ToSchema,
)]
#[sqlx(type_name = "post_state", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum State {
    Draft,
    Scheduled,
    Published,
    Archived,
}

impl State {
    /// The whole machine, in one place. A move that is not here is not a move,
    /// and the check constraint in the schema is written from the same list.
    #[must_use]
    pub fn may_become(self, next: Self) -> bool {
        matches!(
            (self, next),
            (State::Draft, State::Scheduled | State::Published)
                | (State::Scheduled, State::Draft | State::Published)
                | (State::Published, State::Draft | State::Archived)
                | (State::Archived, State::Draft)
        )
    }
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, utoipa::ToSchema,
)]
#[sqlx(type_name = "post_kind", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Post,
    Page,
}

#[derive(Clone, Debug, Serialize, utoipa::ToSchema)]
pub struct Post {
    pub id: Uuid,
    pub kind: Kind,
    pub state: State,
    pub language: String,
    pub slug: String,
    pub title: String,
    pub excerpt: Option<String>,
    pub body: String,
    pub fields: serde_json::Value,
    /// A site's own kind of thing, by name, where it is one.
    #[serde(rename = "type")]
    pub type_key: Option<String>,
    pub author_id: Option<Uuid>,
    /// The picture that goes with it, where a site has chosen one.
    pub cover_media_id: Option<Uuid>,
    /// What a search engine and a chat app show. Both fall back to the title
    /// and the excerpt, which is what a site that says nothing here wants.
    pub seo_title: Option<String>,
    pub seo_description: Option<String>,
    /// Where this was published first, when that was somewhere else.
    pub canonical: Option<String>,
    /// The post this is a translation of, where it is one. What links a group
    /// together; which of them is the original means nothing beyond being
    /// written first.
    pub translation_of: Option<Uuid>,
    pub published_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl Auditable for Post {
    const SUBJECT: &'static str = "post";

    fn subject_id(&self) -> String {
        self.id.to_string()
    }

    fn summary(&self) -> serde_json::Value {
        serde_json::json!({
            "kind": self.kind,
            "state": self.state,
            "language": self.language,
            "slug": self.slug,
            "title": self.title,
        })
    }
}

impl EmitsEvents for Post {
    const EVENTS: &'static [&'static str] = &["post.published", "post.unpublished"];

    fn subject_id(&self) -> String {
        self.id.to_string()
    }

    fn payload(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "kind": self.kind,
            "language": self.language,
            "slug": self.slug,
            "title": self.title,
        })
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewPost {
    pub language: String,
    pub title: Title,
    #[serde(default)]
    pub slug: Option<Slug>,
    #[serde(default)]
    pub kind: Option<Kind>,
    #[serde(default)]
    pub excerpt: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub fields: Option<serde_json::Map<String, serde_json::Value>>,
    /// A site's own kind of thing, by name. What it declares is what may be
    /// written in `fields`.
    #[serde(default, rename = "type")]
    pub type_key: Option<String>,
    #[serde(default)]
    pub cover_media_id: Option<Uuid>,
    #[serde(default)]
    pub seo_title: Option<String>,
    #[serde(default)]
    pub seo_description: Option<String>,
    #[serde(default)]
    pub canonical: Option<String>,
    /// Writing this as another language's version of something already here.
    #[serde(default)]
    pub translation_of: Option<Uuid>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PostChanges {
    pub title: Option<Title>,
    pub slug: Option<Slug>,
    pub excerpt: Option<String>,
    pub body: Option<String>,
    pub fields: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default, rename = "type")]
    pub type_key: Option<String>,
    pub state: Option<State>,
    /// When a scheduled post should go. Required to schedule one, ignored
    /// otherwise.
    pub publish_at: Option<DateTime<Utc>>,
    pub cover_media_id: Option<Uuid>,
    pub seo_title: Option<String>,
    pub seo_description: Option<String>,
    pub canonical: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Filter {
    #[serde(flatten)]
    pub page: Query,
    pub kind: Option<Kind>,
    /// A site's own kind of thing, by name.
    #[serde(rename = "type")]
    pub type_key: Option<String>,
    /// One of that kind's declared fields, and what to ask about it. A field
    /// nothing declared is refused rather than quietly matching nothing.
    pub field: Option<String>,
    /// Equal to this.
    pub is: Option<String>,
    /// A number, at most this. For "every recipe under thirty minutes".
    pub at_most: Option<f64>,
    pub at_least: Option<f64>,
    pub state: Option<State>,
    pub language: Option<String>,
    /// A category or a tag, by its id. One name for both, because a post's
    /// relationship to either is the same relationship.
    pub term: Option<Uuid>,
}

pub(super) fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/posts",
            Guard {
                audience: Audience::User,
                needs: Some(needs(Access::View)),
                rate: RatePolicy::None,
            },
            list,
        )
        .gives::<Page<Post>>(),
        Endpoint::post(
            "/api/posts",
            Guard {
                audience: Audience::User,
                needs: Some(needs(Access::Write)),
                rate: RatePolicy::None,
            },
            create,
        )
        .takes::<NewPost>()
        .gives::<Post>(),
        Endpoint::get(
            "/api/posts/counts",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::Content, Access::View)),
                rate: RatePolicy::None,
            },
            counts,
        )
        .gives::<Counts>(),
        Endpoint::post(
            "/api/posts/actions",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::Content, Access::Write)),
                rate: RatePolicy::None,
            },
            act_on_many,
        )
        .takes::<ActOnMany>()
        .gives::<Acted>(),
        Endpoint::get(
            "/api/posts/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(needs(Access::View)),
                rate: RatePolicy::None,
            },
            read,
        )
        .gives::<Whole>(),
        Endpoint::patch(
            "/api/posts/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(needs(Access::Write)),
                rate: RatePolicy::None,
            },
            update,
        )
        .takes::<PostChanges>()
        .gives::<Post>(),
        Endpoint::delete(
            "/api/posts/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(needs(Access::Delete)),
                rate: RatePolicy::None,
            },
            remove,
        ),
    ]
}

async fn list(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: authz::Permit,
    HttpQuery(filter): HttpQuery<Filter>,
) -> Result<Json<Page<Post>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;
    let asking = asked_about(&mut conn, &filter).await?;

    let rows: Vec<Post> = sqlx::query_as(
        "select p.id, p.kind, p.state, p.language, p.slug, p.title, p.excerpt,
                p.body, p.fields, p.type_key, p.author_id, p.cover_media_id, p.seo_title,
                p.seo_description, p.canonical, p.translation_of, p.published_at,
                p.created_at
           from posts p
          where p.deleted_at is null
            and ($1::post_kind is null or p.kind = $1)
            and ($2::post_state is null or p.state = $2)
            and ($3::text is null or p.language = $3)
            and ($4::uuid is null or exists (
                    select 1 from post_terms t
                     where t.post_id = p.id and t.term_id = $4
                ))
            and ($5::timestamptz is null or p.created_at < $5)
            and ($7::text is null or p.type_key = $7)
            -- One of the type's own fields, asked about by name. The name is
            -- checked against what the type declares before it gets here, so
            -- this reaches into the jsonb rather than into the query.
            and ($8::text is null or p.fields ? $8)
            and ($9::text is null or p.fields ->> $8 = $9)
            and ($10::double precision is null
                 or (p.fields ->> $8)::double precision <= $10)
            and ($11::double precision is null
                 or (p.fields ->> $8)::double precision >= $11)
          order by p.created_at desc, p.id desc
          limit $6",
    )
    .bind(filter.kind)
    .bind(filter.state)
    .bind(filter.language.as_deref())
    .bind(filter.term)
    .bind(cursor(filter.page.after.as_deref()))
    .bind(filter.page.fetch())
    .bind(filter.type_key.as_deref())
    .bind(asking)
    .bind(filter.is.as_deref())
    .bind(filter.at_most)
    .bind(filter.at_least)
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Page::build(&filter.page, rows, |post| {
        post.created_at.to_rfc3339()
    })))
}

/// Which field a listing is asking about, if it is asking about one.
///
/// A name nothing declared is refused rather than quietly matching nothing: a
/// filter that silently returns everything is a screen that shows the wrong
/// posts and says nothing about it.
async fn asked_about(conn: &mut TenantConn, filter: &Filter) -> Result<Option<String>> {
    let Some(field) = filter.field.as_deref() else {
        return Ok(None);
    };

    let key = filter.type_key.as_deref().ok_or_else(|| {
        AppError::Invalid(say::ASKING_ABOUT_A_FIELD_MEANS_SAYING_WHICH_KIND.into())
    })?;

    let ways = usize::from(filter.is.is_some())
        + usize::from(filter.at_most.is_some())
        + usize::from(filter.at_least.is_some());

    if ways > 1 {
        return Err(AppError::Invalid(
            say::A_FIELD_IS_ASKED_ABOUT_ONE_WAY_AT_A_TIME.into(),
        ));
    }

    let declared = super::types::declared(conn, key).await?;

    if !declared.iter().any(|one| one.name == field) {
        return Err(AppError::Invalid(
            Say::of(say::NOTHING_CAN_BE_ASKED_ABOUT_THAT_FIELD).naming("field", field),
        ));
    }

    Ok(Some(field.to_owned()))
}

/// What is written into a post's own fields is what its kind of thing says it
/// carries — checked against the kind it is becoming, where that is what is
/// changing.
async fn still_fits(conn: &mut TenantConn, before: &Post, changes: &PostChanges) -> Result<()> {
    if changes.fields.is_none() && changes.type_key.is_none() {
        return Ok(());
    }

    let key = changes
        .type_key
        .as_deref()
        .or(before.type_key.as_deref())
        .map(str::to_owned);

    let empty = serde_json::Map::new();
    let written = changes.fields.as_ref().unwrap_or(&empty);

    match key {
        Some(key) => super::types::fits(&super::types::declared(conn, &key).await?, written),
        None if written.is_empty() => Ok(()),
        None => Err(AppError::Invalid(
            say::FIELDS_BELONG_TO_A_KIND_OF_THING.into(),
        )),
    }
}

fn cursor(after: Option<&str>) -> Option<DateTime<Utc>> {
    after.and_then(|value| DateTime::parse_from_rfc3339(value).ok().map(Into::into))
}

/// What the tool surface calls. One door into the same work: a post written by
/// an assistant is a post, with the same checks and the same audit row.
pub(super) async fn write_through_a_tool(
    state: &AppState,
    caller: &Caller,
    arguments: &serde_json::Value,
) -> Result<serde_json::Value> {
    let asked: NewPost = serde_json::from_value(arguments.clone())
        .map_err(|_| AppError::Invalid(say::NOT_SOMETHING_A_POST_IS_MADE_OF.into()))?;

    let author = caller.require_user()?;
    let mut conn = state.db.tenant(caller.tenant()).await?;

    known_language(&mut conn, &asked.language).await?;

    if let Some(key) = asked.type_key.as_deref() {
        let declared = super::types::declared(&mut conn, key).await?;
        let empty = serde_json::Map::new();
        super::types::fits(&declared, asked.fields.as_ref().unwrap_or(&empty))?;
    }

    let slug = match &asked.slug {
        Some(slug) => slug.clone(),
        None => Slug::parse(Slug::from_title(asked.title.as_str()).as_str())
            .map_err(|_| AppError::Invalid(say::TITLE_MAKES_NO_ADDRESS.into()))?,
    };

    let post: Post = sqlx::query_as(
        "insert into posts
             (tenant_id, author_id, language, kind, slug, title, excerpt, body, fields,
              type_key, cover_media_id, seo_title, seo_description, canonical,
              translation_of)
         values ($1, $2, $3, coalesce($4, 'post'), $5, $6, $7, coalesce($8, ''),
                 coalesce($9, '{}'::jsonb), $10, $11, $12, $13, $14,
                 -- Pointed at the original rather than at another translation,
                 -- so a group is one level deep however it was made.
                 (select coalesce(o.translation_of, o.id) from posts o
                   where o.id = $15))
         returning id, kind, state, language, slug, title, excerpt, body, fields,
                   type_key, author_id, cover_media_id, seo_title, seo_description,
                   canonical, translation_of, published_at, created_at",
    )
    .bind(caller.tenant().0)
    .bind(author.user_id)
    .bind(&asked.language)
    .bind(asked.kind)
    .bind(slug.as_str())
    .bind(asked.title.as_str())
    .bind(asked.excerpt.as_deref())
    .bind(asked.body.as_deref())
    .bind(asked.fields.clone().map(serde_json::Value::Object))
    .bind(asked.type_key.as_deref())
    .bind(asked.cover_media_id)
    .bind(asked.seo_title.as_deref())
    .bind(asked.seo_description.as_deref())
    .bind(asked.canonical.as_deref())
    .bind(asked.translation_of)
    .fetch_one(conn.conn())
    .await
    .map_err(taken)?;

    crate::pages::look_at(&mut conn, caller.tenant(), post.id).await?;

    audit::record(&mut conn, Actor::of(caller), "wrote", None, Some(&post)).await?;

    conn.commit().await?;

    Ok(serde_json::json!({ "id": post.id, "slug": post.slug, "state": post.state }))
}

/// Changing a post through a tool.
///
/// The id travels with the changes rather than in a path, because a tool has
/// no path; everything after that is the route's own work, so a post cannot be
/// moved into a state through a tool that a person could not move it into.
pub(super) async fn change_through_a_tool(
    state: &AppState,
    caller: &Caller,
    arguments: &serde_json::Value,
) -> Result<serde_json::Value> {
    #[derive(Deserialize)]
    struct Asked {
        id: Uuid,
        #[serde(flatten)]
        changes: PostChanges,
    }

    let asked: Asked = serde_json::from_value(arguments.clone())
        .map_err(|_| AppError::Invalid(say::NOT_SOMETHING_A_POST_IS_MADE_OF.into()))?;

    let mut conn = state.db.tenant(caller.tenant()).await?;

    let (before, after) = changed(state, caller, &mut conn, asked.id, &asked.changes).await?;

    audit::record(
        &mut conn,
        Actor::of(caller),
        "changed",
        Some(&before),
        Some(&after),
    )
    .await?;

    conn.commit().await?;

    Ok(serde_json::json!({ "id": after.id, "slug": after.slug, "state": after.state }))
}

async fn create(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: authz::Permit,
    Json(body): Json<NewPost>,
) -> Result<Audited<(StatusCode, Json<Post>)>> {
    let author = caller.require_user()?;
    let mut conn = state.db.tenant(caller.tenant()).await?;

    known_language(&mut conn, &body.language).await?;

    if let Some(key) = body.type_key.as_deref() {
        let declared = super::types::declared(&mut conn, key).await?;
        let empty = serde_json::Map::new();
        super::types::fits(&declared, body.fields.as_ref().unwrap_or(&empty))?;
    } else if body
        .fields
        .as_ref()
        .is_some_and(|fields| !fields.is_empty())
    {
        return Err(AppError::Invalid(
            say::FIELDS_BELONG_TO_A_KIND_OF_THING.into(),
        ));
    }

    let slug = match body.slug {
        Some(slug) => slug,
        None => Slug::parse(Slug::from_title(body.title.as_str()).as_str())
            .map_err(|_| AppError::Invalid(say::TITLE_MAKES_NO_ADDRESS.into()))?,
    };

    let post: Post = sqlx::query_as(
        "insert into posts
             (tenant_id, author_id, language, kind, slug, title, excerpt, body, fields,
              type_key, cover_media_id, seo_title, seo_description, canonical,
              translation_of)
         values ($1, $2, $3, coalesce($4, 'post'), $5, $6, $7, coalesce($8, ''),
                 coalesce($9, '{}'::jsonb), $10, $11, $12, $13, $14,
                 -- Pointed at the original rather than at another translation,
                 -- so a group is one level deep however it was made.
                 (select coalesce(o.translation_of, o.id) from posts o
                   where o.id = $15))
         returning id, kind, state, language, slug, title, excerpt, body, fields,
                   type_key, author_id, cover_media_id, seo_title, seo_description,
                   canonical, translation_of, published_at, created_at",
    )
    .bind(caller.tenant().0)
    .bind(author.user_id)
    .bind(&body.language)
    .bind(body.kind)
    .bind(slug.as_str())
    .bind(body.title.as_str())
    .bind(body.excerpt.as_deref())
    .bind(body.body.as_deref())
    .bind(body.fields.map(serde_json::Value::Object))
    .bind(body.type_key.as_deref())
    .bind(body.cover_media_id)
    .bind(body.seo_title.as_deref())
    .bind(body.seo_description.as_deref())
    .bind(body.canonical.as_deref())
    .bind(body.translation_of)
    .fetch_one(conn.conn())
    .await
    .map_err(taken)?;

    crate::pages::look_at(&mut conn, caller.tenant(), post.id).await?;

    let receipt = audit::record(&mut conn, Actor::of(&caller), "wrote", None, Some(&post)).await?;
    conn.commit().await?;

    Ok(Audited::new(receipt, (StatusCode::CREATED, Json(post))))
}

/// A unique violation on the slug is a collision, not a failure: two posts in
/// one language cannot answer on one address.
fn taken(error: sqlx::Error) -> AppError {
    match error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
    {
        Some(code) if code == "23505" => {
            AppError::Conflict(say::SOMETHING_ALREADY_ANSWERS_ON_ADDRESS.into())
        }
        _ => AppError::Database(error),
    }
}

async fn known_language(conn: &mut TenantConn, code: &str) -> Result<()> {
    let known: Option<(Uuid,)> = sqlx::query_as("select id from languages where code = $1")
        .bind(code)
        .fetch_optional(conn.conn())
        .await?;

    known
        .map(|_| ())
        .ok_or(AppError::Invalid(say::SITE_NOT_WRITE_LANGUAGE.into()))
}

/// One post, and what else it is: the terms it is filed under and the same
/// writing in other languages.
///
/// Together rather than as three requests: an editor opening a post needs all
/// of it, and three round trips is three chances to draw half a screen.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[schema(as = WholePost)]
pub struct Whole {
    pub post: Post,
    pub term_ids: Vec<Uuid>,
    pub translations: Vec<Translation>,
}

/// The same writing, in another language.
#[derive(Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Translation {
    pub id: Uuid,
    pub language: String,
    pub title: String,
    pub state: State,
}

async fn read(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: authz::Permit,
    Path(id): Path<Uuid>,
) -> Result<Json<Whole>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;
    let post = one(&mut conn, id).await?;

    let term_ids: Vec<(Uuid,)> =
        sqlx::query_as("select term_id from post_terms where post_id = $1")
            .bind(id)
            .fetch_all(conn.conn())
            .await?;

    // The group is whichever post is the original, plus everything pointing at
    // it — asked for from either end, because an editor may have opened any of
    // them.
    let translations: Vec<Translation> = sqlx::query_as(
        "select id, language, title, state from posts
          where deleted_at is null and id <> $1
            and coalesce(translation_of, id) = $2
          order by language",
    )
    .bind(id)
    .bind(post.translation_of.unwrap_or(post.id))
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Whole {
        post,
        term_ids: term_ids.into_iter().map(|(id,)| id).collect(),
        translations,
    }))
}

async fn one(conn: &mut TenantConn, id: Uuid) -> Result<Post> {
    sqlx::query_as(
        "select id, kind, state, language, slug, title, excerpt, body, fields,
                type_key, author_id, cover_media_id, seo_title, seo_description,
                canonical, translation_of, published_at, created_at
           from posts where id = $1 and deleted_at is null",
    )
    .bind(id)
    .fetch_optional(conn.conn())
    .await?
    .ok_or(AppError::NotFound("post"))
}

async fn update(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: authz::Permit,
    Path(id): Path<Uuid>,
    Json(changes): Json<PostChanges>,
) -> Result<Audited<Json<Post>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let (before, after) = changed(&state, &caller, &mut conn, id, &changes).await?;

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

/// Changing a post, wherever the asking came from.
///
/// The route and the tool both end up here, so that whose a post is, which
/// states it may move between, and what its kind of thing allows are decided
/// in one place — three copies of a state machine is how a post comes to be
/// published by the way somebody asked.
async fn changed(
    state: &AppState,
    caller: &Caller,
    conn: &mut TenantConn,
    id: Uuid,
    changes: &PostChanges,
) -> Result<(Post, Post)> {
    let before = one(conn, id).await?;

    // Asked again with the record in hand: the guard on the route knows the
    // capability, and only this knows whose the post is.
    caller.may(
        Needs::new(Capability::Content, Access::Write),
        before.author_id,
    )?;

    if let Some(next) = changes.state
        && next != before.state
        && !before.state.may_become(next)
    {
        return Err(AppError::Conflict(
            Say::of(say::POST_DOES_NOT_BECOME_THAT)
                .naming("from", format!("{:?}", before.state))
                .naming("to", format!("{next:?}")),
        ));
    }

    // Scheduling is a state and a moment. Without one it is a post that waits
    // for nothing, which is a draft with a misleading label.
    if changes.state == Some(State::Scheduled) {
        let when = changes
            .publish_at
            .ok_or_else(|| AppError::Invalid(say::SCHEDULED_FOR_WHEN.into()))?;

        if when <= state.clock.now() {
            return Err(AppError::Invalid(say::MOMENT_ALREADY_PASSED.into()));
        }
    }

    still_fits(conn, &before, changes).await?;

    // What was linked to keeps working: the old address is left behind
    // pointing at the new one.
    if let Some(slug) = changes.slug.as_ref()
        && slug.as_str() != before.slug
    {
        sqlx::query(
            "insert into redirects (tenant_id, post_id, language, was, now_at)
             values ($1, $2, $3, $4, $5)
             on conflict (tenant_id, language, was) do update set now_at = excluded.now_at",
        )
        .bind(caller.tenant().0)
        .bind(id)
        .bind(&before.language)
        .bind(&before.slug)
        .bind(slug.as_str())
        .execute(conn.conn())
        .await?;
    }

    let after: Post = sqlx::query_as(
        "update posts
            set title = coalesce($2, title),
                slug = coalesce($3, slug),
                excerpt = coalesce($4, excerpt),
                body = coalesce($5, body),
                fields = coalesce($6, fields),
                type_key = coalesce($9, type_key),
                cover_media_id = coalesce($10, cover_media_id),
                seo_title = coalesce($11, seo_title),
                seo_description = coalesce($12, seo_description),
                canonical = coalesce($13, canonical),
                state = coalesce($7, state),
                publish_at = case
                    when $7 = 'scheduled'::post_state then $8
                    when $7 is not null then null
                    else publish_at
                end,
                published_at = case
                    when $7 = 'published'::post_state then coalesce(published_at, now())
                    when $7 is not null and $7 <> 'published'::post_state then null
                    else published_at
                end
          where id = $1 and deleted_at is null
         returning id, kind, state, language, slug, title, excerpt, body, fields,
                   type_key, author_id, cover_media_id, seo_title, seo_description,
                   canonical, translation_of, published_at, created_at",
    )
    .bind(id)
    .bind(changes.title.as_ref().map(Title::as_str))
    .bind(changes.slug.as_ref().map(Slug::as_str))
    .bind(changes.excerpt.as_deref())
    .bind(changes.body.as_deref())
    .bind(changes.fields.clone().map(serde_json::Value::Object))
    .bind(changes.state)
    .bind(changes.publish_at)
    .bind(changes.type_key.as_deref())
    .bind(changes.cover_media_id)
    .bind(changes.seo_title.as_deref())
    .bind(changes.seo_description.as_deref())
    .bind(changes.canonical.as_deref())
    .fetch_one(conn.conn())
    .await
    .map_err(taken)?;

    if before.state != after.state {
        match after.state {
            State::Published => events::emit(conn, "post.published", &after).await?,
            _ if before.state == State::Published => {
                events::emit(conn, "post.unpublished", &after).await?
            }
            _ => Uuid::nil(),
        };
    }

    crate::pages::look_at(conn, caller.tenant(), after.id).await?;

    Ok((before, after))
}

/// How many there are in each state.
///
/// Counted by the database rather than by counting the rows on a screen, which
/// would report the size of a page rather than the size of the archive. Always
/// for one language and one kind, because a total across three languages reads
/// as three times as much writing as there is.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Counts {
    pub draft: i64,
    pub scheduled: i64,
    pub published: i64,
    pub archived: i64,
}

async fn counts(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: authz::Permit,
    HttpQuery(filter): HttpQuery<Filter>,
) -> Result<Json<Counts>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let counted: (i64, i64, i64, i64) = sqlx::query_as(
        "select count(*) filter (where state = 'draft'),
                count(*) filter (where state = 'scheduled'),
                count(*) filter (where state = 'published'),
                count(*) filter (where state = 'archived')
           from posts
          where deleted_at is null
            and ($1::post_kind is null or kind = $1)
            and ($2::text is null or language = $2)",
    )
    .bind(filter.kind)
    .bind(filter.language.as_deref())
    .fetch_one(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Counts {
        draft: counted.0,
        scheduled: counted.1,
        published: counted.2,
        archived: counted.3,
    }))
}

/// What to do to a batch, and to which.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ActOnMany {
    /// `publish`, `unpublish`, `trash` or `restore`. Named rather than a state
    /// to set, because restoring is not a state and trashing is not either.
    pub act: String,
    pub ids: Vec<Uuid>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Acted {
    pub acted_on: i64,
    /// What was asked for and did not happen — already in that state, gone, or
    /// somebody else's to change. Named so a screen can say which rather than
    /// only how many.
    pub left_alone: Vec<Uuid>,
}

/// How many at once. A site with ten thousand posts wants them in batches
/// rather than one request holding a transaction open over all of them.
const AT_MOST: usize = 200;

/// One act, on many posts.
///
/// Each is checked the way one would be: what somebody may change about their
/// own is what they may change here, and a post they may not touch is left
/// alone and named rather than quietly counted as done.
async fn act_on_many(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: authz::Permit,
    Json(asked): Json<ActOnMany>,
) -> Result<Audited<Json<Acted>>> {
    if !matches!(
        asked.act.as_str(),
        "publish" | "unpublish" | "trash" | "restore"
    ) {
        return Err(AppError::Invalid(say::NOT_SOMETHING_TO_DO_TO_A_POST.into()));
    }

    if asked.ids.is_empty() || asked.ids.len() > AT_MOST {
        return Err(AppError::Invalid(
            Say::of(say::TOO_MANY_AT_ONCE).naming("most", AT_MOST),
        ));
    }

    let needed = if asked.act == "trash" {
        Access::Delete
    } else {
        Access::Write
    };

    let mut conn = state.db.tenant(caller.tenant()).await?;

    // Whose they are decides which of them this may touch, so it is asked
    // before anything is written rather than one row at a time.
    let theirs: Vec<(Uuid, Option<Uuid>)> =
        sqlx::query_as("select id, author_id from posts where id = any($1)")
            .bind(&asked.ids)
            .fetch_all(conn.conn())
            .await?;

    let mut allowed: Vec<Uuid> = Vec::with_capacity(theirs.len());
    let mut left_alone: Vec<Uuid> = Vec::new();

    for (id, author_id) in theirs {
        if caller
            .may(Needs::new(Capability::Content, needed), author_id)
            .is_ok()
        {
            allowed.push(id);
        } else {
            left_alone.push(id);
        }
    }

    // Anything the site does not have at all is left alone as well, so the two
    // lists together account for everything that was asked about.
    let found: std::collections::HashSet<Uuid> =
        allowed.iter().chain(left_alone.iter()).copied().collect();

    left_alone.extend(asked.ids.iter().filter(|id| !found.contains(id)));

    let acted_on = match asked.act.as_str() {
        "publish" => sqlx::query(
            "update posts
                    set state = 'published', published_at = coalesce(published_at, now())
                  where id = any($1) and deleted_at is null and state <> 'published'",
        ),
        "unpublish" => sqlx::query(
            "update posts set state = 'draft'
              where id = any($1) and deleted_at is null and state = 'published'",
        ),
        "trash" => sqlx::query(
            "update posts set deleted_at = now()
              where id = any($1) and deleted_at is null",
        ),
        _ => sqlx::query(
            "update posts set deleted_at = null
              where id = any($1) and deleted_at is not null",
        ),
    }
    .bind(&allowed)
    .execute(conn.conn())
    .await?
    .rows_affected();

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "acted on many posts",
        "post",
        None,
        &serde_json::json!({
            "act": asked.act,
            "acted_on": acted_on,
            "left_alone": left_alone.len(),
        }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(
        receipt,
        Json(Acted {
            acted_on: i64::try_from(acted_on).unwrap_or(i64::MAX),
            left_alone,
        }),
    ))
}

async fn remove(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: authz::Permit,
    Path(id): Path<Uuid>,
) -> Result<Audited<StatusCode>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;
    let before = one(&mut conn, id).await?;

    caller.may(
        Needs::new(Capability::Content, Access::Delete),
        before.author_id,
    )?;

    sqlx::query("update posts set deleted_at = now() where id = $1")
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

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PublishDue;

impl crate::kernel::queue::Task for PublishDue {
    const KIND: &'static str = "content.publish-due";
}

/// Posts whose moment has come.
///
/// Written down as the moment it was scheduled for and the moment it actually
/// went, because a post that went a day late because nothing was running is
/// something somebody has to be able to see rather than guess at.
pub async fn publish_due(state: &AppState, tenant: crate::kernel::TenantId) -> Result<u64> {
    let mut conn = state.db.tenant(tenant).await?;

    let due: Vec<Post> = sqlx::query_as(
        "update posts
            set state = 'published', published_at = now()
          where state = 'scheduled' and publish_at <= now() and deleted_at is null
         returning id, kind, state, language, slug, title, excerpt, body, fields,
                   type_key, author_id, cover_media_id, seo_title, seo_description,
                   canonical, translation_of, published_at, created_at",
    )
    .fetch_all(conn.conn())
    .await?;

    for post in &due {
        events::emit(&mut conn, "post.published", post).await?;

        audit::record_raw(
            &mut conn,
            Actor::system(crate::kernel::http::RequestId(Uuid::now_v7())),
            "published on schedule",
            "post",
            Some(&post.id.to_string()),
            &serde_json::json!({ "slug": post.slug }),
        )
        .await?;
    }

    conn.commit().await?;

    Ok(due.len() as u64)
}

impl sqlx::FromRow<'_, sqlx::postgres::PgRow> for Post {
    fn from_row(row: &sqlx::postgres::PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id")?,
            kind: row.try_get("kind")?,
            state: row.try_get("state")?,
            language: row.try_get("language")?,
            slug: row.try_get("slug")?,
            title: row.try_get("title")?,
            excerpt: row.try_get("excerpt")?,
            body: row.try_get("body")?,
            fields: row.try_get("fields")?,
            type_key: row.try_get("type_key")?,
            author_id: row.try_get("author_id")?,
            cover_media_id: row.try_get("cover_media_id")?,
            seo_title: row.try_get("seo_title")?,
            seo_description: row.try_get("seo_description")?,
            canonical: row.try_get("canonical")?,
            translation_of: row.try_get("translation_of")?,
            published_at: row.try_get("published_at")?,
            created_at: row.try_get("created_at")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_post_moves_only_the_ways_the_machine_allows() {
        assert!(State::Draft.may_become(State::Published));
        assert!(State::Published.may_become(State::Archived));
        assert!(State::Archived.may_become(State::Draft));

        assert!(!State::Draft.may_become(State::Archived));
        assert!(!State::Archived.may_become(State::Published));
        assert!(!State::Scheduled.may_become(State::Archived));
    }
}
