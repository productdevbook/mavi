//! Signing in with an account somebody already has somewhere else.
//!
//! The provider is a few lines of axum in this file: what is being tested is
//! what this machine does with the answers, not anybody else's login screen.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode, header};
use axum::routing::{get, post};
use axum::{Json, Router};
use http_body_util::BodyExt;
use mavi::kernel::authz::every_grant;
use mavi::kernel::db::Db;
use mavi::kernel::http::AppState;
use tower::ServiceExt;
use uuid::Uuid;

mod common;

use common::harness;
use mavi::testing::{a_role, a_user};

const PASSWORD: &str = "a long enough password";

#[derive(Clone)]
struct Elsewhere {
    /// The address this provider will say the account belongs to.
    email: Arc<String>,
    verified: Arc<std::sync::Mutex<Option<bool>>>,
    exchanges: Arc<AtomicUsize>,
    /// What the last exchange was asked with, so the test can look for the
    /// PKCE verifier rather than assume it was sent.
    asked: Arc<std::sync::Mutex<String>>,
}

async fn token(State(provider): State<Elsewhere>, body: String) -> Json<serde_json::Value> {
    provider.exchanges.fetch_add(1, Ordering::SeqCst);
    *provider.asked.lock().expect("a lock") = body;

    Json(serde_json::json!({ "access_token": "a token this test invented" }))
}

async fn profile(State(provider): State<Elsewhere>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "email": provider.email.as_str(),
        "email_verified": *provider.verified.lock().expect("a lock"),
    }))
}

async fn a_provider(email: &str) -> (String, Elsewhere) {
    let provider = Elsewhere {
        email: Arc::new(email.to_owned()),
        verified: Arc::new(std::sync::Mutex::new(Some(true))),
        exchanges: Arc::new(AtomicUsize::new(0)),
        asked: Arc::new(std::sync::Mutex::new(String::new())),
    };

    let app = Router::new()
        .route("/token", post(token))
        .route("/profile", get(profile))
        .with_state(provider.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a socket");
    let address = listener.local_addr().expect("an address");

    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    (format!("http://{address}"), provider)
}

struct Site {
    router: axum::Router,
    host: String,
    email: String,
    token: String,
    #[expect(dead_code, reason = "kept so the site outlives the leased database")]
    db: Db,
}

impl Site {
    async fn new() -> Self {
        let db = harness().await;
        let host = format!("{}.example", Uuid::now_v7().simple());
        let role = a_role(&db, "owner", &every_grant()).await;
        let (_, email) = a_user(&db, role, PASSWORD).await;

        let mut state = AppState::new(db.clone());
        state.allow_private_destinations = true;

        let site = Self {
            router: mavi::router(state),
            host,
            email,
            token: String::new(),
            db,
        };

        let (status, body) = site
            .send(
                "POST",
                "/api/auth/session",
                None,
                Some(serde_json::json!({ "email": site.email, "password": PASSWORD })),
            )
            .await;

        assert_eq!(status, StatusCode::OK, "{body}");

        Self {
            token: body["token"].as_str().expect("a token").to_owned(),
            ..site
        }
    }

    async fn trusts(&self, at: &str) {
        let (status, body) = self
            .send(
                "PUT",
                "/api/auth/oauth/elsewhere",
                Some(&self.token),
                Some(serde_json::json!({
                    "label": "Elsewhere",
                    "client_id": "a client this test invented",
                    "client_secret": "a secret this test invented",
                    "authorize_url": format!("{at}/authorize"),
                    "token_url": format!("{at}/token"),
                    "profile_url": format!("{at}/profile"),
                })),
            )
            .await;

        assert_eq!(status, StatusCode::OK, "{body}");
    }

    async fn leaves(&self, redirect: Option<&str>) -> (StatusCode, serde_json::Value) {
        let mut body = serde_json::json!({ "redirect_uri": "https://example.test/back" });

        if let Some(redirect) = redirect {
            body["redirect"] = serde_json::Value::String(redirect.to_owned());
        }

        self.send("POST", "/api/auth/oauth/elsewhere/start", None, Some(body))
            .await
    }

    async fn comes_back(&self, state: &str) -> (StatusCode, serde_json::Value) {
        self.send(
            "POST",
            "/api/auth/oauth/elsewhere/callback",
            None,
            Some(serde_json::json!({
                "code": "a code the provider handed over",
                "state": state,
                "redirect_uri": "https://example.test/back",
            })),
        )
        .await
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
async fn somebody_already_invited_arrives_through_a_provider() {
    let site = Site::new().await;
    let (at, provider) = a_provider(&site.email).await;
    site.trusts(&at).await;

    let (status, sent) = site.leaves(Some("/admin/posts")).await;
    assert_eq!(status, StatusCode::OK, "{sent}");

    let url = sent["url"].as_str().expect("somewhere to go");
    assert!(url.starts_with(&format!("{at}/authorize?")), "{url}");
    assert!(
        url.contains("code_challenge_method=S256"),
        "nothing tied the answer to whoever asked: {url}"
    );

    let (status, arrived) = site
        .comes_back(sent["state"].as_str().expect("a state"))
        .await;

    assert_eq!(status, StatusCode::OK, "{arrived}");
    assert!(arrived["token"].is_string());
    assert_eq!(arrived["user"]["email"], site.email);
    assert_eq!(arrived["redirect"], "/admin/posts");
    assert_eq!(provider.exchanges.load(Ordering::SeqCst), 1);

    let asked = provider.asked.lock().expect("a lock").clone();
    assert!(
        asked.contains("code_verifier="),
        "the code was exchanged without the proof of who asked for it: {asked}"
    );

    let (status, me) = site
        .send("GET", "/api/auth/me", arrived["token"].as_str(), None)
        .await;

    assert_eq!(status, StatusCode::OK, "{me}");
}

#[tokio::test]
async fn an_answer_nobody_asked_for_is_not_a_way_in() {
    let site = Site::new().await;
    let (at, _) = a_provider(&site.email).await;
    site.trusts(&at).await;

    let (status, refused) = site.comes_back("a state this test made up").await;

    assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
}

#[tokio::test]
async fn the_same_answer_does_not_arrive_twice() {
    let site = Site::new().await;
    let (at, _) = a_provider(&site.email).await;
    site.trusts(&at).await;

    let (_, sent) = site.leaves(None).await;
    let state = sent["state"].as_str().expect("a state").to_owned();

    let (status, _) = site.comes_back(&state).await;
    assert_eq!(status, StatusCode::OK);

    let (status, refused) = site.comes_back(&state).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "an answer was replayed into a second session: {refused}"
    );
}

#[tokio::test]
async fn an_address_nobody_invited_is_not_let_in() {
    let site = Site::new().await;
    let (at, _) = a_provider("a-stranger@example.invalid").await;
    site.trusts(&at).await;

    let (_, sent) = site.leaves(None).await;
    let (status, refused) = site
        .comes_back(sent["state"].as_str().expect("a state"))
        .await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "owning a mailbox was enough to get into a site: {refused}"
    );
}

#[tokio::test]
async fn an_address_the_provider_has_not_verified_is_not_believed() {
    let site = Site::new().await;
    let (at, provider) = a_provider(&site.email).await;
    site.trusts(&at).await;

    *provider.verified.lock().expect("a lock") = Some(false);

    let (_, sent) = site.leaves(None).await;
    let (status, refused) = site
        .comes_back(sent["state"].as_str().expect("a state"))
        .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
}

#[tokio::test]
async fn nobody_is_sent_anywhere_but_back_into_this_site() {
    let site = Site::new().await;
    let (at, _) = a_provider(&site.email).await;
    site.trusts(&at).await;

    let (status, refused) = site.leaves(Some("https://example.invalid/take-over")).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
}

#[tokio::test]
async fn the_secret_a_site_configured_never_comes_back_out() {
    let site = Site::new().await;
    let (at, _) = a_provider(&site.email).await;
    site.trusts(&at).await;

    let (status, offered) = site.send("GET", "/api/auth/oauth", None, None).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(offered[0]["key"], "elsewhere");
    assert!(
        !offered.to_string().contains("a secret this test invented"),
        "a sign-in screen was told the client secret: {offered}"
    );
}

/// Nothing is configured, so there is nowhere to send anybody. Said as not
/// found rather than by sending them to a provider this site never named.
#[tokio::test]
async fn a_provider_nobody_configured_is_not_one_to_leave_through() {
    let site = Site::new().await;

    let (status, _) = site.leaves(None).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}
