//! What actually happens to a request.
//!
//! No socket and no database: a router is a function from a request to an
//! answer, and every rule this crate claims to enforce can be asked of it
//! directly. What is being checked is not axum — it is that the guard, the
//! audit rule and the shape of a refusal are on the path a real request takes,
//! rather than on one somebody remembered to call.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use mavi_api::{Answers, Code, Endpoint, Is, Method, Parameter, Who};
use mavi_core::grant::{Access, Grants, Needs};
use mavi_http::{Answered, Caller};
use mavi_serve::{Asked, Handler, Site};
use serde_json::{Value, json};
use tower::ServiceExt;

fn nobody() -> mavi_serve::WhoIsAsking {
    Arc::new(|_| Caller::Nobody)
}

fn an_editor() -> mavi_serve::WhoIsAsking {
    Arc::new(|headers| {
        if headers.contains_key("authorization") {
            Caller::AnAccount {
                id: "an-editor".to_owned(),
                grants: Grants::of(["content:write".to_owned(), "content:view".to_owned()]),
            }
        } else {
            Caller::Nobody
        }
    })
}

fn answering(
    what: impl Fn(Asked) -> mavi_core::error::Result<Answered<Value>> + Send + Sync + 'static,
) -> Handler {
    Arc::new(move |asked| {
        let answered = what(asked);

        Box::pin(async move { answered })
    })
}

fn reading() -> Endpoint {
    Endpoint {
        method: Method::Get,
        path: "/api/writings/{id}",
        named: "writings.read",
        about: "One writing.",
        who: Who::AnAccount,
        parameters: vec![Parameter::path("id", Is::Id, "Which writing.")],
        takes: None,
        answers: Answers::With("Writing"),
        refuses: &[Code::NotFound],
        changes: false,
    }
}

fn changing() -> Endpoint {
    Endpoint {
        method: Method::Post,
        path: "/api/writings",
        named: "writings.make",
        about: "Writes one.",
        who: Who::AnAccount,
        parameters: Vec::new(),
        takes: Some("NewWriting"),
        answers: Answers::Made("Writing"),
        refuses: &[],
        changes: true,
    }
}

async fn asked(site: Site, request: Request<Body>) -> (StatusCode, Value) {
    let answer = site
        .into_router()
        .oneshot(request)
        .await
        .expect("an answer");

    let status = answer.status();
    let body = axum::body::to_bytes(answer.into_body(), 64 * 1024)
        .await
        .expect("a body");

    let body = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).unwrap_or(Value::Null)
    };

    (status, body)
}

fn get(path: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .body(Body::empty())
        .expect("a request")
}

fn signed_in(request: Request<Body>) -> Request<Body> {
    let (mut parts, body) = request.into_parts();
    parts.headers.insert(
        "authorization",
        "Bearer whatever".parse().expect("a header"),
    );

    Request::from_parts(parts, body)
}

#[tokio::test]
async fn nobody_is_turned_away_before_the_handler_runs() {
    // Not "the handler checks": the handler is never reached. A handler that
    // has to remember to ask is a handler that one day does not.
    let ran = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let watching = Arc::clone(&ran);

    let site = Site::new(nobody()).mount(
        reading(),
        Some(Needs::new("content", Access::View)),
        answering(move |_| {
            watching.store(true, std::sync::atomic::Ordering::SeqCst);

            Ok(Answered::Read(json!({})))
        }),
    );

    let (status, body) = asked(
        site,
        get("/api/writings/01930000-0000-7000-8000-00000000000a"),
    )
    .await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["key"], "you_are_not_signed_in");
    assert!(
        !ran.load(std::sync::atomic::Ordering::SeqCst),
        "the handler ran for somebody who was not let in"
    );
}

#[tokio::test]
async fn what_the_path_carries_reaches_the_handler_by_name() {
    let site = Site::new(an_editor()).mount(
        reading(),
        Some(Needs::new("content", Access::View)),
        answering(|asked| {
            Ok(Answered::Read(json!({
                "id": asked.path.get("id"),
                "limit": asked.query.get("limit"),
            })))
        }),
    );

    let (status, body) = asked(
        site,
        signed_in(get(
            "/api/writings/01930000-0000-7000-8000-00000000000a?limit=25",
        )),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], "01930000-0000-7000-8000-00000000000a");
    assert_eq!(body["limit"], "25");
}

#[tokio::test]
async fn a_change_that_left_no_record_does_not_answer() {
    // The rule the whole audit gate exists for, asked of the path a request
    // actually takes rather than of the function that implements it.
    let site = Site::new(an_editor()).mount(
        changing(),
        Some(Needs::new("content", Access::Write)),
        answering(|_| Ok(Answered::Read(json!({"id": "made"})))),
    );

    let request = Request::builder()
        .method("POST")
        .uri("/api/writings")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .expect("a request");

    let (status, body) = asked(site, signed_in(request)).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    // And it says nothing about what went wrong: the caller cannot act on it.
    assert_eq!(body["key"], "something_went_wrong_here");
}

#[tokio::test]
async fn a_change_with_a_receipt_answers_the_status_it_declared() {
    let site = Site::new(an_editor()).mount(
        changing(),
        Some(Needs::new("content", Access::Write)),
        answering(|_| {
            Ok(Answered::Changed(
                json!({"id": "made"}),
                mavi_audit::Receipt::pretend(),
            ))
        }),
    );

    let request = Request::builder()
        .method("POST")
        .uri("/api/writings")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .expect("a request");

    let (status, body) = asked(site, signed_in(request)).await;

    // 201, because that is what the endpoint said `Made` means — not 200,
    // which is what sixty-seven operations in the old description claimed
    // while answering something else.
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["id"], "made");
}

#[tokio::test]
async fn holding_the_wrong_grant_is_not_holding_it() {
    let site = Site::new(an_editor()).mount(
        reading(),
        Some(Needs::new("shop", Access::View)),
        answering(|_| Ok(Answered::Read(json!({})))),
    );

    let (status, body) = asked(
        site,
        signed_in(get("/api/writings/01930000-0000-7000-8000-00000000000a")),
    )
    .await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(body["key"].is_string());
}

#[tokio::test]
async fn nothing_answers_where_nothing_is_mounted_and_it_says_so_properly() {
    // The parts of a router nobody wrote still answer, and they answer in the
    // shape the description promises — a client that branches on `key` does
    // not have to special-case the 404 it gets for a path it mistyped.
    let site = Site::new(an_editor()).mount(
        reading(),
        None,
        answering(|_| Ok(Answered::Read(json!({})))),
    );

    let (status, body) = asked(site, signed_in(get("/api/nothing-like-this"))).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["key"], "nothing_answers_there");
    assert!(body["said"].is_string());
}

#[tokio::test]
async fn a_body_that_is_not_json_is_refused_before_the_handler() {
    let site = Site::new(an_editor()).mount(
        changing(),
        None,
        answering(|_| panic!("the handler was reached with a body nothing could read")),
    );

    let request = Request::builder()
        .method("POST")
        .uri("/api/writings")
        .body(Body::from("{not json"))
        .expect("a request");

    let (status, body) = asked(site, signed_in(request)).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["key"], "that_is_not_something_this_understands");
}

#[tokio::test]
async fn two_verbs_on_one_path_are_two_endpoints() {
    let site = Site::new(an_editor())
        .mount(
            changing(),
            None,
            answering(|_| {
                Ok(Answered::Changed(
                    json!({"made": true}),
                    mavi_audit::Receipt::pretend(),
                ))
            }),
        )
        .mount(
            Endpoint {
                method: Method::Get,
                path: "/api/writings",
                named: "writings.list",
                about: "What the site has written.",
                who: Who::AnAccount,
                parameters: Vec::new(),
                takes: None,
                answers: Answers::With("WritingPage"),
                refuses: &[],
                changes: false,
            },
            None,
            answering(|_| Ok(Answered::Read(json!({"listed": true})))),
        );

    let router = site.into_router();

    let listed = router
        .clone()
        .oneshot(signed_in(get("/api/writings")))
        .await
        .expect("an answer");
    assert_eq!(listed.status(), StatusCode::OK);

    let made = router
        .oneshot(signed_in(
            Request::builder()
                .method("POST")
                .uri("/api/writings")
                .body(Body::from("{}"))
                .expect("a request"),
        ))
        .await
        .expect("an answer");
    assert_eq!(made.status(), StatusCode::CREATED);
}

#[test]
fn what_is_described_and_not_mounted_can_be_counted() {
    // The whole point of mounting by handing over the description: a feature
    // that is written, tested, and reachable from nowhere is a feature that
    // does not exist, and this is how anybody finds out.
    let described = mavi_api::Api::of(vec![reading(), changing()]);

    let site = Site::new(nobody()).mount(
        reading(),
        None,
        answering(|_| Ok(Answered::Read(Value::Null))),
    );

    assert_eq!(site.not_reachable(&described), vec!["writings.make"]);
    assert_eq!(site.reachable(), vec!["writings.read"]);
}
