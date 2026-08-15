//! A board opens in the same number of queries whether it holds three cards or
//! two hundred, which is the only thing about a kanban that can go wrong
//! quietly.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mavi::kernel::authz::every_grant;
use mavi::kernel::http::AppState;
use tower::ServiceExt;
use uuid::Uuid;

mod common;

use common::{a_role, a_tenant, a_user, harness};

struct Site {
    router: axum::Router,
    host: String,
    token: String,
}

async fn a_site() -> Site {
    let db = harness().await;
    let host = format!("{}.example", Uuid::now_v7().simple());
    let tenant = a_tenant(&db, &host).await;
    let role = a_role(&db, tenant, "owner", &every_grant()).await;
    let password = "a long enough password";
    let (_, email) = a_user(&db, tenant, role, password).await;

    let router = mavi::router(AppState::new(db));

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

    Site {
        router,
        host,
        token: body["token"].as_str().expect("a token").to_owned(),
    }
}

impl Site {
    async fn send(
        &self,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, &self.host)
            .header(header::AUTHORIZATION, format!("Bearer {}", self.token));

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
}

#[tokio::test]
async fn a_board_arrives_with_somewhere_to_put_things() {
    let site = a_site().await;

    let (status, board) = site
        .send(
            "POST",
            "/api/boards",
            Some(serde_json::json!({ "name": "Sales" })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{board}");

    let id = board["id"].as_str().expect("an id");
    let (status, full) = site.send("GET", &format!("/api/boards/{id}"), None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        full["stages"].as_array().expect("stages").len(),
        3,
        "a board with no columns is a board nothing goes on"
    );
}

#[tokio::test]
async fn a_card_moves_between_columns() {
    let site = a_site().await;

    let (_, board) = site
        .send(
            "POST",
            "/api/boards",
            Some(serde_json::json!({ "name": "Sales", "stages": ["New", "Won"] })),
        )
        .await;

    let id = board["id"].as_str().expect("an id");
    let (_, full) = site.send("GET", &format!("/api/boards/{id}"), None).await;

    let new = full["stages"][0]["id"]
        .as_str()
        .expect("a stage")
        .to_owned();
    let won = full["stages"][1]["id"]
        .as_str()
        .expect("a stage")
        .to_owned();

    let (status, card) = site
        .send(
            "POST",
            &format!("/api/boards/{id}/cards"),
            Some(serde_json::json!({
                "stage_id": new, "title": "A Deal",
                "value_minor": 250_000, "currency": "TRY"
            })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{card}");
    assert_eq!(card["value"]["minor"], 250_000);

    let card_id = card["id"].as_str().expect("an id");

    let (status, moved) = site
        .send(
            "PATCH",
            &format!("/api/cards/{card_id}"),
            Some(serde_json::json!({ "stage_id": won, "position": 1.5 })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{moved}");
    assert_eq!(moved["stage_id"], won);
}

#[tokio::test]
async fn an_amount_arrives_with_its_currency_or_not_at_all() {
    let site = a_site().await;

    let (_, board) = site
        .send(
            "POST",
            "/api/boards",
            Some(serde_json::json!({ "name": "Sales" })),
        )
        .await;

    let id = board["id"].as_str().expect("an id");
    let (_, full) = site.send("GET", &format!("/api/boards/{id}"), None).await;
    let stage = full["stages"][0]["id"]
        .as_str()
        .expect("a stage")
        .to_owned();

    let (status, _) = site
        .send(
            "POST",
            &format!("/api/boards/{id}/cards"),
            Some(serde_json::json!({
                "stage_id": stage, "title": "A Deal", "value_minor": 100
            })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a_board_costs_the_same_however_many_cards_are_on_it() {
    let site = a_site().await;

    let (_, board) = site
        .send(
            "POST",
            "/api/boards",
            Some(serde_json::json!({ "name": "Sales" })),
        )
        .await;

    let id = board["id"].as_str().expect("an id");
    let (_, full) = site.send("GET", &format!("/api/boards/{id}"), None).await;
    let stage = full["stages"][0]["id"]
        .as_str()
        .expect("a stage")
        .to_owned();

    // Warmed first: a pool that opens a connection mid-measurement runs its
    // own `set role` on it, and that is not what is being counted.
    site.send("GET", &format!("/api/boards/{id}"), None).await;

    let small = {
        let (counter, _guard) = common::queries::counting();
        site.send("GET", &format!("/api/boards/{id}"), None).await;
        counter.count()
    };

    for n in 0..30 {
        site.send(
            "POST",
            &format!("/api/boards/{id}/cards"),
            Some(serde_json::json!({
                "stage_id": stage, "title": format!("Deal {n}")
            })),
        )
        .await;
    }

    site.send("GET", &format!("/api/boards/{id}"), None).await;

    let large = {
        let (counter, _guard) = common::queries::counting();
        let (_, full) = site.send("GET", &format!("/api/boards/{id}"), None).await;

        assert_eq!(
            full["stages"][0]["cards"].as_array().expect("cards").len(),
            30
        );

        counter.count()
    };

    // Not equality: a pool under load runs a query of its own now and then,
    // and what is being asked is whether the cost grew with the cards — thirty
    // more rows would be thirty more queries if it did.
    assert!(
        large <= small + 2,
        "a board with thirty cards cost {large} where three cost {small}"
    );
}

#[tokio::test]
async fn a_note_needs_a_card_that_is_there() {
    let site = a_site().await;

    let (status, _) = site
        .send(
            "POST",
            &format!("/api/cards/{}/notes", Uuid::now_v7()),
            Some(serde_json::json!({ "body": "Called them" })),
        )
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}
