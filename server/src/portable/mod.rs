//! A site, as a file.
//!
//! What a site holds, written out and read back in: languages, terms, posts.
//! Enough to move a site or to keep a copy, and deliberately not everything —
//! what is left out is written down rather than guessed at.
use crate::kernel::TenantId;
use axum::Json;
use axum::extract::State as Injected;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::collections::HashMap;
use uuid::Uuid;

use crate::kernel::audit::{self, Actor, Audited};
use crate::kernel::authz::{Access, Capability, Needs, Permit};
use crate::kernel::db::TenantConn;
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::say::{self, Say};

/// What an export says it is. A file made by a later version is refused rather
/// than read hopefully: the shapes below are what this understands, and a
/// reader that guesses is a reader that half-imports somebody's site.
pub const VERSION: u32 = 1;

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/portable/export",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::Content, Access::View)),
                rate: RatePolicy::None,
            },
            export,
        )
        .gives::<Bundle>(),
        Endpoint::post(
            "/api/portable/import",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::Content, Access::Write)),
                rate: RatePolicy::None,
            },
            import,
        )
        .takes::<Bundle>()
        .gives::<Read>(),
    ]
}

/// A site's content, as a file somebody can keep.
///
/// Ids travel with it and are not reused: reading a bundle into a site makes
/// new rows and remembers which old id became which new one, so a post that
/// was under a category still is. Reading the same bundle twice makes one
/// copy, because a post is found by its address rather than by its id.
#[derive(Clone, Debug, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub version: u32,
    #[serde(default)]
    pub languages: Vec<Language>,
    #[serde(default)]
    pub terms: Vec<Term>,
    #[serde(default)]
    pub posts: Vec<Post>,
}

#[derive(Clone, Debug, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = BundledLanguage)]
pub struct Language {
    pub code: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = BundledTerm)]
pub struct Term {
    pub id: Uuid,
    pub kind: String,
    pub language: String,
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = BundledPost)]
pub struct Post {
    pub id: Uuid,
    pub kind: String,
    pub state: String,
    pub language: String,
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub excerpt: Option<String>,
    pub body: String,
    #[serde(default)]
    pub fields: serde_json::Value,
    /// What it is filed under, by the ids in this bundle.
    #[serde(default)]
    pub terms: Vec<Uuid>,
}

/// What reading one did.
#[derive(Clone, Debug, Default, Serialize, utoipa::ToSchema)]
pub struct Read {
    pub languages: u64,
    pub terms: u64,
    pub posts: u64,
    /// What was already here under the same address and was left alone.
    pub left_alone: u64,
}

async fn export(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
) -> Result<Json<Bundle>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;
    let bundle = make(&mut conn).await?;
    conn.commit().await?;

    Ok(Json(bundle))
}

/// A site, as a bundle. Reachable from a transfer as well as from a request,
/// which is why it takes a connection rather than a caller.
pub async fn make(conn: &mut TenantConn) -> Result<Bundle> {
    let languages: Vec<Language> =
        sqlx::query("select code, name, is_default from languages order by code")
            .fetch_all(conn.conn())
            .await?
            .iter()
            .map(|row| Language {
                code: row.get("code"),
                name: row.get("name"),
                is_default: row.get("is_default"),
            })
            .collect();

    let terms: Vec<Term> = sqlx::query(
        "select id, kind::text as kind, language, slug, name, description
           from terms order by created_at",
    )
    .fetch_all(conn.conn())
    .await?
    .iter()
    .map(|row| Term {
        id: row.get("id"),
        kind: row.get("kind"),
        language: row.get("language"),
        slug: row.get("slug"),
        name: row.get("name"),
        description: row.get("description"),
    })
    .collect();

    // Every post with what it is filed under, in two queries rather than one
    // per post.
    let rows = sqlx::query(
        "select p.id, p.kind::text as kind, p.state::text as state, p.language, p.slug,
                p.title, p.excerpt, p.body, p.fields,
                coalesce(array_agg(pt.term_id) filter (where pt.term_id is not null), '{}')
                    as terms
           from posts p
           left join post_terms pt on pt.post_id = p.id
          where p.deleted_at is null
          group by p.id
          order by p.created_at",
    )
    .fetch_all(conn.conn())
    .await?;

    let posts: Vec<Post> = rows
        .iter()
        .map(|row| Post {
            id: row.get("id"),
            kind: row.get("kind"),
            state: row.get("state"),
            language: row.get("language"),
            slug: row.get("slug"),
            title: row.get("title"),
            excerpt: row.get("excerpt"),
            body: row.get("body"),
            fields: row.get("fields"),
            terms: row.get("terms"),
        })
        .collect();

    Ok(Bundle {
        version: VERSION,
        languages,
        terms,
        posts,
    })
}

/// Reads a bundle into this site.
///
/// Nothing is overwritten. What is already here under the same address is left
/// alone and counted, so reading the same bundle twice is the same site rather
/// than two of everything — and a bundle from a version this does not know is
/// refused rather than half-read.
async fn import(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Json(bundle): Json<Bundle>,
) -> Result<Audited<(StatusCode, Json<Read>)>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;
    let read = read_bundle(
        &mut conn,
        caller.tenant(),
        caller.user.as_ref().map(|user| user.user_id),
        &bundle,
    )
    .await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "read a bundle in",
        "portable",
        None,
        &serde_json::json!({
            "posts": read.posts, "terms": read.terms, "left_alone": read.left_alone
        }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, (StatusCode::CREATED, Json(read))))
}

/// Reads a bundle into a site, and says what it did. The same work whether a
/// person asked for it or a transfer is doing it.
pub async fn read_bundle(
    conn: &mut TenantConn,
    tenant: TenantId,
    author: Option<Uuid>,
    bundle: &Bundle,
) -> Result<Read> {
    if bundle.version != VERSION {
        return Err(AppError::Invalid(
            Say::of(say::VERSION_READ_IS_NOT_VERSION_GIVEN)
                .naming("reads", VERSION)
                .naming("given", bundle.version),
        ));
    }

    let mut read = Read::default();

    for language in &bundle.languages {
        read.languages += sqlx::query(
            "insert into languages (tenant_id, code, name, is_default)
             values ($1, $2, $3, false)
             on conflict (tenant_id, code) do nothing",
        )
        .bind(tenant.0)
        .bind(&language.code)
        .bind(&language.name)
        .execute(conn.conn())
        .await?
        .rows_affected();
    }

    // Which id in the bundle became which id here.
    let mut became: HashMap<Uuid, Uuid> = HashMap::new();

    for term in &bundle.terms {
        let id = read_term(conn, tenant, term, &mut read).await?;
        became.insert(term.id, id);
    }

    for post in &bundle.posts {
        read_post(conn, tenant, author, post, &became, &mut read).await?;
    }

    Ok(read)
}

async fn read_term(
    conn: &mut TenantConn,
    tenant: TenantId,
    term: &Term,
    read: &mut Read,
) -> Result<Uuid> {
    let existing: Option<(Uuid,)> = sqlx::query_as(
        "select id from terms where kind = $1::term_kind and language = $2 and slug = $3",
    )
    .bind(&term.kind)
    .bind(&term.language)
    .bind(&term.slug)
    .fetch_optional(conn.conn())
    .await?;

    if let Some((id,)) = existing {
        read.left_alone += 1;
        return Ok(id);
    }

    let made: (Uuid,) = sqlx::query_as(
        "insert into terms (tenant_id, kind, language, slug, name, description)
         values ($1, $2::term_kind, $3, $4, $5, $6)
         returning id",
    )
    .bind(tenant.0)
    .bind(&term.kind)
    .bind(&term.language)
    .bind(&term.slug)
    .bind(&term.name)
    .bind(term.description.as_deref())
    .fetch_one(conn.conn())
    .await?;

    read.terms += 1;

    Ok(made.0)
}

async fn read_post(
    conn: &mut TenantConn,
    tenant: TenantId,
    author: Option<Uuid>,
    post: &Post,
    became: &HashMap<Uuid, Uuid>,
    read: &mut Read,
) -> Result<()> {
    let existing: Option<(Uuid,)> = sqlx::query_as(
        "select id from posts where language = $1 and slug = $2 and deleted_at is null",
    )
    .bind(&post.language)
    .bind(&post.slug)
    .fetch_optional(conn.conn())
    .await?;

    if existing.is_some() {
        read.left_alone += 1;
        return Ok(());
    }

    // A post is written in one of the site's own languages. The column is text
    // rather than a key, so nothing stops this but this: a bundle carrying a
    // language the site does not have would otherwise land as content nothing
    // can serve.
    let known: Option<(Uuid,)> = sqlx::query_as("select id from languages where code = $1")
        .bind(&post.language)
        .fetch_optional(conn.conn())
        .await?;

    if known.is_none() {
        return Err(AppError::Invalid(
            Say::of(say::BUNDLE_IN_A_LANGUAGE_THIS_SITE_DOES_NOT_USE)
                .naming("language", &post.language),
        ));
    }

    // Read in as a draft whatever it was: a bundle read into a live site should
    // not publish forty pages the moment it lands.
    let made: (Uuid,) = sqlx::query_as(
        "insert into posts
             (tenant_id, author_id, language, kind, slug, title, excerpt, body, fields)
         values ($1, $2, $3, $4::post_kind, $5, $6, $7, $8, $9)
         returning id",
    )
    .bind(tenant.0)
    .bind(author)
    .bind(&post.language)
    .bind(&post.kind)
    .bind(&post.slug)
    .bind(&post.title)
    .bind(post.excerpt.as_deref())
    .bind(&post.body)
    .bind(&post.fields)
    .fetch_one(conn.conn())
    .await
    .map_err(|error| {
        match error
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
        {
            Some(code) if code == "23503" => {
                AppError::Invalid(say::BUNDLE_WRITTEN_LANGUAGE_SITE_NOT.into())
            }
            _ => AppError::Database(error),
        }
    })?;

    for term in &post.terms {
        // A term the bundle did not carry is skipped rather than invented.
        let Some(here) = became.get(term) else {
            continue;
        };

        sqlx::query(
            "insert into post_terms (post_id, term_id, tenant_id) values ($1, $2, $3)
             on conflict do nothing",
        )
        .bind(made.0)
        .bind(here)
        .bind(tenant.0)
        .execute(conn.conn())
        .await?;
    }

    read.posts += 1;

    Ok(())
}
