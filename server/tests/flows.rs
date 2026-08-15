//! An event a site cares about, and what it set up to happen next.

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

    async fn work(&self, state: &AppState, times: usize) {
        for _ in 0..times {
            mavi::jobs::tick_within(state, "test", Some(self.tenant))
                .await
                .expect("tick");
        }
    }
}

#[tokio::test]
async fn a_form_filled_in_sets_off_what_the_site_arranged() {
    let site = a_site().await;

    let (status, flow) = site
        .send(
            "POST",
            "/api/flows",
            Some(&site.token),
            Some(serde_json::json!({
                "name": "Say thank you",
                "trigger": "form.submitted",
                "steps": [
                    { "kind": "send_mail", "config": { "to": "somebody@example.test",
                                                       "subject": "Thank you" } }
                ]
            })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{flow}");

    site.send(
        "POST",
        "/api/forms",
        Some(&site.token),
        Some(serde_json::json!({ "slug": "contact", "name": "Get in touch" })),
    )
    .await;

    site.send(
        "POST",
        "/api/sites/forms/contact/submissions",
        None,
        Some(serde_json::json!({ "answers": { "note": "hello" } })),
    )
    .await;

    let state = AppState::new(site.db.clone());

    // Dispatching the event, starting the run, running its one step, and
    // finding there is no next one.
    site.work(&state, 8).await;

    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

    let run: (String, i32) = sqlx::query_as("select state::text, at_step from flow_runs")
        .fetch_one(conn.conn())
        .await
        .expect("a run");

    assert_eq!(run.0, "done");
    assert_eq!(run.1, 1);

    let sent: (i64,) = sqlx::query_as("select count(*) from email_log where subject = 'Thank you'")
        .fetch_one(conn.conn())
        .await
        .expect("the log");

    assert_eq!(sent.0, 1, "the step did not do what it said");

    let steps: (String,) = sqlx::query_as("select outcome from flow_run_steps order by position")
        .fetch_one(conn.conn())
        .await
        .expect("a step");

    assert_eq!(steps.0, "went on");
}

#[tokio::test]
async fn a_flow_waiting_for_something_nothing_emits_is_refused() {
    let site = a_site().await;

    let (status, _) = site
        .send(
            "POST",
            "/api/flows",
            Some(&site.token),
            Some(serde_json::json!({
                "name": "Never", "trigger": "the.moon.rises", "steps": []
            })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn a_step_that_waits_does_not_hold_a_worker() {
    let site = a_site().await;

    site.send(
        "POST",
        "/api/flows",
        Some(&site.token),
        Some(serde_json::json!({
            "name": "Later",
            "trigger": "form.submitted",
            "steps": [
                { "kind": "wait", "config": { "minutes": 60 } },
                { "kind": "send_mail", "config": { "to": "somebody@example.test" } }
            ]
        })),
    )
    .await;

    site.send(
        "POST",
        "/api/forms",
        Some(&site.token),
        Some(serde_json::json!({ "slug": "contact", "name": "Get in touch" })),
    )
    .await;

    site.send(
        "POST",
        "/api/sites/forms/contact/submissions",
        None,
        Some(serde_json::json!({ "answers": { "note": "hello" } })),
    )
    .await;

    let state = AppState::new(site.db.clone());
    site.work(&state, 8).await;

    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

    let run: (String, i32) = sqlx::query_as("select state::text, at_step from flow_runs")
        .fetch_one(conn.conn())
        .await
        .expect("a run");

    assert_eq!(run.0, "running", "a wait finished the run");
    assert_eq!(run.1, 1, "the wait did not move it on");

    // And nothing was sent, because the hour has not passed.
    let sent: (i64,) = sqlx::query_as("select count(*) from email_log")
        .fetch_one(conn.conn())
        .await
        .expect("the log");

    assert_eq!(sent.0, 0);

    let waiting: (String,) = sqlx::query_as(
        "select state::text from jobs where kind = 'flow.step' and state = 'ready'
          order by run_at desc limit 1",
    )
    .fetch_one(conn.conn())
    .await
    .expect("the next step, waiting");

    assert_eq!(waiting.0, "ready");
}

#[tokio::test]
async fn a_step_that_fails_stops_the_run_and_says_why() {
    let site = a_site().await;

    site.send(
        "POST",
        "/api/flows",
        Some(&site.token),
        Some(serde_json::json!({
            "name": "Broken",
            "trigger": "form.submitted",
            "steps": [{ "kind": "send_mail", "config": {} }]
        })),
    )
    .await;

    site.send(
        "POST",
        "/api/forms",
        Some(&site.token),
        Some(serde_json::json!({ "slug": "contact", "name": "Get in touch" })),
    )
    .await;

    site.send(
        "POST",
        "/api/sites/forms/contact/submissions",
        None,
        Some(serde_json::json!({ "answers": { "note": "hello" } })),
    )
    .await;

    let state = AppState::new(site.db.clone());
    site.work(&state, 8).await;

    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

    let run: (String, Option<String>) =
        sqlx::query_as("select state::text, failure from flow_runs")
            .fetch_one(conn.conn())
            .await
            .expect("a run");

    assert_eq!(run.0, "failed");
    assert!(
        run.1.is_some_and(|why| why.contains("nobody to write to")),
        "a failed run has to say what went wrong"
    );
}

#[tokio::test]
async fn a_run_says_which_step_it_stopped_at() {
    let site = a_site().await;

    let (_, flow) = site
        .send(
            "POST",
            "/api/flows",
            Some(&site.token),
            Some(serde_json::json!({
                "name": "Broken",
                "trigger": "form.submitted",
                "steps": [{ "kind": "send_mail", "config": {} }]
            })),
        )
        .await;

    site.send(
        "POST",
        "/api/forms",
        Some(&site.token),
        Some(serde_json::json!({ "slug": "contact", "name": "Get in touch" })),
    )
    .await;

    site.send(
        "POST",
        "/api/sites/forms/contact/submissions",
        None,
        Some(serde_json::json!({ "answers": { "note": "hello" } })),
    )
    .await;

    let state = AppState::new(site.db.clone());
    site.work(&state, 8).await;

    let id = flow["id"].as_str().expect("an id");
    let (status, runs) = site
        .send(
            "GET",
            &format!("/api/flows/{id}/runs"),
            Some(&site.token),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{runs}");

    let run = runs["items"][0]["id"].as_str().expect("a run");
    let (status, whole) = site
        .send(
            "GET",
            &format!("/api/flows/runs/{run}"),
            Some(&site.token),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{whole}");
    assert_eq!(whole["run"]["state"], "failed");
    assert_eq!(whole["steps"][0]["outcome"], "failed");
    assert_eq!(whole["steps"][0]["kind"], "send_mail");
    assert!(
        whole["steps"][0]["detail"]["why"]
            .as_str()
            .is_some_and(|why| why.contains("nobody to write to")),
        "the step that failed has to say what it said: {whole}"
    );
}

#[tokio::test]
async fn a_credential_is_never_handed_back() {
    let site = a_site().await;

    let (status, _) = site
        .send(
            "POST",
            "/api/flows/credentials",
            Some(&site.token),
            Some(serde_json::json!({ "name": "A provider", "secret": "the key itself" })),
        )
        .await;

    assert_eq!(status, StatusCode::NO_CONTENT);

    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

    let stored: (String,) = sqlx::query_as("select sealed from flow_credentials")
        .fetch_one(conn.conn())
        .await
        .expect("a credential");

    assert!(
        !stored.0.contains("the key itself"),
        "a credential is sitting there in plain text"
    );

    // And nothing anybody signed in can ask for reads it back.
    let public: Vec<&str> = mavi::endpoints()
        .iter()
        .map(mavi::kernel::http::Endpoint::path)
        .filter(|path| path.contains("credential"))
        .collect();

    // Two doors on the name and one on a single credential: a list of names, a
    // way to write one, and a way to take one away. None of them answers with
    // a secret, which is what this is checking for.
    assert_eq!(
        public,
        vec![
            "/api/flows/credentials",
            "/api/flows/credentials",
            "/api/flows/credentials/{name}"
        ]
    );
}

#[tokio::test]
async fn a_flow_can_be_read_changed_and_taken_away() {
    let site = a_site().await;

    let (status, made) = site
        .send(
            "POST",
            "/api/flows",
            Some(&site.token),
            Some(serde_json::json!({
                "name": "Say thank you",
                "trigger": "form.submitted",
                "steps": [{ "kind": "wait", "config": { "minutes": 5 } }],
            })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{made}");

    let id = made["id"].as_str().expect("an id").to_owned();

    let (status, whole) = site
        .send("GET", &format!("/api/flows/{id}"), Some(&site.token), None)
        .await;

    assert_eq!(status, StatusCode::OK, "{whole}");
    assert_eq!(whole["steps"].as_array().expect("steps").len(), 1);

    // The steps are written whole: two now, and the old one is gone rather
    // than left underneath.
    let (status, changed) = site
        .send(
            "PATCH",
            &format!("/api/flows/{id}"),
            Some(&site.token),
            Some(serde_json::json!({
                "active": false,
                "steps": [
                    { "kind": "wait", "config": { "minutes": 10 } },
                    { "kind": "send_mail", "config": { "to": "somebody@example.test" } },
                ],
            })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["steps"].as_array().expect("steps").len(), 2);
    assert_eq!(changed["flow"]["active"], false);

    let (status, _) = site
        .send(
            "DELETE",
            &format!("/api/flows/{id}"),
            Some(&site.token),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = site
        .send("GET", &format!("/api/flows/{id}"), Some(&site.token), None)
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}
