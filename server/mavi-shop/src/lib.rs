//! Site-scoped commerce primitives.
//!
//! The shop keeps product snapshots on order lines, holds available stock
//! inside one transaction and exposes an explicit order state machine. Public
//! checkout never calls a payment provider; it creates a waiting order and a
//! future payment worker can use the shared payment port before confirming it.

mod coupons;
mod orders;
mod products;
mod relocation;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use mavi_core::{Cursor, MaviError, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use coupons::{Coupon, CouponKind, CouponListFilter, CreateCoupon};
pub use orders::{
    BasketItem, CheckoutInput, CheckoutReceipt, Order, OrderLine, OrderListFilter, OrderState,
    OrderSummary, OrderTransition, PaymentReceiptInput,
};
pub use products::{
    CreateProduct, Product, ProductListFilter, ProductPrice, PublicProduct,
    PublicProductListFilter, UpdateProduct,
};
pub use relocation::{
    ShopCouponRelocation, ShopCouponUseRelocation, ShopOrderCounterRelocation,
    ShopOrderLineRelocation, ShopOrderRelocation, ShopPaymentReceiptRelocation,
    ShopProductRelocation, ShopRelocation, ShopStockHoldRelocation, ShopStockHoldStatus,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct ShopService;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct RecentCursor {
    pub created_at: DateTime<Utc>,
    pub id: Uuid,
}

pub(crate) fn encode_cursor(created_at: DateTime<Utc>, id: Uuid) -> Result<Cursor> {
    let bytes =
        serde_json::to_vec(&RecentCursor { created_at, id }).map_err(|_| MaviError::Internal)?;
    Cursor::parse(URL_SAFE_NO_PAD.encode(bytes))
}

pub(crate) fn decode_cursor(cursor: &Cursor) -> Result<RecentCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor.as_str())
        .map_err(|_| MaviError::validation("invalid_cursor"))?;
    serde_json::from_slice(&bytes).map_err(|_| MaviError::validation("invalid_cursor"))
}

#[must_use]
pub fn api() -> mavi_contract::Api {
    let mut api = mavi_contract::Api::default();
    api.extend(products::api());
    api.extend(coupons::api());
    api.extend(orders::api());
    api
}

pub(crate) async fn audit(
    tx: &mut mavi_storage::SiteTx,
    context: &mavi_core::SiteContext,
    action: &str,
    resource_type: &str,
    resource_id: Uuid,
    payload: serde_json::Value,
) -> Result<()> {
    mavi_audit::AuditService
        .record(
            tx,
            context,
            &mavi_audit::AuditEntry {
                action: action.to_owned(),
                resource_type: resource_type.to_owned(),
                resource_id: Some(resource_id),
                payload,
            },
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shop_cursor_is_opaque_and_bounded() {
        let created_at = Utc::now();
        let id = Uuid::now_v7();
        let cursor = encode_cursor(created_at, id).expect("cursor");
        let decoded = decode_cursor(&cursor).expect("decoded cursor");
        assert_eq!(cursor.as_str().chars().count(), cursor.as_str().len());
        assert_eq!(decoded.id, id);
        assert_eq!(decoded.created_at, created_at);
        assert!(!cursor.as_str().contains("offset"));
        assert!(!cursor.as_str().contains("page"));
    }
}
