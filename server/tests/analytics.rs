//! Counting what a site was asked for without keeping who asked.

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

struct Site {
    db: Db,
    router: axum::Router,
    host: String,
    tenant: TenantId,
    token: String,
}

fn a_peer(last: u8) -> std::net::SocketAddr {
    use std::net::{IpAddr, Ipv6Addr};

    let bits = Uuid::now_v7().as_u128() & !0xff | u128::from(last);

    std::net::SocketAddr::new(IpAddr::V6(Ipv6Addr::from_bits(bits)), 443)
}

async fn a_site() -> Site {
    let db = harness().await;
    let host = format!("{}.example", Uuid::now_v7().simple());
    let tenant = a_tenant(&db, &host).await;
    let role = a_role(&db, tenant, "owner", &every_grant()).await;
    let password = "a long enough password";
    let (_, email) = a_user(&db, tenant, role, password).await;

    let router = mavi::router(AppState::new(db.clone()));

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
        db,
        router,
        host,
        tenant,
        token: body["token"].as_str().expect("a token").to_owned(),
    }
}

impl Site {
    async fn read(&self, path: &str, from: std::net::SocketAddr) -> StatusCode {
        let request = Request::builder()
            .method("POST")
            .uri("/api/sites/beacon")
            .extension(axum::extract::ConnectInfo(from))
            .header(header::HOST, &self.host)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                serde_json::json!({ "path": path, "lcp": 1200 }).to_string(),
            ))
            .expect("a request");

        self.router
            .clone()
            .oneshot(request)
            .await
            .expect("a response")
            .status()
    }

    async fn summary(&self) -> serde_json::Value {
        let response = self
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/analytics")
                    .header(header::HOST, &self.host)
                    .header(header::AUTHORIZATION, format!("Bearer {}", self.token))
                    .body(Body::empty())
                    .expect("a request"),
            )
            .await
            .expect("a response");

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("a body")
            .to_bytes();

        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    }
}

#[tokio::test]
async fn two_readings_by_one_person_are_two_views_and_one_visitor() {
    let site = a_site().await;
    let somebody = a_peer(1);
    let somebody_else = a_peer(2);

    assert_eq!(site.read("/", somebody).await, StatusCode::NO_CONTENT);
    assert_eq!(site.read("/about", somebody).await, StatusCode::NO_CONTENT);
    assert_eq!(site.read("/", somebody_else).await, StatusCode::NO_CONTENT);

    let summary = site.summary().await;
    let today = &summary["days"][0];

    assert_eq!(today["views"], 3);
    assert_eq!(today["visitors"], 2, "one person was counted twice");
}

#[tokio::test]
async fn nothing_kept_says_who_anybody_was() {
    let site = a_site().await;
    site.read("/", a_peer(7)).await;

    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

    // The columns are the day, the path and a hash. There is nowhere for an
    // address to be, which is the point.
    let columns: Vec<(String,)> = sqlx::query_as(
        "select column_name from information_schema.columns
          where table_name in ('page_views', 'visitor_marks', 'vitals')",
    )
    .fetch_all(conn.conn())
    .await
    .expect("columns");

    let named: Vec<String> = columns.into_iter().map(|(name,)| name).collect();

    for forbidden in ["ip", "from_ip", "address", "user_agent", "email"] {
        assert!(
            !named.contains(&forbidden.to_owned()),
            "a visitor's {forbidden} is being kept"
        );
    }
}

#[tokio::test]
async fn what_a_browser_measured_is_kept_beside_the_count() {
    let site = a_site().await;
    site.read("/", a_peer(3)).await;

    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

    let measured: (i64, i32) =
        sqlx::query_as("select count(*), max(value) from vitals where kind = 'lcp'")
            .fetch_one(conn.conn())
            .await
            .expect("a measurement");

    assert_eq!(measured, (1, 1200));
}

#[tokio::test]
async fn a_measurement_that_is_not_one_is_left_out() {
    let site = a_site().await;

    let request = Request::builder()
        .method("POST")
        .uri("/api/sites/beacon")
        .extension(axum::extract::ConnectInfo(a_peer(4)))
        .header(header::HOST, &site.host)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({ "path": "/", "lcp": -1, "inp": 900_000 }).to_string(),
        ))
        .expect("a request");

    let status = site
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("a response")
        .status();

    assert_eq!(status, StatusCode::NO_CONTENT);

    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

    let kept: (i64,) = sqlx::query_as("select count(*) from vitals")
        .fetch_one(conn.conn())
        .await
        .expect("a count");

    assert_eq!(
        kept.0, 0,
        "a measurement nothing could have measured was kept"
    );
}

#[tokio::test]
async fn yesterday_s_marks_go_and_the_counts_stay() {
    let site = a_site().await;
    site.read("/", a_peer(5)).await;

    let mut conn = site.db.tenant(site.tenant).await.expect("begin");
    sqlx::query("update visitor_marks set on_day = current_date - 5")
        .execute(conn.conn())
        .await
        .expect("walk back");
    conn.commit().await.expect("commit");

    let state = AppState::new(site.db.clone());
    let taken = mavi::analytics::sweep_marks(&state, site.tenant)
        .await
        .expect("sweep");

    assert_eq!(taken, 1);

    let summary = site.summary().await;
    assert_eq!(
        summary["days"][0]["views"], 1,
        "the count went with the mark"
    );
}

impl Site {
    async fn vitals(&self) -> serde_json::Value {
        let response = self
            .router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/analytics/vitals")
                    .header(header::HOST, &self.host)
                    .header(header::AUTHORIZATION, format!("Bearer {}", self.token))
                    .body(Body::empty())
                    .expect("a request"),
            )
            .await
            .expect("a response");

        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("a body")
            .to_bytes();

        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
    }
}

#[tokio::test]
async fn what_a_browser_measured_can_be_read_back_and_judged() {
    let site = a_site().await;

    for peer in 10..16 {
        site.read("/slow", a_peer(peer)).await;
    }

    let read = site.vitals().await;

    let lcp = read["scores"]
        .as_array()
        .expect("a list")
        .iter()
        .find(|score| score["kind"] == "lcp")
        .expect("what was measured");

    assert_eq!(lcp["p75"], 1200);
    assert_eq!(
        lcp["verdict"], "good",
        "1.2 seconds was called something other than good: {read}"
    );
    assert_eq!(read["worst"][0]["path"], "/slow", "{read}");
    assert_eq!(read["worst"][0]["lcp"], 1200, "{read}");
}

#[tokio::test]
async fn a_site_nobody_has_read_says_so_rather_than_nothing() {
    let site = a_site().await;

    let read = site.vitals().await;

    assert_eq!(read["scores"].as_array().expect("a list").len(), 0);
    assert_eq!(read["worst"].as_array().expect("a list").len(), 0);
    assert_eq!(read["days"], 30);
}

#[tokio::test]
async fn what_a_site_adds_up_to_is_one_question() {
    let site = a_site().await;

    site.read("/", a_peer(20)).await;

    let response = site
        .router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/overview")
                .header(header::HOST, &site.host)
                .header(header::AUTHORIZATION, format!("Bearer {}", site.token))
                .body(Body::empty())
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

    let read: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);

    assert_eq!(read["days"], 30, "{read}");
    assert_eq!(read["totals"]["posts"], 0, "{read}");
    assert_eq!(read["visitors"][0]["views"], 1, "{read}");
    assert_eq!(read["mail"]["sent"], 0, "{read}");
}
