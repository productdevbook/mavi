use std::env;

use mavi_core::{MaviError, PageRequest, SiteContext, SiteId};
use mavi_shop::{
    CheckoutInput, CouponListFilter, CreateCoupon, CreateProduct, OrderState, OrderTransition,
    ProductListFilter, ProductPrice, ShopService,
};
use mavi_storage::Database;

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn shop_catalog_checkout_coupons_holds_and_orders_are_site_scoped() {
    let url = env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL");
    let database = Database::connect(&url, 2).await.expect("database");
    database.migrate().await.expect("migrations");
    let first_site = SiteId::new();
    let second_site = SiteId::new();
    database.ensure_site(first_site).await.expect("first site");
    database
        .ensure_site(second_site)
        .await
        .expect("second site");
    let first_context = SiteContext::public(first_site);
    let service = ShopService;

    let mut transaction = database.begin(&first_context).await.expect("shop scope");
    let product = service
        .create_product(
            &mut transaction,
            &first_context,
            &CreateProduct {
                slug: "salt".to_owned(),
                name: "Salt".to_owned(),
                description: Some("A useful thing".to_owned()),
                price: ProductPrice {
                    minor: 1_250,
                    currency: "TRY".to_owned(),
                },
                stock: 3,
                on_sale: true,
            },
        )
        .await
        .expect("product");
    let coupon = service
        .create_coupon(
            &mut transaction,
            &first_context,
            &CreateCoupon {
                code: "SPRING".to_owned(),
                percent: Some(10),
                amount_minor: None,
                currency: None,
                max_uses: Some(1),
                expires_at: None,
            },
        )
        .await
        .expect("coupon");
    assert_eq!(coupon.code, "SPRING");
    transaction.commit().await.expect("catalog commit");

    let mut transaction = database
        .begin(&first_context)
        .await
        .expect("checkout scope");
    let first = service
        .checkout(
            &mut transaction,
            &first_context,
            &CheckoutInput {
                email: "buyer@example.test".to_owned(),
                items: vec![mavi_shop::BasketItem {
                    product_id: product.id,
                    quantity: 2,
                }],
                coupon_code: Some("spring".to_owned()),
                idempotency_key: "checkout-1".to_owned(),
            },
        )
        .await
        .expect("checkout");
    assert_eq!(first.total.minor, 2_250);
    let duplicate = service
        .checkout(
            &mut transaction,
            &first_context,
            &CheckoutInput {
                email: "buyer@example.test".to_owned(),
                items: vec![mavi_shop::BasketItem {
                    product_id: product.id,
                    quantity: 2,
                }],
                coupon_code: None,
                idempotency_key: "checkout-1".to_owned(),
            },
        )
        .await
        .expect("duplicate checkout");
    assert_eq!(duplicate.id, first.id);
    transaction.commit().await.expect("checkout commit");

    let mut transaction = database.begin(&first_context).await.expect("stock scope");
    let insufficient = service
        .checkout(
            &mut transaction,
            &first_context,
            &CheckoutInput {
                email: "buyer@example.test".to_owned(),
                items: vec![mavi_shop::BasketItem {
                    product_id: product.id,
                    quantity: 2,
                }],
                coupon_code: None,
                idempotency_key: "checkout-2".to_owned(),
            },
        )
        .await
        .expect_err("stock must be held");
    assert!(matches!(insufficient, MaviError::Conflict { .. }));
    drop(transaction);

    let mut transaction = database.begin(&first_context).await.expect("order scope");
    let order = service
        .get_order(&mut transaction, &first_context, first.id)
        .await
        .expect("order");
    assert_eq!(order.lines.len(), 1);
    let called_off = service
        .transition_order(
            &mut transaction,
            &first_context,
            first.id,
            &OrderTransition {
                to: OrderState::CalledOff,
                payment: None,
            },
        )
        .await
        .expect("called off");
    assert_eq!(called_off.state, OrderState::CalledOff);
    transaction.commit().await.expect("called off commit");

    let mut transaction = database
        .begin(&first_context)
        .await
        .expect("second checkout");
    let second = service
        .checkout(
            &mut transaction,
            &first_context,
            &CheckoutInput {
                email: "buyer@example.test".to_owned(),
                items: vec![mavi_shop::BasketItem {
                    product_id: product.id,
                    quantity: 1,
                }],
                coupon_code: None,
                idempotency_key: "checkout-3".to_owned(),
            },
        )
        .await
        .expect("second checkout");
    let paid = service
        .transition_order(
            &mut transaction,
            &first_context,
            second.id,
            &OrderTransition {
                to: OrderState::Paid,
                payment: Some(mavi_shop::PaymentReceiptInput {
                    provider: "test".to_owned(),
                    reference: "payment-1".to_owned(),
                }),
            },
        )
        .await
        .expect("paid");
    assert_eq!(paid.state, OrderState::Paid);
    let sent = service
        .transition_order(
            &mut transaction,
            &first_context,
            second.id,
            &OrderTransition {
                to: OrderState::Sent,
                payment: None,
            },
        )
        .await
        .expect("sent");
    assert_eq!(sent.state, OrderState::Sent);
    let returned = service
        .transition_order(
            &mut transaction,
            &first_context,
            second.id,
            &OrderTransition {
                to: OrderState::GivenBack,
                payment: None,
            },
        )
        .await
        .expect("returned");
    assert_eq!(returned.state, OrderState::GivenBack);
    transaction.commit().await.expect("return commit");

    let mut transaction = database.begin(&first_context).await.expect("list scope");
    let products = service
        .list_products(
            &mut transaction,
            &first_context,
            &ProductListFilter {
                page: PageRequest {
                    after: None,
                    limit: Some(1),
                },
            },
        )
        .await
        .expect("products");
    assert_eq!(products.items.len(), 1);
    assert!(products.next_cursor.is_none());
    let coupons = service
        .list_coupons(
            &mut transaction,
            &first_context,
            &CouponListFilter::default(),
        )
        .await
        .expect("coupons");
    assert_eq!(coupons.items.len(), 1);
    let second_context = SiteContext::public(second_site);
    let mut second_transaction = database.begin(&second_context).await.expect("second scope");
    assert!(
        service
            .list_products(
                &mut second_transaction,
                &second_context,
                &ProductListFilter::default(),
            )
            .await
            .expect("second products")
            .items
            .is_empty()
    );
    assert!(matches!(
        service
            .get_order(&mut second_transaction, &second_context, first.id)
            .await,
        Err(MaviError::NotFound { .. })
    ));
    second_transaction.commit().await.expect("second commit");
    transaction.commit().await.expect("list commit");
}
