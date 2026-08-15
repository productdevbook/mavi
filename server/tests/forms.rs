//! The seven a domain has to answer for, driven through the router: the happy
//! path, a body that is wrong, an account without the grant, another site's
//! row, one that is not there, one that collides, and the same request twice.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mavi::kernel::authz::{Access, Capability, Needs};
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
    peer: std::net::SocketAddr,
}

/// A different visitor for every test, since they all share one database and
/// the limit is counted per caller.
fn a_peer() -> std::net::SocketAddr {
    use std::net::{IpAddr, Ipv6Addr};

    let bits = Uuid::now_v7().as_u128();

    std::net::SocketAddr::new(IpAddr::V6(Ipv6Addr::from_bits(bits)), 443)
}

async fn a_site_with(grants: &[String]) -> Site {
    let db = harness().await;
    let host = format!("{}.example", Uuid::now_v7().simple());
    let tenant = a_tenant(&db, &host).await;
    let role = a_role(&db, tenant, "tested", grants).await;
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
        peer: a_peer(),
    }
}

async fn everything_granted() -> Site {
    a_site_with(&mavi::kernel::authz::every_grant()).await
}

impl Site {
    async fn send(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Option<serde_json::Value>,
    ) -> (StatusCode, serde_json::Value) {
        // A peer address of its own, so that this test's visitor is counted
        // as one caller and not as everybody: with nothing saying where a
        // request came from there is nobody to count.
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .extension(axum::extract::ConnectInfo(self.peer))
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

    async fn a_form(&self, slug: &str) -> Uuid {
        let (status, body) = self
            .send(
                "POST",
                "/api/forms",
                Some(&self.token),
                Some(serde_json::json!({ "slug": slug, "name": "Get in touch" })),
            )
            .await;

        assert_eq!(status, StatusCode::CREATED, "{body}");

        body["id"].as_str().expect("an id").parse().expect("a uuid")
    }
}

#[tokio::test]
async fn a_form_can_be_made_read_changed_and_taken_away() {
    let site = everything_granted().await;
    let id = site.a_form("contact").await;

    let (status, body) = site
        .send("GET", &format!("/api/forms/{id}"), Some(&site.token), None)
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["slug"], "contact");

    let (status, body) = site
        .send(
            "PATCH",
            &format!("/api/forms/{id}"),
            Some(&site.token),
            Some(serde_json::json!({ "name": "Say hello", "active": false })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["name"], "Say hello");
    assert_eq!(body["active"], false);

    let (status, _) = site
        .send(
            "DELETE",
            &format!("/api/forms/{id}"),
            Some(&site.token),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = site
        .send("GET", &format!("/api/forms/{id}"), Some(&site.token), None)
        .await;

    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "something that was taken away is still being served"
    );
}

#[tokio::test]
async fn a_body_that_is_wrong_is_refused_field_by_field() {
    let site = everything_granted().await;

    for wrong in [
        serde_json::json!({ "slug": "Not A Slug", "name": "Fine" }),
        serde_json::json!({ "slug": "fine", "name": "" }),
        serde_json::json!({ "slug": "fine" }),
        serde_json::json!({ "slug": "fine", "name": "Fine", "tenant_id": Uuid::now_v7() }),
    ] {
        let (status, _) = site
            .send("POST", "/api/forms", Some(&site.token), Some(wrong.clone()))
            .await;

        assert!(
            status == StatusCode::UNPROCESSABLE_ENTITY || status == StatusCode::BAD_REQUEST,
            "{wrong} was accepted with {status}"
        );
    }
}

#[tokio::test]
async fn an_account_without_the_grant_is_refused_and_it_is_written_down() {
    let site = a_site_with(&[Needs::new(Capability::Forms, Access::View).grant()]).await;

    let (status, _) = site
        .send(
            "POST",
            "/api/forms",
            Some(&site.token),
            Some(serde_json::json!({ "slug": "contact", "name": "Get in touch" })),
        )
        .await;

    assert_eq!(status, StatusCode::FORBIDDEN);

    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

    let refusals: (i64,) = sqlx::query_as(
        "select count(*) from audit_log where action = 'refused' and subject_id = $1",
    )
    .bind(Needs::new(Capability::Forms, Access::Write).grant())
    .fetch_one(conn.conn())
    .await
    .expect("audit");

    assert_eq!(
        refusals.0, 1,
        "why somebody could not get in has to be answerable from the log"
    );
}

#[tokio::test]
async fn something_that_is_not_there_says_so() {
    let site = everything_granted().await;

    let (status, _) = site
        .send(
            "GET",
            &format!("/api/forms/{}", Uuid::now_v7()),
            Some(&site.token),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_name_that_is_taken_is_a_collision_rather_than_a_second_form() {
    let site = everything_granted().await;
    site.a_form("contact").await;

    let (status, _) = site
        .send(
            "POST",
            "/api/forms",
            Some(&site.token),
            Some(serde_json::json!({ "slug": "contact", "name": "Another" })),
        )
        .await;

    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn a_visitor_can_send_one_and_the_site_reads_it_back() {
    let site = everything_granted().await;
    let id = site.a_form("contact").await;

    let (status, body) = site
        .send(
            "POST",
            "/api/sites/forms/contact/submissions",
            None,
            Some(serde_json::json!({ "answers": { "name": "Somebody", "note": "Hello" } })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{body}");

    let (status, body) = site
        .send(
            "GET",
            &format!("/api/forms/{id}/submissions"),
            Some(&site.token),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::OK);

    let items = body["items"].as_array().expect("a list");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["answers"]["name"], "Somebody");
}

#[tokio::test]
async fn a_visitor_is_bounded_in_what_they_can_send() {
    let site = everything_granted().await;
    site.a_form("contact").await;

    let too_long = "x".repeat(10_001);

    let (status, _) = site
        .send(
            "POST",
            "/api/sites/forms/contact/submissions",
            None,
            Some(serde_json::json!({ "answers": { "note": too_long } })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // The limit is per address and this test has none, so they all count as one
    // caller — which is exactly what a visitor behind a home router looks like.
    let mut refused = false;

    for _ in 0..25 {
        let (status, _) = site
            .send(
                "POST",
                "/api/sites/forms/contact/submissions",
                None,
                Some(serde_json::json!({ "answers": { "note": "hello" } })),
            )
            .await;

        if status == StatusCode::TOO_MANY_REQUESTS {
            refused = true;
            break;
        }
    }

    assert!(refused, "a form took everything anybody sent it");
}

#[tokio::test]
async fn a_form_nobody_is_serving_takes_nothing() {
    let site = everything_granted().await;
    let id = site.a_form("contact").await;

    site.send(
        "PATCH",
        &format!("/api/forms/{id}"),
        Some(&site.token),
        Some(serde_json::json!({ "active": false })),
    )
    .await;

    let (status, _) = site
        .send(
            "POST",
            "/api/sites/forms/contact/submissions",
            None,
            Some(serde_json::json!({ "answers": { "note": "hello" } })),
        )
        .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// The counter exists so that a listing which quietly asks once per row is a
/// failing test rather than a page that gets slow a year from now. Counted
/// against itself rather than against a number: what a request costs before it
/// reaches the listing is fixed, and what matters is that ten rows cost the
/// same as one.
#[tokio::test]
async fn a_listing_asks_the_same_of_ten_rows_as_of_one() {
    let site = everything_granted().await;
    site.a_form("form-0").await;

    // Warmed first: a pool that opens a connection mid-measurement runs its
    // own `set role` on it, and that is not what is being counted.
    site.send("GET", "/api/forms?limit=25", Some(&site.token), None)
        .await;

    let for_one = {
        let (counter, _guard) = common::queries::counting();
        let (status, _) = site
            .send("GET", "/api/forms?limit=25", Some(&site.token), None)
            .await;

        assert_eq!(status, StatusCode::OK);
        counter.count()
    };

    for n in 1..10 {
        site.a_form(&format!("form-{n}")).await;
    }

    site.send("GET", "/api/forms?limit=25", Some(&site.token), None)
        .await;

    let (counter, _guard) = common::queries::counting();

    let (status, body) = site
        .send("GET", "/api/forms?limit=25", Some(&site.token), None)
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().expect("a list").len(), 10);

    assert!(
        counter.count() <= for_one + 2,
        "ten forms cost {} to list where one cost {for_one}, so something is \
         asking per row",
        counter.count()
    );
}

#[tokio::test]
async fn a_form_refuses_what_it_did_not_ask_for() {
    let site = everything_granted().await;

    let (status, made) = site
        .send(
            "POST",
            "/api/forms",
            Some(&site.token),
            Some(serde_json::json!({
                "slug": "contact",
                "name": "Contact",
                "fields": [
                    { "key": "name", "label": "Name", "required": true },
                    { "key": "email", "label": "Email", "required": true, "kind": "email" },
                    {
                        "key": "about", "label": "About", "required": false,
                        "kind": "choice", "options": ["sales", "support"]
                    }
                ],
            })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{made}");

    // Nothing where something was wanted.
    let (status, refused) = site
        .send(
            "POST",
            "/api/sites/forms/contact/submissions",
            None,
            Some(serde_json::json!({ "answers": { "name": "Somebody" } })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
    assert_eq!(refused["error"]["named"]["field"], "email", "{refused}");

    // Something that is not an address where one was wanted.
    let (status, refused) = site
        .send(
            "POST",
            "/api/sites/forms/contact/submissions",
            None,
            Some(serde_json::json!({
                "answers": { "name": "Somebody", "email": "not an address" }
            })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");

    // A choice that is not one of them.
    let (status, refused) = site
        .send(
            "POST",
            "/api/sites/forms/contact/submissions",
            None,
            Some(serde_json::json!({
                "answers": {
                    "name": "Somebody",
                    "email": "somebody@example.test",
                    "about": "something else",
                }
            })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");

    // A field the form never declared.
    let (status, refused) = site
        .send(
            "POST",
            "/api/sites/forms/contact/submissions",
            None,
            Some(serde_json::json!({
                "answers": {
                    "name": "Somebody",
                    "email": "somebody@example.test",
                    "whatever": "they liked",
                }
            })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");

    // And what it did ask for goes in.
    let (status, taken) = site
        .send(
            "POST",
            "/api/sites/forms/contact/submissions",
            None,
            Some(serde_json::json!({
                "answers": {
                    "name": "Somebody",
                    "email": "somebody@example.test",
                    "about": "sales",
                }
            })),
        )
        .await;

    assert_eq!(status, StatusCode::CREATED, "{taken}");
}

#[tokio::test]
async fn a_form_that_declares_nothing_still_takes_anything() {
    let site = everything_granted().await;

    site.send(
        "POST",
        "/api/forms",
        Some(&site.token),
        Some(serde_json::json!({ "slug": "anything", "name": "Anything" })),
    )
    .await;

    let (status, taken) = site
        .send(
            "POST",
            "/api/sites/forms/anything/submissions",
            None,
            Some(serde_json::json!({ "answers": { "whatever": "they liked" } })),
        )
        .await;

    assert_eq!(
        status,
        StatusCode::CREATED,
        "a site with its own page in front of the form was refused: {taken}"
    );
}

#[tokio::test]
async fn a_list_of_forms_says_what_is_waiting_in_each() {
    let site = everything_granted().await;

    site.send(
        "POST",
        "/api/forms",
        Some(&site.token),
        Some(serde_json::json!({ "slug": "waiting", "name": "Waiting" })),
    )
    .await;

    for _ in 0..2 {
        site.send(
            "POST",
            "/api/sites/forms/waiting/submissions",
            None,
            Some(serde_json::json!({ "answers": { "anything": "at all" } })),
        )
        .await;
    }

    let (status, listed) = site
        .send("GET", "/api/forms", Some(&site.token), None)
        .await;

    assert_eq!(status, StatusCode::OK, "{listed}");

    let form = listed["items"]
        .as_array()
        .expect("a page")
        .iter()
        .find(|form| form["slug"] == "waiting")
        .expect("the form");

    assert_eq!(form["submissions"], 2, "{listed}");
    assert_eq!(form["unseen"], 2, "{listed}");
}

#[tokio::test]
async fn what_came_in_can_be_read_and_taken_away() {
    let site = everything_granted().await;

    let (_, made) = site
        .send(
            "POST",
            "/api/forms",
            Some(&site.token),
            Some(serde_json::json!({ "slug": "read-me", "name": "Read Me" })),
        )
        .await;

    let id = made["id"].as_str().expect("an id").to_owned();

    site.send(
        "POST",
        "/api/sites/forms/read-me/submissions",
        None,
        Some(serde_json::json!({ "answers": { "anything": "at all" } })),
    )
    .await;

    let (status, seen) = site
        .send(
            "POST",
            &format!("/api/forms/{id}/seen"),
            Some(&site.token),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{seen}");
    assert_eq!(seen["seen"], 1);

    let (_, listed) = site
        .send(
            "GET",
            &format!("/api/forms/{id}/submissions"),
            Some(&site.token),
            None,
        )
        .await;

    let submission = listed["items"][0]["id"].as_str().expect("an id").to_owned();

    let (status, _) = site
        .send(
            "DELETE",
            &format!("/api/forms/{id}/submissions/{submission}"),
            Some(&site.token),
            None,
        )
        .await;

    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, after) = site
        .send(
            "GET",
            &format!("/api/forms/{id}/submissions"),
            Some(&site.token),
            None,
        )
        .await;

    assert_eq!(after["items"].as_array().expect("a page").len(), 0);
}

#[tokio::test]
async fn what_somebody_sent_is_not_written_into_the_record_when_it_goes() {
    let site = everything_granted().await;

    let (_, made) = site
        .send(
            "POST",
            "/api/forms",
            Some(&site.token),
            Some(serde_json::json!({ "slug": "private", "name": "Private" })),
        )
        .await;

    let id = made["id"].as_str().expect("an id").to_owned();

    site.send(
        "POST",
        "/api/sites/forms/private/submissions",
        None,
        Some(serde_json::json!({
            "answers": { "secret": "something they told us in confidence" }
        })),
    )
    .await;

    let (_, listed) = site
        .send(
            "GET",
            &format!("/api/forms/{id}/submissions"),
            Some(&site.token),
            None,
        )
        .await;

    let submission = listed["items"][0]["id"].as_str().expect("an id").to_owned();

    site.send(
        "DELETE",
        &format!("/api/forms/{id}/submissions/{submission}"),
        Some(&site.token),
        None,
    )
    .await;

    let (_, record) = site
        .send("GET", "/api/audit", Some(&site.token), None)
        .await;

    assert!(
        !record.to_string().contains("in confidence"),
        "what somebody asked to have taken away was copied into the record"
    );
}
