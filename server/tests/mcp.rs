//! What is forbidden in the panel cannot be reached through a tool, which is
//! the one thing a second surface onto the same data has to get right.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use mavi::kernel::authz::{Access, Capability, Needs, every_grant};
use mavi::kernel::db::Db;
use mavi::kernel::http::AppState;
use mavi::kernel::tenant::TenantId;
use tower::ServiceExt;
use uuid::Uuid;

mod common;

use common::{a_role, a_tenant, a_user, harness};

struct Site {
    db: Db,
    router: axum::Router,
    host: String,
    tenant: TenantId,
}

async fn a_site() -> Site {
    let db = harness().await;
    let host = format!("{}.example", Uuid::now_v7().simple());
    let tenant = a_tenant(&db, &host).await;

    Site {
        router: mavi::router(AppState::new(db.clone())),
        db,
        host,
        tenant,
    }
}

impl Site {
    async fn somebody(&self, role: &str, grants: &[String]) -> String {
        let role_id = a_role(&self.db, self.tenant, role, grants).await;
        let password = "a long enough password";
        let (_, email) = a_user(&self.db, self.tenant, role_id, password).await;

        let (_, body) = self
            .send(
                "POST",
                "/api/auth/session",
                None,
                Some(serde_json::json!({ "email": email, "password": password })),
            )
            .await;

        body["token"].as_str().expect("a token").to_owned()
    }

    /// A language this site writes in, since a post is written in one.
    async fn writes_in(&self, code: &str) {
        let mut conn = self.db.tenant(self.tenant).await.expect("begin");

        sqlx::query(
            "insert into languages (tenant_id, code, name, is_default)
             values ($1, $2, $2, true)
             on conflict (tenant_id, code) do nothing",
        )
        .bind(self.tenant.0)
        .bind(code)
        .execute(conn.conn())
        .await
        .expect("a language");

        conn.commit().await.expect("commit");
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
async fn a_tool_forbidden_in_the_panel_is_forbidden_here() {
    let site = a_site().await;

    // Somebody who may read what a site has written, and nothing else.
    let token = site
        .somebody(
            "writer",
            &[Needs::new(Capability::Content, Access::View).grant()],
        )
        .await;

    let (status, listed) = site
        .send(
            "POST",
            "/mcp",
            Some(&token),
            Some(serde_json::json!({ "method": "tools/list" })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{listed}");

    let names: Vec<&str> = listed["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();

    // The invariant rather than a frozen list: what is offered is what this
    // caller's grants reach, and adding a tool they can use should not be a
    // test to edit.
    assert!(names.contains(&"posts_list"));
    assert!(
        !names.contains(&"orders_list"),
        "a tool they cannot use was offered: {names:?}"
    );

    for name in &names {
        let needs = listed["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .find(|tool| tool["name"] == *name)
            .and_then(|tool| tool["needs"].as_str())
            .expect("what it needs");

        assert!(
            needs.starts_with("content:view") || needs.starts_with("content:"),
            "{name} needs {needs}, which this caller does not hold"
        );
    }

    let (status, _) = site
        .send(
            "POST",
            "/mcp",
            Some(&token),
            Some(serde_json::json!({
                "method": "tools/call",
                "params": { "name": "orders_list", "arguments": {} }
            })),
        )
        .await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a tool that was not offered was still usable"
    );
}

#[tokio::test]
async fn a_tool_answers_with_this_site_s_own_rows() {
    let site = a_site().await;
    let token = site.somebody("owner", &every_grant()).await;

    let mut conn = site.db.tenant(site.tenant).await.expect("begin");

    sqlx::query(
        "insert into languages (tenant_id, code, name, is_default)
         values ($1, 'en', 'English', true)",
    )
    .bind(site.tenant.0)
    .execute(conn.conn())
    .await
    .expect("a language");

    conn.commit().await.expect("commit");

    site.send(
        "POST",
        "/api/posts",
        Some(&token),
        Some(serde_json::json!({ "language": "en", "title": "Something Written" })),
    )
    .await;

    let (status, answer) = site
        .send(
            "POST",
            "/mcp",
            Some(&token),
            Some(serde_json::json!({
                "method": "tools/call",
                "params": { "name": "posts_list", "arguments": {} }
            })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{answer}");
    assert_eq!(answer.as_array().expect("a list").len(), 1);
    assert_eq!(answer[0]["title"], "Something Written");
}

#[tokio::test]
async fn nothing_reaches_a_tool_without_an_account() {
    let site = a_site().await;

    let (status, _) = site
        .send(
            "POST",
            "/mcp",
            None,
            Some(serde_json::json!({ "method": "tools/list" })),
        )
        .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_site_on_hold_is_read_through_a_tool_and_not_written() {
    let site = a_site().await;
    let token = site.somebody("owner", &every_grant()).await;

    let mut conn = site.db.operator().await.expect("begin");
    sqlx::query("update tenants set state = 'suspended' where id = $1")
        .bind(site.tenant.0)
        .execute(conn.conn())
        .await
        .expect("on hold");
    conn.commit().await.expect("commit");

    // Reading still works, which is the same answer the panel gives.
    let (status, _) = site
        .send(
            "POST",
            "/mcp",
            Some(&token),
            Some(serde_json::json!({
                "method": "tools/call",
                "params": { "name": "posts_list", "arguments": {} }
            })),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn a_method_this_does_not_serve_says_so() {
    let site = a_site().await;
    let token = site.somebody("owner", &every_grant()).await;

    let (status, _) = site
        .send(
            "POST",
            "/mcp",
            Some(&token),
            Some(serde_json::json!({ "method": "resources/list" })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

/// An `:own` grant reaches what the person made. Every tool here reads across
/// a site, so holding one is not enough for a tool — it would hand back
/// everybody's.
#[tokio::test]
async fn a_grant_over_ones_own_does_not_open_a_tool_that_reads_everything() {
    let site = a_site().await;

    let token = site
        .somebody(
            "author",
            &[Needs::new(Capability::Content, Access::View).own_grant()],
        )
        .await;

    let (status, listed) = site
        .send(
            "POST",
            "/mcp",
            Some(&token),
            Some(serde_json::json!({ "method": "tools/list" })),
        )
        .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        listed["tools"].as_array().expect("tools").len(),
        0,
        "a tool that reads a whole site was offered to somebody who may read their own"
    );

    let (status, _) = site
        .send(
            "POST",
            "/mcp",
            Some(&token),
            Some(serde_json::json!({
                "method": "tools/call",
                "params": { "name": "posts_list", "arguments": {} }
            })),
        )
        .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_tool_writes_a_post_the_way_a_screen_would() {
    let site = a_site().await;
    let token = site.somebody("owner", &every_grant()).await;
    site.writes_in("en").await;

    let (status, written) = site
        .send(
            "POST",
            "/mcp",
            Some(&token),
            Some(serde_json::json!({
                "method": "tools/call",
                "params": {
                    "name": "posts_write",
                    "arguments": { "language": "en", "title": "Written by a tool" },
                },
            })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{written}");
    assert_eq!(written["state"], "draft");

    // And it is a post: the same table, with the same checks behind it.
    let (_, listed) = site
        .send(
            "POST",
            "/mcp",
            Some(&token),
            Some(serde_json::json!({
                "method": "tools/call",
                "params": { "name": "posts_list", "arguments": {} },
            })),
        )
        .await;

    assert!(
        listed
            .as_array()
            .expect("a list")
            .iter()
            .any(|post| post["title"] == "Written by a tool"),
        "{listed}"
    );
}

#[tokio::test]
async fn a_tool_cannot_write_a_post_in_a_language_the_site_does_not_have() {
    let site = a_site().await;
    let token = site.somebody("owner", &every_grant()).await;

    let (status, refused) = site
        .send(
            "POST",
            "/mcp",
            Some(&token),
            Some(serde_json::json!({
                "method": "tools/call",
                "params": {
                    "name": "posts_write",
                    "arguments": { "language": "de", "title": "Nope" },
                },
            })),
        )
        .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{refused}");
}

#[tokio::test]
async fn a_tool_changes_a_post_and_cannot_move_it_where_a_screen_could_not() {
    let site = a_site().await;
    let token = site.somebody("owner", &every_grant()).await;
    site.writes_in("en").await;

    let (_, written) = site
        .send(
            "POST",
            "/mcp",
            Some(&token),
            Some(serde_json::json!({
                "method": "tools/call",
                "params": {
                    "name": "posts_write",
                    "arguments": { "language": "en", "title": "A first go" },
                },
            })),
        )
        .await;

    let id = written["id"].as_str().expect("an id");

    let (status, changed) = site
        .send(
            "POST",
            "/mcp",
            Some(&token),
            Some(serde_json::json!({
                "method": "tools/call",
                "params": {
                    "name": "posts_change",
                    "arguments": { "id": id, "title": "What it says now", "state": "published" },
                },
            })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{changed}");
    assert_eq!(changed["state"], "published");

    // The state machine is the route's, and a tool asking is still asking it:
    // published goes to draft or to archived, and nowhere else.
    let (status, refused) = site
        .send(
            "POST",
            "/mcp",
            Some(&token),
            Some(serde_json::json!({
                "method": "tools/call",
                "params": {
                    "name": "posts_change",
                    "arguments": { "id": id, "state": "scheduled" },
                },
            })),
        )
        .await;

    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
}

#[tokio::test]
async fn a_tool_writes_a_design_to_the_draft_and_cannot_put_it_live() {
    let site = a_site().await;
    let token = site.somebody("owner", &every_grant()).await;

    let (status, written) = site
        .send(
            "POST",
            "/mcp",
            Some(&token),
            Some(serde_json::json!({
                "method": "tools/call",
                "params": {
                    "name": "design_write",
                    "arguments": { "path": "public/index.html", "body": "a draft" }
                }
            })),
        )
        .await;

    assert_eq!(status, StatusCode::OK, "{written}");
    assert_eq!(written["branch"], "draft");

    // Asked for live by name, which is the whole of what a tool must not do.
    let (refused_status, refused) = site
        .send(
            "POST",
            "/mcp",
            Some(&token),
            Some(serde_json::json!({
                "method": "tools/call",
                "params": {
                    "name": "design_write",
                    "arguments": {
                        "path": "public/index.html", "body": "live", "branch": "live"
                    }
                }
            })),
        )
        .await;

    assert_eq!(
        refused_status,
        StatusCode::FORBIDDEN,
        "a tool wrote straight to what is being served: {refused}"
    );

    // And what it did write is waiting for somebody rather than published.
    let (_, status_now) = site
        .send(
            "POST",
            "/mcp",
            Some(&token),
            Some(serde_json::json!({
                "method": "tools/call",
                "params": { "name": "design_status", "arguments": {} }
            })),
        )
        .await;

    assert_eq!(
        status_now["changed"][0]["path"], "public/index.html",
        "{status_now}"
    );
    assert!(status_now["live"].is_null());
}

#[tokio::test]
async fn nothing_in_the_design_tools_publishes() {
    let site = a_site().await;
    let token = site.somebody("owner", &every_grant()).await;

    let (_, listed) = site
        .send(
            "POST",
            "/mcp",
            Some(&token),
            Some(serde_json::json!({ "method": "tools/list" })),
        )
        .await;

    let publishes: Vec<&str> = listed["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .filter(|name| name.contains("publish") || name.contains("approve"))
        .collect();

    assert!(
        publishes.is_empty(),
        "publishing is a person's, and a tool was offered it: {publishes:?}"
    );
}
