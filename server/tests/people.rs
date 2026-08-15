//! Who is on a site, what they may do, and how somebody arrives.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mavi::kernel::authz::{Access, Capability, Needs, every_grant};
use mavi::kernel::db::Db;
use mavi::kernel::http::AppState;
use mavi::kernel::mailer::{Mailer, Recorder};
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
    me: Uuid,
    token: String,
    owner_role: Uuid,
    /// Where mail went. Nothing leaves a test, so what would have been sent is
    /// read back from here — which is also how a token reaches this test now
    /// that no response carries one.
    post: Recorder,
}

async fn a_site_with(grants: &[String]) -> Site {
    let db = harness().await;
    let host = format!("{}.example", Uuid::now_v7().simple());
    let tenant = a_tenant(&db, &host).await;
    let owner_role = a_role(&db, tenant, "owner", grants).await;
    let password = "a long enough password";
    let (me, email) = a_user(&db, tenant, owner_role, password).await;

    let post = Recorder::default();
    let mut state = AppState::new(db.clone());
    state.mailer = std::sync::Arc::new(Mailer::Recorded(post.clone()));

    let router = mavi::router(state);

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
        me,
        token: body["token"].as_str().expect("a token").to_owned(),
        owner_role,
        post,
    }
}

async fn a_site() -> Site {
    a_site_with(&every_grant()).await
}

impl Site {
    /// The link out of whatever was last written to somebody. What a person
    /// would click, read the way they would read it.
    async fn ticket_for(&self, address: &str) -> String {
        // The delivery is a job; running it is what puts the letter in front
        // of the recorder.
        let state = AppState::new(self.db.clone());
        let mut state = state;
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

        letter
            .body
            .split("token=")
            .nth(1)
            .expect("a link with a token in it")
            .trim()
            .to_owned()
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
async fn somebody_invited_chooses_a_password_and_is_in() {
    let site = a_site().await;
    let email = format!("new-{}@example.test", Uuid::now_v7().simple());

    let (status, invited) = site
        .send(
            "POST",
            "/api/people",
            Some(&site.token),
            Some(serde_json::json!({
                "email": email, "name": "A New Person", "role_id": site.owner_role
            })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{invited}");

    let ticket = site.ticket_for(&email).await;

    // Until it is spent, there is no password and no way in.
    let (status, _) = site
        .send(
            "POST",
            "/api/auth/session",
            None,
            Some(serde_json::json!({ "email": email, "password": "anything at all" })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    let (status, body) = site
        .send(
            "POST",
            "/api/auth/password",
            None,
            Some(serde_json::json!({ "token": ticket, "password": "a long enough password" })),
        )
        .await;

    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");

    let (status, session) = site
        .send(
            "POST",
            "/api/auth/session",
            None,
            Some(serde_json::json!({ "email": email, "password": "a long enough password" })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{session}");

    // Spending the ticket is what proved the address, so they arrive proved.
    let (_, me) = site
        .send(
            "GET",
            "/api/people",
            Some(session["token"].as_str().expect("a token")),
            None,
        )
        .await;

    let theirs = me["items"]
        .as_array()
        .expect("a list")
        .iter()
        .find(|person| person["email"] == email)
        .expect("the new person");

    assert_eq!(theirs["email_proved"], true);
    assert_eq!(theirs["state"], "active");
}

#[tokio::test]
async fn a_ticket_is_good_once() {
    let site = a_site().await;

    let email = format!("once-{}@example.test", Uuid::now_v7().simple());

    site.send(
        "POST",
        "/api/people",
        Some(&site.token),
        Some(serde_json::json!({
            "email": email, "name": "Once", "role_id": site.owner_role
        })),
    )
    .await;

    let ticket = site.ticket_for(&email).await;
    let choose = serde_json::json!({ "token": ticket, "password": "a long enough password" });

    let (first, _) = site
        .send("POST", "/api/auth/password", None, Some(choose.clone()))
        .await;
    let (again, _) = site
        .send("POST", "/api/auth/password", None, Some(choose))
        .await;

    assert_eq!(first, StatusCode::NO_CONTENT);
    assert_eq!(again, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn nobody_hands_out_more_than_they_hold() {
    let site = a_site_with(&[
        Needs::new(Capability::People, Access::View).grant(),
        Needs::new(Capability::People, Access::Write).grant(),
        Needs::new(Capability::Content, Access::View).grant(),
    ])
    .await;

    let (status, body) = site
        .send(
            "POST",
            "/api/roles",
            Some(&site.token),
            Some(serde_json::json!({
                "key": "sneaky", "name": "Sneaky",
                "grants": ["settings:write"]
            })),
        )
        .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    let (status, _) = site
        .send(
            "POST",
            "/api/roles",
            Some(&site.token),
            Some(serde_json::json!({
                "key": "reader", "name": "Reader",
                "grants": ["content:view"]
            })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "a grant they hold was refused");
}

#[tokio::test]
async fn a_grant_nothing_recognises_is_refused() {
    let site = a_site().await;

    let (status, _) = site
        .send(
            "POST",
            "/api/roles",
            Some(&site.token),
            Some(serde_json::json!({
                "key": "invented", "name": "Invented",
                "grants": ["content:levitate"]
            })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn nobody_changes_what_they_themselves_are() {
    let site = a_site().await;

    let (_, role) = site
        .send(
            "POST",
            "/api/roles",
            Some(&site.token),
            Some(serde_json::json!({
                "key": "lesser", "name": "Lesser", "grants": ["content:view"]
            })),
        )
        .await;

    let (status, _) = site
        .send(
            "PATCH",
            &format!("/api/people/{}", site.me),
            Some(&site.token),
            Some(serde_json::json!({ "role_id": role["id"] })),
        )
        .await;

    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "a site could be left with nobody able to change it back"
    );

    let (status, _) = site
        .send(
            "DELETE",
            &format!("/api/people/{}", site.me),
            Some(&site.token),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn the_last_owner_is_not_taken_away_by_somebody_else_either() {
    let site = a_site().await;
    let password = "a long enough password";
    let admin_role = a_role(
        &site.db,
        site.tenant,
        "admin",
        &["people:delete".to_owned()],
    )
    .await;
    let (_, admin_email) = a_user(&site.db, site.tenant, admin_role, password).await;

    let (_, session) = site
        .send(
            "POST",
            "/api/auth/session",
            None,
            Some(serde_json::json!({ "email": admin_email, "password": password })),
        )
        .await;

    let theirs = session["token"].as_str().expect("a token").to_owned();

    let (status, refused) = site
        .send(
            "DELETE",
            &format!("/api/people/{}", site.me),
            Some(&theirs),
            None,
        )
        .await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "the site's only owner was taken away by somebody else: {refused}"
    );
}

#[tokio::test]
async fn suspending_somebody_takes_away_what_they_are_holding() {
    let site = a_site().await;
    let password = "a long enough password";
    let (them, email) = a_user(&site.db, site.tenant, site.owner_role, password).await;

    let (_, session) = site
        .send(
            "POST",
            "/api/auth/session",
            None,
            Some(serde_json::json!({ "email": email, "password": password })),
        )
        .await;

    let theirs = session["token"].as_str().expect("a token").to_owned();

    let (status, _) = site.send("GET", "/api/people", Some(&theirs), None).await;
    assert_eq!(status, StatusCode::OK);

    site.send(
        "PATCH",
        &format!("/api/people/{them}"),
        Some(&site.token),
        Some(serde_json::json!({ "suspended": true })),
    )
    .await;

    let (status, _) = site.send("GET", "/api/people", Some(&theirs), None).await;

    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a suspended account kept the session it already had"
    );
}

#[tokio::test]
async fn asking_to_reset_says_the_same_whether_or_not_there_is_an_account() {
    let site = a_site().await;

    let (known, first) = site
        .send(
            "POST",
            "/api/auth/reset",
            None,
            Some(serde_json::json!({ "email": format!("a-{}@example.test", Uuid::now_v7().simple()) })),
        )
        .await;

    let (unknown, second) = site
        .send(
            "POST",
            "/api/auth/reset",
            None,
            Some(serde_json::json!({ "email": "nobody@example.test" })),
        )
        .await;

    assert_eq!(known, StatusCode::ACCEPTED);
    assert_eq!(known, unknown);
    assert_eq!(first, second);
}

#[tokio::test]
async fn a_password_that_is_too_short_is_refused() {
    let site = a_site().await;

    let (status, _) = site
        .send(
            "POST",
            "/api/auth/password",
            None,
            Some(serde_json::json!({ "token": "anything", "password": "short" })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn two_accounts_cannot_share_an_address_on_one_site() {
    let site = a_site().await;
    let email = format!("twice-{}@example.test", Uuid::now_v7().simple());

    for expected in [StatusCode::CREATED, StatusCode::CONFLICT] {
        let (status, _) = site
            .send(
                "POST",
                "/api/people",
                Some(&site.token),
                Some(serde_json::json!({
                    "email": email, "name": "Twice", "role_id": site.owner_role
                })),
            )
            .await;

        assert_eq!(status, expected);
    }
}

/// The other half of "nobody hands out more than they hold": a role that
/// already exists, handed out by somebody who does not hold what is on it.
#[tokio::test]
async fn nobody_puts_anybody_into_a_role_beyond_their_own() {
    let site = a_site().await;

    let (_, higher) = site
        .send(
            "POST",
            "/api/roles",
            Some(&site.token),
            Some(serde_json::json!({
                "key": "higher", "name": "Higher", "grants": ["settings:write"]
            })),
        )
        .await;

    let lesser = a_role(
        &site.db,
        site.tenant,
        "lesser",
        &[
            Needs::new(Capability::People, Access::View).grant(),
            Needs::new(Capability::People, Access::Write).grant(),
        ],
    )
    .await;

    let password = "a long enough password";
    let (_, email) = a_user(&site.db, site.tenant, lesser, password).await;

    let (_, session) = site
        .send(
            "POST",
            "/api/auth/session",
            None,
            Some(serde_json::json!({ "email": email, "password": password })),
        )
        .await;

    let theirs = session["token"].as_str().expect("a token").to_owned();

    let (status, body) = site
        .send(
            "POST",
            "/api/people",
            Some(&theirs),
            Some(serde_json::json!({
                "email": format!("climb-{}@example.test", Uuid::now_v7().simple()),
                "name": "A Way Up",
                "role_id": higher["id"],
            })),
        )
        .await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "somebody invited an account into a role above their own: {body}"
    );

    let (status, _) = site
        .send(
            "PATCH",
            &format!("/api/people/{}", site.me),
            Some(&theirs),
            Some(serde_json::json!({ "role_id": higher["id"] })),
        )
        .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

/// Un-suspending somebody who never arrived. `active` would say they have a
/// password, and they do not: the invitation is still what they need.
#[tokio::test]
async fn somebody_invited_and_suspended_goes_back_to_invited() {
    let site = a_site().await;

    let (_, invited) = site
        .send(
            "POST",
            "/api/people",
            Some(&site.token),
            Some(serde_json::json!({
                "email": format!("waiting-{}@example.test", Uuid::now_v7().simple()),
                "name": "Not Yet", "role_id": site.owner_role
            })),
        )
        .await;

    let id = invited["id"].as_str().expect("an id").to_owned();

    for (suspended, expected) in [(true, "suspended"), (false, "invited")] {
        let (status, person) = site
            .send(
                "PATCH",
                &format!("/api/people/{id}"),
                Some(&site.token),
                Some(serde_json::json!({ "suspended": suspended })),
            )
            .await;

        assert_eq!(status, StatusCode::OK, "{person}");
        assert_eq!(person["state"], expected);
    }
}

#[tokio::test]
async fn a_changed_address_is_not_proved_and_is_asked_to_be() {
    let site = a_site().await;

    let first = format!("first-{}@example.test", Uuid::now_v7().simple());

    let (_, made) = site
        .send(
            "POST",
            "/api/people",
            Some(&site.token),
            Some(serde_json::json!({
                "email": first, "name": "Somebody", "role_id": site.owner_role
            })),
        )
        .await;

    let id = made["id"].as_str().expect("an id").to_owned();

    let ticket = site.ticket_for(&first).await;

    site.send(
        "POST",
        "/api/auth/password",
        None,
        Some(serde_json::json!({
            "token": ticket, "password": "a long enough password"
        })),
    )
    .await;

    let second = format!("second-{}@example.test", Uuid::now_v7().simple());

    let (status, changed) = site
        .send(
            "PATCH",
            &format!("/api/people/{id}"),
            Some(&site.token),
            Some(serde_json::json!({ "email": second })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["email"], second);
    assert_eq!(
        changed["email_proved"], false,
        "an address nobody has proved was carried over as proved: {changed}"
    );

    // And the new address is asked to prove itself.
    let sent = site.ticket_for(&second).await;

    assert!(!sent.is_empty());
}

#[tokio::test]
async fn an_address_somebody_else_uses_is_refused() {
    let site = a_site().await;

    let taken = format!("taken-{}@example.test", Uuid::now_v7().simple());

    site.send(
        "POST",
        "/api/people",
        Some(&site.token),
        Some(serde_json::json!({
            "email": taken, "name": "The First", "role_id": site.owner_role
        })),
    )
    .await;

    let (_, other) = site
        .send(
            "POST",
            "/api/people",
            Some(&site.token),
            Some(serde_json::json!({
                "email": format!("other-{}@example.test", Uuid::now_v7().simple()),
                "name": "The Second",
                "role_id": site.owner_role
            })),
        )
        .await;

    let id = other["id"].as_str().expect("an id");

    let (status, refused) = site
        .send(
            "PATCH",
            &format!("/api/people/{id}"),
            Some(&site.token),
            Some(serde_json::json!({ "email": taken })),
        )
        .await;

    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "one account was given another's address: {refused}"
    );
}

#[tokio::test]
async fn somebody_changes_their_own_password_and_everything_open_closes() {
    let site = a_site().await;

    let (status, refused) = site
        .send(
            "PATCH",
            "/api/auth/password",
            Some(&site.token),
            Some(serde_json::json!({
                "current": "not the one they have",
                "next": "a long enough new password",
            })),
        )
        .await;

    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "a password changed without knowing the old one: {refused}"
    );

    let (status, said) = site
        .send(
            "PATCH",
            "/api/auth/password",
            Some(&site.token),
            Some(serde_json::json!({
                "current": "a long enough password",
                "next": "a long enough new password",
            })),
        )
        .await;

    assert_eq!(status, StatusCode::NO_CONTENT, "{said}");

    // What was open before is not open now, including the one that asked.
    let (status, _) = site
        .send("GET", "/api/people", Some(&site.token), None)
        .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_password_too_short_to_be_one_is_refused() {
    let site = a_site().await;

    let (status, refused) = site
        .send(
            "PATCH",
            "/api/auth/password",
            Some(&site.token),
            Some(serde_json::json!({
                "current": "a long enough password",
                "next": "short",
            })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert_eq!(
        refused["error"]["key"], "password_at_least_twelve_characters",
        "{refused}"
    );
}

#[tokio::test]
async fn a_role_can_be_changed_and_taken_away_when_nobody_is_in_it() {
    let site = a_site().await;

    let (status, made) = site
        .send(
            "POST",
            "/api/roles",
            Some(&site.token),
            Some(serde_json::json!({
                "key": "editor",
                "name": "Editor",
                "grants": ["content:view"],
            })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{made}");

    let id = made["id"].as_str().expect("an id").to_owned();

    let (status, changed) = site
        .send(
            "PATCH",
            &format!("/api/roles/{id}"),
            Some(&site.token),
            Some(serde_json::json!({ "grants": ["content:view", "content:write"] })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(
        changed["grants"].as_array().expect("grants").len(),
        2,
        "{changed}"
    );

    let (status, _) = site
        .send(
            "DELETE",
            &format!("/api/roles/{id}"),
            Some(&site.token),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn a_role_somebody_is_in_is_not_taken_away() {
    let site = a_site().await;

    let (_, made) = site
        .send(
            "POST",
            "/api/roles",
            Some(&site.token),
            Some(serde_json::json!({
                "key": "writer",
                "name": "Writer",
                "grants": ["content:view"],
            })),
        )
        .await;

    let id = made["id"].as_str().expect("an id").to_owned();

    site.send(
        "POST",
        "/api/people",
        Some(&site.token),
        Some(serde_json::json!({
            "email": format!("writer-{}@example.test", Uuid::now_v7().simple()),
            "name": "A Writer",
            "role_id": id,
        })),
    )
    .await;

    let (status, refused) = site
        .send(
            "DELETE",
            &format!("/api/roles/{id}"),
            Some(&site.token),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
    assert_eq!(refused["error"]["named"]["people"], "1", "{refused}");
}

#[tokio::test]
async fn the_build_s_own_roles_are_not_a_site_s_to_rewrite() {
    let site = a_site().await;

    let (_, made) = site
        .send(
            "POST",
            "/api/roles",
            Some(&site.token),
            Some(serde_json::json!({
                "key": "reader",
                "name": "Reader",
                "grants": ["content:view"],
            })),
        )
        .await;

    let id: Uuid = made["id"].as_str().expect("an id").parse().expect("a uuid");

    // What setup makes for a new site: a role the build owns rather than one
    // the site wrote.
    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

    sqlx::query("update roles set built_in = true where id = $1")
        .bind(id)
        .execute(conn.conn())
        .await
        .expect("a built-in role");

    conn.commit().await.expect("commit");

    let (status, refused) = site
        .send(
            "PATCH",
            &format!("/api/roles/{id}"),
            Some(&site.token),
            Some(serde_json::json!({ "grants": [] })),
        )
        .await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a site rewrote a role this build made: {refused}"
    );

    let (status, _) = site
        .send(
            "DELETE",
            &format!("/api/roles/{id}"),
            Some(&site.token),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_way_in_is_not_sent_to_an_address_nobody_has_proved() {
    let site = a_site().await;

    let first = format!("first-{}@example.test", Uuid::now_v7().simple());

    let (_, made) = site
        .send(
            "POST",
            "/api/people",
            Some(&site.token),
            Some(serde_json::json!({
                "email": first, "name": "Somebody", "role_id": site.owner_role
            })),
        )
        .await;

    let id = made["id"].as_str().expect("an id").to_owned();
    let ticket = site.ticket_for(&first).await;

    site.send(
        "POST",
        "/api/auth/password",
        None,
        Some(serde_json::json!({
            "token": ticket, "password": "a long enough password"
        })),
    )
    .await;

    // Changed to something nobody has proved yet.
    let second = format!("second-{}@example.test", Uuid::now_v7().simple());

    site.send(
        "PATCH",
        &format!("/api/people/{id}"),
        Some(&site.token),
        Some(serde_json::json!({ "email": second })),
    )
    .await;

    // Asking for a way in says nothing either way — which address exists is
    // not a thing this answers — and nothing is sent.
    let (status, _) = site
        .send(
            "POST",
            "/api/auth/reset",
            None,
            Some(serde_json::json!({ "email": second })),
        )
        .await;

    assert_eq!(status, StatusCode::ACCEPTED);

    let state = AppState::new(site.db.clone());
    let mut state = state;
    state.mailer = std::sync::Arc::new(Mailer::Recorded(site.post.clone()));

    for _ in 0..4 {
        mavi::jobs::tick_within(&state, "test", Some(site.tenant))
            .await
            .expect("tick");
    }

    let letters = site.post.all();

    let ways_in = letters
        .iter()
        .filter(|letter| letter.to == second && letter.body.contains("token="))
        .count();

    assert_eq!(
        ways_in, 1,
        "an address nobody has proved was sent a second way in"
    );
}
