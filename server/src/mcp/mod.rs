//! What an assistant can do here.
//!
//! One door and a list of tools, each asking for the same grant the panel's
//! own endpoint asks for — which is what makes "forbidden in the panel,
//! allowed over MCP" impossible rather than unlikely. Nothing here publishes:
//! that is a person's.
use axum::Json;
use axum::extract::State as Injected;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::kernel::audit::{self, Actor, Audited};
use crate::kernel::authz::{self, Access, Capability, Needs};
use crate::kernel::error::{AppError, Result};
use crate::kernel::http::{AppState, Audience, Caller, Endpoint, Guard, RatePolicy};
use crate::kernel::ratelimit::Limit;
use crate::kernel::say::{self, Say};

/// The same rate a panel screen would generate, and a limit because a tool
/// loop is a thing that runs until told otherwise.
const CALL_LIMIT: Limit = Limit::new(120, 60);

/// What `initialize` answers with. Not negotiated against what a client asks
/// for — there is one shape this serves, so there is one version to claim.
const PROTOCOL_VERSION: &str = "2025-06-18";

/// One tool, and the grant it consumes.
///
/// The grant is the same one the panel's own endpoint asks for, from the same
/// matrix, which is what makes "forbidden in the panel, allowed over MCP"
/// impossible rather than unlikely.
pub struct Tool {
    pub name: &'static str,
    pub about: &'static str,
    pub needs: Needs,
    pub run: fn(&AppState, &Caller, &serde_json::Value) -> ToolFuture,
}

pub type ToolFuture =
    std::pin::Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + 'static>>;

impl std::fmt::Debug for Tool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tool")
            .field("name", &self.name)
            .field("needs", &self.needs)
            .finish_non_exhaustive()
    }
}

/// Every tool there is.
///
/// Long, and one list rather than several: what an assistant can do here is a
/// thing to read in one go, and a tool hidden in another file is a grant nobody
/// checks against the panel's.
#[must_use]
#[expect(clippy::too_many_lines, reason = "one list of what there is")]
pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "posts_list",
            about: "What this site has written.",
            needs: Needs::new(Capability::Content, Access::View),
            run: |state, caller, _arguments| {
                let state = state.clone();
                let caller = caller.clone();

                Box::pin(async move {
                    let mut conn = state.db.tenant(caller.tenant()).await?;

                    let rows: Vec<(uuid::Uuid, String, String)> = sqlx::query_as(
                        "select id, title, slug from posts
                          where deleted_at is null order by created_at desc limit 50",
                    )
                    .fetch_all(conn.conn())
                    .await?;

                    conn.commit().await?;

                    Ok(json!(
                        rows.iter()
                            .map(|(id, title, slug)| json!({
                                "id": id, "title": title, "slug": slug
                            }))
                            .collect::<Vec<_>>()
                    ))
                })
            },
        },
        Tool {
            name: "posts_get",
            about: "One post, whole: its body, its fields, and what it is filed under.",
            needs: Needs::new(Capability::Content, Access::View),
            run: |state, caller, arguments| {
                let state = state.clone();
                let caller = caller.clone();
                let id = uuid_in(arguments, "id");

                Box::pin(async move {
                    let id = id?;
                    let mut conn = state.db.tenant(caller.tenant()).await?;

                    let found: Option<(uuid::Uuid, String, String, String, String, String)> =
                        sqlx::query_as(
                            "select id, title, slug, body, state::text, language
                               from posts where id = $1 and deleted_at is null",
                        )
                        .bind(id)
                        .fetch_optional(conn.conn())
                        .await?;

                    conn.commit().await?;

                    let (id, title, slug, body, state, language) =
                        found.ok_or(AppError::NotFound("post"))?;

                    Ok(json!({
                        "id": id, "title": title, "slug": slug, "body": body,
                        "state": state, "language": language
                    }))
                })
            },
        },
        Tool {
            name: "posts_write",
            about: "Writes a post. What a kind of thing declares is what its fields may hold.",
            needs: Needs::new(Capability::Content, Access::Write),
            run: |state, caller, arguments| {
                let state = state.clone();
                let caller = caller.clone();
                let arguments = arguments.clone();

                Box::pin(async move {
                    crate::content::write_through_a_tool(&state, &caller, &arguments).await
                })
            },
        },
        Tool {
            name: "posts_change",
            about: "Changes a post that is already here, by id. Which states it may move between is what it would be from the panel.",
            needs: Needs::new(Capability::Content, Access::Write),
            run: |state, caller, arguments| {
                let state = state.clone();
                let caller = caller.clone();
                let arguments = arguments.clone();

                Box::pin(async move {
                    crate::content::change_through_a_tool(&state, &caller, &arguments).await
                })
            },
        },
        Tool {
            name: "taxonomy_list",
            about: "The categories and tags this site files things under.",
            needs: Needs::new(Capability::Taxonomy, Access::View),
            run: |state, caller, _arguments| {
                let state = state.clone();
                let caller = caller.clone();

                Box::pin(async move {
                    let mut conn = state.db.tenant(caller.tenant()).await?;

                    let rows: Vec<(uuid::Uuid, String, String, String)> = sqlx::query_as(
                        "select id, kind::text, name, slug from terms
                          where deleted_at is null order by kind, name limit 200",
                    )
                    .fetch_all(conn.conn())
                    .await?;

                    conn.commit().await?;

                    Ok(json!(
                        rows.iter()
                            .map(|(id, kind, name, slug)| json!({
                                "id": id, "kind": kind, "name": name, "slug": slug
                            }))
                            .collect::<Vec<_>>()
                    ))
                })
            },
        },
        Tool {
            name: "media_list",
            about: "What this site has uploaded.",
            needs: Needs::new(Capability::Media, Access::View),
            run: |state, caller, _arguments| {
                let state = state.clone();
                let caller = caller.clone();

                Box::pin(async move {
                    let mut conn = state.db.tenant(caller.tenant()).await?;

                    let rows: Vec<(uuid::Uuid, String, String, i64)> = sqlx::query_as(
                        "select id, original_name, mime, bytes from media
                          where deleted_at is null order by created_at desc limit 50",
                    )
                    .fetch_all(conn.conn())
                    .await?;

                    conn.commit().await?;

                    Ok(json!(
                        rows.iter()
                            .map(|(id, name, mime, bytes)| json!({
                                "id": id, "name": name, "mime": mime, "bytes": bytes,
                                "at": format!("/uploads/{id}")
                            }))
                            .collect::<Vec<_>>()
                    ))
                })
            },
        },
        Tool {
            name: "page_issues",
            about: "What is wrong with this site's pages before anybody reads them.",
            needs: Needs::new(Capability::Content, Access::View),
            run: |state, caller, _arguments| {
                let state = state.clone();
                let caller = caller.clone();

                Box::pin(async move {
                    let mut conn = state.db.tenant(caller.tenant()).await?;

                    let rows: Vec<(uuid::Uuid, String, String, String)> = sqlx::query_as(
                        "select i.post_id, p.title, i.kind, i.weight::text
                           from page_issues i join posts p on p.id = i.post_id
                          where p.deleted_at is null
                          order by i.weight desc limit 200",
                    )
                    .fetch_all(conn.conn())
                    .await?;

                    conn.commit().await?;

                    Ok(json!(
                        rows.iter()
                            .map(|(post, title, kind, weight)| json!({
                                "post_id": post, "title": title, "kind": kind,
                                "weight": weight
                            }))
                            .collect::<Vec<_>>()
                    ))
                })
            },
        },
        Tool {
            name: "trash_list",
            about: "What was thrown away and can still be put back.",
            needs: Needs::new(Capability::Content, Access::View),
            run: |state, caller, _arguments| {
                let state = state.clone();
                let caller = caller.clone();

                Box::pin(async move {
                    let mut conn = state.db.tenant(caller.tenant()).await?;
                    // First page only: an assistant asking what is in the bin
                    // wants an answer, not a walk of the whole thing.
                    let thrown = crate::trash::what_was_thrown(
                        &mut conn,
                        None,
                        i64::from(crate::kernel::page::MAX_LIMIT),
                    )
                    .await?;
                    conn.commit().await?;

                    Ok(json!(thrown))
                })
            },
        },
        Tool {
            name: "site_health",
            about: "Whether this site is well: what is published, what failed, what is not answering.",
            needs: Needs::new(Capability::Settings, Access::View),
            run: |state, caller, _arguments| {
                let state = state.clone();
                let caller = caller.clone();

                Box::pin(async move {
                    let mut conn = state.db.tenant(caller.tenant()).await?;
                    let checks = crate::health::look_at(&mut conn).await?;
                    conn.commit().await?;

                    Ok(json!(checks))
                })
            },
        },
        Tool {
            name: "visitors_overview",
            about: "How many people read this site, day by day.",
            needs: Needs::new(Capability::Settings, Access::View),
            run: |state, caller, _arguments| {
                let state = state.clone();
                let caller = caller.clone();

                Box::pin(async move {
                    let mut conn = state.db.tenant(caller.tenant()).await?;

                    let rows: Vec<(chrono::NaiveDate, i64, i64)> = sqlx::query_as(
                        "select on_day, sum(views)::bigint, sum(visitors)::bigint
                           from page_views
                          where on_day > current_date - 30
                          group by on_day order by on_day desc",
                    )
                    .fetch_all(conn.conn())
                    .await?;

                    conn.commit().await?;

                    Ok(json!(
                        rows.iter()
                            .map(|(on_day, views, visitors)| json!({
                                "on_day": on_day, "views": views, "visitors": visitors
                            }))
                            .collect::<Vec<_>>()
                    ))
                })
            },
        },
        Tool {
            name: "vitals_scores",
            about: "How the site felt to whoever was on it, and which pages are worst.",
            needs: Needs::new(Capability::Settings, Access::View),
            run: |state, caller, arguments| {
                let state = state.clone();
                let caller = caller.clone();
                let arguments = arguments.clone();

                Box::pin(async move {
                    crate::analytics::scores_through_a_tool(&state, &caller, &arguments).await
                })
            },
        },
        Tool {
            name: "design_files",
            about: "What the site's design is made of, on the branch being worked on.",
            needs: Needs::new(Capability::Design, Access::View),
            run: |state, caller, arguments| {
                let state = state.clone();
                let caller = caller.clone();
                let arguments = arguments.clone();

                Box::pin(async move {
                    crate::publishing::tools::files(&state, &caller, &arguments).await
                })
            },
        },
        Tool {
            name: "design_read",
            about: "One design file, whole.",
            needs: Needs::new(Capability::Design, Access::View),
            run: |state, caller, arguments| {
                let state = state.clone();
                let caller = caller.clone();
                let arguments = arguments.clone();

                Box::pin(async move {
                    crate::publishing::tools::read(&state, &caller, &arguments).await
                })
            },
        },
        Tool {
            name: "design_write",
            about: "Writes a design file to the draft. Nothing written here is live: \
                    a person publishes, and there is no tool for it.",
            needs: Needs::new(Capability::Design, Access::Write),
            run: |state, caller, arguments| {
                let state = state.clone();
                let caller = caller.clone();
                let arguments = arguments.clone();

                Box::pin(async move {
                    crate::publishing::tools::write(&state, &caller, &arguments).await
                })
            },
        },
        Tool {
            name: "design_preview",
            about: "Builds the draft somewhere to look at, without touching the live site.",
            needs: Needs::new(Capability::Design, Access::Write),
            run: |state, caller, arguments| {
                let state = state.clone();
                let caller = caller.clone();
                let arguments = arguments.clone();

                Box::pin(async move {
                    crate::publishing::tools::preview(&state, &caller, &arguments).await
                })
            },
        },
        Tool {
            name: "design_status",
            about: "What a publish would change, what is being built, and where to look at it.",
            needs: Needs::new(Capability::Design, Access::View),
            run: |state, caller, arguments| {
                let state = state.clone();
                let caller = caller.clone();
                let arguments = arguments.clone();

                Box::pin(async move {
                    crate::publishing::tools::status(&state, &caller, &arguments).await
                })
            },
        },
        Tool {
            name: "report_a_gap",
            about: "Says that something is missing or broken, to whoever runs the machine.",
            needs: Needs::new(Capability::Content, Access::View),
            run: |state, caller, arguments| {
                let state = state.clone();
                let caller = caller.clone();
                let said = arguments
                    .get("body")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let screen = arguments
                    .get("screen")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned);

                Box::pin(async move {
                    crate::reports::said_by_a_tool(&state, &caller, &said, screen.as_deref()).await
                })
            },
        },
        Tool {
            name: "forms_submissions",
            about: "What people have sent through a form.",
            needs: Needs::new(Capability::Forms, Access::View),
            run: |state, caller, arguments| {
                let state = state.clone();
                let caller = caller.clone();
                let slug = arguments
                    .get("slug")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned();

                Box::pin(async move {
                    let mut conn = state.db.tenant(caller.tenant()).await?;

                    let rows: Vec<(uuid::Uuid, serde_json::Value)> = sqlx::query_as(
                        "select s.id, s.answers
                           from form_submissions s join forms f on f.id = s.form_id
                          where f.slug = $1 and s.deleted_at is null
                          order by s.created_at desc limit 50",
                    )
                    .bind(&slug)
                    .fetch_all(conn.conn())
                    .await?;

                    conn.commit().await?;

                    Ok(json!(
                        rows.iter()
                            .map(|(id, answers)| json!({ "id": id, "answers": answers }))
                            .collect::<Vec<_>>()
                    ))
                })
            },
        },
        Tool {
            name: "orders_list",
            about: "What this site has sold.",
            needs: Needs::new(Capability::Shop, Access::View),
            run: |state, caller, _arguments| {
                let state = state.clone();
                let caller = caller.clone();

                Box::pin(async move {
                    let mut conn = state.db.tenant(caller.tenant()).await?;

                    let rows: Vec<(uuid::Uuid, String, i64)> = sqlx::query_as(
                        "select id, state::text, total_minor from orders
                          order by created_at desc limit 50",
                    )
                    .fetch_all(conn.conn())
                    .await?;

                    conn.commit().await?;

                    Ok(json!(
                        rows.iter()
                            .map(|(id, state, total)| json!({
                                "id": id, "state": state, "total_minor": total
                            }))
                            .collect::<Vec<_>>()
                    ))
                })
            },
        },
    ]
}

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    vec![
        Endpoint::post(
            "/mcp",
            Guard {
                audience: Audience::User,
                needs: None,
                rate: RatePolicy::Per(CALL_LIMIT),
            },
            call,
        )
        .takes::<Request>(),
    ]
}

/// A uuid an argument was supposed to carry.
fn uuid_in(arguments: &serde_json::Value, name: &'static str) -> Result<uuid::Uuid> {
    arguments
        .get(name)
        .and_then(serde_json::Value::as_str)
        .and_then(|said| said.parse().ok())
        .ok_or(AppError::NotFound("post"))
}

/// What a caller sends: a JSON-RPC envelope. Not the whole of the protocol —
/// three methods answered, which are the ones this serves — but the envelope
/// itself is every client's, so it is accepted whole rather than trimmed to
/// what today's handler happens to read. No `deny_unknown_fields`: a field a
/// future protocol version adds is not this caller's mistake.
///
/// No `id` means no answer is wanted — a JSON-RPC notification. A real client
/// never sends `id: null` for a request it wants answered, so that is read the
/// same way as an absent one rather than kept apart with a second `Option`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct Request {
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
    #[serde(default)]
    pub id: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Listed {
    pub tools: Vec<Described>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Described {
    pub name: &'static str,
    pub about: &'static str,
    /// What the caller must hold to use it, said out loud so that a client can
    /// show why something is not there.
    pub needs: String,
}

async fn call(
    Injected(state): Injected<AppState>,
    caller: Caller,
    Json(body): Json<Request>,
) -> Result<Audited<Response>> {
    let id = body.id.clone();

    match body.method.as_str() {
        "initialize" => {
            let result = json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": env!("CARGO_PKG_NAME"),
                    "version": env!("CARGO_PKG_VERSION"),
                },
            });

            let receipt = recorded(&state, &caller, "initialised", "").await?;

            Ok(Audited::new(receipt, answered(id, result)))
        }
        // What this caller may use, not what exists. A tool they cannot reach
        // is not listed, which is the same rule the panel's menu follows — and
        // the check below is what actually stops it.
        "tools/list" => {
            let listed: Vec<Described> = tools()
                .into_iter()
                .filter(|tool| may(&caller, tool.needs).is_ok())
                .map(|tool| Described {
                    name: tool.name,
                    about: tool.about,
                    needs: tool.needs.grant(),
                })
                .collect();

            let receipt = recorded(&state, &caller, "listed tools", "").await?;

            Ok(Audited::new(
                receipt,
                answered(id, json!({ "tools": listed })),
            ))
        }
        "tools/call" => {
            let name = body
                .params
                .get("name")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| AppError::Invalid(say::TOOL.into()))?;

            let arguments = body
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));

            let tools = tools();

            let tool = tools
                .iter()
                .find(|tool| tool.name == name)
                .ok_or(AppError::NotFound("tool"))?;

            // The engine, with the same grant the panel's own endpoint asks
            // for. Nothing here has its own idea of who may do what.
            may(&caller, tool.needs)?;

            let answer = (tool.run)(&state, &caller, &arguments).await?;
            let receipt = recorded(&state, &caller, "used a tool", name).await?;

            Ok(Audited::new(receipt, answered(id, answer)))
        }
        // A request expects a JSON-RPC error object naming the method — the
        // shape a JSON-RPC client parses on any unrecognised call, code
        // -32601 by the spec's own numbering. A notification (no `id`) is not
        // answered at all: a future version of this protocol names methods
        // this build has never heard of, and a client that did not ask for an
        // answer is not told it got one wrong.
        other => {
            let receipt = recorded(&state, &caller, "asked for a method not served", other).await?;

            Ok(Audited::new(receipt, method_not_found(id, other)))
        }
    }
}

/// A JSON-RPC response for a request, or nothing for a notification — `id`'s
/// absence is what tells the two apart, and a notification's sender has said
/// outright that no answer is wanted.
fn answered(id: Option<serde_json::Value>, result: serde_json::Value) -> Response {
    match id {
        Some(id) => Json(json!({ "jsonrpc": "2.0", "id": id, "result": result })).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

/// `-32601` is JSON-RPC's own number for this, not one this build invented —
/// what a generic client matches on rather than the message beside it.
fn method_not_found(id: Option<serde_json::Value>, method: &str) -> Response {
    let message = Say::of(say::NOT_A_METHOD_HERE)
        .naming("method", method)
        .to_string();

    match id {
        Some(id) => Json(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": message },
        }))
        .into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}

/// Asked with no owner, on purpose. Every tool here reads across a site, so
/// holding `content:view:own` — which reaches what the person made — is not
/// enough for one: it would hand back everybody's.
fn may(caller: &Caller, needs: Needs) -> Result<authz::Permit> {
    caller.may(needs, None)
}

async fn recorded(
    state: &AppState,
    caller: &Caller,
    action: &str,
    tool: &str,
) -> Result<crate::kernel::audit::Receipt> {
    let mut conn = state.db.tenant(caller.tenant()).await?;

    let receipt = audit::record_raw(
        &mut conn,
        Actor::of(caller),
        action,
        "mcp",
        Some(tool),
        &json!({}),
    )
    .await?;

    conn.commit().await?;

    Ok(receipt)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every tool consumes a grant from the same matrix the panel uses. A tool
    /// that asked for nothing would be a way around every check there is.
    #[test]
    fn every_tool_asks_for_something() {
        for tool in tools() {
            let grant = tool.needs.grant();

            assert!(
                authz::every_grant().contains(&grant),
                "{} asks for {grant}, which is not a grant",
                tool.name
            );
        }
    }

    #[test]
    fn no_two_tools_answer_to_one_name() {
        let mut names: Vec<&str> = tools().iter().map(|tool| tool.name).collect();
        let count = names.len();

        names.sort_unstable();
        names.dedup();

        assert_eq!(names.len(), count);
    }
}
