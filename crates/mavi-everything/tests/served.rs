//! A request, all the way through, against a real Postgres.
//!
//! Everything else in this workspace is a piece: a rule, a query, a router. This
//! is the first test where one request goes in at the front and comes out
//! having written a row — through the guard, through the domain, through the
//! schema, and through the audit rule that will not let a change answer without
//! a receipt.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use mavi_core::grant::Grants;
use mavi_db::Db;
use mavi_everything::mounted::site;
use mavi_http::Caller;
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, Row};
use tower::ServiceExt;
use uuid::Uuid;

fn postgres() -> Option<String> {
    let address = std::env::var("TEST_DATABASE_URL").ok();

    assert!(
        address.is_some() || std::env::var("CI").is_err(),
        "CI has no TEST_DATABASE_URL, so nothing was ever served"
    );

    address
}

async fn fresh(named: &str) -> Db {
    let address = postgres().expect("checked by the caller");
    let named = format!(
        "mavi_served_{}_{}",
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

/// An editor with everything content is about, when they send a token.
fn an_editor() -> mavi_serve::WhoIsAsking {
    Arc::new(|headers| {
        Box::pin(async move {
            if headers.contains_key("authorization") {
                Caller::AnAccount {
                    id: "01930000-0000-7000-8000-000000000001".to_owned(),
                    grants: Grants::of(["content:write".to_owned(), "content:view".to_owned()]),
                }
            } else {
                Caller::Nobody
            }
        })
    })
}

async fn asked(db: &Db, request: Request<Body>) -> (StatusCode, Value) {
    let answer = site(db, an_editor())
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

fn writing(slug: &str) -> Request<Body> {
    signed_in(
        Request::builder()
            .method("POST")
            .uri("/api/writings")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "kind": "post",
                    "language": "en",
                    "slug": slug,
                    "title": "A Title",
                    "body": "Something written.",
                })
                .to_string(),
            ))
            .expect("a request"),
    )
}

#[tokio::test]
async fn writing_something_writes_the_row_and_the_record_of_it() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("made").await;

    let (status, made) = asked(&db, writing("hello")).await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(made["slug"], "hello");

    // The receipt, written in the same transaction. Its `did` is the
    // endpoint's own name, so what happened to this has one answer.
    let (did, about): (String, Option<String>) =
        sqlx::query("select did, about_id from receipts order by created_at desc limit 1")
            .fetch_one(db.pool())
            .await
            .map(|row| (row.get("did"), row.get("about_id")))
            .expect("a receipt");

    assert_eq!(did, "writings.write");
    assert_eq!(about.as_deref(), made["id"].as_str());
}

#[tokio::test]
async fn nobody_writes_anything_and_nothing_is_written_down_either() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("nobody").await;

    let (mut parts, body) = writing("hello").into_parts();
    parts.headers.remove("authorization");

    let (status, _) = asked(&db, Request::from_parts(parts, body)).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let written: i64 = sqlx::query("select count(*) from writings")
        .fetch_one(db.pool())
        .await
        .expect("a count")
        .get(0);

    assert_eq!(written, 0, "somebody who was turned away wrote a row");
}

#[tokio::test]
async fn two_things_cannot_answer_at_one_address() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("address").await;

    let (first, _) = asked(&db, writing("hello")).await;
    assert_eq!(first, StatusCode::CREATED);

    let (second, refusal) = asked(&db, writing("hello")).await;

    assert_eq!(second, StatusCode::CONFLICT);
    assert_eq!(refusal["key"], "something_else_answers_at_that_address");
}

#[tokio::test]
async fn a_page_ends_where_the_next_one_starts() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("paged").await;

    for at in 0..5 {
        let (status, _) = asked(&db, writing(&format!("hello-{at}"))).await;
        assert_eq!(status, StatusCode::CREATED);
    }

    let (status, first) = asked(
        &db,
        signed_in(
            Request::builder()
                .uri("/api/writings?limit=2")
                .body(Body::empty())
                .expect("a request"),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(first["items"].as_array().expect("items").len(), 2);

    let next = first["next"].as_str().expect("a cursor").to_owned();

    let (status, second) = asked(
        &db,
        signed_in(
            Request::builder()
                .uri(format!("/api/writings?limit=2&after={next}"))
                .body(Body::empty())
                .expect("a request"),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);

    // The whole reason a cursor exists: no row appears on both pages, and none
    // is skipped between them. Five rows written in one second share a
    // `created_at` to the microsecond often enough that an order without the
    // id in it walks straight past some of them.
    let ids = |page: &Value| -> Vec<String> {
        page["items"]
            .as_array()
            .expect("items")
            .iter()
            .map(|item| item["id"].as_str().unwrap_or_default().to_owned())
            .collect()
    };

    let first = ids(&first);
    let second = ids(&second);

    assert!(
        first.iter().all(|id| !second.contains(id)),
        "a row on two pages: {first:?} {second:?}"
    );
    assert_eq!(second.len(), 2);
}

#[tokio::test]
async fn what_is_thrown_away_frees_its_address_and_is_gone_from_the_listing() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("thrown").await;

    let (_, made) = asked(&db, writing("hello")).await;
    let id = made["id"].as_str().expect("an id").to_owned();

    let (status, _) = asked(
        &db,
        signed_in(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/writings/{id}"))
                .body(Body::empty())
                .expect("a request"),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = asked(
        &db,
        signed_in(
            Request::builder()
                .uri(format!("/api/writings/{id}"))
                .body(Body::empty())
                .expect("a request"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // And the address is free, which is the reason the index is partial.
    let (again, _) = asked(&db, writing("hello")).await;
    assert_eq!(again, StatusCode::CREATED);
}

#[tokio::test]
async fn what_is_described_and_not_yet_served() {
    if postgres().is_none() {
        return;
    }

    // Not a failure: a count of the work left, in the one place it can be
    // counted rather than guessed. It goes down as handlers are written, and
    // the day it reaches nothing this stops being a print and becomes a rule.
    let db = fresh("left").await;

    let described = mavi_everything::api();
    let serving = site(&db, an_editor());

    let left = serving.not_reachable(&described);

    println!(
        "{} of {} endpoints are not served yet",
        left.len(),
        described.endpoints.len()
    );

    assert!(
        left.len() < described.endpoints.len(),
        "nothing at all is served"
    );
    assert!(
        !serving.reachable().is_empty(),
        "what is mounted should be something"
    );
}
