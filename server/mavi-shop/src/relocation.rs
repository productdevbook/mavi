use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use mavi_audit::{AuditEntry, AuditService};
use mavi_core::{Currency, Email, MaviError, Result, SiteContext, SiteId};
use mavi_storage::SiteTx;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use super::{CouponKind, OrderState, ShopService};

pub const SHOP_RELOCATION_FORMAT: &str = "mavi.shop.relocation";
pub const SHOP_RELOCATION_VERSION: u16 = 1;
pub const MAX_SHOP_RELOCATION_RECORDS: usize = 100_000;
pub const MAX_SHOP_RELOCATION_BYTES: usize = 256 * 1024 * 1024;
pub const SHOP_RELOCATION_CONFLICT: &str = "shop_relocation_conflict";

/// Authenticated shard-relocation data for the commerce domain.
///
/// This adapter intentionally carries order/customer snapshots and stock
/// accounting, but it does not carry payment-provider credentials or a live
/// checkout session. A waiting order has no transferable payment receipt;
/// provider/reference values found on such a row are discarded on export so a
/// target worker cannot accidentally resume an old payment attempt.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShopRelocation {
    pub format: String,
    pub version: u16,
    pub source_site_id: SiteId,
    pub products: Vec<ShopProductRelocation>,
    pub coupons: Vec<ShopCouponRelocation>,
    pub order_counter: Option<ShopOrderCounterRelocation>,
    pub orders: Vec<ShopOrderRelocation>,
    pub order_lines: Vec<ShopOrderLineRelocation>,
    pub stock_holds: Vec<ShopStockHoldRelocation>,
    pub coupon_uses: Vec<ShopCouponUseRelocation>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShopProductRelocation {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub price_minor: i64,
    pub currency: String,
    pub stock_on_hand: i32,
    pub on_sale: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShopCouponRelocation {
    pub id: Uuid,
    pub code: String,
    pub kind: CouponKind,
    pub percent: Option<i32>,
    pub amount_minor: Option<i64>,
    pub currency: Option<String>,
    pub max_uses: Option<i64>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShopOrderCounterRelocation {
    pub next_number: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShopPaymentReceiptRelocation {
    pub provider: String,
    pub reference: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShopOrderRelocation {
    pub id: Uuid,
    pub number: i64,
    pub state: OrderState,
    pub email: String,
    pub total_minor: i64,
    pub currency: String,
    pub idempotency_key: String,
    pub payment: Option<ShopPaymentReceiptRelocation>,
    pub paid_at: Option<DateTime<Utc>>,
    pub sent_at: Option<DateTime<Utc>>,
    pub called_off_at: Option<DateTime<Utc>>,
    pub given_back_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShopOrderLineRelocation {
    pub id: Uuid,
    pub order_id: Uuid,
    pub product_id: Option<Uuid>,
    pub name: String,
    pub each_minor: i64,
    pub quantity: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShopStockHoldStatus {
    Held,
    Consumed,
    Released,
}

impl ShopStockHoldStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Held => "held",
            Self::Consumed => "consumed",
            Self::Released => "released",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShopStockHoldRelocation {
    pub id: Uuid,
    pub order_id: Uuid,
    pub product_id: Uuid,
    pub quantity: i32,
    pub status: ShopStockHoldStatus,
    pub expires_at: DateTime<Utc>,
    pub settled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ShopCouponUseRelocation {
    pub id: Uuid,
    pub coupon_id: Uuid,
    pub order_id: Uuid,
    pub used_at: DateTime<Utc>,
}

impl ShopRelocation {
    #[must_use]
    pub fn empty(source_site_id: SiteId) -> Self {
        Self {
            format: SHOP_RELOCATION_FORMAT.to_owned(),
            version: SHOP_RELOCATION_VERSION,
            source_site_id,
            products: Vec::new(),
            coupons: Vec::new(),
            order_counter: None,
            orders: Vec::new(),
            order_lines: Vec::new(),
            stock_holds: Vec::new(),
            coupon_uses: Vec::new(),
        }
    }

    #[allow(clippy::too_many_lines)]
    pub fn validate_for_relocation(&self, target_site: SiteId) -> Result<()> {
        if self.format != SHOP_RELOCATION_FORMAT {
            return Err(MaviError::validation("shop_relocation_format_invalid"));
        }
        if self.version != SHOP_RELOCATION_VERSION {
            return Err(MaviError::validation("shop_relocation_version_unsupported"));
        }
        if self.source_site_id != target_site || self.source_site_id.into_uuid().is_nil() {
            return Err(MaviError::conflict("shop_relocation_site_mismatch"));
        }

        let sections = [
            self.products.len(),
            self.coupons.len(),
            usize::from(self.order_counter.is_some()),
            self.orders.len(),
            self.order_lines.len(),
            self.stock_holds.len(),
            self.coupon_uses.len(),
        ];
        if sections
            .iter()
            .any(|count| *count > MAX_SHOP_RELOCATION_RECORDS)
            || sections
                .iter()
                .try_fold(0usize, |total, count| total.checked_add(*count))
                .is_none_or(|total| total > MAX_SHOP_RELOCATION_RECORDS)
        {
            return Err(MaviError::validation("shop_relocation_counts_invalid"));
        }

        let mut product_ids = BTreeSet::new();
        let mut active_product_slugs = BTreeSet::new();
        for product in &self.products {
            if product.id.is_nil()
                || !product_ids.insert(product.id)
                || !valid_slug(&product.slug)
                || !valid_text(&product.name, 300)
                || !product.description.as_deref().is_none_or(valid_description)
                || product.price_minor < 0
                || !valid_currency(&product.currency)
                || !(0..=1_000_000_000).contains(&product.stock_on_hand)
                || (product.deleted_at.is_none()
                    && !active_product_slugs.insert(product.slug.clone()))
            {
                return Err(MaviError::validation("shop_relocation_product_invalid"));
            }
        }

        let mut coupon_ids = BTreeSet::new();
        let mut active_coupon_codes = BTreeSet::new();
        for coupon in &self.coupons {
            let valid_rule = match coupon.kind {
                CouponKind::Percent => {
                    coupon
                        .percent
                        .is_some_and(|value| (1..=100).contains(&value))
                        && coupon.amount_minor.is_none()
                        && coupon.currency.is_none()
                }
                CouponKind::Amount => {
                    coupon.percent.is_none()
                        && coupon.amount_minor.is_some_and(|value| value > 0)
                        && coupon.currency.as_deref().is_some_and(valid_currency)
                }
            };
            if coupon.id.is_nil()
                || !coupon_ids.insert(coupon.id)
                || !valid_coupon_code(&coupon.code)
                || !valid_rule
                || coupon
                    .max_uses
                    .is_some_and(|value| !(1..=1_000_000_000).contains(&value))
                || (coupon.deleted_at.is_none() && !active_coupon_codes.insert(coupon.code.clone()))
            {
                return Err(MaviError::validation("shop_relocation_coupon_invalid"));
            }
        }

        let mut order_ids = BTreeSet::new();
        let mut order_numbers = BTreeSet::new();
        let mut highest_order_number = 0_i64;
        for order in &self.orders {
            let settled_state = matches!(
                order.state,
                OrderState::Paid | OrderState::Sent | OrderState::GivenBack
            );
            let valid_payment = order.payment.as_ref().is_none_or(|payment| {
                settled_state
                    && valid_payment_value(&payment.provider)
                    && valid_payment_value(&payment.reference)
            });
            let valid_timestamps = (!matches!(
                order.state,
                OrderState::Paid | OrderState::Sent | OrderState::GivenBack
            ) || order.paid_at.is_some())
                && (!matches!(order.state, OrderState::Sent) || order.sent_at.is_some())
                && (matches!(order.state, OrderState::CalledOff) == order.called_off_at.is_some())
                && (matches!(order.state, OrderState::GivenBack) == order.given_back_at.is_some());
            if order.id.is_nil()
                || !order_ids.insert(order.id)
                || order.number <= 0
                || !order_numbers.insert(order.number)
                || !valid_email(&order.email)
                || order.total_minor < 0
                || !valid_currency(&order.currency)
                || !valid_idempotency_key(&order.idempotency_key)
                || !valid_payment
                || !valid_timestamps
            {
                return Err(MaviError::validation("shop_relocation_order_invalid"));
            }
            highest_order_number = highest_order_number.max(order.number);
        }

        if self.orders.is_empty() && self.order_counter.is_some() {
            let counter = self.order_counter.as_ref().ok_or(MaviError::Internal)?;
            if counter.next_number <= 0 {
                return Err(MaviError::validation(
                    "shop_relocation_order_counter_invalid",
                ));
            }
        } else if let Some(counter) = &self.order_counter {
            if counter.next_number <= highest_order_number {
                return Err(MaviError::validation(
                    "shop_relocation_order_counter_invalid",
                ));
            }
        } else if !self.orders.is_empty() {
            return Err(MaviError::validation(
                "shop_relocation_order_counter_missing",
            ));
        }

        let mut line_ids = BTreeSet::new();
        for line in &self.order_lines {
            if line.id.is_nil()
                || !line_ids.insert(line.id)
                || !order_ids.contains(&line.order_id)
                || line.product_id.is_some_and(|id| !product_ids.contains(&id))
                || !valid_text(&line.name, 300)
                || line.each_minor < 0
                || !(1..=1_000).contains(&line.quantity)
            {
                return Err(MaviError::validation("shop_relocation_order_line_invalid"));
            }
        }

        let mut hold_ids = BTreeSet::new();
        for hold in &self.stock_holds {
            let valid_settlement = match hold.status {
                ShopStockHoldStatus::Held => hold.settled_at.is_none(),
                ShopStockHoldStatus::Consumed | ShopStockHoldStatus::Released => {
                    hold.settled_at.is_some()
                }
            };
            if hold.id.is_nil()
                || !hold_ids.insert(hold.id)
                || !order_ids.contains(&hold.order_id)
                || !product_ids.contains(&hold.product_id)
                || !(1..=1_000).contains(&hold.quantity)
                || !valid_settlement
            {
                return Err(MaviError::validation("shop_relocation_stock_hold_invalid"));
            }
        }

        let mut use_ids = BTreeSet::new();
        let mut use_pairs = BTreeSet::new();
        for coupon_use in &self.coupon_uses {
            if coupon_use.id.is_nil()
                || !use_ids.insert(coupon_use.id)
                || !coupon_ids.contains(&coupon_use.coupon_id)
                || !order_ids.contains(&coupon_use.order_id)
                || !use_pairs.insert((coupon_use.coupon_id, coupon_use.order_id))
            {
                return Err(MaviError::validation("shop_relocation_coupon_use_invalid"));
            }
        }

        let bytes = serde_json::to_vec(self).map_err(|_| MaviError::Internal)?;
        if bytes.len() > MAX_SHOP_RELOCATION_BYTES {
            return Err(MaviError::validation("shop_relocation_too_large"));
        }
        Ok(())
    }

    pub fn record_count(&self) -> Result<i64> {
        let count = self
            .products
            .len()
            .checked_add(self.coupons.len())
            .and_then(|value| value.checked_add(usize::from(self.order_counter.is_some())))
            .and_then(|value| value.checked_add(self.orders.len()))
            .and_then(|value| value.checked_add(self.order_lines.len()))
            .and_then(|value| value.checked_add(self.stock_holds.len()))
            .and_then(|value| value.checked_add(self.coupon_uses.len()))
            .ok_or(MaviError::validation("shop_relocation_count_overflow"))?;
        i64::try_from(count).map_err(|_| MaviError::validation("shop_relocation_count_overflow"))
    }
}

impl ShopService {
    #[allow(clippy::too_many_lines)]
    pub async fn export_for_relocation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
    ) -> Result<ShopRelocation> {
        let products = sqlx::query(
            "select id, slug, name, description, price_minor, currency, stock_on_hand,
                    on_sale, created_at, updated_at, deleted_at
               from shop_products where site_id = $1 order by created_at asc, id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(product_from_row)
        .collect::<Result<Vec<_>>>()?;

        let coupons = sqlx::query(
            "select id, code, kind, percent, amount_minor, currency, max_uses, expires_at,
                    created_at, updated_at, deleted_at
               from shop_coupons where site_id = $1 order by created_at asc, id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(coupon_from_row)
        .collect::<Result<Vec<_>>>()?;

        let order_counter =
            sqlx::query("select next_number from shop_order_counters where site_id = $1")
                .bind(context.site_id.into_uuid())
                .fetch_optional(tx.conn())
                .await
                .map_err(|_| MaviError::Internal)?
                .map(|row| {
                    Ok(ShopOrderCounterRelocation {
                        next_number: row
                            .try_get("next_number")
                            .map_err(|_| MaviError::Internal)?,
                    })
                })
                .transpose()?;

        let orders = sqlx::query(
            "select id, number, state, email, total_minor, currency, idempotency_key,
                    payment_provider, payment_reference, paid_at, sent_at, called_off_at,
                    given_back_at, created_at, updated_at
               from shop_orders where site_id = $1 order by created_at asc, id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(order_from_row)
        .collect::<Result<Vec<_>>>()?;

        let order_lines = sqlx::query(
            "select id, order_id, product_id, name, each_minor, quantity, created_at
               from shop_order_lines where site_id = $1
              order by order_id asc, created_at asc, id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(ShopOrderLineRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                order_id: row.try_get("order_id").map_err(|_| MaviError::Internal)?,
                product_id: row.try_get("product_id").map_err(|_| MaviError::Internal)?,
                name: row.try_get("name").map_err(|_| MaviError::Internal)?,
                each_minor: row.try_get("each_minor").map_err(|_| MaviError::Internal)?,
                quantity: row.try_get("quantity").map_err(|_| MaviError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let stock_holds = sqlx::query(
            "select id, order_id, product_id, quantity, status, expires_at, settled_at, created_at
               from shop_stock_holds where site_id = $1
              order by order_id asc, created_at asc, id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(ShopStockHoldRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                order_id: row.try_get("order_id").map_err(|_| MaviError::Internal)?,
                product_id: row.try_get("product_id").map_err(|_| MaviError::Internal)?,
                quantity: row.try_get("quantity").map_err(|_| MaviError::Internal)?,
                status: parse_hold_status(
                    row.try_get::<String, _>("status")
                        .map_err(|_| MaviError::Internal)?
                        .as_str(),
                )?,
                expires_at: row.try_get("expires_at").map_err(|_| MaviError::Internal)?,
                settled_at: row.try_get("settled_at").map_err(|_| MaviError::Internal)?,
                created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let coupon_uses = sqlx::query(
            "select id, coupon_id, order_id, used_at
               from shop_coupon_uses where site_id = $1
              order by coupon_id asc, used_at asc, id asc",
        )
        .bind(context.site_id.into_uuid())
        .fetch_all(tx.conn())
        .await
        .map_err(|_| MaviError::Internal)?
        .iter()
        .map(|row| {
            Ok(ShopCouponUseRelocation {
                id: row.try_get("id").map_err(|_| MaviError::Internal)?,
                coupon_id: row.try_get("coupon_id").map_err(|_| MaviError::Internal)?,
                order_id: row.try_get("order_id").map_err(|_| MaviError::Internal)?,
                used_at: row.try_get("used_at").map_err(|_| MaviError::Internal)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

        let relocation = ShopRelocation {
            format: SHOP_RELOCATION_FORMAT.to_owned(),
            version: SHOP_RELOCATION_VERSION,
            source_site_id: context.site_id,
            products,
            coupons,
            order_counter,
            orders,
            order_lines,
            stock_holds,
            coupon_uses,
        };
        relocation.validate_for_relocation(context.site_id)?;
        Ok(relocation)
    }

    #[allow(clippy::too_many_lines)]
    pub async fn import_for_relocation(
        &self,
        tx: &mut SiteTx,
        context: &SiteContext,
        relocation: &ShopRelocation,
    ) -> Result<()> {
        relocation.validate_for_relocation(context.site_id)?;

        for product in &relocation.products {
            sqlx::query(
                "insert into shop_products
                    (site_id, id, slug, name, description, price_minor, currency,
                     stock_on_hand, on_sale, created_at, updated_at, deleted_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                 on conflict (site_id, id) do update set
                    slug = excluded.slug, name = excluded.name, description = excluded.description,
                    price_minor = excluded.price_minor, currency = excluded.currency,
                    stock_on_hand = excluded.stock_on_hand, on_sale = excluded.on_sale,
                    created_at = excluded.created_at, updated_at = excluded.updated_at,
                    deleted_at = excluded.deleted_at",
            )
            .bind(context.site_id.into_uuid())
            .bind(product.id)
            .bind(&product.slug)
            .bind(&product.name)
            .bind(&product.description)
            .bind(product.price_minor)
            .bind(&product.currency)
            .bind(product.stock_on_hand)
            .bind(product.on_sale)
            .bind(product.created_at)
            .bind(product.updated_at)
            .bind(product.deleted_at)
            .execute(tx.conn())
            .await
            .map_err(|error| map_write_error(&error))?;
        }

        for coupon in &relocation.coupons {
            sqlx::query(
                "insert into shop_coupons
                    (site_id, id, code, kind, percent, amount_minor, currency, max_uses,
                     expires_at, created_at, updated_at, deleted_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                 on conflict (site_id, id) do update set
                    code = excluded.code, kind = excluded.kind, percent = excluded.percent,
                    amount_minor = excluded.amount_minor, currency = excluded.currency,
                    max_uses = excluded.max_uses, expires_at = excluded.expires_at,
                    created_at = excluded.created_at, updated_at = excluded.updated_at,
                    deleted_at = excluded.deleted_at",
            )
            .bind(context.site_id.into_uuid())
            .bind(coupon.id)
            .bind(&coupon.code)
            .bind(coupon.kind.as_str())
            .bind(coupon.percent)
            .bind(coupon.amount_minor)
            .bind(&coupon.currency)
            .bind(coupon.max_uses)
            .bind(coupon.expires_at)
            .bind(coupon.created_at)
            .bind(coupon.updated_at)
            .bind(coupon.deleted_at)
            .execute(tx.conn())
            .await
            .map_err(|error| map_write_error(&error))?;
        }

        if let Some(counter) = &relocation.order_counter {
            sqlx::query(
                "insert into shop_order_counters (site_id, next_number)
                 values ($1, $2)
                 on conflict (site_id) do update set
                    next_number = greatest(shop_order_counters.next_number, excluded.next_number)",
            )
            .bind(context.site_id.into_uuid())
            .bind(counter.next_number)
            .execute(tx.conn())
            .await
            .map_err(|error| map_write_error(&error))?;
        }

        for order in &relocation.orders {
            let payment_provider = order
                .payment
                .as_ref()
                .map(|payment| payment.provider.as_str());
            let payment_reference = order
                .payment
                .as_ref()
                .map(|payment| payment.reference.as_str());
            sqlx::query(
                "insert into shop_orders
                    (site_id, id, number, state, email, total_minor, currency, idempotency_key,
                     payment_provider, payment_reference, paid_at, sent_at, called_off_at,
                     given_back_at, created_at, updated_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
                 on conflict (site_id, id) do update set
                    number = excluded.number, state = excluded.state, email = excluded.email,
                    total_minor = excluded.total_minor, currency = excluded.currency,
                    idempotency_key = excluded.idempotency_key,
                    payment_provider = excluded.payment_provider,
                    payment_reference = excluded.payment_reference,
                    paid_at = excluded.paid_at, sent_at = excluded.sent_at,
                    called_off_at = excluded.called_off_at, given_back_at = excluded.given_back_at,
                    created_at = excluded.created_at, updated_at = excluded.updated_at",
            )
            .bind(context.site_id.into_uuid())
            .bind(order.id)
            .bind(order.number)
            .bind(order.state.as_str())
            .bind(&order.email)
            .bind(order.total_minor)
            .bind(&order.currency)
            .bind(&order.idempotency_key)
            .bind(payment_provider)
            .bind(payment_reference)
            .bind(order.paid_at)
            .bind(order.sent_at)
            .bind(order.called_off_at)
            .bind(order.given_back_at)
            .bind(order.created_at)
            .bind(order.updated_at)
            .execute(tx.conn())
            .await
            .map_err(|error| map_write_error(&error))?;
        }

        for line in &relocation.order_lines {
            sqlx::query(
                "insert into shop_order_lines
                    (site_id, id, order_id, product_id, name, each_minor, quantity, created_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8)
                 on conflict (site_id, id) do update set
                    order_id = excluded.order_id, product_id = excluded.product_id,
                    name = excluded.name, each_minor = excluded.each_minor,
                    quantity = excluded.quantity, created_at = excluded.created_at",
            )
            .bind(context.site_id.into_uuid())
            .bind(line.id)
            .bind(line.order_id)
            .bind(line.product_id)
            .bind(&line.name)
            .bind(line.each_minor)
            .bind(line.quantity)
            .bind(line.created_at)
            .execute(tx.conn())
            .await
            .map_err(|error| map_write_error(&error))?;
        }

        for hold in &relocation.stock_holds {
            sqlx::query(
                "insert into shop_stock_holds
                    (site_id, id, order_id, product_id, quantity, status, expires_at,
                     settled_at, created_at)
                 values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 on conflict (site_id, id) do update set
                    order_id = excluded.order_id, product_id = excluded.product_id,
                    quantity = excluded.quantity, status = excluded.status,
                    expires_at = excluded.expires_at, settled_at = excluded.settled_at,
                    created_at = excluded.created_at",
            )
            .bind(context.site_id.into_uuid())
            .bind(hold.id)
            .bind(hold.order_id)
            .bind(hold.product_id)
            .bind(hold.quantity)
            .bind(hold.status.as_str())
            .bind(hold.expires_at)
            .bind(hold.settled_at)
            .bind(hold.created_at)
            .execute(tx.conn())
            .await
            .map_err(|error| map_write_error(&error))?;
        }

        for coupon_use in &relocation.coupon_uses {
            sqlx::query(
                "insert into shop_coupon_uses (site_id, id, coupon_id, order_id, used_at)
                 values ($1, $2, $3, $4, $5)
                 on conflict (site_id, id) do update set
                    coupon_id = excluded.coupon_id, order_id = excluded.order_id,
                    used_at = excluded.used_at",
            )
            .bind(context.site_id.into_uuid())
            .bind(coupon_use.id)
            .bind(coupon_use.coupon_id)
            .bind(coupon_use.order_id)
            .bind(coupon_use.used_at)
            .execute(tx.conn())
            .await
            .map_err(|error| map_write_error(&error))?;
        }

        AuditService
            .record(
                tx,
                context,
                &AuditEntry {
                    action: "portable.shop.relocated".to_owned(),
                    resource_type: "ShopRelocation".to_owned(),
                    resource_id: None,
                    payload: serde_json::json!({
                        "products": relocation.products.len(),
                        "coupons": relocation.coupons.len(),
                        "orders": relocation.orders.len(),
                        "order_lines": relocation.order_lines.len(),
                        "stock_holds": relocation.stock_holds.len(),
                        "coupon_uses": relocation.coupon_uses.len(),
                        "payment_credentials_transferred": false,
                        "live_payment_state_transferred": false,
                    }),
                },
            )
            .await
    }
}

fn product_from_row(row: &sqlx::postgres::PgRow) -> Result<ShopProductRelocation> {
    Ok(ShopProductRelocation {
        id: row.try_get("id").map_err(|_| MaviError::Internal)?,
        slug: row.try_get("slug").map_err(|_| MaviError::Internal)?,
        name: row.try_get("name").map_err(|_| MaviError::Internal)?,
        description: row
            .try_get("description")
            .map_err(|_| MaviError::Internal)?,
        price_minor: row
            .try_get("price_minor")
            .map_err(|_| MaviError::Internal)?,
        currency: row.try_get("currency").map_err(|_| MaviError::Internal)?,
        stock_on_hand: row
            .try_get("stock_on_hand")
            .map_err(|_| MaviError::Internal)?,
        on_sale: row.try_get("on_sale").map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
        deleted_at: row.try_get("deleted_at").map_err(|_| MaviError::Internal)?,
    })
}

fn coupon_from_row(row: &sqlx::postgres::PgRow) -> Result<ShopCouponRelocation> {
    Ok(ShopCouponRelocation {
        id: row.try_get("id").map_err(|_| MaviError::Internal)?,
        code: row.try_get("code").map_err(|_| MaviError::Internal)?,
        kind: parse_coupon_kind(
            row.try_get::<String, _>("kind")
                .map_err(|_| MaviError::Internal)?
                .as_str(),
        )?,
        percent: row.try_get("percent").map_err(|_| MaviError::Internal)?,
        amount_minor: row
            .try_get("amount_minor")
            .map_err(|_| MaviError::Internal)?,
        currency: row.try_get("currency").map_err(|_| MaviError::Internal)?,
        max_uses: row.try_get("max_uses").map_err(|_| MaviError::Internal)?,
        expires_at: row.try_get("expires_at").map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
        deleted_at: row.try_get("deleted_at").map_err(|_| MaviError::Internal)?,
    })
}

fn order_from_row(row: &sqlx::postgres::PgRow) -> Result<ShopOrderRelocation> {
    let state = parse_order_state(
        row.try_get::<String, _>("state")
            .map_err(|_| MaviError::Internal)?
            .as_str(),
    )?;
    let provider: Option<String> = row
        .try_get("payment_provider")
        .map_err(|_| MaviError::Internal)?;
    let reference: Option<String> = row
        .try_get("payment_reference")
        .map_err(|_| MaviError::Internal)?;
    let payment = if matches!(
        state,
        OrderState::Paid | OrderState::Sent | OrderState::GivenBack
    ) {
        match (provider, reference) {
            (Some(provider), Some(reference)) => Some(ShopPaymentReceiptRelocation {
                provider,
                reference,
            }),
            (None, None) => None,
            _ => return Err(MaviError::Internal),
        }
    } else {
        // A waiting/called-off row may contain legacy provider columns. They
        // are live-payment state, not a business receipt, and are intentionally
        // omitted from the trusted relocation envelope.
        None
    };
    Ok(ShopOrderRelocation {
        id: row.try_get("id").map_err(|_| MaviError::Internal)?,
        number: row.try_get("number").map_err(|_| MaviError::Internal)?,
        state,
        email: row.try_get("email").map_err(|_| MaviError::Internal)?,
        total_minor: row
            .try_get("total_minor")
            .map_err(|_| MaviError::Internal)?,
        currency: row.try_get("currency").map_err(|_| MaviError::Internal)?,
        idempotency_key: row
            .try_get("idempotency_key")
            .map_err(|_| MaviError::Internal)?,
        payment,
        paid_at: row.try_get("paid_at").map_err(|_| MaviError::Internal)?,
        sent_at: row.try_get("sent_at").map_err(|_| MaviError::Internal)?,
        called_off_at: row
            .try_get("called_off_at")
            .map_err(|_| MaviError::Internal)?,
        given_back_at: row
            .try_get("given_back_at")
            .map_err(|_| MaviError::Internal)?,
        created_at: row.try_get("created_at").map_err(|_| MaviError::Internal)?,
        updated_at: row.try_get("updated_at").map_err(|_| MaviError::Internal)?,
    })
}

fn parse_order_state(value: &str) -> Result<OrderState> {
    match value {
        "waiting" => Ok(OrderState::Waiting),
        "paid" => Ok(OrderState::Paid),
        "sent" => Ok(OrderState::Sent),
        "called_off" => Ok(OrderState::CalledOff),
        "given_back" => Ok(OrderState::GivenBack),
        _ => Err(MaviError::Internal),
    }
}

fn parse_coupon_kind(value: &str) -> Result<CouponKind> {
    match value {
        "percent" => Ok(CouponKind::Percent),
        "amount" => Ok(CouponKind::Amount),
        _ => Err(MaviError::Internal),
    }
}

fn parse_hold_status(value: &str) -> Result<ShopStockHoldStatus> {
    match value {
        "held" => Ok(ShopStockHoldStatus::Held),
        "consumed" => Ok(ShopStockHoldStatus::Consumed),
        "released" => Ok(ShopStockHoldStatus::Released),
        _ => Err(MaviError::Internal),
    }
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= 160
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

fn valid_text(value: &str, max_chars: usize) -> bool {
    !value.is_empty() && value.chars().count() <= max_chars && !value.chars().any(char::is_control)
}

fn valid_description(value: &str) -> bool {
    value.chars().count() <= 10_000 && !value.contains('\0')
}

fn valid_currency(value: &str) -> bool {
    Currency::parse(value).is_ok()
}

fn valid_coupon_code(value: &str) -> bool {
    (3..=40).contains(&value.chars().count())
        && value.chars().all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '-'
        })
}

fn valid_email(value: &str) -> bool {
    Email::parse(value).is_ok_and(|email| email.as_str() == value)
}

fn valid_idempotency_key(value: &str) -> bool {
    !value.is_empty() && value.chars().count() <= 128 && !value.chars().any(char::is_control)
}

fn valid_payment_value(value: &str) -> bool {
    !value.is_empty() && value.chars().count() <= 128 && !value.chars().any(char::is_control)
}

fn map_write_error(error: &sqlx::Error) -> MaviError {
    if let sqlx::Error::Database(database) = &error
        && database.constraint().is_some()
    {
        return MaviError::conflict(SHOP_RELOCATION_CONFLICT);
    }
    MaviError::Internal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relocation_is_site_bound_and_separates_live_payment_state() {
        let site = SiteId::new();
        let now = Utc::now();
        let order_id = Uuid::now_v7();
        let product_id = Uuid::now_v7();
        let relocation = ShopRelocation {
            format: SHOP_RELOCATION_FORMAT.to_owned(),
            version: SHOP_RELOCATION_VERSION,
            source_site_id: site,
            products: vec![ShopProductRelocation {
                id: product_id,
                slug: "course-seat".to_owned(),
                name: "Course seat".to_owned(),
                description: None,
                price_minor: 1_000,
                currency: "TRY".to_owned(),
                stock_on_hand: 4,
                on_sale: true,
                created_at: now,
                updated_at: now,
                deleted_at: None,
            }],
            coupons: Vec::new(),
            order_counter: Some(ShopOrderCounterRelocation { next_number: 2 }),
            orders: vec![ShopOrderRelocation {
                id: order_id,
                number: 1,
                state: OrderState::Waiting,
                email: "buyer@example.test".to_owned(),
                total_minor: 1_000,
                currency: "TRY".to_owned(),
                idempotency_key: "checkout-1".to_owned(),
                payment: None,
                paid_at: None,
                sent_at: None,
                called_off_at: None,
                given_back_at: None,
                created_at: now,
                updated_at: now,
            }],
            order_lines: vec![ShopOrderLineRelocation {
                id: Uuid::now_v7(),
                order_id,
                product_id: Some(product_id),
                name: "Course seat".to_owned(),
                each_minor: 1_000,
                quantity: 1,
                created_at: now,
            }],
            stock_holds: vec![ShopStockHoldRelocation {
                id: Uuid::now_v7(),
                order_id,
                product_id,
                quantity: 1,
                status: ShopStockHoldStatus::Held,
                expires_at: now,
                settled_at: None,
                created_at: now,
            }],
            coupon_uses: Vec::new(),
        };
        relocation
            .validate_for_relocation(site)
            .expect("valid relocation");
        assert_eq!(relocation.record_count().expect("count"), 5);
        assert!(relocation.validate_for_relocation(SiteId::new()).is_err());
        assert!(relocation.orders[0].payment.is_none());
    }
}
