//! Dragging a card, against a real Postgres.
//!
//! The unit test proves the arithmetic runs out. This proves the thing built
//! on top of it does not: fifty drops between the same two cards, through the
//! router, and the board still keeps the order somebody put it in.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use mavi_core::grant::Grants;
use mavi_db::Db;
use mavi_everything::mounted::site;
use mavi_files::InADirectory;
use mavi_http::Caller;
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
use tower::ServiceExt;
use uuid::Uuid;

fn postgres() -> Option<String> {
    let address = std::env::var("TEST_DATABASE_URL").ok();

    assert!(
        address.is_some() || std::env::var("CI").is_err(),
        "CI has no TEST_DATABASE_URL, so nothing was ever dragged"
    );

    address
}

async fn fresh(named: &str) -> Db {
    let address = postgres().expect("checked by the caller");
    let named = format!(
        "mavi_boards_{}_{}",
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

fn somebody() -> mavi_serve::WhoIsAsking {
    Arc::new(|_| {
        Box::pin(async {
            Caller::AnAccount {
                id: "01930000-0000-7000-8000-000000000001".to_owned(),
                grants: Grants::of(["boards:view", "boards:write"].map(ToOwned::to_owned)),
                session: None,
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
    let answer = site(db, &somewhere_for_files(), somebody())
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

fn posting(path: &str, what: &Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(what.to_string()))
        .expect("a request")
}

fn putting(path: &str, what: &Value) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(what.to_string()))
        .expect("a request")
}

#[tokio::test]
async fn a_board_starts_with_somewhere_to_put_things() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("empty").await;

    let (status, refusal) = asked(
        &db,
        posting("/api/boards", &json!({ "name": "Work", "stages": [] })),
    )
    .await;

    // A board with no columns is a board nothing can be put on, which is a
    // thing somebody finds out one screen later.
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(refusal["key"], "a_board_has_somewhere_to_put_things");
}

#[tokio::test]
async fn fifty_drops_between_the_same_two_cards_still_keeps_the_order() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("drops").await;

    let (status, board) = asked(
        &db,
        posting(
            "/api/boards",
            &json!({ "name": "Work", "stages": ["To do", "Doing"] }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let board_id = board["id"].as_str().expect("an id").to_owned();
    let stage = board["stages"][0]["id"]
        .as_str()
        .expect("a stage")
        .to_owned();

    let carding = |title: &str| {
        posting(
            &format!("/api/boards/{board_id}/cards"),
            &json!({ "stage": stage, "title": title }),
        )
    };

    let (_, top) = asked(&db, carding("The top")).await;
    let (_, bottom) = asked(&db, carding("The bottom")).await;

    let top_id = top["id"].as_str().expect("an id").to_owned();
    let bottom_id = bottom["id"].as_str().expect("an id").to_owned();

    // One card, dropped between the same two, over and over. The arithmetic
    // runs out after about fifty; what happens then is the column is spread
    // out and the drop is tried again, and the only way to know that works is
    // to do it.
    let (_, wanderer) = asked(&db, carding("The one being dragged")).await;
    let wanderer_id = wanderer["id"].as_str().expect("an id").to_owned();

    for at in 0..60 {
        let (status, moved) = asked(
            &db,
            putting(
                &format!("/api/cards/{wanderer_id}/place"),
                &json!({ "stage": stage, "after": top_id, "before": bottom_id }),
            ),
        )
        .await;

        assert_eq!(status, StatusCode::OK, "drop {at} was refused: {moved}");
    }

    let (status, cards) = asked(
        &db,
        Request::builder()
            .uri(format!("/api/boards/{board_id}/cards"))
            .body(Body::empty())
            .expect("a request"),
    )
    .await;

    assert_eq!(status, StatusCode::OK);

    let order: Vec<&str> = cards
        .as_array()
        .expect("cards")
        .iter()
        .map(|card| card["title"].as_str().unwrap_or_default())
        .collect();

    // The order somebody put it in, after sixty drops: top, the wanderer,
    // bottom. Two cards holding one number would show up here as an order
    // that changes between reads.
    assert_eq!(
        order,
        ["The top", "The one being dragged", "The bottom"],
        "the board stopped keeping the order somebody put it in"
    );
}
