//! The API reads as one API.
//!
//! Everything here is asked of the description the endpoints already produce,
//! so a new endpoint that does not fit is caught by a test rather than by
//! whoever has to write against it.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

fn described() -> Value {
    serde_json::to_value(mavi::openapi()).expect("the description")
}

fn paths() -> BTreeMap<String, Value> {
    described()
        .get("paths")
        .and_then(Value::as_object)
        .expect("paths")
        .clone()
        .into_iter()
        .collect()
}

fn answer(operation: &Value) -> Option<Value> {
    operation
        .get("responses")
        .and_then(Value::as_object)?
        .iter()
        .filter(|(status, _)| status.starts_with('2'))
        .find_map(|(_, answer)| answer.pointer("/content/application~1json/schema").cloned())
}

/// A collection answered as a bare list is a listing with no limit on it, and
/// the ones that are allowed to be are the ones whose size is not a site's to
/// grow. Each is named here, which is how it stays a short list.
const NOT_A_PAGE: [&str; 13] = [
    // Fixed by the code: what a site can plug into, and what a role can carry.
    "/api/plugins",
    // Drawn by a sign-in screen before anybody is signed in; a handful at most.
    "/api/auth/oauth",
    // One site's languages, its own trash, and what is wrong with its pages —
    // all bounded by a site's own hand and read whole by a screen that shows
    // them whole.
    "/api/languages",
    "/api/posts/{id}/issues",
    // A theme's files and a site's addresses: both read whole by the screen
    // that shows them whole, one as a tree and one as a short list.
    "/api/design/files",
    "/api/console/sites/{id}/domains",
    "/api/domains",
    "/api/permissions/me",
    // What one person is on. A site sells courses by the handful, not by the
    // thousand, and the screen shows all of somebody's beside their name.
    "/api/students/{id}/enrolments",
    // What a site's own roles are: a handful, made by hand, and shown together
    // because a screen that assigns one shows them all.
    "/api/roles",
    // The credentials a site has kept for its flows: named by hand, a few at
    // most, and shown together because a flow is written against all of them.
    "/api/flows/credentials",
    // The kinds of thing a site has declared: a handful, and a screen that
    // writes a post needs all of them to draw its form.
    "/api/content-types",
    // The letters this machine sends: a handful, fixed by the code, and shown
    // together because a screen that changes one shows them all.
    "/api/mail/letters",
];

#[test]
fn everything_that_lists_answers_one_shape() {
    for (path, item) in paths() {
        let Some(operation) = item.get("get") else {
            continue;
        };

        let Some(schema) = answer(operation) else {
            continue;
        };

        let listed = schema
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|kind| kind == "array");

        if !listed {
            continue;
        }

        assert!(
            NOT_A_PAGE.contains(&path.as_str()),
            "GET {path} answers a bare list; page it, or say here why its size \
             is not a site's to grow"
        );
    }
}

#[test]
fn nothing_named_as_a_page_answers_something_else() {
    for (path, item) in paths() {
        let Some(schema) = item.get("get").and_then(answer) else {
            continue;
        };

        let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
            continue;
        };

        if !properties.contains_key("items") {
            continue;
        }

        assert!(
            properties.contains_key("next"),
            "GET {path} answers something with items in it and no way to ask \
             for the rest"
        );
    }
}

/// What can be written can be read back.
///
/// A collection that takes a POST and answers nothing to a GET is a place
/// things go and are not seen again. Asked of collections rather than of
/// everything: a POST after a record — `/api/orders/{id}/refund` — is an act on
/// that record, and what it did is read by reading the record.
#[test]
fn everything_that_can_be_written_can_be_read() {
    const DOING_RATHER_THAN_KEEPING: [&str; 19] = [
        // Signing in and out, taking a payment, being told about one, leaving
        // a mailing list: each writes something, none of them keeps a
        // collection anybody would list.
        "/api/auth/session",
        "/api/auth/reset",
        "/api/auth/password",
        "/api/auth/second-factor",
        "/api/auth/second-factor/confirm",
        "/api/learn/session",
        "/api/console/session",
        "/api/sites/beacon",
        "/api/sites/checkout",
        "/api/sites/payments/callback",
        "/api/sites/videos/callback",
        "/api/sites/unsubscribe",
        // Handing an assistant a key: what is kept is listed next door, under
        // the keys themselves.
        "/api/assistant/handover",
        // Telling a site what a provider did with a letter it sent, and
        // taking somebody out of every table they are in.
        "/api/mail/events",
        "/api/people/erase",
        "/api/people/export",
        // Doing one thing to many posts. What it did is read by reading the
        // posts, and what it wrote down is in the audit log.
        "/api/posts/actions",
        // Reading a bundle in, here and now. What is kept of one is a transfer.
        "/api/portable/import",
        // The tool surface: one door, and what it does is whatever tool was
        // called through it.
        "/mcp",
    ];

    for (path, item) in paths() {
        let a_collection = !path.contains('{');

        if !a_collection || item.get("post").is_none() || item.get("get").is_some() {
            continue;
        }

        assert!(
            DOING_RATHER_THAN_KEEPING.contains(&path.as_str()),
            "POST {path} writes something nothing can read back"
        );
    }
}

/// Every path is under the same roof, so nothing has to be remembered about
/// where a thing lives.
#[test]
fn everything_is_where_it_would_be_looked_for() {
    const NOT_UNDER_API: [&str; 4] = ["/llms.txt", "/uploads/{id}", "/healthz", "/mcp"];

    for path in paths().keys() {
        assert!(
            path.starts_with("/api/") || NOT_UNDER_API.contains(&path.as_str()),
            "{path} is not where anybody would look for it"
        );
    }
}

/// A filter is a field on a typed struct, bound as a parameter. Where SQL is
/// built by putting something into a string, what goes in is a name the code
/// chose — a constant, or a row of a registry — and never a word somebody sent.
#[test]
fn nothing_builds_a_query_out_of_what_somebody_sent() {
    for file in rust_files(std::path::Path::new("src")) {
        let source = std::fs::read_to_string(&file).expect("a source file");
        let walked = walked_over(&source);

        for (number, block) in source.split("format!(").enumerate().skip(1) {
            let template = block
                .trim_start()
                .strip_prefix('"')
                .and_then(|rest| rest.split_once('"'))
                .map_or("", |(template, _)| template);

            let sql = ["select ", "update ", "insert into ", "delete from "]
                .iter()
                .any(|word| template.contains(word));

            if !sql {
                continue;
            }

            for put_in in placeholders(template) {
                let chosen_here = put_in.chars().all(|c| c.is_ascii_uppercase() || c == '_')
                    || walked.contains(&put_in);

                assert!(
                    !put_in.is_empty() && chosen_here,
                    "{}: the {number}th format! puts {put_in:?} into SQL; \
                     what goes into a query is a name this code chose — a constant, \
                     or something it walked a constant list of",
                    file.display(),
                );
            }
        }
    }
}

/// Names bound by walking something this code wrote down: a registry, or a
/// list written out in the loop itself. Nothing a caller sends is ever one of
/// these, which is what makes them safe to put into a query.
fn walked_over(source: &str) -> Vec<String> {
    let mut names = Vec::new();

    for line in source.lines() {
        let Some(rest) = line.trim().strip_prefix("for ") else {
            continue;
        };

        let Some((bound, over)) = rest.split_once(" in ") else {
            continue;
        };

        let from_a_list = over.trim_start().starts_with('[')
            || over
                .trim_start()
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_uppercase());

        if !from_a_list {
            continue;
        }

        names.extend(
            bound
                .trim_matches(|c: char| c == '(' || c == ')' || c == '&')
                .split(',')
                .map(|name| name.trim().trim_start_matches('&').to_owned())
                .filter(|name| !name.is_empty()),
        );
    }

    names
}

/// The names inside a template's braces. A `format!` that puts something into
/// SQL says which name it is putting there, so this is the whole of what to
/// look at: a positional `{}` comes back empty and is refused.
fn placeholders(template: &str) -> Vec<String> {
    let mut named = Vec::new();
    let mut left = template;

    while let Some(open) = left.find('{') {
        let after = &left[open + 1..];
        let Some(close) = after.find('}') else { break };

        named.push(
            after[..close]
                .split('=')
                .next()
                .unwrap_or_default()
                .trim()
                .to_owned(),
        );
        left = &after[close + 1..];
    }

    named
}

/// Two hand-written copies of the same shape disagree the first time one of
/// them changes. This is the other copy, written from the description.
// The panel is written against the whole API; a smaller build is not what it
// is generated from.
#[test]
fn the_panel_s_types_are_what_the_api_says_they_are() {
    let written = mavi::kernel::typescript::of(&mavi::openapi());
    let at = std::path::Path::new("types/mavicms.ts");

    if std::env::var("UPDATE_SNAPSHOTS").is_ok() {
        std::fs::create_dir_all("types").expect("somewhere to write");
        std::fs::write(at, &written).expect("the types");
        return;
    }

    let on_disk = std::fs::read_to_string(at).unwrap_or_default();

    assert_eq!(
        on_disk, written,
        "the panel's types are not what the API says; \
         UPDATE_SNAPSHOTS=1 cargo test --test api writes them"
    );
}

#[test]
fn every_shape_the_api_names_is_written_down() {
    let types = std::fs::read_to_string("types/mavicms.ts").expect("the types");

    let named: BTreeSet<String> = described()
        .pointer("/components/schemas")
        .and_then(Value::as_object)
        .expect("schemas")
        .keys()
        .cloned()
        .collect();

    for name in named {
        // `Page` is written once, as the generic it is: the description names
        // one concrete page per listing and they are all the same shape.
        let written = types.contains(&format!("export interface {name} "))
            || types.contains(&format!("export type {name} ="))
            || types.contains(&format!("export interface {name}<T> "));

        assert!(written, "{name} is in the API and not in the types");
    }
}

fn rust_files(at: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();

    for entry in std::fs::read_dir(at).expect("a directory") {
        let path = entry.expect("an entry").path();

        if path.is_dir() {
            found.extend(rust_files(&path));
        } else if path.extension().is_some_and(|kind| kind == "rs") {
            found.push(path);
        }
    }

    found.sort();
    found
}

/// Two types that answer to one name leave only the last one in the
/// description, and a client generated from it is quietly wrong about the
/// other. Three types here were called `Credentials`, and the sign-in the panel
/// uses lost the field that asks for a second factor.
#[test]
fn no_two_shapes_answer_to_one_name() {
    let both = mavi::kernel::openapi::clashes(&mavi::endpoints());

    assert!(
        both.is_empty(),
        "these names each describe two different shapes: {both:?}"
    );
}

/// A handler that answers a page and a description that says it answers the
/// thing is a client generated against a shape that does not exist. The trash
/// said `Thrown` and answered `Page<Thrown>` for a month.
#[test]
fn a_handler_that_answers_a_page_says_so() {
    for file in rust_files(std::path::Path::new("src")) {
        let source = std::fs::read_to_string(&file).expect("a source file");

        // Every handler that answers a page, by the name it was declared with.
        for answering in source.split("-> Result<Json<Page<").skip(1) {
            let before = source
                .split_once(answering)
                .map(|(before, _)| before)
                .unwrap_or_default();

            let Some(name) = before
                .rsplit_once("async fn ")
                .and_then(|(_, after)| after.split('(').next())
                .map(str::to_owned)
            else {
                continue;
            };

            let declared = source
                .split(&format!("\n            {name},\n"))
                .nth(1)
                .unwrap_or_default();

            let says = declared
                .split(".gives::<")
                .nth(1)
                .and_then(|after| after.split('>').next())
                .unwrap_or_default();

            assert!(
                says.starts_with("Page<") || says.starts_with("Listing<") || says.is_empty(),
                "{}: {name} answers a page and says it answers {says}",
                file.display(),
            );
        }
    }
}
