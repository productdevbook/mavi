//! Driven through the router rather than by calling handlers, because what is
//! being asked here is what a request gets — including the parts a handler
//! never sees: whose session the token is, and whether the permission on the
//! route let it through at all.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mavi::kernel::http::AppState;
use mavi::kernel::http::Audience;
use tower::ServiceExt;

mod common;

use common::harness;
use mavi::testing::{a_user, an_owner_role};

struct Site {
    router: axum::Router,
    host: String,
}

async fn a_site(password: &str) -> (Site, String) {
    let db = harness().await;
    let host = format!("{}.example", uuid::Uuid::now_v7().simple());
    let role = an_owner_role(&db).await;
    let (_, email) = a_user(&db, role, password).await;

    (
        Site {
            router: mavi::router(AppState::new(db)),
            host,
        },
        email,
    )
}

impl Site {
    async fn send(&self, request: Request<Body>) -> (StatusCode, serde_json::Value) {
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

        let body = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);

        (status, body)
    }

    fn post(&self, path: &str, body: &serde_json::Value) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(path)
            .header(header::HOST, &self.host)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .expect("a request")
    }

    fn get(&self, path: &str, token: Option<&str>) -> Request<Body> {
        let request = Request::builder()
            .method("GET")
            .uri(path)
            .header(header::HOST, &self.host);

        let request = match token {
            Some(token) => request.header(header::AUTHORIZATION, format!("Bearer {token}")),
            None => request,
        };

        request.body(Body::empty()).expect("a request")
    }
}

#[tokio::test]
async fn a_sign_in_hands_back_a_session_that_works() {
    let (site, email) = a_site("a long enough password").await;

    let (status, body) = site
        .send(site.post(
            "/api/auth/session",
            &serde_json::json!({"email": email, "password": "a long enough password"}),
        ))
        .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    let token = body["token"].as_str().expect("a token").to_owned();
    assert_eq!(body["user"]["role"], "owner");

    let (status, me) = site.send(site.get("/api/auth/me", Some(&token))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["email"], email);
}

#[tokio::test]
async fn a_wrong_password_says_the_same_as_an_unknown_address() {
    let (site, email) = a_site("a long enough password").await;

    let (wrong_password, first) = site
        .send(site.post(
            "/api/auth/session",
            &serde_json::json!({"email": email, "password": "not it"}),
        ))
        .await;

    let (unknown, second) = site
        .send(site.post(
            "/api/auth/session",
            &serde_json::json!({"email": "nobody@example.test", "password": "not it"}),
        ))
        .await;

    assert_eq!(wrong_password, unknown);
    assert_eq!(first, second, "the two answers can be told apart");
}

#[tokio::test]
async fn without_a_session_a_signed_in_route_is_refused() {
    let (site, _) = a_site("a long enough password").await;

    let (status, _) = site.send(site.get("/api/auth/me", None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = site
        .send(site.get("/api/auth/me", Some("not a token")))
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_probe_answers_on_a_machine_nobody_has_set_up() {
    let router = mavi::router(AppState::new(harness().await));

    let request = Request::builder()
        .uri("/healthz")
        .body(Body::empty())
        .expect("a request");

    let response = router.oneshot(request).await.expect("a response");
    assert_eq!(response.status(), StatusCode::OK);
}

/// What this build serves without an account, and what limits those carry.
/// Adding to this list is a change to this file, which is the point.
#[test]
fn what_is_public_is_listed() {
    let public: Vec<&str> = mavi::endpoints()
        .iter()
        .filter(|endpoint| endpoint.guard().audience == Audience::Public)
        .map(mavi::kernel::http::Endpoint::path)
        .collect();

    assert_eq!(
        public,
        vec![
            "/api/sites/beacon",
            "/api/auth/session",
            // Which providers a site trusts, and the two halves of arriving
            // through one. All three are reached before anybody is signed in,
            // which is what makes them public.
            "/api/auth/oauth",
            "/api/auth/oauth/{key}/start",
            "/api/auth/oauth/{key}/callback",
            "/api/sites/forms/{slug}/submissions",
            "/api/learn/session",
            "/api/sites/unsubscribe",
            "/uploads/{id}",
            // What a transcoder says when it has finished a video. Signed, and
            // reached on the site's own address by something not signed in.
            "/api/sites/videos/callback",
            "/api/auth/reset",
            "/api/auth/password",
            "/api/sites/products",
            "/api/sites/checkout",
            "/api/sites/payments/callback",
            "/api/sites/orders/{id}",
            "/llms.txt",
        ]
    );
}

#[test]
fn nothing_public_is_without_a_limit() {
    use mavi::kernel::http::RatePolicy;

    for endpoint in mavi::endpoints() {
        if endpoint.guard().audience == Audience::Public {
            assert!(
                matches!(endpoint.guard().rate, RatePolicy::Per(_)),
                "{} is public and takes as many requests as anybody sends",
                endpoint.path()
            );
        }
    }
}

/// The gate itself, on a route built for the purpose: a change that records
/// nothing does not get to answer.
#[tokio::test]
async fn a_change_that_records_nothing_is_refused_by_the_router() {
    use mavi::kernel::http::{Endpoint, Guard, RatePolicy};

    let db = harness().await;
    let host = format!("{}.example", uuid::Uuid::now_v7().simple());

    let router = mavi::kernel::http::mount(
        AppState::new(db),
        vec![Endpoint::post(
            "/forgot",
            Guard {
                audience: Audience::Public,
                needs: None,
                rate: RatePolicy::None,
            },
            || async { StatusCode::CREATED },
        )],
    );

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/forgot")
                .header(header::HOST, &host)
                .body(Body::empty())
                .expect("a request"),
        )
        .await
        .expect("a response");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

/// Read from the registry rather than written per endpoint, so a domain added
/// later is covered the day it is added.
#[tokio::test]
async fn nothing_behind_an_account_answers_without_one() {
    let site = a_site("a long enough password").await.0;

    for endpoint in mavi::endpoints() {
        // Signing in is the one thing that answers somebody with no account,
        // by definition — and so is setting the machine up, which exists for
        // the moment when there is nobody to have an account.
        if endpoint.guard().audience == Audience::Public
            || endpoint.path().ends_with("/session")
            || endpoint.path() == "/api/setup"
        {
            continue;
        }

        // A path parameter stands for something this caller cannot see anyway;
        // what is being asked is only whether the door is shut.
        let path = endpoint
            .path()
            .replace("{id}", &uuid::Uuid::now_v7().to_string())
            .replace("{slug}", "anything");

        let request = Request::builder()
            .method(endpoint.method().to_uppercase().as_str())
            .uri(&path)
            .header(header::HOST, &site.host)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{}"))
            .expect("a request");

        let (status, _) = site.send(request).await;

        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{} {} answered somebody with no account",
            endpoint.method(),
            endpoint.path()
        );
    }
}

#[test]
fn every_endpoint_that_changes_something_asks_for_a_grant() {
    for endpoint in mavi::endpoints() {
        let changes = matches!(endpoint.method(), "post" | "put" | "patch" | "delete");
        let guard = endpoint.guard();

        if !changes || guard.audience == Audience::Public {
            continue;
        }

        // Signing in and out are exceptions: what they change is the caller's
        // own session, and holding a grant is not what entitles somebody to
        // arrive or to leave. So is a student finishing a lesson — a student
        // holds no grants at all, and being on the course is the permission.
        // And so is the tool surface, which asks for the grant of whichever
        // tool is being called rather than one of its own.
        // Turning a second factor on or off changes the caller's own account
        // and is guarded by the password rather than by a grant: needing one
        // would mean somebody with no grants could not protect their login.
        // And setting the machine up is the one change nobody can hold a grant
        // for, because at that moment there is nobody at all.
        // Saying a screen is broken asks for nothing on purpose: what a grant
        // would gate is somebody telling the people who run the machine that
        // it is broken, and a person who cannot do that telephones instead.
        // Changing one's own password is the same kind of exception as a
        // second factor: it changes the caller's own account, it is guarded by
        // the password they already have, and needing a grant would mean
        // somebody with none could never change theirs.
        if endpoint.path() == "/api/auth/password"
            || endpoint.path() == "/api/reports"
            || endpoint.path() == "/api/setup"
            || endpoint.path().starts_with("/api/auth/second-factor")
            || endpoint.path().ends_with("/session")
            || endpoint.path() == "/mcp"
            || endpoint.guard().audience == Audience::Student
        {
            continue;
        }

        assert!(
            guard.needs.is_some(),
            "{} {} changes something and asks for nothing",
            endpoint.method(),
            endpoint.path()
        );
    }
}

#[tokio::test]
async fn every_answer_says_what_a_browser_may_do_with_it() {
    let (site, _) = a_site("a long enough password").await;

    let response = site
        .router
        .clone()
        .oneshot(site.get("/healthz", None))
        .await
        .expect("a response");

    let headers = response.headers();

    for said in [
        "content-security-policy",
        "x-content-type-options",
        "x-frame-options",
        "referrer-policy",
        "cross-origin-resource-policy",
    ] {
        assert!(headers.contains_key(said), "nothing said {said}");
    }

    assert!(
        !headers.contains_key("strict-transport-security"),
        "a plain connection was promised https, which is how somebody locks \
         themselves out of a machine with no certificate"
    );
}

#[tokio::test]
async fn a_promise_of_https_is_made_when_the_request_arrived_over_it() {
    let (site, _) = a_site("a long enough password").await;

    let request = Request::builder()
        .method("GET")
        .uri("/healthz")
        .header(header::HOST, &site.host)
        .header("x-forwarded-proto", "https")
        .body(Body::empty())
        .expect("a request");

    let response = site
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("a response");

    assert!(response.headers().contains_key("strict-transport-security"));
}

#[tokio::test]
async fn a_page_somewhere_else_cannot_change_anything_with_somebody_s_cookie() {
    let password = "a long enough password";
    let (site, email) = a_site(password).await;

    let (status, body) = site
        .send(site.post(
            "/api/auth/session",
            &serde_json::json!({ "email": email, "password": password }),
        ))
        .await;

    assert_eq!(status, StatusCode::OK, "{body}");

    let cookie = format!(
        "{}={}",
        mavi::kernel::http::USER_COOKIE,
        body["token"].as_str().expect("a token")
    );

    let elsewhere = Request::builder()
        .method("POST")
        .uri("/api/posts")
        .header(header::HOST, &site.host)
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, "https://example.invalid")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({ "language": "en", "title": "Written by somebody else" })
                .to_string(),
        ))
        .expect("a request");

    let (status, refused) = site.send(elsewhere).await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a form on another page wrote a post in somebody's name: {refused}"
    );
    assert_eq!(
        refused["error"]["key"],
        "change_asked_for_from_somewhere_else"
    );

    // The same request from the site's own page is not refused for this.
    let ours = Request::builder()
        .method("POST")
        .uri("/api/posts")
        .header(header::HOST, &site.host)
        .header(header::COOKIE, &cookie)
        .header(header::ORIGIN, format!("https://{}", site.host))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({ "language": "en", "title": "Written here" }).to_string(),
        ))
        .expect("a request");

    let (status, _) = site.send(ours).await;

    assert_ne!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_probe_that_says_ready_has_asked_the_database() {
    let (site, _) = a_site("a long enough password").await;

    let (status, _) = site.send(site.get("/readyz", None)).await;

    assert_eq!(status, StatusCode::OK);
}
