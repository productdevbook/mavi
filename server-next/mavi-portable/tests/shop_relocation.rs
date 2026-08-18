use std::env;

use chrono::Utc;
use mavi_core::{SiteContext, SiteId};
use mavi_files::InMemoryFileStore;
use mavi_portable::{ImportStrategy, PortableRelocationRequest, PortableService};
use mavi_storage::Database;
use uuid::Uuid;

fn database_url() -> Option<String> {
    env::var("TEST_DATABASE_URL").ok()
}

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn shop_relocation_preserves_catalog_orders_stock_and_redacts_live_payment_state() {
    let url = database_url().expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 4).await.expect("database");
    database.migrate().await.expect("migrations");

    let source_site = SiteId::new();
    let target_site = SiteId::new();
    database
        .ensure_site(source_site)
        .await
        .expect("source site");
    database
        .ensure_site(target_site)
        .await
        .expect("target site");

    let product_id = Uuid::now_v7();
    let deleted_product_id = Uuid::now_v7();
    let coupon_id = Uuid::now_v7();
    let deleted_coupon_id = Uuid::now_v7();
    let waiting_order_id = Uuid::now_v7();
    let paid_order_id = Uuid::now_v7();
    let called_off_order_id = Uuid::now_v7();
    let waiting_line_id = Uuid::now_v7();
    let paid_line_id = Uuid::now_v7();
    let called_off_line_id = Uuid::now_v7();
    let waiting_hold_id = Uuid::now_v7();
    let paid_hold_id = Uuid::now_v7();
    let called_off_hold_id = Uuid::now_v7();
    let coupon_use_id = Uuid::now_v7();
    let now = Utc::now();
    let source_context = SiteContext::public(source_site);
    let files = InMemoryFileStore::default();
    let portable = PortableService;

    let mut source_tx = database.begin(&source_context).await.expect("source scope");
    sqlx::query(
        "insert into site_settings (site_id, name, timezone)
         values ($1, 'Shop source', 'UTC')",
    )
    .bind(source_site.into_uuid())
    .execute(source_tx.conn())
    .await
    .expect("source settings");
    sqlx::query(
        "insert into site_languages (site_id, tag, name, is_default)
         values ($1, 'en', 'English', true)",
    )
    .bind(source_site.into_uuid())
    .execute(source_tx.conn())
    .await
    .expect("source language");

    sqlx::query(
        "insert into shop_products
            (site_id, id, slug, name, description, price_minor, currency, stock_on_hand,
             on_sale, created_at, updated_at)
         values ($1, $2, 'seat', 'Course seat', 'A seat', 1000, 'TRY', 7, true, $3, $3)",
    )
    .bind(source_site.into_uuid())
    .bind(product_id)
    .bind(now)
    .execute(source_tx.conn())
    .await
    .expect("source product");
    sqlx::query(
        "insert into shop_products
            (site_id, id, slug, name, price_minor, currency, stock_on_hand, on_sale,
             created_at, updated_at, deleted_at)
         values ($1, $2, 'old-seat', 'Old seat', 500, 'TRY', 0, false, $3, $3, $4)",
    )
    .bind(source_site.into_uuid())
    .bind(deleted_product_id)
    .bind(now)
    .bind(now)
    .execute(source_tx.conn())
    .await
    .expect("deleted source product");

    sqlx::query(
        "insert into shop_coupons
            (site_id, id, code, kind, percent, max_uses, created_at, updated_at)
         values ($1, $2, 'SPRING', 'percent', 10, 2, $3, $3)",
    )
    .bind(source_site.into_uuid())
    .bind(coupon_id)
    .bind(now)
    .execute(source_tx.conn())
    .await
    .expect("source coupon");
    sqlx::query(
        "insert into shop_coupons
            (site_id, id, code, kind, amount_minor, currency, created_at, updated_at, deleted_at)
         values ($1, $2, 'OLD', 'amount', 250, 'TRY', $3, $3, $4)",
    )
    .bind(source_site.into_uuid())
    .bind(deleted_coupon_id)
    .bind(now)
    .bind(now)
    .execute(source_tx.conn())
    .await
    .expect("deleted source coupon");
    sqlx::query("insert into shop_order_counters (site_id, next_number) values ($1, 4)")
        .bind(source_site.into_uuid())
        .execute(source_tx.conn())
        .await
        .expect("source order counter");

    sqlx::query(
        "insert into shop_orders
            (site_id, id, number, state, email, total_minor, currency, idempotency_key,
             payment_provider, payment_reference, created_at, updated_at)
         values ($1, $2, 1, 'waiting', 'waiting@example.test', 1000, 'TRY', 'waiting-1',
                 'legacy-provider', 'live-session-should-not-move', $3, $3)",
    )
    .bind(source_site.into_uuid())
    .bind(waiting_order_id)
    .bind(now)
    .execute(source_tx.conn())
    .await
    .expect("waiting order");
    sqlx::query(
        "insert into shop_orders
            (site_id, id, number, state, email, total_minor, currency, idempotency_key,
             payment_provider, payment_reference, paid_at, created_at, updated_at)
         values ($1, $2, 2, 'paid', 'paid@example.test', 1000, 'TRY', 'paid-1',
                 'test', 'payment-receipt-1', $3, $3, $3)",
    )
    .bind(source_site.into_uuid())
    .bind(paid_order_id)
    .bind(now)
    .execute(source_tx.conn())
    .await
    .expect("paid order");
    sqlx::query(
        "insert into shop_orders
            (site_id, id, number, state, email, total_minor, currency, idempotency_key,
             called_off_at, created_at, updated_at)
         values ($1, $2, 3, 'called_off', 'cancelled@example.test', 1000, 'TRY', 'cancelled-1',
                 $3, $3, $3)",
    )
    .bind(source_site.into_uuid())
    .bind(called_off_order_id)
    .bind(now)
    .execute(source_tx.conn())
    .await
    .expect("called-off order");

    for (id, order_id, product, line_name) in [
        (waiting_line_id, waiting_order_id, product_id, "Course seat"),
        (paid_line_id, paid_order_id, product_id, "Course seat"),
        (
            called_off_line_id,
            called_off_order_id,
            deleted_product_id,
            "Old seat",
        ),
    ]
    .into_iter()
    .map(|(id, order_id, product, name)| (id, order_id, Some(product), name))
    {
        sqlx::query(
            "insert into shop_order_lines
                (site_id, id, order_id, product_id, name, each_minor, quantity, created_at)
             values ($1, $2, $3, $4, $5, 1000, 1, $6)",
        )
        .bind(source_site.into_uuid())
        .bind(id)
        .bind(order_id)
        .bind(product)
        .bind(line_name)
        .bind(now)
        .execute(source_tx.conn())
        .await
        .expect("source order line");
    }

    sqlx::query(
        "insert into shop_stock_holds
            (site_id, id, order_id, product_id, quantity, status, expires_at, settled_at, created_at)
         values ($1, $2, $3, $4, 1, 'held', $5, null, $5)",
    )
    .bind(source_site.into_uuid())
    .bind(waiting_hold_id)
    .bind(waiting_order_id)
    .bind(product_id)
    .bind(now)
    .execute(source_tx.conn())
    .await
    .expect("waiting stock hold");
    sqlx::query(
        "insert into shop_stock_holds
            (site_id, id, order_id, product_id, quantity, status, expires_at, settled_at, created_at)
         values ($1, $2, $3, $4, 1, 'consumed', $5, $5, $5)",
    )
    .bind(source_site.into_uuid())
    .bind(paid_hold_id)
    .bind(paid_order_id)
    .bind(product_id)
    .bind(now)
    .execute(source_tx.conn())
    .await
    .expect("consumed stock hold");
    sqlx::query(
        "insert into shop_stock_holds
            (site_id, id, order_id, product_id, quantity, status, expires_at, settled_at, created_at)
         values ($1, $2, $3, $4, 1, 'released', $5, $5, $5)",
    )
    .bind(source_site.into_uuid())
    .bind(called_off_hold_id)
    .bind(called_off_order_id)
    .bind(deleted_product_id)
    .bind(now)
    .execute(source_tx.conn())
    .await
    .expect("released stock hold");
    sqlx::query(
        "insert into shop_coupon_uses (site_id, id, coupon_id, order_id, used_at)
         values ($1, $2, $3, $4, $5)",
    )
    .bind(source_site.into_uuid())
    .bind(coupon_use_id)
    .bind(coupon_id)
    .bind(waiting_order_id)
    .bind(now)
    .execute(source_tx.conn())
    .await
    .expect("coupon use");

    let mut relocation = portable
        .export_for_relocation(&mut source_tx, &source_context, &files)
        .await
        .expect("shop relocation export");
    assert_eq!(relocation.shop.products.len(), 2);
    assert_eq!(relocation.shop.coupons.len(), 2);
    assert_eq!(relocation.shop.orders.len(), 3);
    assert_eq!(relocation.shop.order_lines.len(), 3);
    assert_eq!(relocation.shop.stock_holds.len(), 3);
    assert_eq!(relocation.shop.coupon_uses.len(), 1);
    assert_eq!(
        relocation
            .shop
            .order_counter
            .as_ref()
            .expect("counter")
            .next_number,
        4
    );
    assert!(
        relocation
            .shop
            .orders
            .iter()
            .find(|order| order.id == waiting_order_id)
            .expect("waiting relocation order")
            .payment
            .is_none()
    );
    assert_eq!(
        relocation
            .shop
            .orders
            .iter()
            .find(|order| order.id == paid_order_id)
            .expect("paid relocation order")
            .payment
            .as_ref()
            .expect("payment receipt")
            .reference,
        "payment-receipt-1"
    );
    source_tx.commit().await.expect("source commit");

    relocation.bundle.manifest.source_site_id = target_site;
    relocation.audit.source_site_id = target_site;
    relocation.trash.source_site_id = target_site;
    relocation.forms.source_site_id = target_site;
    relocation.mail.source_site_id = target_site;
    relocation.shop.source_site_id = target_site;
    relocation.courses.source_site_id = target_site;
    relocation.jobs.source_site_id = target_site;
    relocation.flows.source_site_id = target_site;
    relocation.boards.source_site_id = target_site;
    relocation.analytics.source_site_id = target_site;

    let target_context = SiteContext::public(target_site);
    let mut target_tx = database.begin(&target_context).await.expect("target scope");
    portable
        .relocate(
            &mut target_tx,
            &target_context,
            &PortableRelocationRequest {
                bundle: relocation.clone(),
                strategy: ImportStrategy::Upsert,
            },
            &files,
        )
        .await
        .expect("shop relocation import");

    let relocated = portable
        .export_for_relocation(&mut target_tx, &target_context, &files)
        .await
        .expect("target shop export");
    assert_eq!(relocated.shop, relocation.shop);

    let waiting_payment: (Option<String>, Option<String>) = sqlx::query_as(
        "select payment_provider, payment_reference
           from shop_orders where site_id = $1 and id = $2",
    )
    .bind(target_site.into_uuid())
    .bind(waiting_order_id)
    .fetch_one(target_tx.conn())
    .await
    .expect("waiting payment state");
    assert_eq!(waiting_payment, (None, None));

    let statuses: Vec<(Uuid, String, Option<chrono::DateTime<Utc>>)> = sqlx::query_as(
        "select id, status, settled_at from shop_stock_holds
          where site_id = $1 order by id",
    )
    .bind(target_site.into_uuid())
    .fetch_all(target_tx.conn())
    .await
    .expect("target stock holds");
    assert_eq!(statuses.len(), 3);
    assert!(statuses.iter().any(|(id, status, settled_at)| {
        *id == waiting_hold_id && status == "held" && settled_at.is_none()
    }));
    assert!(statuses.iter().any(|(id, status, settled_at)| {
        *id == paid_hold_id && status == "consumed" && settled_at.is_some()
    }));
    assert!(statuses.iter().any(|(id, status, settled_at)| {
        *id == called_off_hold_id && status == "released" && settled_at.is_some()
    }));
    target_tx.commit().await.expect("target commit");
}
