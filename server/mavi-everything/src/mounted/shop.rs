// Domain route module: shop

use mavi_api::Who;
use mavi_core::error::{Error, Result};
use mavi_core::say::Say;
use mavi_db::Db;
use mavi_http::Answered;
use mavi_serve::{Asked, Handler, Site};
use serde_json::Value;

use super::helpers::{a_uuid, asking, handling, wrote_about};

/// The shelf, the orders, and the basket a visitor brings.
#[must_use]
pub fn what_it_sells(mut site: Site, db: &Db) -> Site {
    for endpoint in mavi_shop::endpoints() {
        let db = db.clone();

        let handler: Option<Handler> = match endpoint.named {
            "products.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { products(&db, &asked).await })
            })),
            "products.make" => Some(handling(db, |db, asked| {
                Box::pin(async move { added_a_product(&db, &asked).await })
            })),
            "products.change" => Some(handling(db, |db, asked| {
                Box::pin(async move { changed_a_product(&db, &asked).await })
            })),
            "coupons.remove" => Some(handling(db, |db, asked| {
                Box::pin(async move { took_a_coupon_away(&db, &asked).await })
            })),
            "products.remove" => Some(handling(db, |db, asked| {
                Box::pin(async move { removed_a_product(&db, &asked).await })
            })),
            "coupons.list" => Some(handling(db, |db, _| {
                Box::pin(async move { coupons(&db).await })
            })),
            "coupons.make" => Some(handling(db, |db, asked| {
                Box::pin(async move { made_a_coupon(&db, &asked).await })
            })),
            "orders.list" => Some(handling(db, |db, asked| {
                Box::pin(async move { orders(&db, &asked).await })
            })),
            "orders.read" => Some(handling(db, |db, asked| {
                Box::pin(async move { one_order(&db, &asked).await })
            })),
            "orders.move" => Some(handling(db, |db, asked| {
                Box::pin(async move { moved_an_order(&db, &asked).await })
            })),
            "open.products" => Some(handling(db, |db, asked| {
                Box::pin(async move { what_is_for_sale(&db, &asked).await })
            })),
            "open.order" => Some(handling(db, |db, asked| {
                Box::pin(async move { placed_an_order(&db, &asked).await })
            })),
            _ => None,
        };

        if let Some(handler) = handler {
            let needs = match (endpoint.who, endpoint.changes) {
                (Who::Anybody, _) => None,
                (_, true) => Some(mavi_shop::to_write()),
                (_, false) => Some(mavi_shop::to_read()),
            };

            site = site.mount(endpoint, needs, handler);
        }
    }

    site
}

/// The site's own letters, its lists, and the way out of them.
/// A coupon is reached by its code rather than by an id, because a code is
/// what somebody typed off a poster and what every other coupon endpoint takes.
async fn took_a_coupon_away(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let code = asked.path.get("code").cloned().unwrap_or_default();

    let mut tx = db.begin().await?;

    mavi_shop::store::remove_a_coupon(&mut tx, &code).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "coupons.remove",
        "coupon",
        Some(&code),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

/// Whoever is asking, as an id. Every key endpoint is about their own keys and
/// nobody else's, which is what makes them need no grant.
async fn products(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let page = mavi_shop::store::products(&mut tx, &asking(asked)).await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn what_is_for_sale(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let page = mavi_shop::store::for_sale(&mut tx, &asking(asked)).await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn added_a_product(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let new: mavi_shop::store::NewProduct = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_something_to_sell")))?;

    let mut tx = db.begin().await?;
    let product = mavi_shop::store::add(&mut tx, &new).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "products.make",
        "product",
        Some(&product.id.to_string()),
        &serde_json::json!({ "slug": product.slug }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(product).map_err(Error::internal)?,
        receipt,
    ))
}

async fn changed_a_product(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let changes: mavi_shop::store::ProductChanges = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_change_to_something_for_sale")))?;

    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;
    let product = mavi_shop::store::change(&mut tx, id, &changes).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "products.change",
        "product",
        Some(&id.to_string()),
        // What was changed, not what it became: a price is worth being able to
        // read back, and reading it out of the row is what the row is for.
        &serde_json::json!({ "for_sale": product.for_sale }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(product).map_err(Error::internal)?,
        receipt,
    ))
}

async fn removed_a_product(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let id = a_uuid(asked)?;
    let mut tx = db.begin().await?;

    mavi_shop::store::remove(&mut tx, id).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "products.remove",
        "product",
        Some(&id.to_string()),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(Value::Null, receipt))
}

async fn coupons(db: &Db) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let coupons = mavi_shop::store::coupons(&mut tx).await?;

    Ok(Answered::Read(
        serde_json::to_value(coupons).map_err(Error::internal)?,
    ))
}

async fn made_a_coupon(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let new: mavi_shop::store::NewCoupon = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_code")))?;

    let mut tx = db.begin().await?;
    let coupon = mavi_shop::store::add_coupon(&mut tx, &new).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "coupons.make",
        "coupon",
        Some(&coupon.code),
        &serde_json::json!({}),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(coupon).map_err(Error::internal)?,
        receipt,
    ))
}

async fn orders(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let page = mavi_shop::store::orders(
        &mut tx,
        asked.query.get("state").map(String::as_str),
        &asking(asked),
    )
    .await?;

    Ok(Answered::Read(
        serde_json::to_value(page).map_err(Error::internal)?,
    ))
}

async fn one_order(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let mut tx = db.begin().await?;
    let order = mavi_shop::store::read(&mut tx, a_uuid(asked)?).await?;

    Ok(Answered::Read(
        serde_json::to_value(order).map_err(Error::internal)?,
    ))
}

async fn moved_an_order(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let to = asked.body["to"].as_str().unwrap_or_default().to_owned();
    let id = a_uuid(asked)?;

    let mut tx = db.begin().await?;
    let order = mavi_shop::store::move_to(&mut tx, id, &to).await?;
    let receipt = wrote_about(
        &mut tx,
        asked,
        "orders.move",
        "order",
        Some(&id.to_string()),
        &serde_json::json!({ "to": order.state.as_str() }),
    )
    .await?;

    tx.commit().await?;

    Ok(Answered::Changed(
        serde_json::to_value(order).map_err(Error::internal)?,
        receipt,
    ))
}

async fn placed_an_order(db: &Db, asked: &Asked) -> Result<Answered<Value>> {
    let basket: mavi_shop::store::Basket = serde_json::from_value(asked.body.clone())
        .map_err(|_| Error::invalid(Say::of("that_is_not_a_basket")))?;

    let mut tx = db.begin().await?;
    let order = mavi_shop::store::place(&mut tx, &basket).await?;

    let receipt = wrote_about(
        &mut tx,
        asked,
        "open.order",
        "order",
        Some(&order.id.to_string()),
        // The number and what it came to. Never the address they typed: what
        // is worth recording is that an order was placed.
        &serde_json::json!({ "number": order.number, "total": order.total.minor }),
    )
    .await?;

    tx.commit().await?;

    // What a visitor is told: the order they placed and what it came to. The
    // number, because that is what somebody reads down a telephone.
    Ok(Answered::Changed(
        serde_json::json!({
            "id": order.id,
            "number": order.number,
            "total": order.total,
        }),
        receipt,
    ))
}
