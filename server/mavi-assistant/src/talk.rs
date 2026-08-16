//! The envelope: what an assistant sends, and what goes back.
//!
//! JSON-RPC, which is every client's rather than this installation's. So it is
//! read whole rather than trimmed to the three methods answered today: a field
//! a later version of the protocol adds is not this caller's mistake.
//!
//! **No `id` means no answer is wanted.** That is JSON-RPC's own way of saying
//! "this is a notification", and it decides more than it looks like: a method
//! this build has never heard of gets an error naming it when somebody asked,
//! and silence when nobody did.

use serde_json::{Value, json};

/// What an assistant asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Asked {
    /// Who is answering and what protocol they speak.
    Introduce,
    /// What can be done here.
    WhatIsThere,
    /// Do one of them.
    Use { called: String, arguments: Value },
    /// Something this build does not serve. Carried rather than dropped,
    /// because a client that asked deserves to be told which method it was.
    NotServed(String),
}

/// What came in, if it is anything.
///
/// The `id` comes back separately from what was asked, because it decides
/// whether there is an answer at all rather than what is in one.
#[must_use]
pub fn what_was_asked(body: &Value) -> (Option<Value>, Asked) {
    // `id: null` is read the same as no id. A real client never sends null
    // for something it wants answered, and keeping the two apart would be a
    // second `Option` that nothing ever branches on differently.
    let id = body.get("id").filter(|id| !id.is_null()).cloned();

    let method = body
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();

    let asked = match method {
        "initialize" => Asked::Introduce,
        "tools/list" => Asked::WhatIsThere,
        "tools/call" => {
            let params = body.get("params");

            Asked::Use {
                called: params
                    .and_then(|params| params.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                arguments: params
                    .and_then(|params| params.get("arguments"))
                    .cloned()
                    .unwrap_or_else(|| json!({})),
            }
        }
        other => Asked::NotServed(other.to_owned()),
    };

    (id, asked)
}

/// What an answer looks like going back, or nothing where none was wanted.
pub type Answer = Option<Value>;

/// An answer to something that was asked.
#[must_use]
pub fn answered(id: Option<&Value>, result: Value) -> Answer {
    id.map(|id| json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

/// `-32601` is JSON-RPC's own number for a method that is not served — what a
/// generic client matches on, rather than the sentence beside it.
#[must_use]
pub fn not_a_method(id: Option<&Value>, method: &str) -> Answer {
    id.map(|id| {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32601,
                "message": format!("nothing here answers to {method}"),
            },
        })
    })
}

/// A tool that refused, said the way this protocol says it.
///
/// **Not** a JSON-RPC error. A tool that refused did its job — the model is
/// meant to read what it said and try something else, and a protocol-level
/// error is for the client rather than for the model. `isError` is what tells
/// the two apart, and putting a refusal in the wrong one is how an assistant
/// stops being able to recover from "that address is taken".
#[must_use]
pub fn refused(said: &Value) -> Value {
    json!({
        "isError": true,
        "content": [{
            "type": "text",
            "text": said.to_string(),
        }],
    })
}

/// What a tool answered, said the way this protocol says it.
///
/// The whole answer as text, and the same answer beside it as it was. A model
/// reads the first; anything built on this reads the second rather than
/// parsing a sentence back apart.
#[must_use]
pub fn came_back(what: Value) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": what.to_string(),
        }],
        "structuredContent": what,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_methods_this_serves_are_read_and_everything_else_is_named() {
        let (id, asked) =
            what_was_asked(&json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" }));

        assert_eq!(id, Some(json!(1)));
        assert_eq!(asked, Asked::Introduce);

        let (_, asked) = what_was_asked(&json!({ "method": "tools/list" }));
        assert_eq!(asked, Asked::WhatIsThere);

        let (_, asked) = what_was_asked(&json!({ "method": "resources/list" }));
        assert_eq!(asked, Asked::NotServed("resources/list".to_owned()));
    }

    #[test]
    fn a_call_carries_its_name_and_whatever_was_sent_with_it() {
        let (_, asked) = what_was_asked(&json!({
            "method": "tools/call",
            "params": { "name": "writings_list", "arguments": { "how_many": 5 } },
        }));

        assert_eq!(
            asked,
            Asked::Use {
                called: "writings_list".to_owned(),
                arguments: json!({ "how_many": 5 }),
            }
        );

        // No arguments is an empty object rather than nothing: an endpoint
        // that takes only optional things is called with none of them.
        let (_, asked) = what_was_asked(&json!({
            "method": "tools/call",
            "params": { "name": "writings_list" },
        }));

        assert_eq!(
            asked,
            Asked::Use {
                called: "writings_list".to_owned(),
                arguments: json!({}),
            }
        );
    }

    #[test]
    fn nobody_who_did_not_ask_is_answered() {
        // A notification. Its sender said outright that no answer is wanted,
        // and telling one it got a method wrong is answering it anyway.
        assert_eq!(answered(None, json!({})), None);
        assert_eq!(not_a_method(None, "resources/list"), None);

        // And `id: null` is the same as none, because that is what a client
        // sending it means.
        let (id, _) = what_was_asked(&json!({ "id": null, "method": "tools/list" }));
        assert_eq!(id, None);
    }

    #[test]
    fn a_tool_that_refused_is_not_the_protocol_refusing() {
        // The difference decides whether a model can recover. A refusal it can
        // read and act on must not arrive as a transport error.
        let refusal = refused(&json!({ "key": "something_else_answers_at_that_address" }));

        assert_eq!(refusal["isError"], true);
        assert!(refusal.get("code").is_none());

        let error = not_a_method(Some(&json!(1)), "resources/list").expect("an answer");
        assert_eq!(error["error"]["code"], -32601);
    }

    #[test]
    fn what_came_back_is_readable_and_still_itself() {
        let answer = came_back(json!({ "slug": "hello" }));

        assert_eq!(answer["structuredContent"]["slug"], "hello");
        assert_eq!(answer["content"][0]["type"], "text");
    }
}
