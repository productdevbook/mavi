//! What a site plugs into, and what it gets when it has plugged in nothing.

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

use common::harness;
use mavi::testing::{a_role, a_tenant, a_user};

const PASSWORD: &str = "a long enough password";

/// An address nothing answers on. Only ever handed to a check that is meant to
/// fail, and never connected to.
const NOWHERE: &str = "smtp://user:pass@127.0.0.1:1/";

struct Site {
    db: Db,
    router: axum::Router,
    host: String,
    tenant: TenantId,
    token: String,
}

impl Site {
    async fn new() -> Self {
        let db = harness().await;
        let host = format!("{}.example", Uuid::now_v7().simple());
        let tenant = a_tenant(&db, &host).await;
        let role = a_role(&db, tenant, "owner", &every_grant()).await;
        let (_, email) = a_user(&db, tenant, role, PASSWORD).await;

        let site = Self {
            db: db.clone(),
            router: mavi::router(AppState::new(db)),
            host,
            tenant,
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

        if let Some(token) = token {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
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
async fn what_a_site_can_plug_into_is_listed_before_anything_is_plugged_in() {
    let site = Site::new().await;

    let (status, listed) = site
        .send("GET", "/api/plugins", Some(&site.token), None)
        .await;

    assert_eq!(status, StatusCode::OK, "{listed}");

    let keys: Vec<&str> = listed
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|plugin| plugin["key"].as_str())
        .collect();

    assert!(keys.contains(&"mail"));
    assert!(keys.contains(&"payments"));
    assert_eq!(listed[0]["configured"], false);
}

#[tokio::test]
async fn a_secret_that_was_plugged_in_never_comes_back_out() {
    let site = Site::new().await;

    let (status, plugged) = site
        .send(
            "PUT",
            "/api/plugins/mail",
            Some(&site.token),
            Some(serde_json::json!({
                "settings": { "url": NOWHERE, "from": "post@example.test" },
            })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{plugged}");
    assert_eq!(plugged["settings"]["from"], "post@example.test");
    assert_eq!(plugged["holds"], serde_json::json!(["url"]));

    let (_, listed) = site
        .send("GET", "/api/plugins", Some(&site.token), None)
        .await;

    assert!(
        !listed.to_string().contains("pass@"),
        "a screen was handed the password to a site's mail server: {listed}"
    );
}

#[tokio::test]
async fn changing_one_setting_does_not_mean_typing_the_password_again() {
    let site = Site::new().await;

    site.send(
        "PUT",
        "/api/plugins/mail",
        Some(&site.token),
        Some(serde_json::json!({
            "settings": { "url": NOWHERE, "from": "post@example.test" },
        })),
    )
    .await;

    let (status, plugged) = site
        .send(
            "PUT",
            "/api/plugins/mail",
            Some(&site.token),
            Some(serde_json::json!({ "settings": { "from": "hello@example.test" } })),
        )
        .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "changing the address it sends from asked for the password again: {plugged}"
    );
    assert_eq!(plugged["settings"]["from"], "hello@example.test");
}

#[tokio::test]
async fn an_integration_nobody_declared_cannot_be_configured() {
    let site = Site::new().await;

    let (status, _) = site
        .send(
            "PUT",
            "/api/plugins/anything-at-all",
            Some(&site.token),
            Some(serde_json::json!({ "settings": { "url": "…" } })),
        )
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_setting_an_integration_never_asked_for_is_refused() {
    let site = Site::new().await;

    let (status, refused) = site
        .send(
            "PUT",
            "/api/plugins/mail",
            Some(&site.token),
            Some(serde_json::json!({
                "settings": { "url": NOWHERE, "from": "post@example.test", "smuggled": "x" },
            })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
}

#[tokio::test]
async fn half_an_integration_is_not_switched_on() {
    let site = Site::new().await;

    let (status, refused) = site
        .send(
            "PUT",
            "/api/plugins/payments",
            Some(&site.token),
            Some(serde_json::json!({ "settings": { "name": "somebody" } })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
}

#[tokio::test]
async fn a_mail_server_that_does_not_answer_is_said_to_not_answer() {
    let site = Site::new().await;

    site.send(
        "PUT",
        "/api/plugins/mail",
        Some(&site.token),
        Some(serde_json::json!({
            "settings": { "url": NOWHERE, "from": "post@example.test" },
        })),
    )
    .await;

    let (status, checked) = site
        .send("POST", "/api/plugins/mail/check", Some(&site.token), None)
        .await;

    assert_eq!(status, StatusCode::OK, "{checked}");
    assert_eq!(
        checked["working"], false,
        "a mail server nothing is listening on was called working"
    );
    assert!(checked["note"].is_string());
}

#[tokio::test]
async fn what_a_site_plugged_in_is_not_another_site_s() {
    let one = Site::new().await;
    one.send(
        "PUT",
        "/api/plugins/mail",
        Some(&one.token),
        Some(serde_json::json!({
            "settings": { "url": NOWHERE, "from": "post@example.test" },
        })),
    )
    .await;

    let other = Site::new().await;
    let (_, listed) = other
        .send("GET", "/api/plugins", Some(&other.token), None)
        .await;

    assert_eq!(
        listed[0]["configured"], false,
        "one site's mail server showed up on another's: {listed}"
    );
}

#[tokio::test]
async fn a_site_that_plugged_in_nothing_still_sends() {
    let site = Site::new().await;

    let mailer = mavi::plugins::mailer_for(&AppState::new(site.db.clone()), site.tenant)
        .await
        .expect("a mailer");

    assert!(
        matches!(mailer, mavi::kernel::mailer::Mailer::Recorded(_)),
        "a site that configured nothing was left with nothing to send with"
    );
}
