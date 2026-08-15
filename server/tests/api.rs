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
const NOT_A_PAGE: [&str; 11] = [
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
    "/api/domains",
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
    const DOING_RATHER_THAN_KEEPING: [&str; 17] = [
        // Signing in and out, taking a payment, being told about one, leaving
        // a mailing list: each writes something, none of them keeps a
        // collection anybody would list.
        "/api/auth/session",
        "/api/auth/reset",
        "/api/auth/password",
        "/api/auth/email-proof",
        "/api/auth/second-factor/confirm",
        "/api/learn/session",
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
    const NOT_UNDER_API: [&str; 3] = ["/llms.txt", "/uploads/{id}", "/mcp"];

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

/// A query that pages: found by the one thing every one of them does — bind
/// the extra row `Query::fetch` asks for as its `limit`. What matters about
/// it is the SQL itself and the closure that turns its last row into the
/// cursor handed back.
struct Paginated {
    sql: String,
    closure_field: Option<String>,
}

fn paginated_queries(source: &str) -> Vec<Paginated> {
    let mut found = Vec::new();
    let mut from = 0usize;

    while let Some(at_rel) = source[from..].find("sqlx::query") {
        let mut at = from + at_rel + "sqlx::query".len();

        if source[at..].starts_with("_as") {
            at += "_as".len();
        }

        if !source[at..].starts_with('(') {
            from = at;
            continue;
        }

        let body_start = at + 1;
        from = body_start;

        let window_end = (body_start + 4000).min(source.len());
        let window = &source[body_start..window_end];

        let Some(fetch_at) = [".fetch_all(", ".fetch_one(", ".fetch_optional("]
            .iter()
            .filter_map(|needle| window.find(needle))
            .min()
        else {
            continue;
        };

        let chain = &window[..fetch_at];
        let binds = bind_expressions(chain);

        let is_paged = binds
            .iter()
            .any(|bind| bind.trim_end().ends_with(".fetch()"));

        if !is_paged {
            continue;
        }

        let Some(quote_at) = window.find('"') else {
            continue;
        };

        if quote_at > 200 {
            continue;
        }

        let Some(quote_end) = window[quote_at + 1..].find('"') else {
            continue;
        };

        let sql = window[quote_at + 1..quote_at + 1 + quote_end].to_owned();

        let after =
            &source[body_start + fetch_at..(body_start + fetch_at + 3000).min(source.len())];
        let closure_field = ["Page::build(", "Listing::build("]
            .iter()
            .find_map(|needle| after.find(needle).map(|at| at + needle.len()))
            .and_then(|at| closure_field_of(&after[at..(at + 700).min(after.len())]));

        found.push(Paginated { sql, closure_field });
    }

    found
}

/// Every `.bind(...)` in a chain, in order — so the Nth one is what `$N`
/// binds to. Walked by hand rather than split on `)`, because what is bound
/// is itself sometimes a call with its own parentheses.
fn bind_expressions(chain: &str) -> Vec<&str> {
    let mut binds = Vec::new();
    let bytes = chain.as_bytes();
    let mut from = 0usize;

    while let Some(at_rel) = chain[from..].find(".bind(") {
        let start = from + at_rel + ".bind(".len();
        let mut depth = 1i32;
        let mut at = start;

        while at < bytes.len() && depth > 0 {
            match bytes[at] {
                b'(' => depth += 1,
                b')' => depth -= 1,
                _ => {}
            }
            at += 1;
        }

        binds.push(&chain[start..at.saturating_sub(1)]);
        from = at.max(start + 1);
    }

    binds
}

/// What a closure like `|issue| issue.written_at.to_rfc3339()` hands back:
/// the field read off its one argument, whatever is done to it afterwards.
fn closure_field_of(tail: &str) -> Option<String> {
    let bar_at = tail.find('|')?;
    let rest = &tail[bar_at + 1..];
    let end = rest.find('|')?;
    let name = &rest[..end];

    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }

    let body = &rest[end + 1..(end + 1 + 400).min(rest.len())];
    let needle = format!("{name}.");
    let at = body.find(&needle)?;
    let field: String = body[at + needle.len()..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();

    (!field.is_empty()).then_some(field)
}

/// The column a listing is chiefly ordered by: the first name after
/// `order by`, with whatever table it is qualified by dropped.
fn order_by_primary(sql: &str) -> Option<String> {
    let at = sql.find("order by")?;
    let rest = sql[at + "order by".len()..].trim_start();
    let column: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
        .collect();

    if column.is_empty() {
        return None;
    }

    Some(column.rsplit('.').next().unwrap_or(&column).to_owned())
}

/// Whether a column is ever on the sharp end of a `<` or `>` — the shape
/// every cursor filter in this codebase takes, whatever else surrounds it.
fn compared_with_an_edge(sql: &str, column: &str) -> bool {
    let bytes = sql.as_bytes();
    let mut from = 0usize;

    while let Some(at_rel) = sql[from..].find(column) {
        let at = from + at_rel;
        let before_ok =
            at == 0 || !(bytes[at - 1].is_ascii_alphanumeric() || bytes[at - 1] == b'_');
        let after_at = at + column.len();
        let after_ok = after_at == bytes.len()
            || !(bytes[after_at].is_ascii_alphanumeric() || bytes[after_at] == b'_');

        if before_ok && after_ok {
            let mut j = after_at;
            while j < bytes.len() && bytes[j] == b' ' {
                j += 1;
            }
            if j < bytes.len() && (bytes[j] == b'<' || bytes[j] == b'>') {
                return true;
            }

            let mut i = at;
            while i > 0 && bytes[i - 1] == b' ' {
                i -= 1;
            }
            if i > 0 && (bytes[i - 1] == b'<' || bytes[i - 1] == b'>') {
                return true;
            }
        }

        from = at + column.len().max(1);
    }

    false
}

/// What the `select` list of a query calls each column, where it renames one
/// with `as`: a cursor built from the renamed field is really built from
/// whatever is on the other side of it.
fn select_aliases(sql: &str) -> Vec<(String, String)> {
    let Some(select_at) = sql.find("select") else {
        return Vec::new();
    };

    let Some(from_at) = sql[select_at..].find(" from ") else {
        return Vec::new();
    };

    let list = &sql[select_at + "select".len()..select_at + from_at];
    let bytes = list.as_bytes();

    let mut aliases = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;

    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b',' if depth == 0 => {
                if let Some(pair) = column_alias(&list[start..i]) {
                    aliases.push(pair);
                }
                start = i + 1;
            }
            _ => {}
        }
    }

    if let Some(pair) = column_alias(&list[start..]) {
        aliases.push(pair);
    }

    aliases
}

fn column_alias(part: &str) -> Option<(String, String)> {
    let part = part.trim();
    let lower = part.to_ascii_lowercase();
    let at = lower.rfind(" as ")?;

    let alias = part[at + " as ".len()..].trim().to_owned();
    let expr = part[..at].trim();
    let base = expr.split("::").next().unwrap_or(expr);
    let base = base.rsplit('.').next().unwrap_or(base).trim().to_owned();

    Some((alias, base))
}

/// A page whose cursor does not match what it orders and filters by either
/// repeats what a client already saw or never shows them the rest: `next`
/// on the mail list's subscribers was the subscriber's id while the list was
/// ordered and filtered by when they joined, and a client that kept asking
/// for it got the newest page for ever. What is wrong with a page was
/// ordered worst first and cursored on when it was written, so a page
/// boundary that fell inside the warnings cut every note off from ever being
/// reached. Measured across every paged listing in this crate, this is
/// exactly those two — which is what makes it worth keeping as a rule rather
/// than a pair of one-off fixes.
#[test]
fn a_paged_listing_cursors_on_what_it_orders_and_filters_by() {
    for file in rust_files(std::path::Path::new("src")) {
        let source = std::fs::read_to_string(&file).expect("a source file");

        for query in paginated_queries(&source) {
            let Some(primary) = order_by_primary(&query.sql) else {
                continue;
            };

            let filtered = compared_with_an_edge(&query.sql, &primary);

            let aliases = select_aliases(&query.sql);
            let cursored_on = query.closure_field.as_ref().map(|field| {
                aliases
                    .iter()
                    .find(|(alias, _)| alias == field)
                    .map_or_else(|| field.clone(), |(_, base)| base.clone())
            });

            let matches = cursored_on.as_ref().is_none_or(|field| *field == primary);

            assert!(
                filtered && matches,
                "{}: a listing ordered by {primary} is filtered by a column \
                 that is not compared with `<` or `>` ({filtered}), or hands \
                 back a cursor built from {cursored_on:?} instead — a client \
                 following `next` either loops for ever or never sees some \
                 rows",
                file.display(),
            );
        }
    }
}
