//! What was thrown away, and taking it back.
use axum::Json;
use axum::extract::{Path, State as Injected};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::kernel::audit::{self, Actor, Audited};
use crate::kernel::authz::{Access, Capability, Needs, Permit};
use crate::kernel::db::Tx;
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::page::{Page, Query, older_than};
use crate::kernel::queue::Task;
use crate::kernel::say;
use crate::kernel::trash::{self, KEPT};

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/trash",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::Content, Access::View)),
                rate: RatePolicy::None,
            },
            list,
        )
        .gives::<Page<Thrown>>(),
        Endpoint::post(
            "/api/trash/{kind}/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::Content, Access::Write)),
                rate: RatePolicy::None,
            },
            restore,
        )
        .gives::<Thrown>(),
        Endpoint::delete(
            "/api/trash/{kind}/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(Needs::new(Capability::Content, Access::Delete)),
                rate: RatePolicy::None,
            },
            for_good,
        ),
    ]
}

/// One thing somebody threw away.
#[derive(Clone, Debug, Serialize, utoipa::ToSchema)]
pub struct Thrown {
    pub kind: String,
    pub id: Uuid,
    pub name: String,
    pub thrown_at: DateTime<Utc>,
    /// When it stops being restorable and goes for good.
    pub goes_at: DateTime<Utc>,
}

/// Everything a site has thrown away, whatever kind of thing it is.
///
/// One query per kind rather than one per row, and the kinds are the registry —
/// so a domain that starts soft-deleting appears here without anybody writing a
/// screen for it.
async fn list(
    Injected(state): Injected<AppState>,
    _caller: Caller,
    _permit: Permit,
    axum::extract::Query(page): axum::extract::Query<Query>,
) -> Result<Json<Page<Thrown>>> {
    let mut conn = state.db.begin().await?;
    let after = older_than(page.after.as_deref());
    let mut all = what_was_thrown(&mut conn, after, page.fetch()).await?;

    all.sort_by_key(|thrown| std::cmp::Reverse(thrown.thrown_at));

    conn.commit().await?;

    Ok(Json(Page::build(&page, all, |thrown| {
        thrown.thrown_at.to_rfc3339()
    })))
}

/// What a site threw away, one page's worth (plus one, to say whether there is
/// another) after the given cursor.
///
/// `fetch` rows are asked of *every* table, not `fetch` rows split between
/// them: the true top `fetch` rows of the merged, deleted-at-descending union
/// are always contained in the union of each table's own top `fetch` rows. If
/// a row were among the global top `fetch` but missing from its own table's
/// top `fetch`, that table alone would already hold `fetch` rows thrown away
/// more recently than it, which would place all of those ahead of it globally
/// too — so it could not have been in the global top `fetch` to begin with.
/// That is what makes a per-table, cursor-bounded read enough to walk the
/// whole bin a page at a time, rather than only ever seeing each table's
/// newest hundred.
pub async fn what_was_thrown(
    conn: &mut Tx,
    after: Option<DateTime<Utc>>,
    fetch: i64,
) -> Result<Vec<Thrown>> {
    let mut all = Vec::new();

    for kept in KEPT {
        // The table and the column come from the registry above and never from
        // a request.
        let rows = sqlx::query(&format!(
            "select id, {NAME}::text as name, deleted_at
               from {TABLE}
              where deleted_at is not null
                and ($1::timestamptz is null or deleted_at < $1)
              order by deleted_at desc
              limit $2",
            NAME = kept.named_by,
            TABLE = kept.table,
        ))
        .bind(after)
        .bind(fetch)
        .fetch_all(conn.conn())
        .await?;

        for row in rows {
            let thrown_at: DateTime<Utc> = row.try_get("deleted_at")?;

            all.push(Thrown {
                kind: kept.table.to_owned(),
                id: row.try_get("id")?,
                name: row
                    .try_get::<Option<String>, _>("name")?
                    .unwrap_or_default(),
                thrown_at,
                goes_at: thrown_at + chrono::Duration::days(kept.days),
            });
        }
    }

    Ok(all)
}

async fn restore(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path((kind, id)): Path<(String, Uuid)>,
) -> Result<Audited<Json<Thrown>>> {
    let kept = trash::of(&kind).ok_or(AppError::NotFound("kind"))?;

    let mut conn = state.db.begin().await?;

    let row = sqlx::query(&format!(
        "update {TABLE} set deleted_at = null
          where id = $1 and deleted_at is not null
         returning id, {NAME}::text as name, updated_at",
        TABLE = kept.table,
        NAME = kept.named_by,
    ))
    .bind(id)
    .fetch_optional(conn.conn())
    .await
    .map_err(|error| {
        match error
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
        {
            // Something else took the name while this was away.
            Some(code) if code == "23505" => {
                AppError::Conflict(say::SOMETHING_ELSE_ANSWERS_NOW.into())
            }
            // What it belonged to has gone.
            Some(code) if code == "23503" => {
                AppError::Conflict(say::WHAT_BELONGED_NOT_THERE_ANY_MORE.into())
            }
            _ => AppError::Database(error),
        }
    })?;

    let Some(row) = row else {
        return Err(AppError::NotFound("that"));
    };

    let back = Thrown {
        kind: kept.table.to_owned(),
        id: row.try_get("id")?,
        name: row
            .try_get::<Option<String>, _>("name")?
            .unwrap_or_default(),
        thrown_at: row.try_get("updated_at")?,
        goes_at: row.try_get("updated_at")?,
    };

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "put it back",
        kept.table,
        Some(&id.to_string()),
        &serde_json::json!({ "name": back.name }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, Json(back)))
}

/// Taking something away for good, before the sweep would.
async fn for_good(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path((kind, id)): Path<(String, Uuid)>,
) -> Result<Audited<StatusCode>> {
    let kept = trash::of(&kind).ok_or(AppError::NotFound("kind"))?;

    let mut conn = state.db.begin().await?;

    let gone = sqlx::query(&format!(
        "delete from {TABLE} where id = $1 and deleted_at is not null",
        TABLE = kept.table,
    ))
    .bind(id)
    .execute(conn.conn())
    .await?
    .rows_affected();

    if gone == 0 {
        return Err(AppError::NotFound("that"));
    }

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "took it away for good",
        kept.table,
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::NO_CONTENT))
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Empty;

impl Task for Empty {
    const KIND: &'static str = "trash.empty";
}

#[must_use]
pub fn kinds() -> Vec<String> {
    vec![Empty::KIND.to_owned()]
}

/// What has been in the trash longer than its kind is kept for.
///
/// A trash that grows forever is a trash nobody empties, and what pays for it
/// is the site's own storage bill.
pub async fn empty(state: &AppState) -> Result<u64> {
    let mut conn = state.db.begin().await?;
    let mut taken = 0;

    for kept in KEPT {
        taken += sqlx::query(&format!(
            "delete from {TABLE} where deleted_at < now() - make_interval(days => $1)",
            TABLE = kept.table,
        ))
        .bind(i32::try_from(kept.days).unwrap_or(30))
        .execute(conn.conn())
        .await?
        .rows_affected();
    }

    conn.commit().await?;

    Ok(taken)
}
