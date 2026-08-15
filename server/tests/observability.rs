//! What this machine says about itself.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mavi::kernel::domain;
use mavi::kernel::http::AppState;
use tower::ServiceExt;
use uuid::Uuid;

mod common;

use common::harness;
use mavi::testing::a_tenant;

#[test]
fn every_endpoint_belongs_to_a_domain() {
    for endpoint in mavi::endpoints() {
        assert!(
            domain::known(endpoint.domain()),
            "{} {} belongs to {:?}, which is not a domain",
            endpoint.method(),
            endpoint.path(),
            endpoint.domain()
        );
    }
}

/// A domain in the list with nothing behind it is a name somebody meant to use
/// and did not — except for the ones that are only ever work, which say so here.
#[test]
fn every_domain_has_something_behind_it() {
    const ONLY_WORK: [&str; 1] = ["webhooks"];

    let served: Vec<&str> = mavi::endpoints()
        .iter()
        .map(mavi::kernel::http::Endpoint::domain)
        .collect();

    for named in domain::DOMAINS {
        assert!(
            served.contains(named) || ONLY_WORK.contains(named),
            "{named} is a domain with nothing behind it"
        );
    }
}

#[tokio::test]
async fn what_a_domain_answered_is_counted_under_the_domain() {
    let db = harness().await;
    let host = format!("{}.example", Uuid::now_v7().simple());
    a_tenant(&db, &host).await;

    let router = mavi::router(AppState::new(db));

    // Reached without an account: what is counted is that the domain answered,
    // not that it said yes.
    let asked = Request::builder()
        .method("GET")
        .uri("/api/posts")
        .header(header::HOST, &host)
        .body(Body::empty())
        .expect("a request");

    let response = router.clone().oneshot(asked).await.expect("a response");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let numbers = Request::builder()
        .method("GET")
        .uri("/metrics")
        .header(header::HOST, &host)
        .body(Body::empty())
        .expect("a request");

    let response = router.oneshot(numbers).await.expect("a response");
    let body = response
        .into_body()
        .collect()
        .await
        .expect("a body")
        .to_bytes();

    let published = String::from_utf8_lossy(&body);

    assert!(
        published.contains("domain_requests_total") && published.contains("domain=\"content\""),
        "nothing counted what the content domain answered"
    );
}

#[test]
fn what_a_domain_says_happened_is_counted_under_the_domain() {
    // The counter is by name, so an event whose subject nothing recognises
    // still lands somewhere a dashboard can read rather than nowhere.
    assert_eq!(domain::of_event("order.paid"), "shop");
    assert_eq!(domain::of_event("post.published"), "content");
    assert!(domain::known(domain::of_event("nothing.like.this")));
}
