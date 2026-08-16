//! Buying something, and what happens to the shelf.
//!
//! Every rule the shop crate states about money and stock, asked of the thing
//! that actually runs: a basket goes in at the front and the shelf comes back
//! changed, or the order is refused and the shelf is exactly as it was.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use mavi_core::grant::Grants;
use mavi_db::Db;
use mavi_everything::mounted::site;
use mavi_files::InADirectory;
use mavi_http::Caller;
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row};
use tower::ServiceExt;
use uuid::Uuid;

fn postgres() -> Option<String> {
    let address = std::env::var("TEST_DATABASE_URL").ok();

    assert!(
        address.is_some() || std::env::var("CI").is_err(),
        "CI has no TEST_DATABASE_URL, so nobody ever bought anything"
    );

    address
}

async fn fresh(named: &str) -> Db {
    let address = postgres().expect("checked by the caller");
    let named = format!(
        "mavi_shop_{}_{}",
        named.replace('-', "_"),
        Uuid::now_v7().simple()
    );

    let mut admin = PgConnection::connect(&address).await.expect("a connection");
    sqlx::query(&format!("create database {named}"))
        .execute(&mut admin)
        .await
        .expect("a database of its own");

    let (front, _) = address
        .rsplit_once('/')
        .expect("an address with a database");
    let db = Db::open(&format!("{front}/{named}"), 4)
        .await
        .expect("the new database");

    db.migrate().await.expect("every migration");

    db
}

fn a_shopkeeper() -> mavi_serve::WhoIsAsking {
    Arc::new(|headers| {
        Box::pin(async move {
            if headers.contains_key("authorization") {
                Caller::AnAccount {
                    id: "01930000-0000-7000-8000-000000000001".to_owned(),
                    grants: Grants::of(["shop:view", "shop:write"].map(ToOwned::to_owned)),
                    session: None,
                }
            } else {
                Caller::Nobody
            }
        })
    })
}

/// Somewhere for files to go, of this test's own.
fn somewhere_for_files() -> Arc<dyn mavi_core::ports::Files> {
    Arc::new(InADirectory::at(
        std::env::temp_dir().join(format!("mavi-{}", Uuid::now_v7())),
    ))
}

async fn asked(db: &Db, request: Request<Body>) -> (StatusCode, Value) {
    let answer = site(db, somewhere_for_files(), a_shopkeeper())
        .into_router()
        .oneshot(request)
        .await
        .expect("an answer");

    let status = answer.status();
    let body = axum::body::to_bytes(answer.into_body(), 256 * 1024)
        .await
        .expect("a body");

    let body = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).unwrap_or(Value::Null)
    };

    (status, body)
}

fn signed_in(mut request: Request<Body>) -> Request<Body> {
    request.headers_mut().insert(
        "authorization",
        "Bearer whatever".parse().expect("a header"),
    );

    request
}

fn posting(path: &str, what: &Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(what.to_string()))
        .expect("a request")
}

async fn on_the_shelf(db: &Db, slug: &str, how_many: i32) -> String {
    let (status, product) = asked(
        db,
        signed_in(posting(
            "/api/products",
            &json!({
                "slug": slug,
                "name": "A Thing",
                "price_minor": 1250,
                "currency": "TRY",
                "on_the_shelf": how_many,
            }),
        )),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{product}");

    product["id"].as_str().expect("an id").to_owned()
}

async fn how_many_left(db: &Db, id: &str) -> i32 {
    sqlx::query("select on_the_shelf from products where id = $1")
        .bind(Uuid::parse_str(id).expect("an id"))
        .fetch_one(db.pool())
        .await
        .expect("the row")
        .get("on_the_shelf")
}

fn a_basket(product: &str, how_many: u32, once: &str) -> Value {
    basket_of("somebody@example.test", product, how_many, once)
}

fn basket_of(email: &str, product: &str, how_many: u32, once: &str) -> Value {
    json!({
        "email": email,
        "wanted": [{ "product": product, "how_many": how_many }],
        "said_once": once,
    })
}

#[tokio::test]
async fn buying_something_takes_it_off_the_shelf_and_holds_it() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("bought").await;
    let thing = on_the_shelf(&db, "a-thing", 3).await;

    let (status, order) = asked(
        &db,
        posting("/api/open/orders", &a_basket(&thing, 2, "one")),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(order["total"]["minor"], 2500);
    // A number somebody can read down a telephone, rather than a uuid.
    assert!(order["number"].is_number());

    assert_eq!(how_many_left(&db, &thing).await, 1);
}

#[tokio::test]
async fn the_same_request_twice_is_one_order() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("once").await;
    let thing = on_the_shelf(&db, "a-thing", 5).await;

    let (first_status, first) = asked(
        &db,
        posting("/api/open/orders", &a_basket(&thing, 2, "the-same-request")),
    )
    .await;
    let (second_status, second) = asked(
        &db,
        posting("/api/open/orders", &a_basket(&thing, 2, "the-same-request")),
    )
    .await;

    assert_eq!(first_status, StatusCode::CREATED);
    assert_eq!(second_status, StatusCode::CREATED);

    // The same order, and the shelf moved once. A retry after a timeout is the
    // ordinary case, not the strange one.
    assert_eq!(first["id"], second["id"]);
    assert_eq!(how_many_left(&db, &thing).await, 3);
}

#[tokio::test]
async fn asking_for_more_than_there_are_moves_nothing() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("not_enough").await;
    let thing = on_the_shelf(&db, "a-thing", 1).await;

    let (status, refusal) = asked(
        &db,
        posting("/api/open/orders", &a_basket(&thing, 2, "too-many")),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(refusal["key"], "not_that_many_left");
    // And what is left is named, so a page can say how many rather than making
    // somebody press the button again to find out.
    assert_eq!(refusal["named"]["left"], "1");

    assert_eq!(
        how_many_left(&db, &thing).await,
        1,
        "the shelf moved anyway"
    );
}

#[tokio::test]
async fn an_order_called_off_puts_the_stock_back_once() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("back").await;
    let thing = on_the_shelf(&db, "a-thing", 3).await;

    let (_, order) = asked(
        &db,
        posting("/api/open/orders", &a_basket(&thing, 2, "called-off")),
    )
    .await;
    let id = order["id"].as_str().expect("an id").to_owned();

    assert_eq!(how_many_left(&db, &thing).await, 1);

    let calling_off = || {
        signed_in(posting(
            &format!("/api/orders/{id}/moves"),
            &json!({ "to": "called_off" }),
        ))
    };

    let (status, _) = asked(&db, calling_off()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(how_many_left(&db, &thing).await, 3);

    // Twice does not invent two more. The hold is settled, and a settled hold
    // is not put back again.
    let (again, _) = asked(&db, calling_off()).await;

    assert_eq!(again, StatusCode::CONFLICT, "an order went back a step");
    assert_eq!(how_many_left(&db, &thing).await, 3, "stock was invented");
}

#[tokio::test]
async fn nothing_is_sent_that_was_not_paid_for() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("unpaid").await;
    let thing = on_the_shelf(&db, "a-thing", 3).await;

    let (_, order) = asked(
        &db,
        posting("/api/open/orders", &a_basket(&thing, 1, "unpaid")),
    )
    .await;
    let id = order["id"].as_str().expect("an id").to_owned();

    let (status, refusal) = asked(
        &db,
        signed_in(posting(
            &format!("/api/orders/{id}/moves"),
            &json!({ "to": "sent" }),
        )),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(refusal["key"], "that_is_not_where_an_order_goes_next");

    // Paid first, then sent, and the stock stays gone: a hold that became a
    // sale is not put back.
    for to in ["paid", "sent"] {
        let (status, _) = asked(
            &db,
            signed_in(posting(
                &format!("/api/orders/{id}/moves"),
                &json!({ "to": to }),
            )),
        )
        .await;

        assert_eq!(status, StatusCode::OK, "could not move to {to}");
    }

    assert_eq!(how_many_left(&db, &thing).await, 2);
}

#[tokio::test]
async fn a_code_comes_off_and_is_used_once() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("code").await;
    let thing = on_the_shelf(&db, "a-thing", 5).await;

    let (status, _) = asked(
        &db,
        signed_in(posting(
            "/api/coupons",
            &json!({ "code": "spring-26", "percent": 10, "at_most_uses": 1 }),
        )),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let with_the_code = |once: &str| {
        let mut basket = a_basket(&thing, 1, once);
        basket["code"] = json!("SPRING-26");

        posting("/api/open/orders", &basket)
    };

    let (status, order) = asked(&db, with_the_code("first")).await;

    assert_eq!(status, StatusCode::CREATED);
    // Ten per cent off 12.50, rounded the shop's way: 11.25.
    assert_eq!(order["total"]["minor"], 1125);

    // Once means once, and the second order is refused rather than quietly
    // charged the full price.
    let (status, refusal) = asked(&db, with_the_code("second")).await;

    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(refusal["key"], "that_code_has_run_out");
}

#[tokio::test]
async fn a_page_is_never_told_how_many_are_left() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("shown").await;
    on_the_shelf(&db, "a-thing", 3).await;

    let (status, shown) = asked(
        &db,
        Request::builder()
            .uri("/api/open/products")
            .body(Body::empty())
            .expect("a request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);

    let first = &shown["items"][0];
    let mut keys: Vec<&str> = first
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();

    // A shop that answers "three left" to anybody who asks has published its
    // stock list. What a page needs is whether it can be bought.
    assert_eq!(keys, ["about", "can_be_bought", "name", "price", "slug"]);
    assert_eq!(first["can_be_bought"], true);
}

#[tokio::test]
async fn one_persons_key_does_not_answer_with_another_persons_order() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("guessed").await;
    let thing = on_the_shelf(&db, "a-thing", 5).await;

    // Anybody may place an order, and the key is theirs to choose — so a key
    // is something a stranger can guess. What an order carries is the address
    // somebody typed, what they bought and what they paid.
    let (status, theirs) = asked(
        &db,
        posting(
            "/api/open/orders",
            &basket_of("somebody@example.test", &thing, 1, "abc123"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, guessed) = asked(
        &db,
        posting(
            "/api/open/orders",
            &basket_of("a-stranger@example.test", &thing, 1, "abc123"),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_ne!(
        theirs["id"], guessed["id"],
        "a guessed key answered with somebody else's order"
    );

    // And the shelf moved twice, because these are two orders rather than one
    // read back.
    assert_eq!(how_many_left(&db, &thing).await, 3);
}
