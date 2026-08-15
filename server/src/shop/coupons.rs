//! What a site takes off an order, and for whom.
//!
//! Checkout has always spent these and nothing could make one, so a site's
//! discounts existed only if somebody wrote them into the database by hand.

use axum::Json;
use axum::extract::{Path, Query as HttpQuery, State as Injected};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::kernel::audit::{self, Actor, Audited};
use crate::kernel::authz::{Access, Capability, Needs, Permit};
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::money::Currency;
use crate::kernel::page::{Page, Query, older_than};
use crate::kernel::say::{self, Say};

fn shop(access: Access) -> Needs {
    Needs::new(Capability::Shop, access)
}

pub(super) fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/coupons",
            Guard {
                audience: Audience::User,
                needs: Some(shop(Access::View)),
                rate: RatePolicy::None,
            },
            list,
        )
        .gives::<Page<Coupon>>(),
        Endpoint::post(
            "/api/coupons",
            Guard {
                audience: Audience::User,
                needs: Some(shop(Access::Write)),
                rate: RatePolicy::None,
            },
            make,
        )
        .takes::<NewCoupon>()
        .gives::<Coupon>(),
        Endpoint::delete(
            "/api/coupons/{code}",
            Guard {
                audience: Audience::User,
                needs: Some(shop(Access::Delete)),
                rate: RatePolicy::None,
            },
            stop,
        ),
    ]
}

#[derive(Clone, Debug, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Coupon {
    pub id: Uuid,
    pub code: String,
    /// `percent` or `amount`, and `value` is read by it.
    pub kind: String,
    pub value: i64,
    pub uses_allowed: Option<i32>,
    /// What the basket has to reach before it may be used, in the smallest
    /// unit. Zero is no minimum.
    pub minimum_minor: i64,
    /// How many times one address may use it. Null is as often as they like.
    pub per_shopper: Option<i32>,
    /// What an amount off is an amount of. A percentage does not read it.
    pub currency: Currency,
    /// How many times it has actually been spent. What somebody looks at before
    /// deciding whether it is still doing its job.
    pub used: i64,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewCoupon {
    /// Typed by a customer, so it is kept as one case and compared as one:
    /// upper, which is what the database already insists on.
    pub code: String,
    pub kind: String,
    /// A percentage, or an amount in the smallest unit. The kind says which.
    pub value: i64,
    #[serde(default)]
    pub uses_allowed: Option<i32>,
    #[serde(default)]
    pub minimum_minor: Option<i64>,
    #[serde(default)]
    pub per_shopper: Option<i32>,
    #[serde(default)]
    pub currency: Option<Currency>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
}

const COLUMNS: &str = "c.id, c.code, c.kind::text as kind, c.value, c.uses_allowed,
     c.minimum_minor, c.per_shopper, c.currency,
     (select count(*) from coupon_uses u where u.coupon_id = c.id) as used,
     c.expires_at, c.created_at";

async fn list(
    Injected(state): Injected<AppState>,
    _caller: Caller,
    _permit: Permit,
    HttpQuery(page): HttpQuery<Query>,
) -> Result<Json<Page<Coupon>>> {
    let mut conn = state.db.begin().await?;

    let rows: Vec<Coupon> = sqlx::query_as(&format!(
        "select {COLUMNS} from coupons c
          where ($1::timestamptz is null or c.created_at < $1)
          order by c.created_at desc
          limit $2"
    ))
    .bind(older_than(page.after.as_deref()))
    .bind(page.fetch())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Page::build(&page, rows, |coupon| {
        coupon.created_at.to_rfc3339()
    })))
}

async fn make(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Json(body): Json<NewCoupon>,
) -> Result<Audited<(StatusCode, Json<Coupon>)>> {
    if !matches!(body.kind.as_str(), "percent" | "amount") {
        return Err(AppError::Invalid(
            say::A_COUPON_TAKES_OFF_A_PERCENTAGE_OR_AN_AMOUNT.into(),
        ));
    }

    if body.value <= 0 || (body.kind == "percent" && body.value > 100) {
        return Err(AppError::Invalid(
            Say::of(say::THAT_IS_NOT_A_DISCOUNT).naming("value", body.value),
        ));
    }

    // A coupon that has already expired is one nobody can use, which is not a
    // coupon: said now rather than found out at a checkout.
    if body
        .expires_at
        .is_some_and(|when| when <= state.clock.now())
    {
        return Err(AppError::Invalid(say::MOMENT_ALREADY_PASSED.into()));
    }

    let mut conn = state.db.begin().await?;

    let made: Coupon = sqlx::query_as(&format!(
        "with made as (
             insert into coupons
                 (code, kind, value, uses_allowed, expires_at,
                  minimum_minor, per_shopper, currency)
             values (upper($1), $2::coupon_kind, $3, $4, $5,
                     coalesce($6, 0), $7, coalesce($8, 'TRY'::currency))
             returning id, code, kind, value, uses_allowed, minimum_minor,
                       per_shopper, currency, expires_at, created_at
         )
         select {COLUMNS} from made c"
    ))
    .bind(&body.code)
    .bind(&body.kind)
    .bind(body.value)
    .bind(body.uses_allowed)
    .bind(body.expires_at)
    .bind(body.minimum_minor)
    .bind(body.per_shopper)
    .bind(body.currency)
    .fetch_one(conn.conn())
    .await
    .map_err(named_wrongly)?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "made a coupon",
        "coupon",
        Some(&made.code),
        &serde_json::json!({ "kind": made.kind, "value": made.value }),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, (StatusCode::CREATED, Json(made))))
}

/// Stops one working.
///
/// The row goes and what it was spent on stays: an order that was discounted
/// was discounted, and a coupon taken away later does not change what somebody
/// paid.
async fn stop(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(code): Path<String>,
) -> Result<Audited<StatusCode>> {
    let mut conn = state.db.begin().await?;

    let gone = sqlx::query("delete from coupons where code = upper($1)")
        .bind(&code)
        .execute(conn.conn())
        .await?
        .rows_affected();

    if gone == 0 {
        return Err(AppError::NotFound("coupon"));
    }

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(&caller),
        "stopped a coupon",
        "coupon",
        Some(&code.to_uppercase()),
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
            AppError::Conflict(say::THERE_IS_ALREADY_A_COUPON_WITH_THAT_CODE.into())
        }
        Some(code) if code == "23514" => {
            AppError::Invalid(say::A_CODE_IS_BETWEEN_THREE_AND_FORTY_CHARACTERS.into())
        }
        other => {
            let _ = other;
            AppError::Database(error)
        }
    }
}
