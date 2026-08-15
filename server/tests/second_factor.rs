//! The second thing somebody has, and what happens when they lose it.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mavi::kernel::authz::every_grant;
use mavi::kernel::db::Db;
use mavi::kernel::http::AppState;
use mavi::kernel::tenant::TenantId;
use mavi::kernel::totp;
use tower::ServiceExt;
use uuid::Uuid;

mod common;

use common::harness;
use mavi::testing::{a_role, a_tenant, a_user};

const PASSWORD: &str = "a long enough password";

/// The next code the app will show, rather than the one on screen now.
///
/// Confirming spends the code it was confirmed with, so the one on screen at
/// that moment is already used up. A person waits half a minute; a test asks
/// for the step after, which is inside the drift a phone is allowed.
fn next_code(secret: &[u8]) -> String {
    totp::code_for(secret, totp::step_at(chrono::Utc::now()) + 1)
}

struct Site {
    router: axum::Router,
    host: String,
    email: String,
    token: String,
    #[expect(dead_code, reason = "kept so the site outlives the leased database")]
    db: Db,
    #[expect(dead_code, reason = "the same")]
    tenant: TenantId,
}

impl Site {
    async fn new() -> Self {
        let db = harness().await;
        let host = format!("{}.example", Uuid::now_v7().simple());
        let tenant = a_tenant(&db, &host).await;
        let role = a_role(&db, tenant, "owner", &every_grant()).await;
        let (_, email) = a_user(&db, tenant, role, PASSWORD).await;

        let router = mavi::router(AppState::new(db.clone()));

        let site = Self {
            router,
            host,
            email,
            token: String::new(),
            db,
            tenant,
        };

        let (status, body) = site.sign_in(None).await;
        assert_eq!(status, StatusCode::OK, "{body}");

        Self {
            token: body["token"].as_str().expect("a token").to_owned(),
            ..site
        }
    }

    async fn sign_in(&self, code: Option<&str>) -> (StatusCode, serde_json::Value) {
        let mut body = serde_json::json!({ "email": self.email, "password": PASSWORD });

        if let Some(code) = code {
            body["code"] = serde_json::Value::String(code.to_owned());
        }

        self.send("POST", "/api/auth/session", None, Some(body))
            .await
    }

    /// Turns it on the way a person would: begin, scan, type in what the app
    /// is showing. What comes back is the codes for when the phone is gone.
    async fn turn_it_on(&self) -> (Vec<u8>, Vec<String>) {
        let (status, begun) = self
            .send("POST", "/api/auth/second-factor", Some(&self.token), None)
            .await;

        assert_eq!(status, StatusCode::OK, "{begun}");

        let secret =
            totp::from_base32(begun["secret"].as_str().expect("a secret")).expect("base32");
        let code = totp::code_for(&secret, totp::step_at(chrono::Utc::now()));

        let (status, confirmed) = self
            .send(
                "POST",
                "/api/auth/second-factor/confirm",
                Some(&self.token),
                Some(serde_json::json!({ "code": code })),
            )
            .await;

        assert_eq!(status, StatusCode::OK, "{confirmed}");

        let codes = confirmed["codes"]
            .as_array()
            .expect("recovery codes")
            .iter()
            .map(|code| code.as_str().expect("a code").to_owned())
            .collect();

        (secret, codes)
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
async fn the_password_alone_stops_being_enough() {
    let site = Site::new().await;
    let (secret, _) = site.turn_it_on().await;

    let (status, refused) = site.sign_in(None).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED, "{refused}");
    assert_eq!(
        refused["error"]["code"], "second_factor_required",
        "the panel has to be able to tell this from a wrong password: {refused}"
    );

    let code = next_code(&secret);
    let (status, body) = site.sign_in(Some(&code)).await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["token"].is_string());
}

#[tokio::test]
async fn digits_read_over_a_shoulder_are_no_use_twice() {
    let site = Site::new().await;
    let (secret, _) = site.turn_it_on().await;

    let code = next_code(&secret);

    let (status, _) = site.sign_in(Some(&code)).await;
    assert_eq!(status, StatusCode::OK);

    let (status, refused) = site.sign_in(Some(&code)).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "the same six digits got in twice: {refused}"
    );
}

#[tokio::test]
async fn the_wrong_digits_are_not_a_way_in() {
    let site = Site::new().await;
    site.turn_it_on().await;

    let (status, _) = site.sign_in(Some("000000")).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_recovery_code_gets_somebody_back_in_once() {
    let site = Site::new().await;
    let (_, codes) = site.turn_it_on().await;

    assert_eq!(codes.len(), 10);

    let (status, body) = site.sign_in(Some(&codes[0])).await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let (status, refused) = site.sign_in(Some(&codes[0])).await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a recovery code was spent twice: {refused}"
    );

    let token = body["token"].as_str().expect("a token").to_owned();
    let (_, standing) = site
        .send("GET", "/api/auth/second-factor", Some(&token), None)
        .await;

    assert_eq!(standing["enabled"], true);
    assert_eq!(standing["recovery_codes_left"], 9);
}

#[tokio::test]
async fn taking_it_off_asks_for_the_password_again() {
    let site = Site::new().await;
    let (secret, _) = site.turn_it_on().await;

    let code = next_code(&secret);
    let (_, body) = site.sign_in(Some(&code)).await;
    let token = body["token"].as_str().expect("a token").to_owned();

    let (status, _) = site
        .send(
            "DELETE",
            "/api/auth/second-factor",
            Some(&token),
            Some(serde_json::json!({ "password": "not the password" })),
        )
        .await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a borrowed session took the second factor off"
    );

    let (status, _) = site
        .send(
            "DELETE",
            "/api/auth/second-factor",
            Some(&token),
            Some(serde_json::json!({ "password": PASSWORD })),
        )
        .await;

    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, body) = site.sign_in(None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn nothing_says_whether_an_account_has_one_before_the_password_is_right() {
    let site = Site::new().await;
    site.turn_it_on().await;

    let (status, body) = site
        .send(
            "POST",
            "/api/auth/session",
            None,
            Some(serde_json::json!({
                "email": site.email,
                "password": "not the password",
            })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_ne!(
        body["error"]["code"], "second_factor_required",
        "a wrong password told somebody the account is worth attacking"
    );
}

#[tokio::test]
async fn the_issuer_is_the_sites_own_name_not_the_word_site() {
    let site = Site::new().await;

    let (status, _) = site
        .send(
            "PATCH",
            "/api/site",
            Some(&site.token),
            Some(serde_json::json!({ "name": "Café Örnek: A Test Site" })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, begun) = site
        .send("POST", "/api/auth/second-factor", Some(&site.token), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{begun}");

    let uri = begun["uri"].as_str().expect("a uri");

    // The colon, the space and the non-ASCII letters are all significant in
    // an otpauth:// URI, so a name carrying them has to survive as bytes an
    // authenticator can still parse, not as a broken issuer.
    assert!(
        uri.contains("issuer=Caf%C3%A9%20%C3%96rnek%3A%20A%20Test%20Site"),
        "the site's own name was not carried into the issuer, colon and all: {uri}"
    );
}

#[tokio::test]
async fn a_site_with_no_name_yet_still_gets_an_issuer() {
    let site = Site::new().await;

    let (status, begun) = site
        .send("POST", "/api/auth/second-factor", Some(&site.token), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{begun}");

    let uri = begun["uri"].as_str().expect("a uri");

    assert!(
        !uri.ends_with("issuer="),
        "an empty issuer is worse than a wrong one: {uri}"
    );
    assert!(
        uri.contains("issuer=A%20site"),
        "an unnamed site should still say something rather than nothing: {uri}"
    );
}

#[tokio::test]
async fn a_second_authenticator_does_not_quietly_replace_the_first() {
    let site = Site::new().await;
    let (secret, _) = site.turn_it_on().await;

    let code = next_code(&secret);
    let (_, body) = site.sign_in(Some(&code)).await;
    let token = body["token"].as_str().expect("a token").to_owned();

    let (status, _) = site
        .send("POST", "/api/auth/second-factor", Some(&token), None)
        .await;

    assert_eq!(status, StatusCode::CONFLICT);
}
