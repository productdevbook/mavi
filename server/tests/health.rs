//! Whether a site is well, and whether its addresses work.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::routing::get;
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
    db: Db,
    router: axum::Router,
    host: String,
    tenant: TenantId,
    token: String,
}

impl Site {
    async fn new() -> Self {
        Self::answering_on(&format!("{}.example", Uuid::now_v7().simple())).await
    }

    async fn answering_on(host: &str) -> Self {
        let db = harness().await;
        let tenant = a_tenant(&db, host).await;
        let role = a_role(&db, tenant, "owner", &every_grant()).await;
        let (_, email) = a_user(&db, tenant, role, PASSWORD).await;

        let site = Self {
            db: db.clone(),
            router: mavi::router(AppState::new(db)),
            host: host.to_owned(),
            tenant,
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

    let site = Site::new().await;

    // The stand-in is attached as a second address of this site, so that
    // signing in still happens on an address a site actually answers on.
    let mut conn = site.db.operator().await.expect("begin");
    conn.across_sites().await.expect("across sites");

    sqlx::query("insert into tenant_domains (host, tenant_id, is_primary) values ($1, $2, false)")
        .bind(format!("127.0.0.1:{}", address.port()))
        .bind(site.tenant.0)
        .execute(conn.conn())
        .await
        .expect("an address");

    conn.commit().await.expect("commit");

    let mut state = AppState::new(site.db.clone());
    state.allow_private_destinations = true;

    let looked = mavi::health::check_domains(&state, site.tenant)
        .await
        .expect("a look");

    assert_eq!(looked, 2);

    let (_, domains) = site.send("GET", "/api/domains", None).await;

    let stand_in = domains
        .as_array()
        .expect("a list")
        .iter()
        .find(|domain| {
            domain["host"]
                .as_str()
                .is_some_and(|host| host.starts_with("127.0.0.1"))
        })
        .expect("the address that answers");

    assert_eq!(stand_in["resolves"], true, "{domains}");
    assert_eq!(stand_in["answered"], true, "{domains}");
}

#[tokio::test]
async fn an_address_that_resolves_to_nothing_says_which_and_why() {
    // Invented, and reserved by the standard for exactly this: a name nothing
    // will ever resolve.
    let site = Site::answering_on(&format!("{}.example.invalid", Uuid::now_v7().simple())).await;

    let state = AppState::new(site.db.clone());

    mavi::health::check_domains(&state, site.tenant)
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

#[tokio::test]
async fn another_site_s_addresses_are_not_this_one_s() {
    let one = Site::new().await;
    let other = Site::new().await;

    let (_, domains) = other.send("GET", "/api/domains", None).await;

    assert_eq!(domains.as_array().expect("a list").len(), 1);
    assert_ne!(domains[0]["host"], one.host);
}

/// The address being checked is one somebody attached, so it is somewhere they
/// chose. Asking about it must not become a way to have this machine fetch
/// from inside its own network.
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

    let site = Site::new().await;
    let mut conn = site.db.operator().await.expect("begin");
    conn.across_sites().await.expect("across sites");

    sqlx::query("insert into tenant_domains (host, tenant_id, is_primary) values ($1, $2, false)")
        .bind(format!("127.0.0.1:{}", address.port()))
        .bind(site.tenant.0)
        .execute(conn.conn())
        .await
        .expect("an address");

    conn.commit().await.expect("commit");

    // The machine as it runs anywhere real: private addresses are not reached.
    let state = AppState::new(site.db.clone());

    mavi::health::check_domains(&state, site.tenant)
        .await
        .expect("a look");

    let (_, domains) = site.send("GET", "/api/domains", None).await;

    let inside = domains
        .as_array()
        .expect("a list")
        .iter()
        .find(|domain| {
            domain["host"]
                .as_str()
                .is_some_and(|host| host.starts_with("127.0.0.1"))
        })
        .expect("the address pointing inside");

    assert_eq!(
        inside["answered"], false,
        "a check reached something on this machine's own network: {domains}"
    );
}
