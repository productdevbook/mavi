//! Buying, and what happens to the shelf while somebody pays.
//!
//! Stock is held at checkout and let go if nobody pays, so two people cannot
//! buy the last one. An order moves one way — the state machine says which
//! ways — and what a provider says happened is signed before it is believed.
use axum::Json;
use axum::extract::{Path, Query as HttpQuery, State as Injected};
use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use super::products::BROWSE_LIMIT;
use super::shop;
use crate::kernel::TenantId;
use crate::kernel::audit::{self, Actor, Auditable, Audited};
use crate::kernel::authz::{Access, Permit};
use crate::kernel::db::TenantConn;
use crate::kernel::error::{AppError, Result};
use crate::kernel::events::{self, EmitsEvents};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::money::{Currency, Money};
use crate::kernel::page::{Page, Query};
use crate::kernel::queue::Task;
use crate::kernel::ratelimit::Limit;
use crate::kernel::say::{self, Say};
use crate::kernel::types::Email;

/// How long stock is held for a checkout nobody has paid for. Long enough to
/// find a card, short enough that the last one of something is not held all
/// afternoon by somebody who wandered off.
const HOLD_MINUTES: i64 = 30;

/// And how long an order may sit unpaid before it is let go entirely.
const STUCK_HOURS: i64 = 24;

/// Checking out is the expensive one: it takes stock and makes an order.
const CHECKOUT_LIMIT: Limit = Limit::new(10, 60);

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, utoipa::ToSchema,
)]
#[sqlx(type_name = "order_state", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum OrderState {
    Pending,
    Paid,
    Fulfilled,
    Cancelled,
    Refunded,
}

impl OrderState {
    /// Every move an order makes. Money only ever goes one way through this,
    /// which is why it is one function and not a condition in four handlers.
    #[must_use]
    pub fn may_become(self, next: Self) -> bool {
        match self {
            OrderState::Pending => matches!(next, OrderState::Paid | OrderState::Cancelled),
            OrderState::Paid => matches!(next, OrderState::Fulfilled | OrderState::Refunded),
            OrderState::Fulfilled => matches!(next, OrderState::Refunded),
            OrderState::Cancelled | OrderState::Refunded => false,
        }
    }
}

#[derive(Clone, Debug, Serialize, utoipa::ToSchema)]
pub struct Order {
    pub id: Uuid,
    /// What a customer calls it, counted per site. Nobody writes to a shop
    /// about a uuid.
    pub number: i64,
    pub state: OrderState,
    pub email: String,
    pub total: Money,
    pub created_at: DateTime<Utc>,
}

impl Auditable for Order {
    const SUBJECT: &'static str = "order";

    fn subject_id(&self) -> String {
        self.id.to_string()
    }

    /// What was ordered and what it came to. Never a card, never a fragment of
    /// one: there is none to leak, and this is where one would end up.
    fn summary(&self) -> serde_json::Value {
        serde_json::json!({
            "state": self.state,
            "email": self.email,
            "total": self.total.minor,
            "currency": self.total.currency,
        })
    }
}

impl EmitsEvents for Order {
    const EVENTS: &'static [&'static str] = &["order.paid", "order.fulfilled", "refund.made"];

    fn subject_id(&self) -> String {
        self.id.to_string()
    }

    fn payload(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "state": self.state,
            "total": self.total.minor,
            "currency": self.total.currency,
        })
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Wanted {
    pub product_id: Uuid,
    pub quantity: i32,
}

/// What comes back from checking out: the order, and where to go and pay for
/// it. A site with no provider configured has the first and not the second.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Placed {
    pub order: Order,
    pub pay_at: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Checkout {
    pub email: Email,
    pub items: Vec<Wanted>,
    #[serde(default)]
    pub coupon: Option<String>,
    /// The caller's own name for this attempt. Sending it twice buys one thing
    /// once, which is what a browser's back button and a flaky network need.
    pub idempotency_key: String,
}

pub(super) fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::post(
            "/api/sites/checkout",
            Guard {
                audience: Audience::Public,
                needs: None,
                rate: RatePolicy::Per(CHECKOUT_LIMIT),
            },
            checkout,
        )
        .takes::<Checkout>()
        .gives::<Placed>(),
        Endpoint::get(
            "/api/orders",
            Guard {
                audience: Audience::User,
                needs: Some(shop(Access::View)),
                rate: RatePolicy::None,
            },
            list,
        )
        .gives::<Page<Order>>(),
        Endpoint::get(
            "/api/orders/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(shop(Access::View)),
                rate: RatePolicy::None,
            },
            whole,
        )
        .gives::<Whole>(),
        Endpoint::post(
            "/api/orders/{id}/paid",
            Guard {
                audience: Audience::User,
                needs: Some(shop(Access::Write)),
                rate: RatePolicy::None,
            },
            mark_paid,
        )
        .gives::<Order>(),
        Endpoint::post(
            "/api/orders/{id}/fulfilled",
            Guard {
                audience: Audience::User,
                needs: Some(shop(Access::Write)),
                rate: RatePolicy::None,
            },
            fulfil,
        )
        .gives::<Order>(),
        Endpoint::post(
            "/api/orders/{id}/refund",
            Guard {
                audience: Audience::User,
                needs: Some(shop(Access::Write)),
                rate: RatePolicy::None,
            },
            refund,
        )
        .gives::<Order>(),
        Endpoint::post(
            "/api/sites/payments/callback",
            Guard {
                audience: Audience::Public,
                needs: None,
                rate: RatePolicy::Per(CHECKOUT_LIMIT),
            },
            paid_callback,
        ),
        Endpoint::get(
            "/api/sites/orders/{id}",
            Guard {
                audience: Audience::Public,
                needs: None,
                rate: RatePolicy::Per(BROWSE_LIMIT),
            },
            look_up,
        )
        .gives::<Order>(),
    ]
}

const COLUMNS: &str = "id, number, state, email, total_minor, currency, created_at";

/// Takes the stock, makes the order, and answers with the same order if the
/// same key arrives again.
///
/// The stock is taken with the row locked, so two people reaching for the last
/// one of something do not both get it — this is the fault that made the old
/// shop sell what it did not have.
async fn checkout(
    Injected(state): Injected<AppState>,
    caller: Caller,
    Json(body): Json<Checkout>,
) -> Result<Audited<(StatusCode, Json<Placed>)>> {
    if body.items.is_empty() || body.items.len() > 100 {
        return Err(AppError::Invalid(
            say::ORDER_BETWEEN_ONE_HUNDRED_LINES.into(),
        ));
    }

    if body.idempotency_key.len() < 8 || body.idempotency_key.len() > 200 {
        return Err(AppError::Invalid(
            say::IDEMPOTENCY_KEY_BETWEEN_EIGHT_TWO_HUNDRED.into(),
        ));
    }

    let mut conn = state.db.tenant(caller.tenant()).await?;

    // The same attempt twice is the same order. Asked before anything is
    // taken, so a repeat does not move stock again.
    if let Some(already) = by_key(&mut conn, &body.idempotency_key).await? {
        let receipt = audit::record_raw(
            &mut conn,
            Actor::system(caller.request_id),
            "checked out again",
            "order",
            Some(&already.id.to_string()),
            &serde_json::json!({}),
        )
        .await?;

        // The same place to pay as the first time, so a repeat is answerable
        // with somewhere to go rather than with an order and no way to pay.
        let pay_at: Option<(String,)> =
            sqlx::query_as("select pay_at from payments where order_id = $1")
                .bind(already.id)
                .fetch_optional(conn.conn())
                .await?;

        conn.commit().await?;

        return Ok(Audited::new(
            receipt,
            (
                StatusCode::OK,
                Json(Placed {
                    order: already,
                    pay_at: pay_at.map(|(at,)| at),
                }),
            ),
        ));
    }

    let (lines, currency, priced) = price(&mut conn, &body.items).await?;

    let coupon = match body.coupon.as_deref() {
        Some(code) => Some(a_coupon(&mut conn, code, priced, currency, body.email.as_str()).await?),
        None => None,
    };

    let total = match &coupon {
        Some(coupon) => discounted(priced, &coupon.kind, coupon.value),
        None => priced,
    };

    let order: Order = sqlx::query_as(&format!(
        "insert into orders (tenant_id, email, total_minor, currency, idempotency_key)
         values ($1, $2, $3, $4, $5)
         returning {COLUMNS}"
    ))
    .bind(caller.tenant().0)
    .bind(body.email.as_str())
    .bind(total)
    .bind(currency)
    .bind(&body.idempotency_key)
    .fetch_one(conn.conn())
    .await
    .map_err(|error| {
        match error
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code)
        {
            // Two of the same request at the same time: the one that lost the
            // race asks again and finds the order the other made.
            Some(code) if code == "23505" => AppError::Conflict(say::ORDER_BEING_MADE.into()),
            _ => AppError::Database(error),
        }
    })?;

    take_stock(
        &mut conn,
        caller.tenant(),
        order.id,
        &lines,
        state.clock.now() + Duration::minutes(HOLD_MINUTES),
    )
    .await?;

    if let Some(coupon) = coupon {
        spend_coupon(&mut conn, caller.tenant(), coupon.id, order.id).await?;
    }

    let pay_at = somewhere_to_pay(&state, &mut conn, caller.tenant(), &order, &body.email).await?;

    let receipt = audit::record(
        &mut conn,
        Actor::system(caller.request_id),
        "ordered",
        None,
        Some(&order),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(
        receipt,
        (StatusCode::CREATED, Json(Placed { order, pay_at })),
    ))
}

/// One line at a time, with the product's row locked for the length of the
/// transaction: what is read here is what is written below, with nobody else
/// in between. This is the fault that made the old shop sell what it did not
/// have.
async fn price(conn: &mut TenantConn, wanted: &[Wanted]) -> Result<(Vec<Line>, Currency, i64)> {
    let mut lines = Vec::with_capacity(wanted.len());
    let mut currency: Option<Currency> = None;
    let mut total = 0_i64;

    for want in wanted {
        if want.quantity <= 0 || want.quantity > 1000 {
            return Err(AppError::Invalid(say::NOT_NUMBER_BUY.into()));
        }

        let product: Option<(String, i64, Currency, i32)> = sqlx::query_as(
            "select name, price_minor, currency, stock
               from products
              where id = $1 and active and deleted_at is null
                for update",
        )
        .bind(want.product_id)
        .fetch_optional(conn.conn())
        .await?;

        let Some((name, price_minor, product_currency, stock)) = product else {
            return Err(AppError::NotFound("product"));
        };

        if currency.is_some_and(|already| already != product_currency) {
            return Err(AppError::Invalid(say::ORDER_ONE_CURRENCY.into()));
        }

        currency = Some(product_currency);

        if stock < want.quantity {
            return Err(AppError::Conflict(
                Say::of(say::ONLY_SO_MANY_LEFT)
                    .naming("left", stock)
                    .naming("name", &name),
            ));
        }

        let quantity = u32::try_from(want.quantity)
            .map_err(|_| AppError::Invalid(say::NOT_NUMBER_BUY.into()))?;

        total = total
            .checked_add(
                Money::new(price_minor, product_currency)
                    .times(quantity)?
                    .minor,
            )
            .ok_or(AppError::Bug("an order came to more than money goes"))?;

        lines.push(Line {
            product_id: want.product_id,
            name,
            unit_minor: price_minor,
            quantity: want.quantity,
        });
    }

    let currency = currency.ok_or_else(|| AppError::Invalid(say::ORDER_LINES.into()))?;

    Ok((lines, currency, total))
}

struct Line {
    product_id: Uuid,
    name: String,
    unit_minor: i64,
    quantity: i32,
}

/// Off the shelf and into a hold. The check constraint refuses a negative, so
/// a race that got this far is a transaction that fails rather than a shop
/// that owes somebody something.
async fn take_stock(
    conn: &mut TenantConn,
    tenant: TenantId,
    order: Uuid,
    lines: &[Line],
    held_until: DateTime<Utc>,
) -> Result<()> {
    for line in lines {
        sqlx::query(
            "insert into order_items (tenant_id, order_id, product_id, name, unit_minor, quantity)
             values ($1, $2, $3, $4, $5, $6)",
        )
        .bind(tenant.0)
        .bind(order)
        .bind(line.product_id)
        .bind(&line.name)
        .bind(line.unit_minor)
        .bind(line.quantity)
        .execute(conn.conn())
        .await?;

        sqlx::query("update products set stock = stock - $2 where id = $1")
            .bind(line.product_id)
            .bind(line.quantity)
            .execute(conn.conn())
            .await
            .map_err(|_| {
                AppError::Conflict(Say::of(say::NOT_THAT_MANY_LEFT).naming("name", &line.name))
            })?;

        sqlx::query(
            "insert into stock_holds (tenant_id, order_id, product_id, quantity, expires_at)
             values ($1, $2, $3, $4, $5)",
        )
        .bind(tenant.0)
        .bind(order)
        .bind(line.product_id)
        .bind(line.quantity)
        .bind(held_until)
        .execute(conn.conn())
        .await?;
    }

    Ok(())
}

/// A one-use code spent twice is what the unique key refuses. Nothing here
/// counts anything and hopes.
async fn spend_coupon(
    conn: &mut TenantConn,
    tenant: TenantId,
    coupon: Uuid,
    order: Uuid,
) -> Result<()> {
    sqlx::query("insert into coupon_uses (coupon_id, order_id, tenant_id) values ($1, $2, $3)")
        .bind(coupon)
        .bind(order)
        .bind(tenant.0)
        .execute(conn.conn())
        .await
        .map_err(|_| AppError::Conflict(say::CODE_BEEN_USED.into()))?;

    let spent: (i64, Option<i32>) = sqlx::query_as(
        "select count(*), max(c.uses_allowed)
           from coupon_uses u join coupons c on c.id = u.coupon_id
          where u.coupon_id = $1",
    )
    .bind(coupon)
    .fetch_one(conn.conn())
    .await?;

    if let (used, Some(allowed)) = spent
        && used > i64::from(allowed)
    {
        return Err(AppError::Conflict(say::CODE_BEEN_USED.into()));
    }

    Ok(())
}

/// Where to send them to pay, if anything here can take money.
///
/// A site with no provider gets an order and no way to pay for it, which is
/// what it has: answering with somewhere that does not exist would make a shop
/// that cannot sell look like one that can.
async fn somewhere_to_pay(
    state: &AppState,
    conn: &mut TenantConn,
    tenant: TenantId,
    order: &Order,
    email: &Email,
) -> Result<Option<String>> {
    // The site's own provider where it has one, and the machine's where it has
    // not. Read here rather than held in the state, because which provider a
    // site uses is the site's.
    let provider = crate::plugins::payments_for(state, tenant).await?;

    let taking = match provider
        .ask(&crate::kernel::payments::Asking {
            order_id: order.id,
            amount: order.total,
            email: email.as_str().to_owned(),
            back_to: format!("/orders/{}", order.id),
        })
        .await
    {
        Ok(taking) => taking,
        Err(AppError::Refused(_)) => return Ok(None),
        Err(other) => return Err(other),
    };

    sqlx::query(
        "insert into payments
             (tenant_id, order_id, provider, provider_ref, amount_minor, currency, pay_at)
         values ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(tenant.0)
    .bind(order.id)
    .bind(provider.name())
    .bind(&taking.provider_ref)
    .bind(order.total.minor)
    .bind(order.total.currency)
    .bind(&taking.pay_at)
    .execute(conn.conn())
    .await?;

    Ok(Some(taking.pay_at))
}

fn discounted(total: i64, kind: &str, value: i64) -> i64 {
    let off = if kind == "percent" {
        total.saturating_mul(value) / 100
    } else {
        value
    };

    (total - off).max(0)
}

/// One order, and what was in it, as a screen asks for it.
///
/// Separate from the listing because the lines are what makes an order a thing
/// somebody can act on, and asking for them per row in a list is a query per
/// order.
async fn whole(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> Result<Json<Whole>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let order: Option<Order> =
        sqlx::query_as(&format!("select {COLUMNS} from orders where id = $1"))
            .bind(id)
            .fetch_optional(conn.conn())
            .await?;

    let order = order.ok_or(AppError::NotFound("order"))?;

    let lines: Vec<Bought> = sqlx::query_as(
        "select name, quantity, unit_minor, currency
           from order_items i join orders o on o.id = i.order_id
          where i.order_id = $1
          order by i.created_at",
    )
    .bind(id)
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Whole { order, lines }))
}

/// An order with its lines.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Whole {
    /// Nested rather than flattened: a flattened struct has no shape a
    /// generated client can name, and the panel's types come from this.
    pub order: Order,
    /// Called lines rather than items: `items` is what a page of something
    /// holds, everywhere else in this API, and one order is not a page.
    pub lines: Vec<Bought>,
}

/// One line of an order, as it was when it was bought and as a screen reads it.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Bought {
    pub name: String,
    pub quantity: i32,
    pub each: Money,
}

impl sqlx::FromRow<'_, sqlx::postgres::PgRow> for Bought {
    fn from_row(row: &sqlx::postgres::PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            name: row.try_get("name")?,
            quantity: row.try_get("quantity")?,
            each: Money::new(row.try_get("unit_minor")?, row.try_get("currency")?),
        })
    }
}

/// The coupon's row is locked for the length of the transaction, so two
/// checkouts reaching for the last use of a one-use code are two transactions
/// in a queue rather than two that each count one use and each see one.
async fn a_coupon(
    conn: &mut TenantConn,
    code: &str,
    basket: i64,
    currency: Currency,
    email: &str,
) -> Result<Spending> {
    let found: Option<Spending> = sqlx::query_as(
        "select id, kind::text as kind, value, minimum_minor, per_shopper, currency
           from coupons
          where code = $1 and (expires_at is null or expires_at > now())
            for update",
    )
    .bind(code.to_uppercase())
    .fetch_optional(conn.conn())
    .await?;

    let found = found.ok_or_else(|| AppError::Invalid(say::CODE_NOT_ONE_OURS.into()))?;

    // An amount off is an amount of something. Taking five hundred off a
    // basket in another currency is taking off whatever the number happens to
    // mean there, which is not a discount anybody decided.
    if found.kind == "amount" && found.currency != currency {
        return Err(AppError::Invalid(say::CODE_NOT_ONE_OURS.into()));
    }

    // Said now rather than after the order is made: what a basket has to reach
    // is something somebody can act on, and only before they have paid.
    if basket < found.minimum_minor {
        return Err(AppError::Invalid(
            Say::of(say::BASKET_HAS_NOT_REACHED_WHAT_THAT_CODE_ASKS)
                .naming("minimum", found.minimum_minor),
        ));
    }

    if let Some(each) = found.per_shopper {
        let (mine,): (i64,) = sqlx::query_as(
            "select count(*) from coupon_uses u
               join orders o on o.id = u.order_id
              where u.coupon_id = $1 and o.email = $2",
        )
        .bind(found.id)
        .bind(email)
        .fetch_one(conn.conn())
        .await?;

        if mine >= i64::from(each) {
            return Err(AppError::Conflict(say::CODE_BEEN_USED.into()));
        }
    }

    Ok(found)
}

/// A coupon as a checkout reads it.
#[derive(Debug, sqlx::FromRow)]
struct Spending {
    id: Uuid,
    kind: String,
    value: i64,
    minimum_minor: i64,
    per_shopper: Option<i32>,
    currency: Currency,
}

async fn by_key(conn: &mut TenantConn, key: &str) -> Result<Option<Order>> {
    Ok(sqlx::query_as(&format!(
        "select {COLUMNS} from orders where idempotency_key = $1"
    ))
    .bind(key)
    .fetch_optional(conn.conn())
    .await?)
}

/// What the provider says happened, in the provider's own words, signed.
///
/// The signature is the whole of the authentication: without it this is
/// somebody saying an order was paid for. What it does is idempotent, because
/// a provider sends the same thing twice.
async fn paid_callback(
    Injected(state): Injected<AppState>,
    caller: Caller,
    headers: axum::http::HeaderMap,
    body: String,
) -> Result<Audited<StatusCode>> {
    let signature = headers
        .get("webhook-signature")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();

    let provider = crate::plugins::payments_for(&state, caller.tenant()).await?;

    if !provider.signature_holds(&body, signature) {
        return Err(AppError::Forbidden);
    }

    let said: crate::kernel::payments::Settled = serde_json::from_str(&body)
        .map_err(|_| AppError::Invalid(say::NOT_SOMETHING_PROVIDER_SAYS.into()))?;

    let mut conn = state.db.tenant(caller.tenant()).await?;

    let payment: Option<(Uuid, Uuid, i64, String)> = sqlx::query_as(
        "select id, order_id, amount_minor, state::text from payments
          where provider_ref = $1 for update",
    )
    .bind(&said.provider_ref)
    .fetch_optional(conn.conn())
    .await?;

    let Some((payment_id, order_id, expected, was)) = payment else {
        return Err(AppError::NotFound("payment"));
    };

    // What the provider says it took has to be what was asked for. A callback
    // saying an order was paid for less than it costs is not a payment.
    if said.state == "paid" && said.amount_minor != expected {
        return Err(AppError::Conflict(say::NOT_WHAT_ORDER_CAME.into()));
    }

    let receipt = audit::record_raw(
        &mut conn,
        Actor::system(caller.request_id),
        "heard from the payment provider",
        "payment",
        Some(&said.provider_ref),
        &serde_json::json!({ "state": said.state, "was": was }),
    )
    .await?;

    // The same callback twice changes nothing the second time.
    if was != "waiting" {
        conn.commit().await?;
        return Ok(Audited::new(receipt, StatusCode::OK));
    }

    sqlx::query("update payments set state = $2::payment_state, settled_at = now() where id = $1")
        .bind(payment_id)
        .bind(&said.state)
        .execute(conn.conn())
        .await?;

    if said.state == "paid" {
        let order: Option<Order> = sqlx::query_as(&format!(
            "update orders
                set state = 'paid', paid_at = now()
              where id = $1 and state = 'pending'
             returning {COLUMNS}"
        ))
        .bind(order_id)
        .fetch_optional(conn.conn())
        .await?;

        if let Some(order) = order {
            // Paid for: the hold becomes a sale, and nothing puts the stock
            // back when it lapses.
            sqlx::query("update stock_holds set released_at = now() where order_id = $1")
                .bind(order_id)
                .execute(conn.conn())
                .await?;

            events::emit(&mut conn, "order.paid", &order).await?;
        }
    }

    conn.commit().await?;

    Ok(Audited::new(receipt, StatusCode::CREATED))
}

async fn list(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    HttpQuery(query): HttpQuery<Query>,
) -> Result<Json<Page<Order>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let rows: Vec<Order> = sqlx::query_as(&format!(
        "select {COLUMNS} from orders
          where ($1::timestamptz is null or created_at < $1)
          order by created_at desc, id desc
          limit $2"
    ))
    .bind(
        query
            .after
            .as_deref()
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(DateTime::<Utc>::from),
    )
    .bind(query.fetch())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Page::build(&query, rows, |order| {
        order.created_at.to_rfc3339()
    })))
}

/// What somebody who ordered can see of their own order: that it exists and
/// where it has got to. Nothing about anybody else's, and nothing they would
/// have to be signed in to be shown.
async fn look_up(
    Injected(state): Injected<AppState>,
    caller: Caller,
    Path(id): Path<Uuid>,
) -> Result<Json<Order>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;
    let order = one(&mut conn, id).await?;
    conn.commit().await?;

    Ok(Json(order))
}

async fn one(conn: &mut TenantConn, id: Uuid) -> Result<Order> {
    sqlx::query_as(&format!("select {COLUMNS} from orders where id = $1"))
        .bind(id)
        .fetch_optional(conn.conn())
        .await?
        .ok_or(AppError::NotFound("order"))
}

async fn mark_paid(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
) -> Result<Audited<Json<Order>>> {
    moved(&state, &caller, id, OrderState::Paid, "order.paid").await
}

async fn fulfil(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
) -> Result<Audited<Json<Order>>> {
    moved(
        &state,
        &caller,
        id,
        OrderState::Fulfilled,
        "order.fulfilled",
    )
    .await
}

async fn refund(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
) -> Result<Audited<Json<Order>>> {
    moved(&state, &caller, id, OrderState::Refunded, "refund.made").await
}

/// One way through the state machine for all three, because the difference
/// between them is which state and which event, and everything else about
/// moving an order is the same.
async fn moved(
    state: &AppState,
    caller: &Caller,
    id: Uuid,
    next: OrderState,
    event: &str,
) -> Result<Audited<Json<Order>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let before: Option<Order> = sqlx::query_as(&format!(
        "select {COLUMNS} from orders where id = $1 for update"
    ))
    .bind(id)
    .fetch_optional(conn.conn())
    .await?;

    let before = before.ok_or(AppError::NotFound("order"))?;

    if !before.state.may_become(next) {
        return Err(AppError::Conflict(
            Say::of(say::ORDER_DOES_NOT_BECOME_THAT)
                .naming("from", format!("{:?}", before.state))
                .naming("to", format!("{next:?}")),
        ));
    }

    let after: Order = sqlx::query_as(&format!(
        "update orders
            set state = $2,
                paid_at = case when $2 = 'paid'::order_state then now() else paid_at end,
                fulfilled_at = case when $2 = 'fulfilled'::order_state then now()
                                    else fulfilled_at end,
                refunded_at = case when $2 = 'refunded'::order_state then now()
                                   else refunded_at end
          where id = $1
         returning {COLUMNS}"
    ))
    .bind(id)
    .bind(next)
    .fetch_one(conn.conn())
    .await?;

    // Paying for it turns a hold into a sale: the stock is already gone, and
    // nothing must put it back when the hold runs out.
    if next == OrderState::Paid {
        sqlx::query("update stock_holds set released_at = now() where order_id = $1")
            .bind(id)
            .execute(conn.conn())
            .await?;
    }

    // A refund puts the stock back, because whatever was sold is coming home.
    if next == OrderState::Refunded {
        put_back(&mut conn, id).await?;
    }

    events::emit(&mut conn, event, &after).await?;

    let receipt = audit::record(
        &mut conn,
        Actor::of(caller),
        match next {
            OrderState::Paid => "was paid",
            OrderState::Fulfilled => "was sent",
            OrderState::Refunded => "was refunded",
            _ => "changed",
        },
        Some(&before),
        Some(&after),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, Json(after)))
}

async fn put_back(conn: &mut TenantConn, order: Uuid) -> Result<()> {
    let items = sqlx::query(
        "select product_id, quantity from order_items
          where order_id = $1 and product_id is not null",
    )
    .bind(order)
    .fetch_all(conn.conn())
    .await?;

    for item in items {
        sqlx::query("update products set stock = stock + $2 where id = $1")
            .bind(item.get::<Uuid, _>("product_id"))
            .bind(item.get::<i32, _>("quantity"))
            .execute(conn.conn())
            .await?;
    }

    Ok(())
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ReleaseHolds;

impl Task for ReleaseHolds {
    const KIND: &'static str = "shop.release-holds";
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DropStuck;

impl Task for DropStuck {
    const KIND: &'static str = "shop.drop-stuck";
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Reconcile;

impl Task for Reconcile {
    const KIND: &'static str = "shop.reconcile";
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SweepOrders;

impl Task for SweepOrders {
    const KIND: &'static str = "shop.sweep-orders";
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct WarnOnLowStock;

impl Task for WarnOnLowStock {
    const KIND: &'static str = "shop.low-stock";
}

/// Stock held for a checkout nobody finished goes back on the shelf.
pub async fn release_holds(state: &AppState, tenant: TenantId) -> Result<u64> {
    let mut conn = state.db.tenant(tenant).await?;

    let lapsed = sqlx::query(
        "update stock_holds
            set released_at = now()
          where released_at is null and expires_at < now()
         returning order_id, product_id, quantity",
    )
    .fetch_all(conn.conn())
    .await?;

    for hold in &lapsed {
        sqlx::query("update products set stock = stock + $2 where id = $1")
            .bind(hold.get::<Uuid, _>("product_id"))
            .bind(hold.get::<i32, _>("quantity"))
            .execute(conn.conn())
            .await?;

        // And the order goes with it. Putting the stock back while leaving the
        // order payable is how a shop comes to owe somebody something it has
        // already sold to somebody else: the state machine refuses to pay for
        // a cancelled order, and that is what makes this safe rather than a
        // race nobody sees until it happens.
        sqlx::query(
            "update orders set state = 'cancelled', cancelled_at = now()
              where id = $1 and state = 'pending'",
        )
        .bind(hold.get::<Uuid, _>("order_id"))
        .execute(conn.conn())
        .await?;
    }

    conn.commit().await?;

    Ok(lapsed.len() as u64)
}

/// An order nobody ever paid for is let go, so that a shop's list of orders is
/// what happened rather than what was attempted.
pub async fn drop_stuck(state: &AppState, tenant: TenantId) -> Result<u64> {
    let mut conn = state.db.tenant(tenant).await?;

    let dropped = sqlx::query(
        "update orders
            set state = 'cancelled', cancelled_at = now()
          where state = 'pending' and created_at < now() - make_interval(hours => $1)
         returning id",
    )
    .bind(i32::try_from(STUCK_HOURS).unwrap_or(24))
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(dropped.len() as u64)
}

/// What the provider says it has, against what this says it has.
///
/// A callback that never arrived, or arrived and was lost, leaves an order
/// unpaid that somebody has paid for. This asks the provider directly and puts
/// the difference right — and says what it found, because a difference that
/// keeps appearing is a fault somewhere else.
pub async fn reconcile(state: &AppState, tenant: TenantId) -> Result<u64> {
    let since = state.clock.now() - Duration::days(7);
    let theirs = crate::plugins::payments_for(state, tenant)
        .await?
        .taken_since(since)
        .await?;

    if theirs.is_empty() {
        return Ok(0);
    }

    let mut conn = state.db.tenant(tenant).await?;
    let mut put_right = 0;

    for settled in &theirs {
        if settled.state != "paid" {
            continue;
        }

        let ours: Option<(Uuid, String, i64)> = sqlx::query_as(
            "select order_id, state::text, amount_minor from payments
              where provider_ref = $1 for update",
        )
        .bind(&settled.provider_ref)
        .fetch_optional(conn.conn())
        .await?;

        let Some((order_id, was, expected)) = ours else {
            continue;
        };

        if was == "paid" || settled.amount_minor != expected {
            continue;
        }

        tracing::warn!(
            provider_ref = %settled.provider_ref,
            "the provider took money for an order this said was unpaid"
        );

        sqlx::query(
            "update payments set state = 'paid', settled_at = now() where provider_ref = $1",
        )
        .bind(&settled.provider_ref)
        .execute(conn.conn())
        .await?;

        let order: Option<Order> = sqlx::query_as(&format!(
            "update orders set state = 'paid', paid_at = now()
              where id = $1 and state = 'pending'
             returning {COLUMNS}"
        ))
        .bind(order_id)
        .fetch_optional(conn.conn())
        .await?;

        if let Some(order) = order {
            sqlx::query("update stock_holds set released_at = now() where order_id = $1")
                .bind(order_id)
                .execute(conn.conn())
                .await?;

            events::emit(&mut conn, "order.paid", &order).await?;
            put_right += 1;
        }
    }

    conn.commit().await?;

    Ok(put_right)
}

/// An order carries an address, and an address belongs to a person. Ten years
/// is what a financial record is kept for; after that it goes, lines and all.
pub async fn sweep_orders(state: &AppState, tenant: TenantId) -> Result<u64> {
    let mut conn = state.db.tenant(tenant).await?;

    let taken =
        sqlx::query("delete from orders where created_at < now() - make_interval(days => $1)")
            .bind(3650)
            .execute(conn.conn())
            .await?
            .rows_affected();

    conn.commit().await?;

    Ok(taken)
}

/// Says when there is nearly none of something left, once, rather than every
/// time somebody looks.
pub async fn warn_on_low_stock(state: &AppState, tenant: TenantId) -> Result<u64> {
    let mut conn = state.db.tenant(tenant).await?;

    let low = sqlx::query(
        "select id, name, stock from products
          where low_stock_at is not null and stock <= low_stock_at
            and active and deleted_at is null",
    )
    .fetch_all(conn.conn())
    .await?;

    for product in &low {
        events::emit(
            &mut conn,
            "stock.low",
            &LowStock {
                id: product.get("id"),
                name: product.get("name"),
                stock: product.get("stock"),
            },
        )
        .await?;
    }

    conn.commit().await?;

    Ok(low.len() as u64)
}

struct LowStock {
    id: Uuid,
    name: String,
    stock: i32,
}

impl EmitsEvents for LowStock {
    const EVENTS: &'static [&'static str] = &["stock.low"];

    fn subject_id(&self) -> String {
        self.id.to_string()
    }

    fn payload(&self) -> serde_json::Value {
        serde_json::json!({ "id": self.id, "name": self.name, "stock": self.stock })
    }
}

impl sqlx::FromRow<'_, sqlx::postgres::PgRow> for Order {
    fn from_row(row: &sqlx::postgres::PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id")?,
            number: row.try_get("number")?,
            state: row.try_get("state")?,
            email: row.try_get("email")?,
            total: Money::new(row.try_get("total_minor")?, row.try_get("currency")?),
            created_at: row.try_get("created_at")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_only_goes_one_way_through_an_order() {
        assert!(OrderState::Pending.may_become(OrderState::Paid));
        assert!(OrderState::Paid.may_become(OrderState::Refunded));
        assert!(OrderState::Fulfilled.may_become(OrderState::Refunded));

        assert!(!OrderState::Pending.may_become(OrderState::Fulfilled));
        assert!(!OrderState::Refunded.may_become(OrderState::Paid));
        assert!(!OrderState::Cancelled.may_become(OrderState::Paid));
    }

    #[test]
    fn a_discount_never_turns_into_money_owed() {
        assert_eq!(discounted(1000, "percent", 10), 900);
        assert_eq!(discounted(1000, "amount", 250), 750);
        assert_eq!(discounted(1000, "amount", 5000), 0);
        assert_eq!(discounted(0, "percent", 100), 0);
    }
}
