//! What a site sells, and how many are left.
use axum::Json;
use axum::extract::{Path, Query as HttpQuery, State as Injected};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::shop;
use crate::kernel::audit::{self, Actor, Auditable, Audited};
use crate::kernel::authz::{Access, Permit};
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::money::{Currency, Money};
use crate::kernel::page::{Page, Query};
use crate::kernel::ratelimit::Limit;
use crate::kernel::say;
use crate::kernel::types::{Slug, Title};

/// What a visitor may ask of the shop. A price list is cheap to read and
/// expensive to be scraped a thousand times a second.
pub(super) const BROWSE_LIMIT: Limit = Limit::new(120, 60);

#[derive(Clone, Debug, Serialize, utoipa::ToSchema)]
pub struct Product {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub price: Money,
    pub stock: i32,
    pub active: bool,
    pub created_at: DateTime<Utc>,
}

impl Auditable for Product {
    const SUBJECT: &'static str = "product";

    fn subject_id(&self) -> String {
        self.id.to_string()
    }

    fn summary(&self) -> serde_json::Value {
        serde_json::json!({
            "slug": self.slug,
            "name": self.name,
            "price": self.price.minor,
            "currency": self.price.currency,
            "stock": self.stock,
            "active": self.active,
        })
    }
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NewProduct {
    pub slug: Slug,
    pub name: Title,
    #[serde(default)]
    pub description: Option<String>,
    /// In minor units. There is no way to send a price as a decimal, which is
    /// how a price comes to be 19.989999999.
    pub price_minor: i64,
    pub currency: Currency,
    #[serde(default)]
    pub stock: Option<i32>,
    #[serde(default)]
    pub low_stock_at: Option<i32>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ProductChanges {
    pub name: Option<Title>,
    pub description: Option<String>,
    pub price_minor: Option<i64>,
    pub stock: Option<i32>,
    pub low_stock_at: Option<i32>,
    pub active: Option<bool>,
}

pub(super) fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::get(
            "/api/products",
            Guard {
                audience: Audience::User,
                needs: Some(shop(Access::View)),
                rate: RatePolicy::None,
            },
            list,
        )
        .gives::<Page<Product>>(),
        Endpoint::post(
            "/api/products",
            Guard {
                audience: Audience::User,
                needs: Some(shop(Access::Write)),
                rate: RatePolicy::None,
            },
            create,
        )
        .takes::<NewProduct>()
        .gives::<Product>(),
        Endpoint::patch(
            "/api/products/{id}",
            Guard {
                audience: Audience::User,
                needs: Some(shop(Access::Write)),
                rate: RatePolicy::None,
            },
            change,
        )
        .takes::<ProductChanges>()
        .gives::<Product>(),
        Endpoint::get(
            "/api/sites/products",
            Guard {
                audience: Audience::Public,
                needs: None,
                rate: RatePolicy::Per(BROWSE_LIMIT),
            },
            on_sale,
        )
        .gives::<Page<Product>>(),
    ]
}

const COLUMNS: &str =
    "id, slug, name, description, price_minor, currency, stock, active, created_at";

async fn list(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    HttpQuery(query): HttpQuery<Query>,
) -> Result<Json<Page<Product>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let rows: Vec<Product> = sqlx::query_as(&format!(
        "select {COLUMNS} from products
          where deleted_at is null
            and ($1::timestamptz is null or created_at < $1)
          order by created_at desc, id desc
          limit $2"
    ))
    .bind(cursor(query.after.as_deref()))
    .bind(query.fetch())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Page::build(&query, rows, |product| {
        product.created_at.to_rfc3339()
    })))
}

/// What a visitor sees: what is for sale, and how much of it there is. Not
/// what is not for sale, and nothing about what has been sold.
async fn on_sale(
    Injected(state): Injected<AppState>,
    caller: Caller,
    HttpQuery(query): HttpQuery<Query>,
) -> Result<Json<Page<Product>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let rows: Vec<Product> = sqlx::query_as(&format!(
        "select {COLUMNS} from products
          where deleted_at is null and active
            and ($1::timestamptz is null or created_at < $1)
          order by created_at desc, id desc
          limit $2"
    ))
    .bind(cursor(query.after.as_deref()))
    .bind(query.fetch())
    .fetch_all(conn.conn())
    .await?;

    conn.commit().await?;

    Ok(Json(Page::build(&query, rows, |product| {
        product.created_at.to_rfc3339()
    })))
}

fn cursor(after: Option<&str>) -> Option<DateTime<Utc>> {
    after.and_then(|value| DateTime::parse_from_rfc3339(value).ok().map(Into::into))
}

async fn create(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Json(body): Json<NewProduct>,
) -> Result<Audited<(StatusCode, Json<Product>)>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let product: Product = sqlx::query_as(&format!(
        "insert into products
             (tenant_id, slug, name, description, price_minor, currency, stock, low_stock_at)
         values ($1, $2, $3, $4, $5, $6, coalesce($7, 0), $8)
         returning {COLUMNS}"
    ))
    .bind(caller.tenant().0)
    .bind(body.slug.as_str())
    .bind(body.name.as_str())
    .bind(body.description.as_deref())
    .bind(body.price_minor)
    .bind(body.currency)
    .bind(body.stock)
    .bind(body.low_stock_at)
    .fetch_one(conn.conn())
    .await
    .map_err(taken)?;

    let receipt = audit::record(
        &mut conn,
        Actor::of(&caller),
        "listed",
        None,
        Some(&product),
    )
    .await?;

    conn.commit().await?;

    Ok(Audited::new(receipt, (StatusCode::CREATED, Json(product))))
}

async fn change(
    Injected(state): Injected<AppState>,
    caller: Caller,
    _permit: Permit,
    Path(id): Path<Uuid>,
    Json(changes): Json<ProductChanges>,
) -> Result<Audited<Json<Product>>> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let before: Product = sqlx::query_as(&format!(
        "select {COLUMNS} from products where id = $1 and deleted_at is null"
    ))
    .bind(id)
    .fetch_optional(conn.conn())
    .await?
    .ok_or(AppError::NotFound("product"))?;

    let after: Product = sqlx::query_as(&format!(
        "update products
            set name = coalesce($2, name),
                description = coalesce($3, description),
                price_minor = coalesce($4, price_minor),
                stock = coalesce($5, stock),
                low_stock_at = coalesce($6, low_stock_at),
                active = coalesce($7, active)
          where id = $1 and deleted_at is null
         returning {COLUMNS}"
    ))
    .bind(id)
    .bind(changes.name.as_ref().map(Title::as_str))
    .bind(changes.description.as_deref())
    .bind(changes.price_minor)
    .bind(changes.stock)
    .bind(changes.low_stock_at)
    .bind(changes.active)
    .fetch_one(conn.conn())
    .await?;

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

fn taken(error: sqlx::Error) -> AppError {
    match error
        .as_database_error()
        .and_then(sqlx::error::DatabaseError::code)
    {
        Some(code) if code == "23505" => {
            AppError::Conflict(say::SOMETHING_ALREADY_SOLD_UNDER_NAME.into())
        }
        Some(code) if code == "23514" => {
            AppError::Invalid(say::PRICE_COUNT_NEVER_BELOW_ZERO.into())
        }
        _ => AppError::Database(error),
    }
}

impl sqlx::FromRow<'_, sqlx::postgres::PgRow> for Product {
    fn from_row(row: &sqlx::postgres::PgRow) -> sqlx::Result<Self> {
        use sqlx::Row as _;

        Ok(Self {
            id: row.try_get("id")?,
            slug: row.try_get("slug")?,
            name: row.try_get("name")?,
            description: row.try_get("description")?,
            price: Money::new(row.try_get("price_minor")?, row.try_get("currency")?),
            stock: row.try_get("stock")?,
            active: row.try_get("active")?,
            created_at: row.try_get("created_at")?,
        })
    }
}
