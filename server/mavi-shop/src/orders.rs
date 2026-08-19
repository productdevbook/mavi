use chrono::{DateTime, Duration, Utc};
use mavi_contract::{Endpoint, Method, Permission, Shape};
use mavi_core::{
    Action, Capability, Currency, Email, ErrorCode, MaviError, Money, OrderId, OrderLineId, Page,
    PageRequest, ProductId, Result, SiteContext, StockHoldId,
};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use crate::coupons;
use crate::products;
use crate::{ShopService, decode_cursor, encode_cursor};

pub const ORDER_NOT_FOUND: &str = "shop_order_not_found";
pub const ORDER_EMAIL_INVALID: &str = "shop_order_email_invalid";
pub const ORDER_IDEMPOTENCY_INVALID: &str = "shop_order_idempotency_invalid";
pub const ORDER_ITEMS_INVALID: &str = "shop_order_items_invalid";
pub const ORDER_QUANTITY_INVALID: &str = "shop_order_quantity_invalid";
pub const ORDER_STOCK_UNAVAILABLE: &str = "shop_order_stock_unavailable";
pub const ORDER_CURRENCY_MISMATCH: &str = "shop_order_currency_mismatch";
pub const ORDER_TRANSITION_INVALID: &str = "shop_order_transition_invalid";
pub const PAYMENT_RECEIPT_INVALID: &str = "shop_payment_receipt_invalid";

const MAX_IDEMPOTENCY_KEY_CHARS: usize = 128;
const MAX_PAYMENT_VALUE_CHARS: usize = 128;
const MAX_ITEM_QUANTITY: u32 = 1_000;
const HOLD_MINUTES: i64 = 30;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderState {
    Waiting,
    Paid,
    Sent,
    CalledOff,
    GivenBack,
}

impl OrderState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Paid => "paid",
            Self::Sent => "sent",
            Self::CalledOff => "called_off",
            Self::GivenBack => "given_back",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "waiting" => Ok(Self::Waiting),
            "paid" => Ok(Self::Paid),
            "sent" => Ok(Self::Sent),
            "called_off" => Ok(Self::CalledOff),
            "given_back" => Ok(Self::GivenBack),
            _ => Err(MaviError::Internal),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BasketItem {
    pub product_id: ProductId,
    pub quantity: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckoutInput {
    pub email: String,
    pub items: Vec<BasketItem>,
    pub coupon_code: Option<String>,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PaymentReceiptInput {
    pub provider: String,
    pub reference: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OrderTransition {
    pub to: OrderState,
    pub payment: Option<PaymentReceiptInput>,
}

#[derive(Clone, Debug, Serialize)]
pub struct OrderLine {
    pub id: OrderLineId,
    pub product_id: Option<ProductId>,
    pub name: String,
    pub each: Money,
    pub quantity: u32,
}

#[derive(Clone, Debug, Serialize)]
pub struct Order {
    pub id: OrderId,
    pub number: i64,
    pub state: OrderState,
    pub email: String,
    pub total: Money,
    pub lines: Vec<OrderLine>,
    pub payment_provider: Option<String>,
    pub payment_reference: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct OrderSummary {
    pub id: OrderId,
    pub number: i64,
    pub state: OrderState,
    pub email: String,
    pub total: Money,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CheckoutReceipt {
    pub id: OrderId,
    pub number: i64,
    pub state: OrderState,
    pub total: Money,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct OrderListFilter {
    #[serde(flatten)]
    pub page: PageRequest,
    pub state: Option<OrderState>,
}

pub fn api() -> mavi_contract::Api {
    mavi_contract::Api::new(endpoints()).with_shapes(shapes())
}

fn endpoints() -> Vec<Endpoint> {
    let view = Permission {
        capability: Capability::Shop,
        action: Action::View,
    };
    let write = Permission {
        capability: Capability::Shop,
        action: Action::Write,
    };
    vec![
        Endpoint::new(
            Method::Get,
            "/api/v1/shop/orders",
            "shop.orders.list",
            "List site orders with an opaque cursor",
        )
        .account_or_assistant()
        .requires(view)
        .takes_query("OrderListFilter")
        .returns(200, "OrderSummaryPage")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::Validation,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Get,
            "/api/v1/shop/orders/{id}",
            "shop.orders.read",
            "Read one order with immutable line snapshots",
        )
        .account_or_assistant()
        .requires(view)
        .returns(200, "Order")
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/api/v1/shop/orders/{id}/transition",
            "shop.orders.transition",
            "Move an order through its explicit state machine",
        )
        .account_or_assistant()
        .requires(write)
        .takes("OrderTransition")
        .returns(200, "Order")
        .changes(false)
        .refuses([
            ErrorCode::Forbidden,
            ErrorCode::NotFound,
            ErrorCode::Validation,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
        Endpoint::new(
            Method::Post,
            "/public/v1/shop/orders",
            "shop.public.orders.checkout",
            "Place an idempotent public order and hold available stock",
        )
        .public_changes(true)
        .takes("CheckoutInput")
        .returns(201, "CheckoutReceipt")
        .refuses([
            ErrorCode::Validation,
            ErrorCode::NotFound,
            ErrorCode::Conflict,
            ErrorCode::Internal,
        ]),
    ]
}

fn shapes() -> Vec<Shape> {
    vec![
        Shape::new(
            "OrderState",
            json!({"type": "string", "enum": ["waiting", "paid", "sent", "called_off", "given_back"]}),
        ),
        Shape::new(
            "BasketItem",
            json!({"type": "object", "required": ["product_id", "quantity"], "additionalProperties": false, "properties": {
                "product_id": {"type": "string", "format": "uuid"},
                "quantity": {"type": "integer", "minimum": 1, "maximum": MAX_ITEM_QUANTITY}
            }}),
        ),
        Shape::new(
            "CheckoutInput",
            json!({"type": "object", "required": ["email", "items", "idempotency_key"], "additionalProperties": false, "properties": {
                "email": {"type": "string", "format": "email"},
                "items": {"type": "array", "minItems": 1, "maxItems": 100, "items": {"$ref": "#/components/schemas/BasketItem"}},
                "coupon_code": {"type": ["string", "null"], "maxLength": 40},
                "idempotency_key": {"type": "string", "minLength": 1, "maxLength": MAX_IDEMPOTENCY_KEY_CHARS}
            }}),
        ),
        Shape::new(
            "PaymentReceiptInput",
            json!({"type": "object", "required": ["provider", "reference"], "additionalProperties": false, "properties": {
                "provider": {"type": "string", "minLength": 1, "maxLength": MAX_PAYMENT_VALUE_CHARS},
                "reference": {"type": "string", "minLength": 1, "maxLength": MAX_PAYMENT_VALUE_CHARS}
            }}),
        ),
        Shape::new(
            "OrderTransition",
            json!({"type": "object", "required": ["to"], "additionalProperties": false, "properties": {
                "to": {"$ref": "#/components/schemas/OrderState"},
                "payment": {"oneOf": [{"$ref": "#/components/schemas/PaymentReceiptInput"}, {"type": "null"}]}
            }}),
        ),
        Shape::new(
            "OrderLine",
            json!({"type": "object", "required": ["id", "product_id", "name", "each", "quantity"], "properties": {
                "id": {"type": "string", "format": "uuid"},
                "product_id": {"type": ["string", "null"], "format": "uuid"},
                "name": {"type": "string"},
                "each": {"$ref": "#/components/schemas/Money"},
                "quantity": {"type": "integer", "minimum": 1, "maximum": MAX_ITEM_QUANTITY}
            }}),
        ),
        Shape::new(
            "Order",
            json!({"type": "object", "required": ["id", "number", "state", "email", "total", "lines", "payment_provider", "payment_reference", "created_at", "updated_at"], "properties": {
                "id": {"type": "string", "format": "uuid"},
                "number": {"type": "integer", "format": "int64", "minimum": 1},
                "state": {"$ref": "#/components/schemas/OrderState"},
                "email": {"type": "string", "format": "email"},
                "total": {"$ref": "#/components/schemas/Money"},
                "lines": {"type": "array", "items": {"$ref": "#/components/schemas/OrderLine"}},
                "payment_provider": {"type": ["string", "null"]},
                "payment_reference": {"type": ["string", "null"]},
                "created_at": {"type": "string", "format": "date-time"},
                "updated_at": {"type": "string", "format": "date-time"}
            }}),
        ),
        Shape::new(
            "OrderSummary",
            json!({"type": "object", "required": ["id", "number", "state", "email", "total", "created_at", "updated_at"], "properties": {
                "id": {"type": "string", "format": "uuid"},
                "number": {"type": "integer", "format": "int64", "minimum": 1},
                "state": {"$ref": "#/components/schemas/OrderState"},
                "email": {"type": "string", "format": "email"},
                "total": {"$ref": "#/components/schemas/Money"},
                "created_at": {"type": "string", "format": "date-time"},
                "updated_at": {"type": "string", "format": "date-time"}
            }}),
        ),
        Shape::new(
            "CheckoutReceipt",
            json!({"type": "object", "required": ["id", "number", "state", "total"], "properties": {
                "id": {"type": "string", "format": "uuid"},
                "number": {"type": "integer", "format": "int64", "minimum": 1},
                "state": {"$ref": "#/components/schemas/OrderState"},
                "total": {"$ref": "#/components/schemas/Money"}
            }}),
        ),
        Shape::new(
            "OrderListFilter",
            json!({"type": "object", "properties": {
                "after": {"type": ["string", "null"], "maxLength": 512},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                "state": {"$ref": "#/components/schemas/OrderState"}
            }}),
        ),
        Shape::new(
            "OrderSummaryPage",
            json!({"type": "object", "required": ["items", "next_cursor"], "properties": {
                "items": {"type": "array", "items": {"$ref": "#/components/schemas/OrderSummary"}},
                "next_cursor": {"type": ["string", "null"], "maxLength": 512}
            }}),
        ),
    ]
}

impl ShopService {
    pub async fn list_orders(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        filter: &OrderListFilter,
    ) -> Result<Page<OrderSummary>> {
        let after = filter.page.after.as_ref().map(decode_cursor).transpose()?;
        let limit = i64::from(filter.page.effective_limit());
        let mut query = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "select id, number, state, email, total_minor, currency, created_at, updated_at
               from shop_orders where site_id = ",
        );
        query.push_bind(context.site_id.into_uuid());
        if let Some(state) = filter.state {
            query.push(" and state = ").push_bind(state.as_str());
        }
        if let Some(after) = after {
            query
                .push(" and (created_at, id) < (")
                .push_bind(after.created_at)
                .push(", ")
                .push_bind(after.id)
                .push(")");
        }
        let rows = query
            .push(" order by created_at desc, id desc limit ")
            .push_bind(limit + 1)
            .build()
            .fetch_all(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        let mut items = rows
            .iter()
            .map(summary_from_row)
            .collect::<Result<Vec<_>>>()?;
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

    pub async fn get_order(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: OrderId,
    ) -> Result<Order> {
        let row = sqlx::query(
            "select id, number, state, email, total_minor, currency, payment_provider,
                    payment_reference, created_at, updated_at
               from shop_orders where site_id = $1 and id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .ok_or(MaviError::NotFound {
            resource: ORDER_NOT_FOUND,
        })?;
        let mut order = order_from_row(&row)?;
        order.lines = load_lines(tx, context, order.id, order.total.currency).await?;
        Ok(order)
    }

    #[allow(clippy::too_many_lines)]
    pub async fn checkout(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        input: &CheckoutInput,
    ) -> Result<CheckoutReceipt> {
        let email = Email::parse(&input.email)
            .map_err(|_| MaviError::validation_field(ORDER_EMAIL_INVALID, "email"))?;
        let idempotency_key = validate_idempotency_key(&input.idempotency_key)?;
        let items = normalize_items(&input.items)?;

        if let Some(existing_id) = sqlx::query_scalar::<_, Uuid>(
            "select id from shop_orders
              where site_id = $1 and email = $2 and idempotency_key = $3",
        )
        .bind(context.site_id.into_uuid())
        .bind(email.as_str())
        .bind(&idempotency_key)
        .fetch_optional(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        {
            return self
                .get_order(tx, context, OrderId::from_uuid(existing_id))
                .await
                .map(|order| receipt(&order));
        }

        let mut snapshots = Vec::with_capacity(items.len());
        for item in &items {
            let product = products::lock_product(tx, context, item.product_id).await?;
            if !product.on_sale {
                return Err(MaviError::conflict(products::PRODUCT_NOT_FOR_SALE));
            }
            if product.stock < i32::try_from(item.quantity).map_err(|_| MaviError::Internal)? {
                return Err(MaviError::conflict(ORDER_STOCK_UNAVAILABLE));
            }
            snapshots.push((item, product));
        }
        let lines = snapshots
            .iter()
            .map(|(item, product)| OrderLineDraft {
                product_id: product.id,
                name: product.name.clone(),
                each: product.price,
                quantity: item.quantity,
            })
            .collect::<Vec<_>>();
        let mut total = total_for(&lines)?;
        let coupon = match input.coupon_code.as_deref() {
            Some(code) => {
                let coupon = coupons::load_for_checkout(tx, context, code).await?;
                total = coupons::discount(total, &coupon, Utc::now())?;
                Some(coupon)
            }
            None => None,
        };

        let number = next_order_number(tx, context).await?;
        let order_id = OrderId::new();
        let inserted = sqlx::query(
            "insert into shop_orders
                (site_id, id, number, state, email, total_minor, currency, idempotency_key)
             values ($1, $2, $3, 'waiting', $4, $5, $6, $7)
             on conflict do nothing
             returning id",
        )
        .bind(context.site_id.into_uuid())
        .bind(order_id.into_uuid())
        .bind(number)
        .bind(email.as_str())
        .bind(total.minor)
        .bind(total.currency.to_string())
        .bind(&idempotency_key)
        .fetch_optional(tx.conn())
        .await
        .map_err(|error| map_order_write_error(&error))?;
        let Some(_) = inserted else {
            let existing_id: Uuid = sqlx::query_scalar(
                "select id from shop_orders
                  where site_id = $1 and email = $2 and idempotency_key = $3",
            )
            .bind(context.site_id.into_uuid())
            .bind(email.as_str())
            .bind(&idempotency_key)
            .fetch_one(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
            return self
                .get_order(tx, context, OrderId::from_uuid(existing_id))
                .await
                .map(|order| receipt(&order));
        };

        let expires_at = Utc::now() + Duration::minutes(HOLD_MINUTES);
        for line in &lines {
            let quantity = i32::try_from(line.quantity).map_err(|_| MaviError::Internal)?;
            let updated = sqlx::query(
                "update shop_products set stock_on_hand = stock_on_hand - $3,
                        updated_at = clock_timestamp()
                  where site_id = $1 and id = $2 and stock_on_hand >= $3",
            )
            .bind(context.site_id.into_uuid())
            .bind(line.product_id.into_uuid())
            .bind(quantity)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
            if updated.rows_affected() == 0 {
                return Err(MaviError::conflict(ORDER_STOCK_UNAVAILABLE));
            }
            sqlx::query(
                "insert into shop_order_lines
                    (site_id, id, order_id, product_id, name, each_minor, quantity)
                 values ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(context.site_id.into_uuid())
            .bind(OrderLineId::new().into_uuid())
            .bind(order_id.into_uuid())
            .bind(line.product_id.into_uuid())
            .bind(&line.name)
            .bind(line.each.minor)
            .bind(quantity)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
            sqlx::query(
                "insert into shop_stock_holds
                    (site_id, id, order_id, product_id, quantity, status, expires_at)
                 values ($1, $2, $3, $4, $5, 'held', $6)",
            )
            .bind(context.site_id.into_uuid())
            .bind(StockHoldId::new().into_uuid())
            .bind(order_id.into_uuid())
            .bind(line.product_id.into_uuid())
            .bind(quantity)
            .bind(expires_at)
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }
        if let Some(coupon) = coupon {
            sqlx::query(
                "insert into shop_coupon_uses (site_id, id, coupon_id, order_id)
                 values ($1, $2, $3, $4)",
            )
            .bind(context.site_id.into_uuid())
            .bind(Uuid::now_v7())
            .bind(coupon.id.into_uuid())
            .bind(order_id.into_uuid())
            .execute(tx.conn())
            .await
            .map_err(|_| MaviError::Internal)?;
        }
        crate::audit(
            tx,
            context,
            "shop.order.created",
            "ShopOrder",
            order_id.into_uuid(),
            json!({"number": number, "total_minor": total.minor}),
        )
        .await?;
        let order = self.get_order(tx, context, order_id).await?;
        Ok(receipt(&order))
    }

    pub async fn transition_order(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        id: OrderId,
        input: &OrderTransition,
    ) -> Result<Order> {
        let row =
            sqlx::query("select state from shop_orders where site_id = $1 and id = $2 for update")
                .bind(context.site_id.into_uuid())
                .bind(id.into_uuid())
                .fetch_optional(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?
                .ok_or(MaviError::NotFound {
                    resource: ORDER_NOT_FOUND,
                })?;
        let from = OrderState::parse(
            &row.try_get::<String, _>("state")
                .map_err(|_| MaviError::Internal)?,
        )?;
        validate_transition(from, input.to)?;
        let payment = if input.to == OrderState::Paid {
            let payment = input
                .payment
                .as_ref()
                .ok_or_else(|| MaviError::validation_field(PAYMENT_RECEIPT_INVALID, "payment"))?;
            Some((
                validate_payment_value(&payment.provider)?,
                validate_payment_value(&payment.reference)?,
            ))
        } else {
            None
        };
        sqlx::query(
            "update shop_orders
                set state = $3,
                    paid_at = case when $3 = 'paid' then clock_timestamp() else paid_at end,
                    sent_at = case when $3 = 'sent' then clock_timestamp() else sent_at end,
                    called_off_at = case when $3 = 'called_off' then clock_timestamp() else called_off_at end,
                    given_back_at = case when $3 = 'given_back' then clock_timestamp() else given_back_at end,
                    payment_provider = coalesce($4, payment_provider),
                    payment_reference = coalesce($5, payment_reference),
                    updated_at = clock_timestamp()
              where site_id = $1 and id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(id.into_uuid())
        .bind(input.to.as_str())
        .bind(payment.as_ref().map(|values| &values.0))
        .bind(payment.as_ref().map(|values| &values.1))
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;

        match input.to {
            OrderState::Paid => consume_holds(tx, context, id).await?,
            OrderState::CalledOff => release_holds(tx, context, id, &['h']).await?,
            OrderState::GivenBack => release_holds(tx, context, id, &['h', 'c']).await?,
            OrderState::Waiting | OrderState::Sent => {}
        }
        crate::audit(
            tx,
            context,
            "shop.order.transitioned",
            "ShopOrder",
            id.into_uuid(),
            json!({"from": from, "to": input.to}),
        )
        .await?;
        self.get_order(tx, context, id).await
    }

    /// Releases waiting orders whose stock hold has expired. A worker calls
    /// this outside the HTTP request path; each transition is audited.
    pub async fn release_expired_holds(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        limit: u16,
    ) -> Result<u64> {
        let limit = i64::from(limit.clamp(1, 100));
        let ids = sqlx::query_scalar::<_, Uuid>(
            "select o.id from shop_orders o
              where o.site_id = $1 and o.state = 'waiting'
                and exists (select 1 from shop_stock_holds h
                              where h.site_id = o.site_id and h.order_id = o.id
                                and h.status = 'held' and h.expires_at <= clock_timestamp())
              order by o.created_at asc, o.id asc
              for update skip locked limit $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(limit)
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
        let mut released = 0_u64;
        for id in ids {
            self.transition_order(
                tx,
                context,
                OrderId::from_uuid(id),
                &OrderTransition {
                    to: OrderState::CalledOff,
                    payment: None,
                },
            )
            .await?;
            released += 1;
        }
        Ok(released)
    }
}

#[derive(Clone, Debug)]
struct OrderLineDraft {
    product_id: ProductId,
    name: String,
    each: Money,
    quantity: u32,
}

fn normalize_items(items: &[BasketItem]) -> Result<Vec<BasketItem>> {
    if items.is_empty() || items.len() > 100 {
        return Err(MaviError::validation(ORDER_ITEMS_INVALID));
    }
    let mut items = items.to_vec();
    items.sort_unstable_by_key(|item| item.product_id);
    let mut normalized: Vec<BasketItem> = Vec::with_capacity(items.len());
    for item in items {
        if item.quantity == 0 || item.quantity > MAX_ITEM_QUANTITY {
            return Err(MaviError::validation_field(
                ORDER_QUANTITY_INVALID,
                "items.quantity",
            ));
        }
        if let Some(previous) = normalized.last_mut()
            && previous.product_id == item.product_id
        {
            previous.quantity = previous
                .quantity
                .checked_add(item.quantity)
                .filter(|quantity| *quantity <= MAX_ITEM_QUANTITY)
                .ok_or_else(|| MaviError::validation(ORDER_QUANTITY_INVALID))?;
        } else {
            normalized.push(item);
        }
    }
    Ok(normalized)
}

fn total_for(lines: &[OrderLineDraft]) -> Result<Money> {
    let Some(first) = lines.first() else {
        return Err(MaviError::validation(ORDER_ITEMS_INVALID));
    };
    let mut total = Money::new(0, first.each.currency)?;
    for line in lines {
        if line.each.currency != total.currency {
            return Err(MaviError::validation(ORDER_CURRENCY_MISMATCH));
        }
        total = total.plus(line.each.times(line.quantity)?)?;
    }
    Ok(total)
}

fn validate_idempotency_key(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > MAX_IDEMPOTENCY_KEY_CHARS
        || value.chars().any(char::is_control)
    {
        return Err(MaviError::validation_field(
            ORDER_IDEMPOTENCY_INVALID,
            "idempotency_key",
        ));
    }
    Ok(value.to_owned())
}

fn validate_payment_value(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > MAX_PAYMENT_VALUE_CHARS
        || value.chars().any(char::is_control)
    {
        return Err(MaviError::validation(PAYMENT_RECEIPT_INVALID));
    }
    Ok(value.to_owned())
}

fn validate_transition(from: OrderState, to: OrderState) -> Result<()> {
    let allowed = matches!(
        (from, to),
        (
            OrderState::Waiting,
            OrderState::Paid | OrderState::CalledOff
        ) | (OrderState::Paid, OrderState::Sent | OrderState::GivenBack)
            | (OrderState::Sent, OrderState::GivenBack)
    );
    if allowed {
        return Ok(());
    }
    Err(MaviError::conflict(ORDER_TRANSITION_INVALID))
}

async fn next_order_number(tx: &mut SiteTx, context: &SiteContext) -> Result<i64> {
    sqlx::query(
        "insert into shop_order_counters (site_id, next_number) values ($1, 2)
         on conflict (site_id) do nothing",
    )
    .bind(context.site_id.into_uuid())
    .execute(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    sqlx::query_scalar(
        "update shop_order_counters set next_number = next_number + 1
          where site_id = $1 returning next_number - 1",
    )
    .bind(context.site_id.into_uuid())
    .fetch_one(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)
}

async fn load_lines(
    tx: &mut SiteTx,
    context: &SiteContext,
    order_id: OrderId,
    currency: Currency,
) -> Result<Vec<OrderLine>> {
    let rows = sqlx::query(
        "select id, product_id, name, each_minor, quantity
           from shop_order_lines
          where site_id = $1 and order_id = $2
          order by created_at asc, id asc",
    )
    .bind(context.site_id.into_uuid())
    .bind(order_id.into_uuid())
    .fetch_all(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    rows.iter()
        .map(|row| {
            let quantity: i32 = row.try_get("quantity").map_err(|_| MaviError::Internal)?;
            Ok(OrderLine {
                id: OrderLineId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
                product_id: row
                    .try_get::<Option<Uuid>, _>("product_id")
                    .map_err(|_| MaviError::Internal)?
                    .map(ProductId::from_uuid),
                name: row.try_get("name").map_err(|_| MaviError::Internal)?,
                each: Money::new(
                    row.try_get("each_minor").map_err(|_| MaviError::Internal)?,
                    currency,
                )?,
                quantity: u32::try_from(quantity).map_err(|_| MaviError::Internal)?,
            })
        })
        .collect()
}

async fn consume_holds(tx: &mut SiteTx, context: &SiteContext, order_id: OrderId) -> Result<()> {
    sqlx::query(
        "update shop_stock_holds set status = 'consumed', settled_at = clock_timestamp()
          where site_id = $1 and order_id = $2 and status = 'held'",
    )
    .bind(context.site_id.into_uuid())
    .bind(order_id.into_uuid())
    .execute(tx.conn())
    .await
    .map_err(|_| MaviError::Internal)?;
    Ok(())
}

async fn release_holds(
    tx: &mut SiteTx,
    context: &SiteContext,
    order_id: OrderId,
    statuses: &[char],
) -> Result<()> {
    let include_consumed = statuses.contains(&'c');
    let rows = if include_consumed {
        sqlx::query(
            "update shop_stock_holds set status = 'released', settled_at = clock_timestamp()
              where site_id = $1 and order_id = $2 and status in ('held', 'consumed')
             returning product_id, quantity",
        )
        .bind(context.site_id.into_uuid())
        .bind(order_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
    } else {
        sqlx::query(
            "update shop_stock_holds set status = 'released', settled_at = clock_timestamp()
              where site_id = $1 and order_id = $2 and status = 'held'
             returning product_id, quantity",
        )
        .bind(context.site_id.into_uuid())
        .bind(order_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
    };
    for row in rows {
        let product_id: Uuid = row.try_get("product_id").map_err(|_| MaviError::Internal)?;
        let quantity: i32 = row.try_get("quantity").map_err(|_| MaviError::Internal)?;
        sqlx::query(
            "update shop_products set stock_on_hand = stock_on_hand + $3,
                    updated_at = clock_timestamp()
              where site_id = $1 and id = $2",
        )
        .bind(context.site_id.into_uuid())
        .bind(product_id)
        .bind(quantity)
        .execute(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?;
    }
    Ok(())
}

fn receipt(order: &Order) -> CheckoutReceipt {
    CheckoutReceipt {
        id: order.id,
        number: order.number,
        state: order.state,
        total: order.total,
    }
}

fn summary_from_row(row: &sqlx::postgres::PgRow) -> Result<OrderSummary> {
    let order = base_order_from_row(row)?;
    Ok(OrderSummary {
        id: order.id,
        number: order.number,
        state: order.state,
        email: order.email,
        total: order.total,
        created_at: order.created_at,
        updated_at: order.updated_at,
    })
}

fn order_from_row(row: &sqlx::postgres::PgRow) -> Result<Order> {
    base_order_from_row(row)
}

fn base_order_from_row(row: &sqlx::postgres::PgRow) -> Result<Order> {
    let currency = Currency::parse(
        &row.try_get::<String, _>("currency")
            .map_err(|_| MaviError::Internal)?,
    )?;
    Ok(Order {
        id: OrderId::from_uuid(row.try_get("id").map_err(|_| MaviError::Internal)?),
        number: row.try_get("number").map_err(|_| MaviError::Internal)?,
        state: OrderState::parse(
            &row.try_get::<String, _>("state")
                .map_err(|_| MaviError::Internal)?,
        )?,
        email: row.try_get("email").map_err(|_| MaviError::Internal)?,
        total: Money::new(
            row.try_get("total_minor")
                .map_err(|_| MaviError::Internal)?,
            currency,
        )?,
        lines: Vec::new(),
        payment_provider: row.try_get("payment_provider").unwrap_or(None),
        payment_reference: row.try_get("payment_reference").unwrap_or(None),
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

fn map_order_write_error(error: &sqlx::Error) -> MaviError {
    if let sqlx::Error::Database(database) = error
        && database.constraint() == Some("shop_orders_site_email_idempotency")
    {
        return MaviError::conflict(ORDER_IDEMPOTENCY_INVALID);
    }
    MaviError::Internal
}

#[cfg(test)]
mod tests {
    use super::*;

    fn product(value: u128, quantity: u32) -> BasketItem {
        BasketItem {
            product_id: ProductId::from_uuid(Uuid::from_u128(value)),
            quantity,
        }
    }

    #[test]
    fn basket_items_are_sorted_merged_and_bounded() {
        let items = normalize_items(&[product(2, 2), product(1, 1), product(2, 3)]).expect("items");
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0].product_id,
            ProductId::from_uuid(Uuid::from_u128(1))
        );
        assert_eq!(items[1].quantity, 5);
        assert!(normalize_items(&[product(1, 0)]).is_err());
    }

    #[test]
    fn order_state_machine_never_sends_unpaid_stock() {
        assert!(validate_transition(OrderState::Waiting, OrderState::Paid).is_ok());
        assert!(validate_transition(OrderState::Paid, OrderState::Sent).is_ok());
        assert!(validate_transition(OrderState::Waiting, OrderState::Sent).is_err());
        assert!(validate_transition(OrderState::GivenBack, OrderState::Paid).is_err());
    }

    #[test]
    fn order_contract_is_cursor_only_and_public_checkout_is_idempotent() {
        let contract = serde_json::to_string(&api()).expect("contract");
        assert!(contract.contains("shop.public.orders.checkout"));
        assert!(contract.contains("\"idempotent\":true"));
        assert!(!contract.contains("offset"));
        assert!(!contract.contains("page_number"));
    }
}
