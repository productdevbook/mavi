//! Setting a site up, and getting into it.
//!
//! The first two things anybody does with an installation, and the pair that
//! decides whether anything else can be reached at all. Everything here goes
//! through the router: a token comes out of one request and is what the next
//! one is admitted by.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
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
        "CI has no TEST_DATABASE_URL, so nobody ever signed in"
    );

    address
}

async fn fresh(named: &str) -> Db {
    let address = postgres().expect("checked by the caller");
    let named = format!(
        "mavi_in_{}_{}",
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

/// Whoever is holding the token, worked out the way a running installation
/// works it out: one query against the sessions table, on the same runtime and
/// the same pool as everything else.
fn whoever_holds(db: Db) -> mavi_serve::WhoIsAsking {
    Arc::new(move |headers| {
        let db = db.clone();

        Box::pin(async move {
            let Some(token) = headers
                .get("authorization")
                .and_then(|said| said.to_str().ok())
                .and_then(|said| said.strip_prefix("Bearer "))
                .map(ToOwned::to_owned)
            else {
                return Caller::Nobody;
            };

            let Ok(mut tx) = db.begin().await else {
                return Caller::Nobody;
            };

            match mavi_people::store::whoever_holds(&mut tx, &token).await {
                Ok(Some(person)) => Caller::AnAccount {
                    id: person.id.to_string(),
                    grants: mavi_core::grant::Grants::of(person.grants),
                },
                _ => Caller::Nobody,
            }
        })
    })
}

async fn asked(db: &Db, request: Request<Body>) -> (StatusCode, Value) {
    let answer = site(db, whoever_holds(db.clone()))
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

fn holding(request: Request<Body>, token: &str) -> Request<Body> {
    let (mut parts, body) = request.into_parts();
    parts.headers.insert(
        "authorization",
        format!("Bearer {token}").parse().expect("a header"),
    );

    Request::from_parts(parts, body)
}

fn setting_up() -> Value {
    json!({
        "site": "A Site",
        "name": "Somebody",
        "email": "somebody@example.test",
        "password": "a long enough password",
    })
}

#[tokio::test]
async fn setting_a_site_up_makes_it_and_lets_the_owner_straight_in() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("setup").await;

    let (status, ready) = asked(&db, posting("/api/setup", &setting_up())).await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(ready["person"]["email"], "somebody@example.test");

    // The token comes back from setting up rather than being asked for
    // afterwards: telling somebody to sign in with the password they typed ten
    // seconds ago is a step that can only fail.
    let token = ready["token"].as_str().expect("a token").to_owned();

    let (status, who) = asked(
        &db,
        holding(
            Request::builder()
                .uri("/api/people")
                .body(Body::empty())
                .expect("a request"),
            &token,
        ),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(who["items"].as_array().expect("items").len(), 1);
}

#[tokio::test]
async fn a_site_is_set_up_once() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("twice").await;

    let (first, _) = asked(&db, posting("/api/setup", &setting_up())).await;
    assert_eq!(first, StatusCode::CREATED);

    let mut again = setting_up();
    again["email"] = json!("somebody-else@example.test");

    let (second, refusal) = asked(&db, posting("/api/setup", &again)).await;

    // One installation is one site, refused by the database rather than by a
    // look first — so two people setting it up in the same second is one site
    // and one refusal rather than a race nobody sees.
    assert_eq!(second, StatusCode::CONFLICT);
    assert_eq!(refusal["key"], "this_site_is_already_set_up");
}

#[tokio::test]
async fn what_is_kept_is_not_the_password() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("kept").await;

    asked(&db, posting("/api/setup", &setting_up())).await;

    let kept: String = sqlx::query("select password from people limit 1")
        .fetch_one(db.pool())
        .await
        .expect("the account")
        .get("password");

    assert!(!kept.contains("a long enough password"));
    assert!(kept.starts_with("$argon2"));

    // And the session is a hash too: a stolen database is not a stolen set of
    // sessions.
    let token: Vec<u8> = sqlx::query("select token from sessions limit 1")
        .fetch_one(db.pool())
        .await
        .expect("the session")
        .get("token");

    assert_eq!(token.len(), 32);
}

#[tokio::test]
async fn signing_in_with_the_wrong_password_says_what_a_stranger_is_told() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("wrong").await;
    asked(&db, posting("/api/setup", &setting_up())).await;

    let wrong = json!({
        "email": "somebody@example.test",
        "password": "not the password at all",
    });

    let nobody = json!({
        "email": "nobody-at-all@example.test",
        "password": "a long enough password",
    });

    let (wrong_status, wrong_said) = asked(&db, posting("/api/sessions", &wrong)).await;
    let (nobody_status, nobody_said) = asked(&db, posting("/api/sessions", &nobody)).await;

    // The same answer to both. The difference between them is a way to ask
    // which addresses have accounts here.
    assert_eq!(wrong_status, StatusCode::FORBIDDEN);
    assert_eq!(nobody_status, StatusCode::FORBIDDEN);
    assert_eq!(wrong_said["key"], nobody_said["key"]);
}

#[tokio::test]
async fn signing_in_hands_back_a_token_that_reaches_what_the_role_holds() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("in").await;
    asked(&db, posting("/api/setup", &setting_up())).await;

    let (status, session) = asked(
        &db,
        posting(
            "/api/sessions",
            &json!({
                "email": "SomeBody@Example.test",
                "password": "a long enough password",
            }),
        ),
    )
    .await;

    // Folded: the address they typed with a capital in it is the same address.
    assert_eq!(status, StatusCode::CREATED);

    let token = session["token"].as_str().expect("a token").to_owned();

    let (status, _) = asked(
        &db,
        holding(
            Request::builder()
                .uri("/api/writings")
                .body(Body::empty())
                .expect("a request"),
            &token,
        ),
    )
    .await;

    // The owner's role holds everything, so the same token reaches another
    // domain's listing without anything else being arranged.
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn a_token_nobody_was_given_reaches_nothing() {
    if postgres().is_none() {
        return;
    }

    let db = fresh("stranger").await;
    asked(&db, posting("/api/setup", &setting_up())).await;

    let (status, _) = asked(
        &db,
        holding(
            Request::builder()
                .uri("/api/people")
                .body(Body::empty())
                .expect("a request"),
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ),
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
