//! What makes an endpoint reachable.
//!
//! Everywhere else in this workspace an endpoint is a **description**. Here it
//! becomes a route, and the two are the same declaration — a handler is
//! mounted by handing over the `Endpoint` that describes it, so a route that
//! is not described cannot exist and a description with no route is something
//! a test can name.
//!
//! That last one is the point. "Written and tested" does not mean reachable: a
//! function with no route and no caller is a feature that does not exist, and
//! the only way to know is to compare the two lists.
//!
//! Everything a request passes through on the way in is here and is not
//! optional:
//!
//! 1. who is asking, worked out once;
//! 2. [`mavi_http::admit`], which is the only gate — one path, not two;
//! 3. the handler;
//! 4. [`mavi_http::admit::wrote_it_down`], which refuses to answer a change
//!    that left no receipt.

pub mod refusal;

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{RawPathParams, RawQuery, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::MethodRouter;
use mavi_api::{Api, Endpoint, Method};
use mavi_core::error::{Code, Error, Result};
use mavi_core::grant::Needs;
use mavi_core::say::Say;
use mavi_http::{Answered, Caller, admit};
use serde_json::Value;

pub use refusal::Refusal;

/// The name an endpoint uses when what it takes is the bytes themselves.
///
/// One name, in one place, because the router has to know which endpoints do
/// not carry JSON and the endpoint already has to say what it takes.
#[derive(Debug)]
pub struct TheBytes;

impl TheBytes {
    /// The description's own, rather than a second copy of the word. The
    /// question "is every named body described" has to skip exactly this one,
    /// and two spellings of it is that question quietly answering wrongly.
    pub const NAMED: &'static str = mavi_api::THE_BYTES;
}

/// What a handler is given.
///
/// One shape rather than axum's extractors, so that a handler is a plain
/// function of what arrived — and so that what arrived can be built in a test
/// without a socket.
#[derive(Clone, Debug)]
pub struct Asked {
    pub caller: Caller,
    /// The `{holes}` in the path, by the names the endpoint declared.
    pub path: BTreeMap<String, String>,
    pub query: BTreeMap<String, String>,
    /// The body, where the endpoint said it takes one **and it is JSON**.
    /// `Value::Null` otherwise.
    pub body: Value,
    /// The body as it arrived. What an upload is: bytes, whose kind is decided
    /// by reading them rather than by what they were called.
    pub raw: Vec<u8>,
}

type Answer = Pin<Box<dyn Future<Output = Result<Answered<Value>>> + Send>>;

/// What answers one endpoint.
pub type Handler = Arc<dyn Fn(Asked) -> Answer + Send + Sync>;

/// Who is asking, worked out from the request.
///
/// Handed in rather than written here: what a token is and where it is kept is
/// a decision for whatever runs this, and this crate's business is that the
/// answer is asked for exactly once and then carried.
///
/// It answers a future, because working out who is asking means reading a
/// session out of the database in every installation that has one. A version
/// of this that could not wait forced whoever wrote it to reach for a thread
/// and a second runtime — and a pool belongs to the runtime it was made on, so
/// that arrangement does not fail loudly, it simply never finds anybody.
pub type WhoIsAsking =
    Arc<dyn Fn(HeaderMap) -> Pin<Box<dyn Future<Output = Caller> + Send>> + Send + Sync>;

/// One endpoint, its rule, and what answers it.
///
/// Public because a request is not the only way in. Something that answers by
/// **name** rather than by address — an assistant asking to use a tool — has
/// to reach the same handler behind the same rule, and the way to make that
/// certain is for there to be nothing else to reach.
#[derive(Clone)]
pub struct Door {
    pub endpoint: Endpoint,
    pub needs: Option<Needs>,
    pub handler: Handler,
}

impl std::fmt::Debug for Door {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Door")
            .field("endpoint", &self.endpoint.named)
            .field("needs", &self.needs)
            .finish_non_exhaustive()
    }
}

/// Who is asking, as the router carries it.
///
/// The router's state is this and nothing else: what a handler needs comes
/// from the request, and what the guard needs comes from the endpoint it was
/// mounted with.
#[derive(Clone)]
struct Asking(WhoIsAsking);

/// Everything this installation serves.
#[derive(Clone)]
pub struct Site {
    who_is_asking: WhoIsAsking,
    mounted: Vec<Door>,
}

impl std::fmt::Debug for Site {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Site")
            .field("mounted", &self.mounted.len())
            .finish_non_exhaustive()
    }
}

impl Site {
    #[must_use]
    pub fn new(who_is_asking: WhoIsAsking) -> Self {
        Self {
            who_is_asking,
            mounted: Vec::new(),
        }
    }

    /// Mounts one endpoint.
    ///
    /// The description and the route are the same value, so a route nobody
    /// described is not something anybody can write here.
    #[must_use]
    pub fn mount(mut self, endpoint: Endpoint, needs: Option<Needs>, handler: Handler) -> Self {
        self.mounted.push(Door {
            endpoint,
            needs,
            handler,
        });

        self
    }

    /// What is described and not mounted.
    ///
    /// Empty is the only acceptable answer once an installation is finished,
    /// and until then it is the list of what is left — measured rather than
    /// remembered. "Written and tested" does not mean reachable.
    #[must_use]
    pub fn not_reachable(&self, described: &Api) -> Vec<&'static str> {
        described
            .endpoints
            .iter()
            .filter(|endpoint| {
                !self
                    .mounted
                    .iter()
                    .any(|there| there.endpoint.named == endpoint.named)
            })
            .map(|endpoint| endpoint.named)
            .collect()
    }

    /// What is mounted here, by name — the description, the rule and the
    /// handler together.
    ///
    /// What this is for is the one thing that must not grow a second copy of
    /// itself: a door that answers by name reaches these and nothing else, so
    /// "forbidden in the panel, allowed over there" is impossible rather than
    /// unlikely. Whatever is mounted **after** this is asked for is not in it,
    /// which is what keeps such a door from being able to call itself.
    #[must_use]
    pub fn by_name(&self) -> BTreeMap<&'static str, Door> {
        self.mounted
            .iter()
            .map(|door| (door.endpoint.named, door.clone()))
            .collect()
    }

    /// What is mounted here, by name.
    #[must_use]
    pub fn reachable(&self) -> Vec<&'static str> {
        self.mounted
            .iter()
            .map(|there| there.endpoint.named)
            .collect()
    }

    /// What is mounted, as something that can answer requests.
    ///
    /// Everything nobody wrote a route for answers in the same shape as
    /// everything else, because a description that says every operation can
    /// refuse like this is only true if the parts nobody wrote do too.
    pub fn into_router(self) -> Router {
        let asking = Asking(Arc::clone(&self.who_is_asking));

        let mut by_path: BTreeMap<&'static str, Vec<Door>> = BTreeMap::new();

        for one in self.mounted {
            by_path.entry(one.endpoint.path).or_default().push(one);
        }

        let mut router: Router<Asking> = Router::new();

        for (path, here) in by_path {
            let mut methods: Option<MethodRouter<Asking>> = None;

            for one in here {
                let method = one.endpoint.method;
                let carried = Arc::new(one);

                let answering = move |State(Asking(who)): State<Asking>,
                                      params: RawPathParams,
                                      RawQuery(query): RawQuery,
                                      headers: HeaderMap,
                                      body: Bytes| {
                    let carried = Arc::clone(&carried);

                    async move {
                        through(&carried, &who, &params, query.as_deref(), headers, &body).await
                    }
                };

                let route = match method {
                    Method::Get => axum::routing::get(answering),
                    Method::Post => axum::routing::post(answering),
                    Method::Put => axum::routing::put(answering),
                    Method::Patch => axum::routing::patch(answering),
                    Method::Delete => axum::routing::delete(answering),
                };

                methods = Some(methods.map_or(route.clone(), |before| before.merge(route)));
            }

            if let Some(methods) = methods {
                router = router.route(path, methods);
            }
        }

        router
            .fallback(|| async { refusal::nothing_answers_there() })
            .with_state(asking)
    }
}

/// Everything between a request arriving and an answer leaving.
async fn through(
    mounted: &Door,
    who_is_asking: &WhoIsAsking,
    params: &RawPathParams,
    query: Option<&str>,
    headers: HeaderMap,
    body: &[u8],
) -> Response {
    // Once, and then carried. Working out who is asking twice is two answers
    // to one question, and the second one is the one nobody tested.
    let caller = who_is_asking(headers).await;

    let path = params
        .iter()
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect();

    match answered(mounted, caller, path, query, body).await {
        Ok(response) => response,
        Err(error) => refusal::answer(&error),
    }
}

impl Door {
    /// One call, all the way through.
    ///
    /// The gate, the handler, and the rule that a change leaves a record —
    /// in that order, which is the order that matters: somebody who may not
    /// do this is told so before their body is read, so a caller who is both
    /// unauthorised and malformed hears the first thing rather than the
    /// second.
    ///
    /// What arrives is the pieces rather than an [`Asked`], because building
    /// one means knowing whether this endpoint takes JSON or takes the bytes
    /// — and something that could be handed an `Asked` it built itself would
    /// be a second answer to that question.
    pub async fn call(
        &self,
        caller: Caller,
        path: BTreeMap<String, String>,
        query: Option<&str>,
        body: &[u8],
    ) -> Result<Value> {
        // One gate. Not one per way in, which is how the crate this replaces
        // came to have a console whose writes answered before leaving a
        // record.
        admit::admit(&caller, &self.endpoint, self.needs, None)?;

        // Read as JSON where the endpoint says it takes something and that
        // something is not the bytes themselves. An upload is a body too, and
        // asking `serde_json` to read a picture is a refusal nobody can act
        // on.
        let read_as_json = self
            .endpoint
            .takes
            .is_some_and(|takes| takes != TheBytes::NAMED);

        let asked = Asked {
            caller,
            path,
            query: unpicked(query),
            body: if read_as_json {
                read(body)?
            } else {
                Value::Null
            },
            raw: body.to_vec(),
        };

        let answered = (self.handler)(asked).await?;

        // A change that left no record does not answer. Held against what the
        // endpoint said about itself, never against the verb it arrived by.
        admit::wrote_it_down(&self.endpoint, &answered)?;

        Ok(answered.into_inner())
    }
}

async fn answered(
    mounted: &Door,
    caller: Caller,
    path: BTreeMap<String, String>,
    query: Option<&str>,
    body: &[u8],
) -> Result<Response> {
    let what = mounted.call(caller, path, query, body).await?;

    let status = StatusCode::from_u16(mounted.endpoint.answers.status())
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

    Ok(match what {
        Value::Null => status.into_response(),
        what => (status, axum::Json(what)).into_response(),
    })
}

fn read(body: &[u8]) -> Result<Value> {
    if body.is_empty() {
        return Ok(Value::Null);
    }

    serde_json::from_slice(body).map_err(|_| {
        Error::new(
            Code::Invalid,
            Say::of(refusal::THAT_IS_NOT_SOMETHING_THIS_UNDERSTANDS),
        )
    })
}

/// A query string, as pairs. Written out rather than deserialised into a type,
/// because every listing takes a different set and they are all optional.
fn unpicked(query: Option<&str>) -> BTreeMap<String, String> {
    let Some(query) = query else {
        return BTreeMap::new();
    };

    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .filter_map(|pair| {
            let (name, value) = pair.split_once('=')?;

            Some((undone(name), undone(value)))
        })
        .collect()
}

/// Percent-decoding, and `+` for a space, which is what a browser sends.
fn undone(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut at = 0;

    while at < bytes.len() {
        match bytes[at] {
            b'+' => {
                out.push(b' ');
                at += 1;
            }
            b'%' if at + 2 < bytes.len() => {
                let pair = std::str::from_utf8(&bytes[at + 1..at + 3]).unwrap_or_default();

                if let Ok(byte) = u8::from_str_radix(pair, 16) {
                    out.push(byte);
                    at += 3;
                } else {
                    // Not two hex digits, so it is a per cent sign somebody
                    // typed rather than something encoded.
                    out.push(bytes[at]);
                    at += 1;
                }
            }
            byte => {
                out.push(byte);
                at += 1;
            }
        }
    }

    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_string_is_unpicked_the_way_a_browser_writes_one() {
        let query = unpicked(Some("limit=25&after=abc%3D%3D&q=two+words"));

        assert_eq!(query.get("limit").map(String::as_str), Some("25"));
        // A cursor is base64url and ends in padding, which arrives encoded.
        assert_eq!(query.get("after").map(String::as_str), Some("abc=="));
        assert_eq!(query.get("q").map(String::as_str), Some("two words"));
    }

    #[test]
    fn a_query_string_that_is_nonsense_is_nothing_rather_than_a_panic() {
        assert!(unpicked(None).is_empty());
        assert!(unpicked(Some("")).is_empty());
        assert!(unpicked(Some("&&&")).is_empty());
        assert_eq!(
            unpicked(Some("a=%")).get("a").map(String::as_str),
            Some("%")
        );
        assert_eq!(
            unpicked(Some("a=%zz")).get("a").map(String::as_str),
            Some("%zz")
        );
    }

    #[test]
    fn a_body_that_is_not_json_is_a_refusal_a_caller_can_read() {
        let refused = read(b"not json at all").expect_err("a refusal");

        assert_eq!(refused.code(), Code::Invalid);
        assert_eq!(
            refused.said().expect("a sentence").key,
            refusal::THAT_IS_NOT_SOMETHING_THIS_UNDERSTANDS
        );
    }
}
