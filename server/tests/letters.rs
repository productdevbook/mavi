//! What a site's own letters say.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mavi::kernel::authz::every_grant;
use mavi::kernel::db::Db;
use mavi::kernel::http::AppState;
use mavi::kernel::mailer::{Mailer, Recorder};
use mavi::kernel::tenant::TenantId;
use tower::ServiceExt;
use uuid::Uuid;

mod common;

use common::harness;
use mavi::testing::{a_role, a_tenant, a_user};

const PASSWORD: &str = "a long enough password";

struct Site {
    db: Db,
    role: Uuid,
    router: axum::Router,
    host: String,
    tenant: TenantId,
    token: String,
    post: Recorder,
}

impl Site {
    async fn new() -> Self {
        let db = harness().await;
        let host = format!("{}.example", Uuid::now_v7().simple());
        let tenant = a_tenant(&db, &host).await;
        let role = a_role(&db, tenant, "owner", &every_grant()).await;
        let (_, email) = a_user(&db, tenant, role, PASSWORD).await;

        let mut conn = db.tenant(tenant).await.expect("begin");

        sqlx::query(
            "insert into site_settings (tenant_id, name) values ($1, 'A shop')
             on conflict (tenant_id) do update set name = 'A shop'",
        )
        .bind(tenant.0)
        .execute(conn.conn())
        .await
        .expect("a name");

        sqlx::query(
            "insert into languages (tenant_id, code, name, is_default)
             values ($1, 'tr', 'Türkçe', true)",
        )
        .bind(tenant.0)
        .execute(conn.conn())
        .await
        .expect("a language");

        conn.commit().await.expect("commit");

        let post = Recorder::default();
        let mut state = AppState::new(db.clone());
        state.mailer = std::sync::Arc::new(Mailer::Recorded(post.clone()));

        let site = Self {
            db,
            role,
            router: mavi::router(state),
            host,
            tenant,
            token: String::new(),
            post,
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

    /// What was written to somebody, once the queue has been run.
    async fn letter_to(&self, address: &str) -> (String, String) {
        let mut state = AppState::new(self.db.clone());
        state.mailer = std::sync::Arc::new(Mailer::Recorded(self.post.clone()));

        for _ in 0..8 {
            mavi::jobs::tick_within(&state, "test", Some(self.tenant))
                .await
                .expect("tick");
        }

        let letters = self.post.all();

        let letter = letters
            .iter()
            .rev()
            .find(|letter| letter.to == address)
            .unwrap_or_else(|| panic!("nothing was written to {address}"));

        (letter.subject.clone(), letter.body.clone())
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
async fn a_site_that_has_written_nothing_still_sends_something() {
    let site = Site::new().await;
    let invited = format!("new-{}@example.test", Uuid::now_v7().simple());

    let (status, _) = site
        .send(
            "POST",
            "/api/people",
            Some(serde_json::json!({
                "email": invited,
                "name": "Somebody",
                "role_id": site.role,
            })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED);

    let (subject, body) = site.letter_to(&invited).await;

    assert_eq!(subject, "You have been invited");
    assert!(body.contains("Somebody"), "{body}");
    assert!(
        body.contains("A shop"),
        "the site's own name is not in it: {body}"
    );
    assert!(body.contains("/forgotten?token="), "{body}");
}

#[tokio::test]
async fn what_a_site_writes_is_what_goes_out() {
    let site = Site::new().await;

    let (status, written) = site
        .send(
            "PUT",
            "/api/mail/letters/invitation",
            Some(serde_json::json!({
                "language": "tr",
                "subject": "{site} sizi bekliyor",
                "body": "Merhaba {name},\n\n{site} için bir hesabınız var: {link}\n",
            })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{written}");

    let invited = format!("new-{}@example.test", Uuid::now_v7().simple());

    site.send(
        "POST",
        "/api/people",
        Some(serde_json::json!({ "email": invited, "name": "Biri", "role_id": site.role })),
    )
    .await;

    let (subject, body) = site.letter_to(&invited).await;

    assert_eq!(subject, "A shop sizi bekliyor");
    assert!(body.starts_with("Merhaba Biri,"), "{body}");
}

#[tokio::test]
async fn a_wording_that_asks_for_what_nothing_fills_in_is_refused() {
    let site = Site::new().await;

    let (status, refused) = site
        .send(
            "PUT",
            "/api/mail/letters/invitation",
            Some(serde_json::json!({
                "language": "en",
                "subject": "Hello",
                "body": "Your order came to {total}",
            })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert_eq!(refused["error"]["named"]["name"], "total");
}

#[tokio::test]
async fn a_letter_nothing_sends_cannot_be_written() {
    let site = Site::new().await;

    let (status, _) = site
        .send(
            "PUT",
            "/api/mail/letters/something-nobody-sends",
            Some(serde_json::json!({
                "language": "en",
                "subject": "Hello",
                "body": "Hello",
            })),
        )
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_site_can_go_back_to_what_the_machine_says() {
    let site = Site::new().await;

    site.send(
        "PUT",
        "/api/mail/letters/invitation",
        Some(serde_json::json!({
            "language": "tr",
            "subject": "Bizimkisi",
            "body": "Merhaba {name}: {link}",
        })),
    )
    .await;

    let (status, _) = site
        .send("DELETE", "/api/mail/letters/invitation", None)
        .await;

    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, listed) = site.send("GET", "/api/mail/letters", None).await;

    let invitation = listed
        .as_array()
        .expect("a list")
        .iter()
        .find(|letter| letter["kind"] == "invitation")
        .expect("the invitation");

    assert_eq!(invitation["theirs"], false);
    assert_eq!(invitation["subject"], "You have been invited");
}

#[tokio::test]
async fn every_letter_is_listed_with_what_it_can_name() {
    let site = Site::new().await;

    let (status, listed) = site.send("GET", "/api/mail/letters", None).await;

    assert_eq!(status, StatusCode::OK);

    let kinds: Vec<&str> = listed
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|letter| letter["kind"].as_str())
        .collect();

    for wanted in ["invitation", "password", "order.paid"] {
        assert!(kinds.contains(&wanted), "{wanted} is not listed: {kinds:?}");
    }

    let order = listed
        .as_array()
        .expect("a list")
        .iter()
        .find(|letter| letter["kind"] == "order.paid")
        .expect("the receipt");

    assert!(
        order["names"]
            .as_array()
            .expect("names")
            .iter()
            .any(|name| name == "total"),
        "a receipt cannot say what it came to: {order}"
    );
}

#[tokio::test]
async fn another_site_s_wording_is_not_this_one_s() {
    let one = Site::new().await;

    one.send(
        "PUT",
        "/api/mail/letters/invitation",
        Some(serde_json::json!({
            "language": "tr",
            "subject": "Bir yerden",
            "body": "Merhaba {name}: {link}",
        })),
    )
    .await;

    let other = Site::new().await;
    let (_, listed) = other.send("GET", "/api/mail/letters", None).await;

    let invitation = listed
        .as_array()
        .expect("a list")
        .iter()
        .find(|letter| letter["kind"] == "invitation")
        .expect("the invitation");

    assert_eq!(invitation["theirs"], false);
}
