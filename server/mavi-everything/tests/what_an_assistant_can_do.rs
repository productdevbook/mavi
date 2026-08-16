//! An assistant, all the way through, against a real Postgres.
//!
//! The claim this is here to hold up: there is no second way in. A tool is an
//! endpoint, so what an assistant may do is what its account may do, what it
//! is refused is refused in the same words, and what it changes leaves the
//! same receipt.

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
        "CI has no TEST_DATABASE_URL, so no assistant ever did anything"
    );

    address
}

async fn fresh(named: &str) -> Db {
    let address = postgres().expect("checked by the caller");
    let named = format!(
        "mavi_assistant_{}_{}",
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

fn somewhere_for_files() -> Arc<dyn mavi_core::ports::Files> {
    Arc::new(InADirectory::at(
        std::env::temp_dir().join(format!("mavi-{}", Uuid::now_v7())),
    ))
}

/// Somebody holding exactly these, and nothing else.
fn holding(what: &'static [&'static str]) -> mavi_serve::WhoIsAsking {
    Arc::new(move |headers| {
        Box::pin(async move {
            if headers.contains_key("authorization") {
                Caller::AnAccount {
                    id: "01930000-0000-7000-8000-000000000001".to_owned(),
                    grants: Grants::of(what.iter().map(|held| (*held).to_owned())),
                    session: None,
                }
            } else {
                Caller::Nobody
            }
        })
    })
}

/// One thing said to the assistant's door.
async fn said(db: &Db, holds: &'static [&'static str], body: Value) -> (StatusCode, Value) {
    let answer = site(db, &somewhere_for_files(), holding(holds))
        .into_router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/assistant")
                .header("content-type", "application/json")
                .header("authorization", "Bearer whatever")
                .body(Body::from(body.to_string()))
                .expect("a request"),
        )
        .await
        .expect("an answer");

    let status = answer.status();
    let body = axum::body::to_bytes(answer.into_body(), 512 * 1024)
        .await
        .expect("a body");

    let body = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).unwrap_or(Value::Null)
    };

    (status, body)
}

fn asked(id: u32, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

#[tokio::test]
async fn an_assistant_is_told_what_it_can_do_and_not_what_it_cannot() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("listed").await;

    let (status, answer) = said(&db, &["content:view"], asked(1, "tools/list", json!({}))).await;

    assert_eq!(status, StatusCode::OK);

    let tools: Vec<&str> = answer["result"]["tools"]
        .as_array()
        .expect("a list of tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();

    // What this account holds reaches reading, and nothing else.
    assert!(tools.contains(&"writings_list"), "{tools:#?}");
    assert!(tools.contains(&"writings_read"), "{tools:#?}");
    assert!(!tools.contains(&"writings_write"), "{tools:#?}");
    assert!(!tools.contains(&"people_write"), "{tools:#?}");

    // The door is not among what comes through it. Mounted after everything
    // else, so it is not in the list it builds — an assistant cannot ask this
    // installation to talk to itself.
    assert!(!tools.contains(&"assistant_talk"), "{tools:#?}");
}

#[tokio::test]
async fn what_a_tool_takes_is_what_the_endpoint_declared() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("takes").await;

    let (_, answer) = said(&db, &["content:view"], asked(1, "tools/list", json!({}))).await;

    let one = answer["result"]["tools"]
        .as_array()
        .expect("a list")
        .iter()
        .find(|tool| tool["name"] == "writings_read")
        .expect("reading one")
        .clone();

    // The hole in `/api/writings/{id}`, described where an assistant looks for
    // it — and described as a uuid, because that is what the endpoint said.
    assert_eq!(one["inputSchema"]["properties"]["id"]["type"], "string");
    assert_eq!(one["inputSchema"]["properties"]["id"]["format"], "uuid");
    assert_eq!(one["inputSchema"]["required"][0], "id");
}

#[tokio::test]
async fn a_tool_writes_the_row_and_the_receipt_a_request_would_have() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("wrote").await;

    let (status, answer) = said(
        &db,
        &["content:view", "content:write"],
        asked(
            1,
            "tools/call",
            json!({
                "name": "writings_write",
                "arguments": {
                    "body": {
                        "kind": "post",
                        "language": "en",
                        "slug": "hello",
                        "title": "A Title",
                        "body": "Something written.",
                    },
                },
            }),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert!(
        answer["result"]["isError"].is_null(),
        "{:#?}",
        answer["result"]
    );
    assert_eq!(answer["result"]["structuredContent"]["slug"], "hello");

    // The row, and the record of it. The receipt says the tool's own endpoint
    // rather than the door it came through, because what happened to a writing
    // has one answer however it was asked for.
    let (did, about): (String, Option<String>) =
        sqlx::query("select did, about_id from receipts order by created_at desc limit 1")
            .fetch_one(db.pool())
            .await
            .map(|row| (row.get("did"), row.get("about_id")))
            .expect("a receipt");

    assert_eq!(did, "writings.write");
    assert_eq!(
        about.as_deref(),
        answer["result"]["structuredContent"]["id"].as_str()
    );
}

#[tokio::test]
async fn a_tool_nobody_may_use_refuses_in_the_same_words() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("refused").await;

    // Listed or not, the guard is what stops it — so it is asked for by name
    // rather than found in the list first.
    let (status, answer) = said(
        &db,
        &["content:view"],
        asked(
            1,
            "tools/call",
            json!({
                "name": "writings_write",
                "arguments": { "body": { "kind": "post", "language": "en", "slug": "hello", "title": "A Title" } },
            }),
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(answer["result"]["isError"], true);

    let written: i64 = sqlx::query("select count(*) from writings")
        .fetch_one(db.pool())
        .await
        .expect("a count")
        .get(0);

    assert_eq!(written, 0, "a tool nobody may use wrote a row");
}

#[tokio::test]
async fn a_tool_that_refused_is_not_the_protocol_refusing() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("conflict").await;

    let writing = |slug: &'static str| {
        asked(
            1,
            "tools/call",
            json!({
                "name": "writings_write",
                "arguments": {
                    "body": {
                        "kind": "post",
                        "language": "en",
                        "slug": slug,
                        "title": "A Title",
                        "body": "Something written.",
                    },
                },
            }),
        )
    };

    let holds: &[&str] = &["content:view", "content:write"];

    let (_, first) = said(&db, holds, writing("hello")).await;
    assert!(first["result"]["isError"].is_null());

    let (status, again) = said(&db, holds, writing("hello")).await;

    // The same address twice. What comes back is a tool result the model can
    // read and act on — a new slug — rather than a transport error it cannot.
    assert_eq!(status, StatusCode::OK);
    assert_eq!(again["result"]["isError"], true);
    assert!(again["error"].is_null());

    let text = again["result"]["content"][0]["text"]
        .as_str()
        .expect("something to read");

    assert!(
        text.contains("something_else_answers_at_that_address"),
        "{text}"
    );
}

#[tokio::test]
async fn a_method_this_does_not_serve_is_named_back_and_a_notification_is_not() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("method").await;

    let (status, answer) = said(
        &db,
        &["content:view"],
        asked(7, "resources/list", json!({})),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    // JSON-RPC's own number for it, which is what a generic client matches on.
    assert_eq!(answer["error"]["code"], -32601);
    assert_eq!(answer["id"], 7);

    // Nobody who did not ask is answered. No `id` is JSON-RPC for "this is a
    // notification", and telling one it got a method wrong is answering it.
    let (status, answer) = said(
        &db,
        &["content:view"],
        json!({ "jsonrpc": "2.0", "method": "resources/list" }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(answer, Value::Null);
}

#[tokio::test]
async fn nobody_gets_in_at_all() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("nobody").await;

    let answer = site(&db, &somewhere_for_files(), holding(&["content:view"]))
        .into_router()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/assistant")
                .header("content-type", "application/json")
                .body(Body::from(asked(1, "tools/list", json!({})).to_string()))
                .expect("a request"),
        )
        .await
        .expect("an answer");

    // The door itself needs an account. A protocol that lists what is there
    // before asking who is asking is one that describes an installation to
    // anybody who connects.
    assert_eq!(answer.status(), StatusCode::UNAUTHORIZED);
}
