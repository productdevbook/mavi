//! Money and stock, which are the two things that cannot be put right by
//! apologising. Everything here that could be a race is run as one.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mavi::kernel::authz::every_grant;
use mavi::kernel::db::Db;
use mavi::kernel::http::AppState;
use mavi::kernel::payments::{Hosted, Payments};
use mavi::kernel::secret::Secret;
use mavi::kernel::tenant::TenantId;
use mavi::kernel::webhook;
use tower::ServiceExt;
use uuid::Uuid;

mod common;

use common::harness;
use mavi::testing::{a_role, a_tenant, a_user};

#[derive(Clone)]
struct Shop {
    db: Db,
    router: axum::Router,
    host: String,
    tenant: TenantId,
    token: String,
    /// Where this shop's provider is listening, for the tests that ask it
    /// directly.
    provider_at: String,
}

async fn a_shop() -> Shop {
    let db = harness().await;
    let host = format!("{}.example", Uuid::now_v7().simple());
    let tenant = a_tenant(&db, &host).await;
    let role = a_role(&db, tenant, "owner", &every_grant()).await;
    let password = "a long enough password";
    let (_, email) = a_user(&db, tenant, role, password).await;

    let router = mavi::router(AppState::new(db.clone()));

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/session")
                .header(header::HOST, &host)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({ "email": email, "password": password }).to_string(),
                ))
                .expect("a request"),
        )
        .await
        .expect("a response");

    let body: serde_json::Value = serde_json::from_slice(
        &response
            .into_body()
            .collect()
            .await
            .expect("a body")
            .to_bytes(),
    )
    .expect("json");

    Shop {
        db,
        router,
        host,
        tenant,
        token: body["token"].as_str().expect("a token").to_owned(),
        provider_at: String::new(),
    }
}

impl Shop {
    async fn send(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, &self.host);

        if let Some(token) = token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }

        let request = match body {
            Some(body) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        }
        .expect("a request");

        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("a response");

        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("a body")
            .to_bytes();

        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
        )
    }

    async fn a_product(&self, price_minor: i64, stock: i32) -> Uuid {
        let (status, body) = self
            .send(
                "POST",
                "/api/products",
                Some(&self.token),
                Some(serde_json::json!({
                    "slug": format!("thing-{}", Uuid::now_v7().simple()),
                    "name": "A Thing",
                    "price_minor": price_minor,
                    "currency": "TRY",
                    "stock": stock,
                })),
            )
            .await;

        assert_eq!(status, StatusCode::CREATED, "{body}");

        body["id"].as_str().expect("an id").parse().expect("a uuid")
    }

    async fn buy(
        &self,
        product: Uuid,
        quantity: i32,
        key: &str,
    ) -> (StatusCode, serde_json::Value) {
        self.send(
            "POST",
            "/api/sites/checkout",
            None,
            Some(serde_json::json!({
                "email": "somebody@example.test",
                "items": [{ "product_id": product, "quantity": quantity }],
                "idempotency_key": key,
            })),
        )
        .await
    }

    async fn stock_of(&self, product: Uuid) -> i32 {
        let mut conn = self.db.tenant(self.tenant).await.expect("begin");

        let stock: (i32,) = sqlx::query_as("select stock from products where id = $1")
            .bind(product)
            .fetch_one(conn.conn())
            .await
            .expect("a product");

        stock.0
    }
}

#[tokio::test]
async fn buying_something_takes_it_off_the_shelf_and_holds_it() {
    let shop = a_shop().await;
    let thing = shop.a_product(1250, 3).await;

    let (status, order) = shop.buy(thing, 2, &format!("k-{}", Uuid::now_v7())).await;

    assert_eq!(status, StatusCode::CREATED, "{order}");
    assert_eq!(order["order"]["total"]["minor"], 2500);
    assert_eq!(order["order"]["total"]["currency"], "TRY");
    assert_eq!(order["order"]["state"], "pending");
    assert_eq!(shop.stock_of(thing).await, 1);

    let mut conn = shop.db.tenant(shop.tenant).await.expect("begin");

    let held: (i64,) = sqlx::query_as(
        "select coalesce(sum(quantity), 0) from stock_holds where released_at is null",
    )
    .fetch_one(conn.conn())
    .await
    .expect("a count");

    assert_eq!(held.0, 2, "what was taken is not being held for anybody");
}

#[tokio::test]
async fn the_same_attempt_twice_buys_one_thing_once() {
    let shop = a_shop().await;
    let thing = shop.a_product(1000, 5).await;
    let key = format!("k-{}", Uuid::now_v7());

    let (first, one) = shop.buy(thing, 1, &key).await;
    let (again, two) = shop.buy(thing, 1, &key).await;

    assert_eq!(first, StatusCode::CREATED);
    assert_eq!(again, StatusCode::OK, "{two}");
    assert_eq!(
        one["order"]["id"], two["order"]["id"],
        "the same key made two orders"
    );
    assert_eq!(shop.stock_of(thing).await, 4, "stock moved twice");
}

/// The fault the old shop had, run the way it happened: two people reaching
/// for the last one at the same time, on two connections, really at once.
#[tokio::test]
async fn two_people_cannot_buy_the_same_last_one() {
    let shop = a_shop().await;
    let thing = shop.a_product(1000, 1).await;

    let one = shop.clone();
    let two = shop.clone();
    let first_key = format!("k-{}", Uuid::now_v7());
    let second_key = format!("k-{}", Uuid::now_v7());

    let (first, second) = tokio::join!(
        tokio::spawn(async move { one.buy(thing, 1, &first_key).await }),
        tokio::spawn(async move { two.buy(thing, 1, &second_key).await }),
    );

    let (first, _) = first.expect("a result");
    let (second, _) = second.expect("a result");

    let sold = [first, second]
        .iter()
        .filter(|status| **status == StatusCode::CREATED)
        .count();

    assert_eq!(sold, 1, "the last one was sold {sold} times");
    assert_eq!(shop.stock_of(thing).await, 0);
}

#[tokio::test]
async fn buying_more_than_there_is_is_refused() {
    let shop = a_shop().await;
    let thing = shop.a_product(1000, 2).await;

    let (status, _) = shop.buy(thing, 3, &format!("k-{}", Uuid::now_v7())).await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(shop.stock_of(thing).await, 2, "stock moved on a refusal");
}

#[tokio::test]
async fn a_hold_that_runs_out_puts_the_stock_back() {
    let shop = a_shop().await;
    let thing = shop.a_product(1000, 2).await;

    shop.buy(thing, 2, &format!("k-{}", Uuid::now_v7())).await;
    assert_eq!(shop.stock_of(thing).await, 0);

    // The hold is half an hour; walking the clock forward is the same thing as
    // waiting for it, without the half hour.
    let mut conn = shop.db.tenant(shop.tenant).await.expect("begin");
    sqlx::query("update stock_holds set expires_at = now() - interval '1 minute'")
        .execute(conn.conn())
        .await
        .expect("walk forward");
    conn.commit().await.expect("commit");

    let state = AppState::new(shop.db.clone());
    let released = mavi::shop::release_holds(&state, shop.tenant)
        .await
        .expect("release");

    assert_eq!(released, 1);
    assert_eq!(
        shop.stock_of(thing).await,
        2,
        "an abandoned checkout kept it"
    );
}

#[tokio::test]
async fn paying_for_it_keeps_the_stock_gone() {
    let shop = a_shop().await;
    let thing = shop.a_product(1000, 2).await;

    let (_, order) = shop.buy(thing, 1, &format!("k-{}", Uuid::now_v7())).await;
    let id = order["order"]["id"].as_str().expect("an id");

    let (status, paid) = shop
        .send(
            "POST",
            &format!("/api/orders/{id}/paid"),
            Some(&shop.token),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{paid}");
    assert_eq!(paid["state"], "paid");

    let mut conn = shop.db.tenant(shop.tenant).await.expect("begin");
    sqlx::query("update stock_holds set expires_at = now() - interval '1 minute'")
        .execute(conn.conn())
        .await
        .expect("walk forward");
    conn.commit().await.expect("commit");

    let state = AppState::new(shop.db.clone());
    mavi::shop::release_holds(&state, shop.tenant)
        .await
        .expect("release");

    assert_eq!(
        shop.stock_of(thing).await,
        1,
        "something paid for came back onto the shelf"
    );
}

#[tokio::test]
async fn an_order_moves_only_the_ways_money_goes() {
    let shop = a_shop().await;
    let thing = shop.a_product(1000, 5).await;
    let (_, order) = shop.buy(thing, 1, &format!("k-{}", Uuid::now_v7())).await;
    let id = order["order"]["id"].as_str().expect("an id").to_owned();

    // Sending something nobody has paid for.
    let (status, _) = shop
        .send(
            "POST",
            &format!("/api/orders/{id}/fulfilled"),
            Some(&shop.token),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::CONFLICT);

    for step in ["paid", "fulfilled", "refund"] {
        let (status, body) = shop
            .send(
                "POST",
                &format!("/api/orders/{id}/{step}"),
                Some(&shop.token),
                None,
            )
            .await;

        assert_eq!(status, StatusCode::OK, "{step}: {body}");
    }

    // Refunded is the end of it.
    let (status, _) = shop
        .send(
            "POST",
            &format!("/api/orders/{id}/paid"),
            Some(&shop.token),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(shop.stock_of(thing).await, 5, "a refund kept the stock");
}

#[tokio::test]
async fn a_one_use_code_is_used_once() {
    let shop = a_shop().await;
    let thing = shop.a_product(1000, 10).await;
    let code = format!(
        "SAVE{}",
        Uuid::now_v7().simple().to_string()[..8].to_uppercase()
    );

    let mut conn = shop.db.tenant(shop.tenant).await.expect("begin");
    sqlx::query(
        "insert into coupons (tenant_id, code, kind, value, uses_allowed)
         values ($1, $2, 'percent', 10, 1)",
    )
    .bind(shop.tenant.0)
    .bind(&code)
    .execute(conn.conn())
    .await
    .expect("a coupon");
    conn.commit().await.expect("commit");

    let with_code = |key: String| {
        let shop = shop.clone();
        let code = code.clone();

        async move {
            shop.send(
                "POST",
                "/api/sites/checkout",
                None,
                Some(serde_json::json!({
                    "email": "somebody@example.test",
                    "items": [{ "product_id": thing, "quantity": 1 }],
                    "coupon": code,
                    "idempotency_key": key,
                })),
            )
            .await
        }
    };

    let (first, order) = with_code(format!("k-{}", Uuid::now_v7())).await;
    let (again, _) = with_code(format!("k-{}", Uuid::now_v7())).await;

    assert_eq!(first, StatusCode::CREATED);
    assert_eq!(
        order["order"]["total"]["minor"], 900,
        "the discount was not taken off"
    );
    assert_eq!(again, StatusCode::CONFLICT, "a one-use code was used twice");
}

#[tokio::test]
async fn an_order_nobody_paid_for_is_let_go() {
    let shop = a_shop().await;
    let thing = shop.a_product(1000, 2).await;
    shop.buy(thing, 1, &format!("k-{}", Uuid::now_v7())).await;

    let mut conn = shop.db.tenant(shop.tenant).await.expect("begin");
    sqlx::query("update orders set created_at = now() - interval '2 days'")
        .execute(conn.conn())
        .await
        .expect("walk forward");
    conn.commit().await.expect("commit");

    let state = AppState::new(shop.db.clone());
    let dropped = mavi::shop::drop_stuck(&state, shop.tenant)
        .await
        .expect("drop");

    assert_eq!(dropped, 1);
}

#[tokio::test]
async fn a_shop_says_when_it_is_running_out() {
    let shop = a_shop().await;

    let (_, product) = shop
        .send(
            "POST",
            "/api/products",
            Some(&shop.token),
            Some(serde_json::json!({
                "slug": format!("scarce-{}", Uuid::now_v7().simple()),
                "name": "Nearly Gone",
                "price_minor": 500,
                "currency": "TRY",
                "stock": 2,
                "low_stock_at": 3,
            })),
        )
        .await;

    let state = AppState::new(shop.db.clone());
    let warned = mavi::shop::warn_on_low_stock(&state, shop.tenant)
        .await
        .expect("warn");

    assert_eq!(warned, 1);

    let mut conn = shop.db.tenant(shop.tenant).await.expect("begin");

    let events: Vec<(String, Option<String>)> =
        sqlx::query_as("select event, subject_id from outbox where event = 'stock.low'")
            .fetch_all(conn.conn())
            .await
            .expect("the outbox");

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].1.as_deref(), product["id"].as_str());
}

/// Two checkouts reaching for the last use of a one-use code at the same time.
/// Counting uses after inserting one is not enough on its own: both count one,
/// both see one, both go through. The coupon's row is locked instead.
#[tokio::test]
async fn a_one_use_code_is_used_once_even_by_two_at_once() {
    let shop = a_shop().await;
    let thing = shop.a_product(1000, 10).await;
    let code = format!(
        "RACE{}",
        Uuid::now_v7().simple().to_string()[..8].to_uppercase()
    );

    let mut conn = shop.db.tenant(shop.tenant).await.expect("begin");
    sqlx::query(
        "insert into coupons (tenant_id, code, kind, value, uses_allowed)
         values ($1, $2, 'amount', 100, 1)",
    )
    .bind(shop.tenant.0)
    .bind(&code)
    .execute(conn.conn())
    .await
    .expect("a coupon");
    conn.commit().await.expect("commit");

    let one = shop.clone();
    let two = shop.clone();
    let first_code = code.clone();
    let second_code = code.clone();

    let buy = |shop: Shop, code: String, product: Uuid| async move {
        shop.send(
            "POST",
            "/api/sites/checkout",
            None,
            Some(serde_json::json!({
                "email": "somebody@example.test",
                "items": [{ "product_id": product, "quantity": 1 }],
                "coupon": code,
                "idempotency_key": format!("k-{}", Uuid::now_v7()),
            })),
        )
        .await
    };

    let (first, second) = tokio::join!(
        tokio::spawn(buy(one, first_code, thing)),
        tokio::spawn(buy(two, second_code, thing)),
    );

    let (first, _) = first.expect("a result");
    let (second, _) = second.expect("a result");

    let used = [first, second]
        .iter()
        .filter(|status| **status == StatusCode::CREATED)
        .count();

    assert_eq!(used, 1, "a one-use code was used {used} times");
}

/// The hole this closes: a hold lapses, the stock goes back on the shelf, and
/// the order it was for is still payable — so the shop takes the money for
/// something it has already sold to somebody else.
#[tokio::test]
async fn an_order_whose_hold_lapsed_cannot_then_be_paid_for() {
    let shop = a_shop().await;
    let thing = shop.a_product(1000, 1).await;

    let (_, order) = shop.buy(thing, 1, &format!("k-{}", Uuid::now_v7())).await;
    let id = order["order"]["id"].as_str().expect("an id").to_owned();

    let mut conn = shop.db.tenant(shop.tenant).await.expect("begin");
    sqlx::query("update stock_holds set expires_at = now() - interval '1 minute'")
        .execute(conn.conn())
        .await
        .expect("walk forward");
    conn.commit().await.expect("commit");

    let state = AppState::new(shop.db.clone());
    mavi::shop::release_holds(&state, shop.tenant)
        .await
        .expect("release");

    assert_eq!(shop.stock_of(thing).await, 1, "the stock did not come back");

    let (status, body) = shop
        .send(
            "POST",
            &format!("/api/orders/{id}/paid"),
            Some(&shop.token),
            None,
        )
        .await;

    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "an order whose stock went back was paid for anyway: {body}"
    );
}

/// A provider that answers on a socket, so that what is being tested is what
/// this machine sends and what it does with the answer.
mod provider {
    use axum::extract::State;
    use axum::routing::{get, post};
    use axum::{Json, Router};

    #[derive(Clone, Default)]
    pub struct Took(pub std::sync::Arc<std::sync::Mutex<Vec<serde_json::Value>>>);

    pub async fn listening() -> (String, Took) {
        let took = Took::default();

        let app = Router::new()
            .route(
                "/payments",
                post(
                    |State(took): State<Took>, Json(asked): Json<serde_json::Value>| async move {
                        let reference = format!("pay_{}", uuid::Uuid::now_v7().simple());

                        took.0.lock().expect("a lock").push(serde_json::json!({
                            "provider_ref": reference,
                            "amount_minor": asked["amount_minor"],
                        }));

                        Json(serde_json::json!({
                            "provider_ref": reference,
                            "pay_at": format!("https://payments.example/{reference}"),
                        }))
                    },
                )
                .get(|State(took): State<Took>| async move {
                    let taken: Vec<serde_json::Value> = took
                        .0
                        .lock()
                        .expect("a lock")
                        .iter()
                        .map(|one| {
                            serde_json::json!({
                                "provider_ref": one["provider_ref"],
                                "amount_minor": one["amount_minor"],
                                "state": "paid",
                            })
                        })
                        .collect();

                    Json(taken)
                }),
            )
            .route("/healthz", get(|| async { "ok" }))
            .with_state(took.clone());

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a socket");
        let address = listener.local_addr().expect("an address");

        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        (format!("http://{address}"), took)
    }
}

/// The signing key this test's provider and this machine share. Invented here
/// and nowhere near anything real.
const SIGNING: &str = "a signing key for a test";

fn paying(at: &str) -> Payments {
    Payments::Hosted(Hosted {
        name: "hosted".to_owned(),
        at: at.to_owned(),
        key: Secret::new("a key".to_owned()),
        signing: Secret::new(SIGNING.to_owned()),
    })
}

fn signed(body: &str) -> String {
    webhook::sign(
        &Secret::new(SIGNING.as_bytes().to_vec()),
        "payment",
        0,
        body,
    )
}

async fn a_shop_that_takes_money() -> (Shop, provider::Took) {
    let (at, took) = provider::listening().await;
    let mut shop = a_shop().await;

    let mut state = AppState::new(shop.db.clone());
    state.payments = std::sync::Arc::new(paying(&at));
    state.allow_private_destinations = true;

    shop.router = mavi::router(state);
    shop.provider_at = at;

    // The token was minted against the old router; the new one shares its
    // database, so it still works.
    (shop, took)
}

#[tokio::test]
async fn checking_out_says_where_to_go_and_pay() {
    let (shop, took) = a_shop_that_takes_money().await;
    let thing = shop.a_product(2500, 3).await;

    let (status, placed) = shop.buy(thing, 2, &format!("k-{}", Uuid::now_v7())).await;

    assert_eq!(status, StatusCode::CREATED, "{placed}");
    assert_eq!(placed["order"]["total"]["minor"], 5000);
    assert!(
        placed["pay_at"]
            .as_str()
            .is_some_and(|at| at.starts_with("https://payments.example/")),
        "checking out gave nowhere to pay: {placed}"
    );

    let asked = took.0.lock().expect("a lock").clone();
    assert_eq!(asked.len(), 1);
    assert_eq!(
        asked[0]["amount_minor"], 5000,
        "the provider was asked for the wrong amount"
    );
}

#[tokio::test]
async fn a_callback_nobody_signed_pays_for_nothing() {
    let (shop, _) = a_shop_that_takes_money().await;
    let thing = shop.a_product(1000, 1).await;
    let (_, placed) = shop.buy(thing, 1, &format!("k-{}", Uuid::now_v7())).await;

    let mut conn = shop.db.tenant(shop.tenant).await.expect("begin");
    let payment: (String,) = sqlx::query_as("select provider_ref from payments")
        .fetch_one(conn.conn())
        .await
        .expect("a payment");

    let body = serde_json::json!({
        "provider_ref": payment.0, "amount_minor": 1000, "state": "paid"
    })
    .to_string();

    let response = shop
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/sites/payments/callback")
                .header(header::HOST, &shop.host)
                .header(header::CONTENT_TYPE, "application/json")
                .header("webhook-signature", "v1,not a signature")
                .body(Body::from(body))
                .expect("a request"),
        )
        .await
        .expect("a response");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let id = placed["order"]["id"].as_str().expect("an id");
    let (_, order) = shop
        .send("GET", &format!("/api/sites/orders/{id}"), None, None)
        .await;

    assert_eq!(
        order["state"], "pending",
        "an unsigned callback paid an order"
    );
}

#[tokio::test]
async fn a_signed_callback_pays_the_order_once() {
    let (shop, _) = a_shop_that_takes_money().await;
    let thing = shop.a_product(1000, 2).await;
    let (_, placed) = shop.buy(thing, 1, &format!("k-{}", Uuid::now_v7())).await;

    let mut conn = shop.db.tenant(shop.tenant).await.expect("begin");
    let payment: (String,) = sqlx::query_as("select provider_ref from payments")
        .fetch_one(conn.conn())
        .await
        .expect("a payment");

    let body = serde_json::json!({
        "provider_ref": payment.0, "amount_minor": 1000, "state": "paid"
    })
    .to_string();

    let callback = |body: String| {
        let host = shop.host.clone();
        let router = shop.router.clone();

        async move {
            let signature = signed(&body);

            router
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/sites/payments/callback")
                        .header(header::HOST, host)
                        .header(header::CONTENT_TYPE, "application/json")
                        .header("webhook-signature", signature)
                        .body(Body::from(body))
                        .expect("a request"),
                )
                .await
                .expect("a response")
                .status()
        }
    };

    assert_eq!(callback(body.clone()).await, StatusCode::CREATED);
    assert_eq!(
        callback(body).await,
        StatusCode::OK,
        "the same callback twice did something twice"
    );

    let id = placed["order"]["id"].as_str().expect("an id");
    let (_, order) = shop
        .send("GET", &format!("/api/sites/orders/{id}"), None, None)
        .await;

    assert_eq!(order["state"], "paid");

    // Paid for, so the hold is a sale: a lapse must not put the stock back.
    let mut conn = shop.db.tenant(shop.tenant).await.expect("begin");
    sqlx::query("update stock_holds set expires_at = now() - interval '1 minute'")
        .execute(conn.conn())
        .await
        .expect("walk forward");
    conn.commit().await.expect("commit");

    let state = AppState::new(shop.db.clone());
    mavi::shop::release_holds(&state, shop.tenant)
        .await
        .expect("release");

    assert_eq!(
        shop.stock_of(thing).await,
        1,
        "something paid for came back"
    );
}

#[tokio::test]
async fn a_callback_for_the_wrong_amount_is_not_a_payment() {
    let (shop, _) = a_shop_that_takes_money().await;
    let thing = shop.a_product(1000, 1).await;
    shop.buy(thing, 1, &format!("k-{}", Uuid::now_v7())).await;

    let mut conn = shop.db.tenant(shop.tenant).await.expect("begin");
    let payment: (String,) = sqlx::query_as("select provider_ref from payments")
        .fetch_one(conn.conn())
        .await
        .expect("a payment");

    let body = serde_json::json!({
        "provider_ref": payment.0, "amount_minor": 1, "state": "paid"
    })
    .to_string();

    let response = shop
        .router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/sites/payments/callback")
                .header(header::HOST, &shop.host)
                .header(header::CONTENT_TYPE, "application/json")
                .header("webhook-signature", signed(&body))
                .body(Body::from(body))
                .expect("a request"),
        )
        .await
        .expect("a response");

    assert_eq!(response.status(), StatusCode::CONFLICT);
}

/// The callback that never arrived. The provider has the money and this says
/// the order is unpaid, which is the difference reconciliation exists to find.
#[tokio::test]
async fn reconciliation_finds_what_a_lost_callback_left_behind() {
    let (shop, _) = a_shop_that_takes_money().await;
    let thing = shop.a_product(1000, 3).await;
    let (_, placed) = shop.buy(thing, 1, &format!("k-{}", Uuid::now_v7())).await;

    let id = placed["order"]["id"].as_str().expect("an id").to_owned();

    let (_, before) = shop
        .send("GET", &format!("/api/sites/orders/{id}"), None, None)
        .await;

    assert_eq!(before["state"], "pending");

    // Asked of the provider this shop actually used, which is holding the
    // money and says so.
    let mut state = AppState::new(shop.db.clone());
    state.payments = std::sync::Arc::new(paying(&shop.provider_at));

    let put_right = mavi::shop::reconcile(&state, shop.tenant)
        .await
        .expect("reconcile");

    assert_eq!(put_right, 1, "the difference was not put right");

    let (_, after) = shop
        .send("GET", &format!("/api/sites/orders/{id}"), None, None)
        .await;

    assert_eq!(after["state"], "paid");
}

#[tokio::test]
async fn a_code_that_asks_for_a_basket_is_refused_below_it() {
    let shop = a_shop().await;
    let thing = shop.a_product(1_000, 5).await;

    let (status, made) = shop
        .send(
            "POST",
            "/api/coupons",
            Some(&shop.token),
            Some(serde_json::json!({
                "code": "bigbasket",
                "kind": "amount",
                "value": 500,
                "minimum_minor": 100_000,
            })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{made}");

    let (status, refused) = shop
        .send(
            "POST",
            "/api/sites/checkout",
            None,
            Some(serde_json::json!({
                "email": "somebody@example.test",
                "items": [{ "product_id": thing, "quantity": 1 }],
                "coupon": "BIGBASKET",
                "idempotency_key": format!("k-{}", Uuid::now_v7()),
            })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert_eq!(
        refused["error"]["key"], "basket_has_not_reached_what_that_code_asks",
        "{refused}"
    );
}

#[tokio::test]
async fn a_code_once_per_person_is_once_for_that_person() {
    let shop = a_shop().await;
    let thing = shop.a_product(1_000, 5).await;

    shop.send(
        "POST",
        "/api/coupons",
        Some(&shop.token),
        Some(serde_json::json!({
            "code": "oneeach",
            "kind": "amount",
            "value": 100,
            "per_shopper": 1,
        })),
    )
    .await;

    let buy = |who: &'static str| {
        let shop = shop.clone();

        async move {
            shop.send(
                "POST",
                "/api/sites/checkout",
                None,
                Some(serde_json::json!({
                    "email": who,
                    "items": [{ "product_id": thing, "quantity": 1 }],
                    "coupon": "ONEEACH",
                    "idempotency_key": format!("k-{}", Uuid::now_v7()),
                })),
            )
            .await
        }
    };

    let (first, said) = buy("somebody@example.test").await;
    assert_eq!(first, StatusCode::CREATED, "{said}");

    let (again, refused) = buy("somebody@example.test").await;
    assert_eq!(
        again,
        StatusCode::CONFLICT,
        "one person used a once-each code twice: {refused}"
    );

    // Somebody else's first time is still their first time.
    let (theirs, said) = buy("somebody-else@example.test").await;
    assert_eq!(theirs, StatusCode::CREATED, "{said}");
}

#[tokio::test]
async fn an_order_has_a_number_a_customer_can_say() {
    let shop = a_shop().await;
    let thing = shop.a_product(1_000, 5).await;

    let (_, first) = shop.buy(thing, 1, &format!("k-{}", Uuid::now_v7())).await;
    let (_, second) = shop.buy(thing, 1, &format!("k-{}", Uuid::now_v7())).await;

    let one = first["order"]["number"].as_i64().expect("a number");
    let two = second["order"]["number"].as_i64().expect("a number");

    // From one, and then one more each time. A number somebody reads out on
    // the telephone is no use if it is a uuid or if it skips.
    assert_eq!(one, 1, "the first order was not the first: {first}");
    assert_eq!(two, one + 1, "{first} then {second}");
}

#[tokio::test]
async fn one_order_says_what_was_in_it() {
    let shop = a_shop().await;
    let thing = shop.a_product(1_000, 5).await;

    let (_, placed) = shop.buy(thing, 2, &format!("k-{}", Uuid::now_v7())).await;

    let id = placed["order"]["id"].as_str().expect("an id").to_owned();

    let (status, whole) = shop
        .send("GET", &format!("/api/orders/{id}"), Some(&shop.token), None)
        .await;

    assert_eq!(status, StatusCode::OK, "{whole}");
    assert_eq!(whole["lines"][0]["quantity"], 2, "{whole}");
    assert_eq!(whole["lines"][0]["each"]["minor"], 1_000, "{whole}");
    assert_eq!(whole["order"]["number"], placed["order"]["number"]);
}
