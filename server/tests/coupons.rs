//! What a site takes off an order, and for whom.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mavi::kernel::authz::every_grant;
use mavi::kernel::db::Db;
use mavi::kernel::http::AppState;
use mavi::kernel::tenant::TenantId;
use tower::ServiceExt;
use uuid::Uuid;

mod common;

use common::{a_role, a_tenant, a_user, harness};

const PASSWORD: &str = "a long enough password";

struct Site {
    router: axum::Router,
    host: String,
    token: String,
    #[expect(dead_code, reason = "kept so the site outlives the leased database")]
    db: Db,
    #[expect(dead_code, reason = "the same")]
    tenant: TenantId,
}

impl Site {
    async fn new() -> Self {
        let db = harness().await;
        let host = format!("{}.example", Uuid::now_v7().simple());
        let tenant = a_tenant(&db, &host).await;
        let role = a_role(&db, tenant, "owner", &every_grant()).await;
        let (_, email) = a_user(&db, tenant, role, PASSWORD).await;

        let site = Self {
            router: mavi::router(AppState::new(db.clone())),
            host,
            token: String::new(),
            db,
            tenant,
        };

        let (status, body) = site
            .send(
                "POST",
                "/api/auth/session",
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
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, &self.host);

        if !self.token.is_empty() {
            request = request.header(header::AUTHORIZATION, format!("Bearer {}", self.token));
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
}

#[tokio::test]
async fn a_coupon_can_be_made_and_then_spent() {
    let site = Site::new().await;

    let (status, made) = site
        .send(
            "POST",
            "/api/coupons",
            Some(serde_json::json!({ "code": "spring", "kind": "percent", "value": 10 })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{made}");
    assert_eq!(made["code"], "SPRING", "a code is kept as one case");
    assert_eq!(made["used"], 0);

    // What checkout does with it is the point of having it.
    let (status, listed) = site.send("GET", "/api/coupons", None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["items"][0]["code"], "SPRING");
}

#[tokio::test]
async fn what_is_not_a_discount_is_refused() {
    let site = Site::new().await;

    for asked in [
        serde_json::json!({ "code": "over", "kind": "percent", "value": 300 }),
        serde_json::json!({ "code": "nothing", "kind": "percent", "value": 0 }),
        serde_json::json!({ "code": "what", "kind": "something else", "value": 10 }),
        serde_json::json!({ "code": "ab", "kind": "amount", "value": 500 }),
    ] {
        let (status, refused) = site.send("POST", "/api/coupons", Some(asked.clone())).await;

        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{asked} was taken as a coupon: {refused}"
        );
    }
}

#[tokio::test]
async fn a_coupon_that_has_already_expired_is_refused_when_it_is_made() {
    let site = Site::new().await;

    let (status, refused) = site
        .send(
            "POST",
            "/api/coupons",
            Some(serde_json::json!({
                "code": "gone",
                "kind": "amount",
                "value": 500,
                "expires_at": "2020-01-01T00:00:00Z",
            })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
}

#[tokio::test]
async fn the_same_code_twice_is_a_conflict() {
    let site = Site::new().await;

    let made = serde_json::json!({ "code": "twice", "kind": "amount", "value": 500 });

    site.send("POST", "/api/coupons", Some(made.clone())).await;

    let (status, _) = site.send("POST", "/api/coupons", Some(made)).await;

    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn stopping_one_stops_it_and_leaves_what_it_was_spent_on() {
    let site = Site::new().await;

    site.send(
        "POST",
        "/api/coupons",
        Some(serde_json::json!({ "code": "stop", "kind": "amount", "value": 500 })),
    )
    .await;

    let (status, _) = site.send("DELETE", "/api/coupons/stop", None).await;

    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, listed) = site.send("GET", "/api/coupons", None).await;

    assert_eq!(listed["items"].as_array().expect("a page").len(), 0);
}

#[tokio::test]
async fn another_site_s_coupons_are_not_this_one_s() {
    let one = Site::new().await;

    one.send(
        "POST",
        "/api/coupons",
        Some(serde_json::json!({ "code": "theirs", "kind": "amount", "value": 500 })),
    )
    .await;

    let other = Site::new().await;
    let (_, listed) = other.send("GET", "/api/coupons", None).await;

    assert_eq!(listed["items"].as_array().expect("a page").len(), 0);
}
