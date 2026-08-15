//! Somebody saying a screen is broken, and the answer coming back.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mavi::kernel::authz::every_grant;
use mavi::kernel::http::AppState;
use tower::ServiceExt;
use uuid::Uuid;

mod common;

use common::harness;
use mavi::testing::{a_role, a_tenant, a_user};

const PASSWORD: &str = "a long enough password";

struct Site {
    router: axum::Router,
    host: String,
    token: String,
}

impl Site {
    async fn new() -> Self {
        Self::where_somebody_can(&every_grant()).await
    }

    /// A site whose account holds exactly these grants — for asking whether
    /// saying something is gated on any of them.
    async fn where_somebody_can(grants: &[String]) -> Self {
        let db = harness().await;
        let host = format!("{}.example", Uuid::now_v7().simple());
        let tenant = a_tenant(&db, &host).await;
        let role = a_role(&db, tenant, "somebody", grants).await;
        let (_, email) = a_user(&db, tenant, role, PASSWORD).await;

        let site = Self {
            router: mavi::router(AppState::new(db)),
            host,
            token: String::new(),
        };

        let (status, body) = site
            .send(
                "POST",
                "/api/auth/session",
                None,
                Some(serde_json::json!({ "email": email, "password": PASSWORD })),
            )
            .await;

        assert_eq!(status, StatusCode::OK, "{body}");

        Self {
            token: body["token"].as_str().expect("a token").to_owned(),
            ..site
        }
    }

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

        if let Some(token) = token
            .or(Some(&self.token))
            .filter(|token| !token.is_empty())
        {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }

        let request = match body {
            Some(body) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        }
        .expect("a request");

        answered(&self.router, request).await
    }
}

async fn answered(
    router: &axum::Router,
    request: Request<Body>,
) -> (StatusCode, serde_json::Value) {
    let response = router.clone().oneshot(request).await.expect("a response");
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

#[tokio::test]
async fn saying_something_does_not_need_a_grant() {
    // Somebody who may do nothing at all: what a grant would gate here is
    // telling the people who run the machine that something is broken.
    let site = Site::where_somebody_can(&[]).await;

    let (status, said) = site
        .send(
            "POST",
            "/api/reports",
            None,
            Some(serde_json::json!({
                "screen": "/admin/posts",
                "body": "The list is empty and there are forty posts",
            })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{said}");
    assert_eq!(said["state"], "said");
}

#[tokio::test]
async fn nothing_said_is_not_a_report() {
    let site = Site::new().await;

    let (status, _) = site
        .send(
            "POST",
            "/api/reports",
            None,
            Some(serde_json::json!({ "body": "   " })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn what_a_site_said_it_can_see() {
    let site = Site::new().await;

    site.send(
        "POST",
        "/api/reports",
        None,
        Some(serde_json::json!({ "body": "Something is wrong" })),
    )
    .await;

    let (status, ours) = site.send("GET", "/api/reports", None, None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(ours["items"][0]["body"], "Something is wrong");
}

#[tokio::test]
async fn another_site_s_report_is_not_this_one_s() {
    let one = Site::new().await;
    one.send(
        "POST",
        "/api/reports",
        None,
        Some(serde_json::json!({ "body": "One site said this" })),
    )
    .await;

    let other = Site::new().await;
    let (_, ours) = other.send("GET", "/api/reports", None, None).await;

    assert_eq!(
        ours["items"].as_array().expect("a page").len(),
        0,
        "a site saw what another said"
    );
}

#[tokio::test]
async fn a_report_carries_what_makes_it_actionable() {
    let site = Site::new().await;

    let (status, said) = site
        .send(
            "POST",
            "/api/reports",
            None,
            Some(serde_json::json!({
                "kind": "missing",
                "screen": "/dashboard/orders",
                "body": "There is no way to refund half of an order.",
                "environment": { "browser": "Something 1.2", "width": 390 },
            })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{said}");
    assert_eq!(said["kind"], "missing");
    assert_eq!(said["environment"]["width"], 390, "{said}");
}

#[tokio::test]
async fn something_that_is_not_a_kind_of_report_is_refused() {
    let site = Site::new().await;

    let (status, refused) = site
        .send(
            "POST",
            "/api/reports",
            None,
            Some(serde_json::json!({ "kind": "whatever", "body": "Something" })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
}
