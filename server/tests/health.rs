//! Whether a site is well, and whether its addresses work.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::routing::get;
use http_body_util::BodyExt;
use mavi::kernel::Address;
use mavi::kernel::authz::every_grant;
use mavi::kernel::http::AppState;
use tower::ServiceExt;
use uuid::Uuid;

mod common;

use common::harness;
use mavi::testing::{a_role, a_user};

const PASSWORD: &str = "a long enough password";

struct Site {
    state: AppState,
    router: axum::Router,
    host: String,
    token: String,
}

impl Site {
    async fn new() -> Self {
        Self::answering_on(&format!("{}.example", Uuid::now_v7().simple())).await
    }

    /// The address is the installation's own — what it was started with —
    /// rather than a row somebody attached, so a test that wants to be asked
    /// about a particular address says so here.
    async fn answering_on(host: &str) -> Self {
        let db = harness().await;
        let role = a_role(&db, "owner", &every_grant()).await;
        let (_, email) = a_user(&db, role, PASSWORD).await;

        let mut state = AppState::new(db);
        state.address =
            std::sync::Arc::new(Address::read(&format!("http://{host}")).expect("an address"));

        let site = Self {
            router: mavi::router(state.clone()),
            state,
            host: host.to_owned(),
            token: String::new(),
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
async fn a_site_with_nothing_on_it_says_so() {
    let site = Site::new().await;

    let (status, health) = site.send("GET", "/api/health", None).await;

    assert_eq!(status, StatusCode::OK, "{health}");
    assert_eq!(health["well"], false, "{health}");

    let pages = health["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .find(|check| check["what"] == "site.has-pages")
        .expect("whether it has any pages");

    assert_eq!(pages["well"], false);
    assert_eq!(pages["detail"]["published"], 0);
}

#[tokio::test]
async fn an_address_nothing_has_looked_at_says_nothing_rather_than_broken() {
    let site = Site::new().await;

    let (status, domains) = site.send("GET", "/api/domains", None).await;

    assert_eq!(status, StatusCode::OK, "{domains}");
    assert_eq!(domains[0]["host"], site.host);
    assert!(
        domains[0]["answered"].is_null(),
        "an address nobody has checked was called broken: {domains}"
    );
}

#[tokio::test]
async fn an_address_that_answers_is_written_down_as_answering() {
    // A stand-in for this machine, on an address that resolves: what the check
    // asks for is /healthz, and what it wants back is a success.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a socket");
    let address = listener.local_addr().expect("an address");

    tokio::spawn(async move {
        let app = Router::new().route("/healthz", get(|| async { "ok" }));
        let _ = axum::serve(listener, app).await;
    });

    let site = Site::answering_on(&format!("127.0.0.1:{}", address.port())).await;

    let mut state = site.state.clone();
    state.allow_private_destinations = true;

    let looked = mavi::health::check_domains(&state).await.expect("a look");

    assert_eq!(looked, 1);

    let (_, domains) = site.send("GET", "/api/domains", None).await;

    assert_eq!(domains[0]["resolves"], true, "{domains}");
    assert_eq!(domains[0]["answered"], true, "{domains}");
}

#[tokio::test]
async fn an_address_that_resolves_to_nothing_says_which_and_why() {
    // Invented, and reserved by the standard for exactly this: a name nothing
    // will ever resolve.
    let site = Site::answering_on(&format!("{}.example.invalid", Uuid::now_v7().simple())).await;

    mavi::health::check_domains(&site.state)
        .await
        .expect("a look");

    let (_, domains) = site.send("GET", "/api/domains", None).await;

    assert_eq!(domains[0]["answered"], false, "{domains}");
    assert!(
        domains[0]["note"].is_string(),
        "nothing said why it does not work: {domains}"
    );

    let (_, health) = site.send("GET", "/api/health", None).await;

    let addresses = health["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .find(|check| check["what"] == "domains.answering")
        .expect("whether its addresses answer");

    assert_eq!(addresses["well"], false);
    assert_eq!(addresses["detail"]["not_answering"], 1);
}

/// The address being checked is whatever the installation was started with, so
/// it is somewhere somebody chose rather than something this machine worked
/// out. Asking about it must not become a way to have the machine fetch from
/// inside its own network.
#[tokio::test]
async fn checking_an_address_is_not_a_way_to_reach_inside_this_machine() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a socket");
    let address = listener.local_addr().expect("an address");

    tokio::spawn(async move {
        let app = Router::new().route("/healthz", get(|| async { "ok" }));
        let _ = axum::serve(listener, app).await;
    });

    let site = Site::answering_on(&format!("127.0.0.1:{}", address.port())).await;

    // The machine as it runs anywhere real: private addresses are not reached.
    mavi::health::check_domains(&site.state)
        .await
        .expect("a look");

    let (_, domains) = site.send("GET", "/api/domains", None).await;

    assert_eq!(
        domains[0]["answered"], false,
        "a check reached something on this machine's own network: {domains}"
    );
}
