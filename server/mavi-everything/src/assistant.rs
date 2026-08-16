//! The one door an assistant comes through.
//!
//! Everything an assistant can do here is an endpoint that already exists,
//! reached through the same guard, answering with the same refusals and
//! leaving the same receipts. There is no list of tools, no second query, and
//! no second idea of who may do what — which is what makes "forbidden in the
//! panel, allowed over here" impossible rather than unlikely.
//!
//! [`mavi_assistant`] decides everything about the protocol and touches
//! nothing. This is where the two meet: a name becomes an endpoint, arguments
//! become the pieces a call is made of, and the answer becomes what the
//! protocol says an answer looks like.

use std::collections::BTreeMap;
use std::sync::Arc;

use mavi_api::{Answers, Endpoint, Method, Who};
use mavi_core::error::{Code, Error, Result};
use mavi_core::say::Say;
use mavi_http::{Answered, Caller};
use mavi_serve::{Asked, Door, Handler, Refusal, Site};
use serde_json::{Value, json};

pub const THERE_IS_NO_TOOL_LIKE_THAT: &str = "there_is_no_tool_like_that";

/// Where an assistant talks to this installation.
///
/// One address for all of it, because that is what the protocol is: an
/// envelope carrying which method, over one connection.
#[must_use]
pub fn endpoint() -> Endpoint {
    Endpoint {
        method: Method::Post,
        path: "/api/assistant",
        named: "assistant.talk",
        about: "What an assistant can do here, and doing it. Speaks MCP over JSON-RPC.",
        who: Who::AnAccount,
        parameters: Vec::new(),
        takes: Some("AssistantAsked"),
        answers: Answers::With("AssistantAnswer"),
        refuses: &[Code::NotFound],
        // A single `POST` carrying a protocol has reads under it. What is
        // recorded is what the tool did, by the tool's own endpoint and its
        // own rule — asking the verb is how listing an assistant's tools came
        // to be written down as a change to the site.
        changes: false,
    }
}

/// Mounts the door.
///
/// **After everything else**, and that is the whole of the arrangement: what
/// an assistant can reach is what was mounted before this line, so the door
/// is not among them and cannot be asked to call itself.
#[must_use]
pub fn mounted(site: Site) -> Site {
    let reachable = Arc::new(site.by_name());

    let handler: Handler = Arc::new(move |asked: Asked| {
        let reachable = Arc::clone(&reachable);

        Box::pin(async move { talked(&reachable, asked).await })
    });

    site.mount(endpoint(), None, handler)
}

/// One envelope in, one answer out.
async fn talked(reachable: &BTreeMap<&'static str, Door>, asked: Asked) -> Result<Answered<Value>> {
    let (id, what) = mavi_assistant::what_was_asked(&asked.body);

    let answer = match what {
        mavi_assistant::Asked::Introduce => mavi_assistant::answered(id.as_ref(), &introduced()),
        mavi_assistant::Asked::WhatIsThere => {
            mavi_assistant::answered(id.as_ref(), &what_is_there(reachable, &asked.caller))
        }
        mavi_assistant::Asked::Use { called, arguments } => {
            let used = used(reachable, &asked.caller, &called, &arguments).await?;

            mavi_assistant::answered(id.as_ref(), &used)
        }
        mavi_assistant::Asked::NotServed(method) => {
            mavi_assistant::not_a_method(id.as_ref(), &method)
        }
    };

    // Nothing where nobody asked. A notification's sender said outright that
    // no answer is wanted, and the endpoint answers `null` rather than
    // inventing one.
    Ok(Answered::Read(answer.unwrap_or(Value::Null)))
}

fn introduced() -> Value {
    json!({
        "protocolVersion": mavi_assistant::PROTOCOL,
        "capabilities": { "tools": {} },
        // What this software is, not which crate answered. An assistant shows
        // this to whoever is using it.
        "serverInfo": {
            "name": "mavi",
            "version": env!("CARGO_PKG_VERSION"),
        },
    })
}

/// What this caller can do, not what exists.
///
/// A tool somebody cannot reach is not listed — the same rule the panel's own
/// menu follows. The listing is a courtesy; the guard below is what actually
/// stops it, and a tool that was listed by mistake still refuses.
fn what_is_there(reachable: &BTreeMap<&'static str, Door>, caller: &Caller) -> Value {
    let tools: Vec<Value> = reachable
        .values()
        .filter(|door| may(door, caller).is_ok())
        .map(|door| {
            json!({
                "name": mavi_assistant::named(&door.endpoint),
                "description": door.endpoint.about,
                "inputSchema": mavi_assistant::takes(&door.endpoint),
            })
        })
        .collect();

    json!({ "tools": tools })
}

/// Whether this caller could reach this endpoint at all.
///
/// Asked with no owner, on purpose. An `:own` grant reaches what somebody made
/// themselves, and a listing is a question about nobody in particular — so
/// holding one is not enough to be shown a tool that would answer about
/// everybody.
fn may(door: &Door, caller: &Caller) -> Result<()> {
    mavi_http::admit(caller, &door.endpoint, door.needs, None).map(|_| ())
}

/// One tool, used.
async fn used(
    reachable: &BTreeMap<&'static str, Door>,
    caller: &Caller,
    called: &str,
    arguments: &Value,
) -> Result<Value> {
    let Some(door) = reachable
        .values()
        .find(|door| mavi_assistant::named(&door.endpoint) == called)
    else {
        return Err(Error::not_found(
            Say::of(THERE_IS_NO_TOOL_LIKE_THAT).with("tool", &called),
        ));
    };

    let (path, query, body) = mavi_assistant::pieces(&door.endpoint, arguments);

    let body = if body.is_null() {
        Vec::new()
    } else {
        body.to_string().into_bytes()
    };

    // The same call a request makes: the guard, the handler, and the rule that
    // a change leaves a record. Nothing here decides who may do it.
    let went = door
        .call(
            caller.clone(),
            path.into_iter().collect(),
            (!query.is_empty()).then_some(query.as_str()),
            &body,
        )
        .await;

    Ok(match went {
        Ok(what) => mavi_assistant::came_back(&what),
        // A tool that refused did its job. The model is meant to read what it
        // said and try something else, so it comes back as a tool result
        // rather than as the protocol failing — which is what a client, not a
        // model, is told about.
        Err(why) => mavi_assistant::refused(&said(&why)),
    })
}

/// A refusal, in the one shape every refusal comes back in.
///
/// The same value a request would have been given, not a second rendering of
/// it: an assistant that has to read refusals differently from every other
/// client is one more place for the two to drift.
fn said(why: &Error) -> Value {
    let refusal = why.said().map_or_else(
        || Refusal::of(&Say::of("something_went_wrong_here")),
        Refusal::of,
    );

    serde_json::to_value(refusal).unwrap_or_else(|_| json!({ "key": "something_went_wrong_here" }))
}
