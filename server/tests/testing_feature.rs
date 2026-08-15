//! Plays the part of a crate outside this one: builds a database and a
//! signed-in user through nothing but `mavi::testing` and the rest of this
//! crate's public interface — no `pub(crate)`, no path into `tests/common`
//! — and proves a request carrying that token is admitted.
#![cfg(feature = "testing")]

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mavi::kernel::authz::every_grant;
use mavi::kernel::http::AppState;
use tower::ServiceExt;
use uuid::Uuid;

const PASSWORD: &str = "a long enough password";

#[tokio::test]
async fn a_user_built_through_the_testing_module_can_sign_in() {
    let db = mavi::testing::harness().await;
    let host = format!("{}.example", Uuid::now_v7().simple());

    let tenant = mavi::testing::a_tenant(&db, &host).await;
    let role = mavi::testing::a_role(&db, tenant, "owner", &every_grant()).await;
    let (_, email) = mavi::testing::a_user(&db, tenant, role, PASSWORD).await;

    let router = mavi::router(AppState::new(db));

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/session")
                .header(header::HOST, &host)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({ "email": email, "password": PASSWORD }).to_string(),
                ))
                .expect("a request"),
        )
        .await
        .expect("a response");

    assert_eq!(response.status(), StatusCode::OK);

    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("a body")
        .to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&bytes).expect("json");

    assert!(
        body["token"]
            .as_str()
            .is_some_and(|token| !token.is_empty()),
        "signing in through the public endpoint gave no token: {body}"
    );
}

#[tokio::test]
async fn an_owner_role_built_through_the_testing_module_is_admitted_to_a_guarded_endpoint() {
    let db = mavi::testing::harness().await;
    let host = format!("{}.example", Uuid::now_v7().simple());

    let tenant = mavi::testing::a_tenant(&db, &host).await;
    let role = mavi::testing::an_owner_role(&db, tenant).await;
    let (_, email) = mavi::testing::a_user(&db, tenant, role, PASSWORD).await;

    let router = mavi::router(AppState::new(db));

    let signed_in = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/session")
                .header(header::HOST, &host)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({ "email": email, "password": PASSWORD }).to_string(),
                ))
                .expect("a request"),
        )
        .await
        .expect("a response");

    assert_eq!(signed_in.status(), StatusCode::OK);

    let bytes = signed_in
        .into_body()
        .collect()
        .await
        .expect("a body")
        .to_bytes();
    let token = serde_json::from_slice::<serde_json::Value>(&bytes).expect("json")["token"]
        .as_str()
        .expect("a token")
        .to_owned();

    // An owner's own account, reached through a built-in endpoint that
    // requires being signed in — nothing about the guard itself belongs to
    // this test, only that a token minted through `mavi::testing` clears it.
    let admitted = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/auth/me")
                .header(header::HOST, &host)
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .expect("a request"),
        )
        .await
        .expect("a response");

    assert_eq!(admitted.status(), StatusCode::OK);
}
