//! A form, from both sides.
//!
//! The one domain whose writing side is open to anybody at all, which makes it
//! the one where the difference between "checked" and "checked where it
//! matters" is a row in somebody's inbox.

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
        "CI has no TEST_DATABASE_URL, so nobody ever filled a form in"
    );

    address
}

async fn fresh(named: &str) -> Db {
    let address = postgres().expect("checked by the caller");
    let named = format!(
        "mavi_forms_{}_{}",
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

fn an_editor() -> mavi_serve::WhoIsAsking {
    Arc::new(|headers| {
        Box::pin(async move {
            if headers.contains_key("authorization") {
                Caller::AnAccount {
                    id: "01930000-0000-7000-8000-000000000001".to_owned(),
                    grants: Grants::of(["forms:view", "forms:write"].map(ToOwned::to_owned)),
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
    let answer = site(db, somewhere_for_files(), an_editor())
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

async fn a_contact_form(db: &Db) -> Value {
    let (status, form) = asked(
        db,
        signed_in(posting(
            "/api/forms",
            &json!({
                "slug": "contact",
                "name": "Contact",
                "fields": [
                    { "key": "name", "label": "Your name", "required": true, "kind": "text" },
                    { "key": "email", "label": "Your address", "required": true, "kind": "email" },
                ],
            }),
        )),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED, "{form}");

    form
}

#[tokio::test]
async fn anybody_can_fill_a_form_in_and_only_an_account_can_read_what_came() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("both_sides").await;
    let form = a_contact_form(&db).await;

    // No token at all: this is the one writing endpoint in the whole API that
    // a stranger reaches.
    let (status, received) = asked(
        &db,
        posting(
            "/api/open/forms/contact",
            &json!({ "answers": { "name": "A Visitor", "email": "somebody@example.test" } }),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert!(received["id"].is_string(), "{received}");

    // And what a visitor is told is that it arrived. Nothing about the site.
    assert_eq!(received.as_object().expect("an object").len(), 1);

    let id = form["id"].as_str().expect("an id");

    let (status, _) = asked(
        &db,
        Request::builder()
            .uri(format!("/api/forms/{id}/filled"))
            .body(Body::empty())
            .expect("a request"),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a stranger read the inbox"
    );

    let (status, came) = asked(
        &db,
        signed_in(
            Request::builder()
                .uri(format!("/api/forms/{id}/filled"))
                .body(Body::empty())
                .expect("a request"),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(came["items"].as_array().expect("items").len(), 1);
    assert_eq!(came["items"][0]["answers"]["name"], "A Visitor");
}

#[tokio::test]
async fn what_the_form_never_asked_for_is_not_stored() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("unasked").await;
    a_contact_form(&db).await;

    let (status, refusal) = asked(
        &db,
        posting(
            "/api/open/forms/contact",
            &json!({
                "answers": {
                    "name": "A Visitor",
                    "email": "somebody@example.test",
                    "role": "owner",
                }
            }),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(refusal["key"], "that_form_has_no_such_field");

    let written: i64 = sqlx::query("select count(*) from filled")
        .fetch_one(db.pool())
        .await
        .expect("a count")
        .get(0);

    assert_eq!(written, 0, "a key nobody asked for was stored anyway");
}

#[tokio::test]
async fn a_closed_form_answers_the_way_one_that_was_never_made_does() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("closed").await;
    let form = a_contact_form(&db).await;
    let id = form["id"].as_str().expect("an id").to_owned();

    let (status, _) = asked(
        &db,
        signed_in(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/forms/{id}"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "open": false }).to_string()))
                .expect("a request"),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let closed = asked(
        &db,
        Request::builder()
            .uri("/api/open/forms/contact")
            .body(Body::empty())
            .expect("a request"),
    )
    .await;

    let never_made = asked(
        &db,
        Request::builder()
            .uri("/api/open/forms/nothing-like-this")
            .body(Body::empty())
            .expect("a request"),
    )
    .await;

    // The same answer to both, because the difference is a way to ask what
    // forms this site has.
    assert_eq!(closed.0, StatusCode::NOT_FOUND);
    assert_eq!(never_made.0, StatusCode::NOT_FOUND);
    assert_eq!(closed.1["key"], never_made.1["key"]);
}

#[tokio::test]
async fn something_that_arrives_while_somebody_is_reading_is_not_marked_read() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("seen").await;
    let form = a_contact_form(&db).await;
    let id = form["id"].as_str().expect("an id").to_owned();

    let sending = |name: &'static str| {
        posting(
            "/api/open/forms/contact",
            &json!({ "answers": { "name": name, "email": "somebody@example.test" } }),
        )
    };

    asked(&db, sending("The First")).await;

    let (status, seen) = asked(
        &db,
        signed_in(posting(&format!("/api/forms/{id}/seen"), &json!({}))),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(seen["seen"], 1);

    // One arrives afterwards. Marking everything read rather than everything
    // up to that moment is how a message goes unanswered.
    asked(&db, sending("The Second")).await;

    let (status, unread) = asked(
        &db,
        signed_in(
            Request::builder()
                .uri(format!("/api/forms/{id}/filled?unseen=true"))
                .body(Body::empty())
                .expect("a request"),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(unread["items"].as_array().expect("items").len(), 1);
    assert_eq!(unread["items"][0]["answers"]["name"], "The Second");
}
