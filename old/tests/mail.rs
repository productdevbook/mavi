//! A campaign that gets slower the further it gets is the fault this domain
//! was rewritten around, so what is measured here is what a batch costs.

use std::collections::HashSet;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mavi::kernel::authz::every_grant;
use mavi::kernel::db::Db;
use mavi::kernel::http::AppState;
use tower::ServiceExt;
use uuid::Uuid;

mod common;

use common::{harness, percent_encoded};
use mavi::testing::{a_role, a_user};

struct Site {
    db: Db,
    router: axum::Router,
    host: String,
    token: String,
}

async fn a_site() -> Site {
    let db = harness().await;
    let host = format!("{}.example", Uuid::now_v7().simple());
    let role = a_role(&db, "owner", &every_grant()).await;
    let password = "a long enough password";
    let (_, email) = a_user(&db, role, password).await;

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

    async fn a_list(&self) -> Uuid {
        let (status, body) = self
            .send(
                "POST",
                "/api/mail/lists",
                Some(&self.token),
                Some(serde_json::json!({ "name": "Everybody" })),
            )
            .await;

        assert_eq!(status, StatusCode::CREATED, "{body}");

        body["id"].as_str().expect("an id").parse().expect("a uuid")
    }

    /// Straight into the tables, because what is being tested is the sending
    /// rather than the adding.
    async fn subscribers(&self, list: Uuid, how_many: usize) {
        let mut conn = self.db.begin().await.expect("begin");

        for _ in 0..how_many {
            let id: (Uuid,) = sqlx::query_as(
                "insert into subscribers (email, token_hash)
                 values ($1, sha256($2::bytea)) returning id",
            )
            .bind(format!("reader-{}@example.test", Uuid::now_v7().simple()))
            .bind(Uuid::now_v7().as_bytes().to_vec())
            .fetch_one(conn.conn())
            .await
            .expect("a subscriber");

            sqlx::query(
                "insert into subscriber_lists (subscriber_id, list_id)
                 values ($1, $2)",
            )
            .bind(id.0)
            .bind(list)
            .execute(conn.conn())
            .await
            .expect("on the list");
        }

        conn.commit().await.expect("commit");
    }
}

#[tokio::test]
async fn a_campaign_sends_to_a_list_and_says_when_it_is_done() {
    let site = a_site().await;
    let list = site.a_list().await;
    site.subscribers(list, 5).await;

    let (status, campaign) = site
        .send(
            "POST",
            "/api/mail/campaigns",
            Some(&site.token),
            Some(serde_json::json!({
                "list_id": list, "subject": "Hello", "body": "Something to read"
            })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{campaign}");

    let id = campaign["id"].as_str().expect("an id");

    let (status, started) = site
        .send(
            "POST",
            &format!("/api/mail/campaigns/{id}/send"),
            Some(&site.token),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{started}");

    // A batch queues the next batch and a delivery per letter, so this works
    // until there is nothing left rather than counting ticks.
    let state = AppState::new(site.db.clone());

    for _ in 0..12 {
        mavi::jobs::tick(&state, "test").await.expect("tick");
    }

    let mut conn = site.db.begin().await.expect("begin");

    let done: (String, i32) =
        sqlx::query_as("select state::text, sent_count from campaigns where id = $1")
            .bind(Uuid::parse_str(id).expect("a uuid"))
            .fetch_one(conn.conn())
            .await
            .expect("the campaign");

    assert_eq!(done, ("sent".to_owned(), 5));

    let logged: (i64,) = sqlx::query_as("select count(*) from email_log where campaign_id = $1")
        .bind(Uuid::parse_str(id).expect("a uuid"))
        .fetch_one(conn.conn())
        .await
        .expect("the log");

    assert_eq!(
        logged.0, 5,
        "what was sent has to be countable, or it is not billed"
    );
}

/// What #166 was: each batch read everything already sent to find the next few,
/// so the last batch cost more than the first. Measured rather than asserted
/// about, on two batches of the same size.
#[tokio::test]
async fn the_last_batch_costs_what_the_first_one_did() {
    let site = a_site().await;
    let list = site.a_list().await;
    site.subscribers(list, 250).await;

    let (_, campaign) = site
        .send(
            "POST",
            "/api/mail/campaigns",
            Some(&site.token),
            Some(serde_json::json!({
                "list_id": list, "subject": "Hello", "body": "Something to read"
            })),
        )
        .await;

    let id = campaign["id"].as_str().expect("an id");

    site.send(
        "POST",
        &format!("/api/mail/campaigns/{id}/send"),
        Some(&site.token),
        None,
    )
    .await;

    let state = AppState::new(site.db.clone());

    let first = {
        let (counter, _guard) = common::queries::counting();
        mavi::jobs::tick(&state, "test").await.expect("tick");
        counter.count()
    };

    let second = {
        let (counter, _guard) = common::queries::counting();
        mavi::jobs::tick(&state, "test").await.expect("tick");
        counter.count()
    };

    assert!(
        second <= first + 2,
        "the second batch cost {second} where the first cost {first}, which is \
         how a campaign comes to take all night"
    );
}

/// What #36 was: the cursor handed back was a subscriber's id, but the list
/// was ordered and filtered by when they were added. A client that kept
/// asking for `next` got the newest page again, for ever.
#[tokio::test]
async fn walking_every_page_of_subscribers_finds_each_one_once() {
    let site = a_site().await;
    let list = site.a_list().await;
    site.subscribers(list, 47).await;

    let mut seen = HashSet::new();
    let mut after: Option<String> = None;
    let mut pages = 0;

    loop {
        pages += 1;
        assert!(
            pages <= 10,
            "following `next` did not stop after {pages} pages of 47 subscribers"
        );

        let path = after.as_ref().map_or_else(
            || format!("/api/mail/lists/{list}/subscribers?limit=10"),
            |cursor| {
                format!(
                    "/api/mail/lists/{list}/subscribers?limit=10&after={}",
                    percent_encoded(cursor)
                )
            },
        );

        let (status, body) = site.send("GET", &path, Some(&site.token), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");

        for item in body["items"].as_array().expect("a page") {
            let id = item["id"].as_str().expect("an id").to_owned();
            assert!(seen.insert(id.clone()), "subscriber {id} came back twice");
        }

        after = body["next"].as_str().map(str::to_owned);

        if after.is_none() {
            break;
        }
    }

    assert_eq!(
        seen.len(),
        47,
        "not every subscriber came back exactly once"
    );
}

#[tokio::test]
async fn somebody_who_left_is_not_sent_to_again() {
    let site = a_site().await;
    let list = site.a_list().await;

    let (_, subscriber) = site
        .send(
            "POST",
            &format!("/api/mail/lists/{list}/subscribers"),
            Some(&site.token),
            Some(serde_json::json!({ "email": "reader@example.test" })),
        )
        .await;

    // The token is what a link at the bottom of a message carries; here it is
    // taken from the row, because nothing sends the message yet.
    let secret = format!("a token this test invented {}", Uuid::now_v7());

    let mut conn = site.db.begin().await.expect("begin");
    sqlx::query("update subscribers set token_hash = sha256($2::bytea) where id = $1")
        .bind(
            subscriber["id"]
                .as_str()
                .expect("an id")
                .parse::<Uuid>()
                .expect("a uuid"),
        )
        .bind(secret.as_bytes())
        .execute(conn.conn())
        .await
        .expect("a known token");
    conn.commit().await.expect("commit");

    let (status, _) = site
        .send(
            "POST",
            "/api/sites/unsubscribe",
            None,
            Some(serde_json::json!({ "token": secret })),
        )
        .await;

    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, campaign) = site
        .send(
            "POST",
            "/api/mail/campaigns",
            Some(&site.token),
            Some(serde_json::json!({
                "list_id": list, "subject": "Hello", "body": "Something"
            })),
        )
        .await;

    let id = campaign["id"].as_str().expect("an id");

    site.send(
        "POST",
        &format!("/api/mail/campaigns/{id}/send"),
        Some(&site.token),
        None,
    )
    .await;

    let state = AppState::new(site.db.clone());

    for _ in 0..2 {
        mavi::jobs::tick(&state, "test").await.expect("tick");
    }

    let mut conn = site.db.begin().await.expect("begin");

    let sent: (i64,) = sqlx::query_as("select count(*) from email_log")
        .fetch_one(conn.conn())
        .await
        .expect("the log");

    assert_eq!(sent.0, 0, "somebody who unsubscribed was sent to");
}

#[tokio::test]
async fn an_unknown_token_says_what_a_known_one_says() {
    let site = a_site().await;

    let (known, first) = site
        .send(
            "POST",
            "/api/sites/unsubscribe",
            None,
            Some(serde_json::json!({ "token": "not a token of ours" })),
        )
        .await;

    let (unknown, second) = site
        .send(
            "POST",
            "/api/sites/unsubscribe",
            None,
            Some(serde_json::json!({ "token": "nor is this one" })),
        )
        .await;

    assert_eq!(known, StatusCode::NO_CONTENT);
    assert_eq!(known, unknown);
    assert_eq!(first, second);
}

#[tokio::test]
async fn a_campaign_is_started_once() {
    let site = a_site().await;
    let list = site.a_list().await;

    let (_, campaign) = site
        .send(
            "POST",
            "/api/mail/campaigns",
            Some(&site.token),
            Some(serde_json::json!({
                "list_id": list, "subject": "Hello", "body": "Something"
            })),
        )
        .await;

    let id = campaign["id"].as_str().expect("an id");
    let path = format!("/api/mail/campaigns/{id}/send");

    let (first, _) = site.send("POST", &path, Some(&site.token), None).await;
    let (again, _) = site.send("POST", &path, Some(&site.token), None).await;

    assert_eq!(first, StatusCode::OK);
    assert_eq!(again, StatusCode::CONFLICT, "a campaign was started twice");
}

#[tokio::test]
async fn a_campaign_is_handed_over_and_says_so() {
    let site = a_site().await;
    let list = site.a_list().await;
    site.subscribers(list, 3).await;

    let (_, campaign) = site
        .send(
            "POST",
            "/api/mail/campaigns",
            Some(&site.token),
            Some(serde_json::json!({
                "list_id": list, "subject": "Hello", "body": "Something to read"
            })),
        )
        .await;

    let id = campaign["id"].as_str().expect("an id").to_owned();

    site.send(
        "POST",
        &format!("/api/mail/campaigns/{id}/send"),
        Some(&site.token),
        None,
    )
    .await;

    let post = mavi::kernel::mailer::Recorder::default();
    let mut state = AppState::new(site.db.clone());
    state.mailer = std::sync::Arc::new(mavi::kernel::mailer::Mailer::Recorded(post.clone()));

    for _ in 0..12 {
        mavi::jobs::tick(&state, "test").await.expect("tick");
    }

    let letters = post.all();

    assert_eq!(letters.len(), 3, "three on the list, three letters");
    assert!(
        letters.iter().all(|letter| letter.unsubscribe.is_some()),
        "a campaign went out with no way to leave the list"
    );

    let mut conn = site.db.begin().await.expect("begin");

    let sent: (i64,) = sqlx::query_as("select count(*) from email_log where state = 'sent'")
        .fetch_one(conn.conn())
        .await
        .expect("the log");

    assert_eq!(sent.0, 3);
}

#[tokio::test]
async fn a_bounce_stops_the_site_writing_to_them_again() {
    let site = a_site().await;
    let list = site.a_list().await;

    let (_, subscriber) = site
        .send(
            "POST",
            &format!("/api/mail/lists/{list}/subscribers"),
            Some(&site.token),
            Some(serde_json::json!({ "email": "gone-away@example.test" })),
        )
        .await;

    let (_, campaign) = site
        .send(
            "POST",
            "/api/mail/campaigns",
            Some(&site.token),
            Some(serde_json::json!({
                "list_id": list, "subject": "Hello", "body": "Something"
            })),
        )
        .await;

    let id = campaign["id"].as_str().expect("an id").to_owned();

    site.send(
        "POST",
        &format!("/api/mail/campaigns/{id}/send"),
        Some(&site.token),
        None,
    )
    .await;

    let state = AppState::new(site.db.clone());

    for _ in 0..8 {
        mavi::jobs::tick(&state, "test").await.expect("tick");
    }

    let mut conn = site.db.begin().await.expect("begin");

    let reference: (String,) =
        sqlx::query_as("select provider_ref from email_log where state = 'sent'")
            .fetch_one(conn.conn())
            .await
            .expect("something sent");

    // The provider says it bounced. Twice, because they do.
    for (nth, expected) in [(1, StatusCode::CREATED), (2, StatusCode::OK)] {
        let (status, body) = site
            .send(
                "POST",
                "/api/mail/events",
                Some(&site.token),
                Some(serde_json::json!({
                    "provider_ref": reference.0,
                    "kind": "bounced",
                    "detail": "no such address"
                })),
            )
            .await;

        assert_eq!(status, expected, "the {nth} time: {body}");
    }

    let mut conn = site.db.begin().await.expect("begin");

    let state_of: (String,) = sqlx::query_as("select state::text from subscribers where id = $1")
        .bind(
            subscriber["id"]
                .as_str()
                .expect("an id")
                .parse::<Uuid>()
                .expect("a uuid"),
        )
        .fetch_one(conn.conn())
        .await
        .expect("the subscriber");

    assert_eq!(state_of.0, "bounced");

    let events: (i64,) = sqlx::query_as("select count(*) from mail_events")
        .fetch_one(conn.conn())
        .await
        .expect("the events");

    assert_eq!(events.0, 1, "the same event twice was recorded twice");
}
