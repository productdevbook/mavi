//! What is wrong with a page, before anybody notices.
//!
//! A missing description, a title nothing would click, an address two things
//! answer on. Looked at when a post is written rather than asked for, so the
//! answer is there when somebody opens the screen.
use axum::Json;
use axum::extract::{Path, State as Injected};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::kernel::TenantId;
use crate::kernel::authz::{Access, Capability, Needs, Permit};
use crate::kernel::db::TenantConn;
use crate::kernel::error::Result;
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
// Aliased: what is being checked here is also called a page, and the two are
// not the same thing.
use crate::kernel::page::{Page as Listing, Query, older_than};

/// What a title should be, in characters. Longer than this and a search engine
/// stops showing the end of it.
const TITLE: std::ops::Range<usize> = 10..70;

/// And a summary.
const EXCERPT: std::ops::Range<usize> = 50..160;

/// What a page has to say before it is worth reading.
const SHORTEST_BODY: usize = 300;

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/pages/issues",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::Content, Access::View)),
                rate: RatePolicy::None,
            },
            all,
        )
        .gives::<Listing<Issue>>(),
        Endpoint::get(
            "/api/posts/{id}/issues",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::Content, Access::View)),
                rate: RatePolicy::None,
            },
            of_post,
        )
        .gives::<Vec<Issue>>(),
    ]
}

#[derive(Clone, Debug, Serialize, utoipa::ToSchema)]
pub struct Issue {
    pub post_id: Uuid,
    pub title: String,
    /// What is wrong, as a name rather than a sentence: the sentence belongs to
    /// whatever is showing it, in whatever language it is being shown in.
    pub kind: String,
    pub weight: String,
    pub detail: serde_json::Value,
    /// When the page it is about was written. What a listing is ordered and
    /// paged by, and what a screen shows beside it.
    pub written_at: DateTime<Utc>,
}

/// One thing that might be wrong with a page.
struct Check {
    kind: &'static str,
    weight: &'static str,
    /// What it found, or nothing where there is nothing wrong.
    look: fn(&Page) -> Option<serde_json::Value>,
}

/// What a check is given: a page, as it would be read.
#[derive(Debug)]
pub struct Page {
    pub title: String,
    pub excerpt: Option<String>,
    pub body: String,
    pub slug: String,
}

const CHECKS: &[Check] = &[
    Check {
        kind: "title.short",
        weight: "warning",
        look: |page| {
            let length = page.title.chars().count();
            (length < TITLE.start).then(|| serde_json::json!({ "characters": length }))
        },
    },
    Check {
        kind: "title.long",
        weight: "note",
        look: |page| {
            let length = page.title.chars().count();
            (length > TITLE.end).then(|| serde_json::json!({ "characters": length }))
        },
    },
    Check {
        kind: "excerpt.missing",
        weight: "warning",
        look: |page| {
            page.excerpt
                .as_ref()
                .is_none_or(|excerpt| excerpt.trim().is_empty())
                .then(|| serde_json::json!({}))
        },
    },
    Check {
        kind: "excerpt.long",
        weight: "note",
        look: |page| {
            page.excerpt.as_ref().and_then(|excerpt| {
                let length = excerpt.chars().count();
                (length > EXCERPT.end).then(|| serde_json::json!({ "characters": length }))
            })
        },
    },
    Check {
        kind: "body.thin",
        weight: "warning",
        look: |page| {
            let length = page.body.chars().count();
            (length < SHORTEST_BODY).then(|| serde_json::json!({ "characters": length }))
        },
    },
    Check {
        kind: "slug.long",
        weight: "note",
        look: |page| {
            let length = page.slug.chars().count();
            (length > 60).then(|| serde_json::json!({ "characters": length }))
        },
    },
    Check {
        kind: "body.link.empty",
        weight: "note",
        look: |page| {
            // A link with nothing between its tags: a person reading with a
            // screen reader is told "link" and nothing else.
            let empty = page.body.matches("></a>").count();

            (empty > 0).then(|| serde_json::json!({ "links": empty }))
        },
    },
    Check {
        kind: "body.image.unnamed",
        weight: "warning",
        look: |page| {
            let images = page.body.matches("<img").count();
            let named = page.body.matches("alt=").count();

            (images > named).then(|| serde_json::json!({ "images": images - named }))
        },
    },
];

/// Works out what is wrong with one page and writes it down.
///
/// Called when a post is written or changed, rather than when a screen asks:
/// computing this per visit is a screen that reads every post to draw a badge.
pub async fn look_at(conn: &mut TenantConn, tenant: TenantId, post_id: Uuid) -> Result<u64> {
    let found: Option<(String, Option<String>, String, String)> = sqlx::query_as(
        "select title, excerpt, body, slug from posts where id = $1 and deleted_at is null",
    )
    .bind(post_id)
    .fetch_optional(conn.conn())
    .await?;

    let Some((title, excerpt, body, slug)) = found else {
        return Ok(0);
    };

    let page = Page {
        title,
        excerpt,
        body,
        slug,
    };

    // Everything that was wrong before goes; what is wrong now is written. A
    // page that was fixed should stop saying it is broken.
    sqlx::query("delete from page_issues where post_id = $1")
        .bind(post_id)
        .execute(conn.conn())
        .await?;

    let mut found = 0;

    for check in CHECKS {
        let Some(detail) = (check.look)(&page) else {
            continue;
        };

        sqlx::query(
            "insert into page_issues (tenant_id, post_id, kind, weight, detail)
             values ($1, $2, $3, $4::issue_weight, $5)
             on conflict (tenant_id, post_id, kind) do update set detail = excluded.detail",
        )
        .bind(tenant.0)
        .bind(post_id)
        .bind(check.kind)
        .bind(check.weight)
        .bind(&detail)
        .execute(conn.conn())
        .await?;

        found += 1;
    }

    Ok(found)
}

async fn all(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    axum::extract::Query(page): axum::extract::Query<Query>,
) -> Result<Json<Listing<Issue>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let rows = sqlx::query(
        "select i.post_id, p.title, i.kind, i.weight::text as weight, i.detail,
                p.created_at as written_at
           from page_issues i join posts p on p.id = i.post_id
          where p.deleted_at is null
            and ($1::timestamptz is null or p.created_at < $1)
          order by i.weight desc, p.created_at desc
          limit $2",
    )
    .bind(older_than(page.after.as_deref()))
    .bind(page.fetch())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Listing::build(
        &page,
        rows.iter().map(row_to_issue).collect(),
        |issue| issue.written_at.to_rfc3339(),
    )))
}

async fn of_post(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<Issue>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let rows = sqlx::query(
        "select i.post_id, p.title, i.kind, i.weight::text as weight, i.detail,
                p.created_at as written_at
           from page_issues i join posts p on p.id = i.post_id
          where i.post_id = $1
          order by i.weight desc, i.kind",
    )
    .bind(id)
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(rows.iter().map(row_to_issue).collect()))
}

fn row_to_issue(row: &sqlx::postgres::PgRow) -> Issue {
    Issue {
        written_at: row.get("written_at"),
        post_id: row.get("post_id"),
        title: row.get("title"),
        kind: row.get("kind"),
        weight: row.get("weight"),
        detail: row.get("detail"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_page() -> Page {
        Page {
            title: "A Title That Is About Right".to_owned(),
            excerpt: Some(
                "A summary long enough to be worth showing under a search result, \
                 and short enough to be shown in full."
                    .to_owned(),
            ),
            body: "x".repeat(600),
            slug: "a-title-that-is-about-right".to_owned(),
        }
    }

    fn what_is_wrong(page: &Page) -> Vec<&'static str> {
        CHECKS
            .iter()
            .filter(|check| (check.look)(page).is_some())
            .map(|check| check.kind)
            .collect()
    }

    #[test]
    fn a_page_that_is_fine_has_nothing_wrong_with_it() {
        assert!(what_is_wrong(&a_page()).is_empty());
    }

    #[test]
    fn a_title_is_long_enough_to_read_and_short_enough_to_show() {
        let mut page = a_page();
        page.title = "Short".to_owned();
        assert!(what_is_wrong(&page).contains(&"title.short"));

        page.title = "A title that goes on and on and will be cut off before it ends, \
                      somewhere around here"
            .to_owned();
        assert!(what_is_wrong(&page).contains(&"title.long"));
    }

    #[test]
    fn a_page_with_nothing_on_it_says_so() {
        let mut page = a_page();
        page.body = "Two words".to_owned();

        assert!(what_is_wrong(&page).contains(&"body.thin"));
    }

    #[test]
    fn a_picture_nobody_can_see_needs_words() {
        let mut page = a_page();
        page.body = format!("{}<img src=\"a.png\">", "x".repeat(600));

        assert!(what_is_wrong(&page).contains(&"body.image.unnamed"));

        page.body = format!("{}<img src=\"a.png\" alt=\"a cat\">", "x".repeat(600));

        assert!(!what_is_wrong(&page).contains(&"body.image.unnamed"));
    }

    #[test]
    fn a_summary_is_wanted_and_can_be_too_long() {
        let mut page = a_page();
        page.excerpt = None;
        assert!(what_is_wrong(&page).contains(&"excerpt.missing"));

        page.excerpt = Some("word ".repeat(80));
        assert!(what_is_wrong(&page).contains(&"excerpt.long"));
    }
}
