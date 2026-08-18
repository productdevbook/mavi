use axum::{
    Router,
    http::{Method, StatusCode},
};
use serde_json::json;

mod support;
use support::{bootstrap, login, response_json, send};

#[tokio::test]
#[ignore = "requires TEST_DATABASE_URL and a non-superuser PostgreSQL role"]
#[allow(clippy::too_many_lines)]
async fn shop_routes_keep_public_stock_private_and_orders_idempotent() {
    let app = support::build_app().await;
    let owner_token = bootstrap(&app, "HTTP shop test").await;

    let product = send(
        &app,
        Method::POST,
        "/api/v1/shop/products",
        Some(&owner_token),
        Some(json!({
            "slug": "salt",
            "name": "Salt",
            "description": "A useful thing",
            "price": {"minor": 1250, "currency": "TRY"},
            "stock": 3
        })),
    )
    .await;
    assert_eq!(product.status(), StatusCode::CREATED);
    let product = response_json(product).await;
    let product_id = product["id"].as_str().expect("product id").to_owned();
    assert_eq!(product["price"]["currency"], "TRY");

    let public = send(
        &app,
        Method::GET,
        "/public/v1/shop/products?limit=1",
        None,
        None,
    )
    .await;
    assert_eq!(public.status(), StatusCode::OK);
    let public = response_json(public).await;
    let public_product = &public["items"][0];
    assert_eq!(public_product["can_be_bought"], true);
    assert!(public_product.get("stock").is_none());
    assert!(public["next_cursor"].is_null());

    let checkout_body = json!({
        "email": "buyer@example.test",
        "items": [{"product_id": product_id, "quantity": 2}],
        "idempotency_key": "http-checkout-1"
    });
    let checkout = send(
        &app,
        Method::POST,
        "/public/v1/shop/orders",
        None,
        Some(checkout_body.clone()),
    )
    .await;
    assert_eq!(checkout.status(), StatusCode::CREATED);
    let checkout = response_json(checkout).await;
    assert_eq!(checkout["total"]["minor"], 2500);
    let order_id = checkout["id"].as_str().expect("order id").to_owned();

    let duplicate = send(
        &app,
        Method::POST,
        "/public/v1/shop/orders",
        None,
        Some(checkout_body),
    )
    .await;
    assert_eq!(duplicate.status(), StatusCode::CREATED);
    assert_eq!(response_json(duplicate).await["id"], order_id);

    let orders = send(
        &app,
        Method::GET,
        "/api/v1/shop/orders?limit=1",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(orders.status(), StatusCode::OK);
    let orders = response_json(orders).await;
    assert_eq!(orders["items"].as_array().expect("orders").len(), 1);
    assert!(orders["next_cursor"].is_null());

    let invalid_transition = send(
        &app,
        Method::POST,
        &format!("/api/v1/shop/orders/{order_id}/transition"),
        Some(&owner_token),
        Some(json!({"to": "sent"})),
    )
    .await;
    assert_eq!(invalid_transition.status(), StatusCode::CONFLICT);

    let paid = send(
        &app,
        Method::POST,
        &format!("/api/v1/shop/orders/{order_id}/transition"),
        Some(&owner_token),
        Some(json!({"to": "paid", "payment": {"provider": "test", "reference": "p-1"}})),
    )
    .await;
    assert_eq!(paid.status(), StatusCode::OK);
    assert_eq!(response_json(paid).await["state"], "paid");

    let reader_token = create_reader(&app, &owner_token).await;
    let reader_products = send(
        &app,
        Method::GET,
        "/api/v1/shop/products",
        Some(&reader_token),
        None,
    )
    .await;
    assert_eq!(reader_products.status(), StatusCode::OK);
    let reader_write = send(
        &app,
        Method::POST,
        "/api/v1/shop/products",
        Some(&reader_token),
        Some(json!({
            "slug": "reader-product",
            "name": "Reader cannot write",
            "price": {"minor": 10, "currency": "TRY"},
            "stock": 1
        })),
    )
    .await;
    assert_eq!(reader_write.status(), StatusCode::FORBIDDEN);
}

async fn create_reader(app: &Router, owner_token: &str) -> String {
    let role = send(
        app,
        Method::POST,
        "/api/v1/roles",
        Some(owner_token),
        Some(json!({
            "name": "shop-reader",
            "grants": [{"capability": "shop", "action": "view"}]
        })),
    )
    .await;
    assert_eq!(role.status(), StatusCode::CREATED);
    let role_id = response_json(role).await["id"]
        .as_str()
        .expect("role id")
        .to_owned();
    let person = send(
        app,
        Method::POST,
        "/api/v1/people",
        Some(owner_token),
        Some(json!({
            "email": "shop-reader@example.com",
            "name": "Shop Reader",
            "password": "long-enough-password",
            "role_ids": [role_id]
        })),
    )
    .await;
    assert_eq!(person.status(), StatusCode::CREATED);
    support::verify_email(app, &response_json(person).await, "shop-reader@example.com").await;
    login(app, "shop-reader@example.com").await
}
