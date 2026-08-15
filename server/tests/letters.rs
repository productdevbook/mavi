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
    is_a_url_somebody_can_click(&body, "/forgotten?token=");
}

/// Read the way somebody reading the letter would read it: find the word that
/// is meant to be the link and ask whether it is a whole URL. Asserting the
/// path alone is what held the bare-path shape in place — `/forgotten?token=`
/// is in the body either way, and only one of the two is clickable.
fn is_a_url_somebody_can_click(body: &str, ending_in: &str) {
    let link = body
        .split_whitespace()
        .find(|word| word.contains(ending_in))
        .unwrap_or_else(|| panic!("nothing in it looks like a link: {body}"));

    let (scheme, rest) = link
        .split_once("://")
        .unwrap_or_else(|| panic!("a bare path is not a link: {link}"));

    assert!(
        matches!(scheme, "http" | "https"),
        "a mail client will not follow {scheme}: {link}"
    );

    let (host, path) = rest
        .split_once('/')
        .unwrap_or_else(|| panic!("a link with no path on it: {link}"));

    assert!(!host.is_empty(), "a link with no host in it: {link}");
    assert!(path.contains(ending_in.trim_start_matches('/')), "{link}");
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

/// Somebody whose address changed is told to prove the new one, not told they
/// have been invited: asserted by which kind was pressed — the subject line a
/// site never wrote its own words for is still the machine's default one for
/// `email_proof`, not `invitation`'s — rather than by reading the body.
#[tokio::test]
async fn a_changed_address_is_told_to_prove_itself_not_invited_again() {
    let site = Site::new().await;
    let invited = format!("first-{}@example.test", Uuid::now_v7().simple());

    let (status, made) = site
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

    assert_eq!(status, StatusCode::CREATED, "{made}");
    let id = made["id"].as_str().expect("an id");

    let changed = format!("second-{}@example.test", Uuid::now_v7().simple());

    let (status, _) = site
        .send(
            "PATCH",
            &format!("/api/people/{id}"),
            Some(serde_json::json!({ "email": changed })),
        )
        .await;

    assert_eq!(status, StatusCode::OK);

    let (subject, _) = site.letter_to(&changed).await;

    assert_eq!(subject, "Confirm your new address");
    assert_ne!(subject, "You have been invited");
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

/// A kind a site can word and nothing sends is a letter that can be
/// translated, previewed, and never once goes out. Read the same way
/// `tests/messages.rs` reads the source for an English string written where a
/// refusal belongs: every call this crate makes to `letters::press` names its
/// kind as a literal, so the whole list of what is actually pressed can be
/// read back out of `src` and held against `KINDS`.
#[test]
fn a_kind_a_site_can_word_is_a_kind_something_presses() {
    use mavi::mail::letters::{KINDS, NEVER_PRESSED};

    let pressed = kinds_pressed_in(std::path::Path::new("src"));

    for kind in KINDS {
        if let Some((_, reason)) = NEVER_PRESSED.iter().find(|(name, _)| *name == kind.name) {
            assert!(
                !reason.is_empty(),
                "{} is exempt from being pressed with no reason given",
                kind.name
            );
            continue;
        }

        assert!(
            pressed.iter().any(|name| name == kind.name),
            "{} can be written and worded and nothing presses it — either \
             send it somewhere or list it in NEVER_PRESSED with why not",
            kind.name
        );
    }

    for (name, _) in NEVER_PRESSED {
        assert!(
            KINDS.iter().any(|kind| kind.name == *name),
            "{name} is exempt from being pressed and is not a letter kind at all"
        );
    }
}

/// Every literal kind named at a `letters::press(` call anywhere under `at`.
fn kinds_pressed_in(at: &std::path::Path) -> Vec<String> {
    let mut found = Vec::new();

    for entry in std::fs::read_dir(at).expect("a directory") {
        let path = entry.expect("an entry").path();

        if path.is_dir() {
            found.extend(kinds_pressed_in(&path));
            continue;
        }

        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }

        let source = std::fs::read_to_string(&path).expect("a source file");
        let mut left = source.as_str();

        while let Some(found) = left.find("letters::press(") {
            let after = &left[found + "letters::press(".len()..];

            if let Some(kind) = after.split('"').nth(1) {
                found.push(kind.to_owned());
            }

            left = after;
        }
    }

    found
}
