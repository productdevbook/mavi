//! What an endpoint is.
//!
//! This crate exists because of a measurement. The API it replaces described
//! **177 operations** and, across all of them:
//!
//! - not one parameter, while forty read a query string and eighty-three sat
//!   under a templated path;
//! - not one failure, while every operation could answer at least four;
//! - no way to authenticate, while the server took a bearer token or a cookie;
//! - and sixty-seven answered a status they did not declare.
//!
//! Every one of those is the same mistake: the description was something
//! written *beside* the endpoint rather than something the endpoint had to
//! say. So it was written once, at the start, and never again.
//!
//! Here an endpoint cannot be declared without them. [`Endpoint::describe`]
//! reads only what the endpoint said, and [`Api::holes`] is the test that says
//! what is still missing — measured rather than assumed, so that "the
//! description is complete" is a number rather than a feeling.

pub mod describe;
pub mod shape;
pub mod typescript;

use std::collections::BTreeSet;

use mavi_core::error::Code;

pub use describe::openapi;
pub use shape::{Field, Of, Shape, What};
pub use typescript::typescript;

/// The name an endpoint uses when what it takes is the bytes themselves.
///
/// Said here as well as where the router reads it, because the question "is
/// every named body described" has to know the one name that is deliberately
/// not a body.
pub const THE_BYTES: &str = "TheBytes";

/// How a request arrives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    #[must_use]
    pub const fn lower(self) -> &'static str {
        match self {
            Method::Get => "get",
            Method::Post => "post",
            Method::Put => "put",
            Method::Patch => "patch",
            Method::Delete => "delete",
        }
    }

    /// Whether this is the kind of request that changes something.
    ///
    /// **Not** what decides whether an audit row is required — that is the
    /// endpoint's own [`Endpoint::changes`], because a single `POST` carrying
    /// a protocol has both reads and writes underneath it and the verb cannot
    /// tell them apart. Asking the verb was how listing an assistant's tools
    /// came to be recorded as a change to the site.
    #[must_use]
    pub const fn usually_changes(self) -> bool {
        !matches!(self, Method::Get)
    }
}

/// Where a parameter is carried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum In {
    Path,
    Query,
}

/// What a parameter is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Is {
    Text,
    Number,
    Bool,
    Id,
    Moment,
}

impl Is {
    #[must_use]
    pub const fn json(self) -> &'static str {
        match self {
            Is::Text | Is::Id | Is::Moment => "string",
            Is::Number => "integer",
            Is::Bool => "boolean",
        }
    }

    /// The format a generator uses to pick a type. `None` where JSON's own
    /// name is the whole of it.
    #[must_use]
    pub const fn format(self) -> Option<&'static str> {
        match self {
            Is::Id => Some("uuid"),
            Is::Moment => Some("date-time"),
            _ => None,
        }
    }
}

/// One parameter, described.
#[derive(Clone, Debug)]
pub struct Parameter {
    pub name: &'static str,
    pub carried: In,
    pub is: Is,
    pub required: bool,
    pub about: &'static str,
}

impl Parameter {
    /// A parameter in the path. Always required — a path with a hole in it is
    /// not a path.
    #[must_use]
    pub const fn path(name: &'static str, is: Is, about: &'static str) -> Self {
        Self {
            name,
            carried: In::Path,
            is,
            required: true,
            about,
        }
    }

    #[must_use]
    pub const fn query(name: &'static str, is: Is, about: &'static str) -> Self {
        Self {
            name,
            carried: In::Query,
            is,
            required: false,
            about,
        }
    }

    #[must_use]
    pub const fn required(mut self) -> Self {
        self.required = true;
        self
    }
}

/// What a caller has to be, to be let in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Who {
    /// Anybody. A search engine, a visitor, a form on somebody's own site.
    Anybody,
    /// Somebody with an account here.
    AnAccount,
    /// Somebody enrolled on a course, who is not an account.
    AStudent,
}

/// What answering successfully looks like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answers {
    /// A body, and `200`.
    With(&'static str),
    /// A body, and `201` — something now exists that did not.
    Made(&'static str),
    /// Accepted, and `202` — it will happen, and has not yet.
    Later,
    /// Nothing, and `204`.
    Nothing,
}

impl Answers {
    #[must_use]
    pub const fn status(self) -> u16 {
        match self {
            Answers::With(_) => 200,
            Answers::Made(_) => 201,
            Answers::Later => 202,
            Answers::Nothing => 204,
        }
    }

    #[must_use]
    pub const fn body(self) -> Option<&'static str> {
        match self {
            Answers::With(name) | Answers::Made(name) => Some(name),
            _ => None,
        }
    }
}

/// One endpoint, said completely.
///
/// Every field is required to build one. That is the design: a description
/// generated from a declaration that cannot omit anything is a description
/// that cannot drift from what the endpoint does.
#[derive(Clone, Debug)]
pub struct Endpoint {
    pub method: Method,
    pub path: &'static str,
    /// Stable, and part of the API. A generator names its method from this, so
    /// changing one renames a method in every client.
    pub named: &'static str,
    pub about: &'static str,
    pub who: Who,
    pub parameters: Vec<Parameter>,
    /// The named shape of the request body, where there is one.
    pub takes: Option<&'static str>,
    pub answers: Answers,
    /// Every refusal a caller can receive, beyond the ones the guard adds for
    /// everybody. Declared, so a client can branch on them.
    pub refuses: &'static [Code],
    /// Whether this changes anything, which is what decides that an audit row
    /// must be written before it can answer.
    pub changes: bool,
}

/// Everything reachable, and what can be asked of it.
#[derive(Debug, Default)]
pub struct Api {
    pub endpoints: Vec<Endpoint>,
    /// The bodies those endpoints name. Separate from the endpoints because
    /// one shape is named by several of them, and describing it once is the
    /// point.
    pub shapes: Vec<Shape>,
}

/// Something an endpoint did not say, found by asking rather than by reading.
#[derive(Debug, PartialEq, Eq)]
pub struct Hole {
    pub named: &'static str,
    pub missing: &'static str,
}

impl Api {
    #[must_use]
    pub fn of(endpoints: Vec<Endpoint>) -> Self {
        Self {
            endpoints,
            shapes: Vec::new(),
        }
    }

    /// The same, with the bodies described.
    #[must_use]
    pub fn and(mut self, shapes: Vec<Shape>) -> Self {
        self.shapes = shapes;
        self
    }

    /// Every name referred to that nothing describes.
    ///
    /// A description whose references point at nothing is one a client cannot
    /// be generated from — and the way that stays true is that this is
    /// measured rather than remembered. Endpoints and shapes both refer: a
    /// shape naming another that does not exist is the same hole one step
    /// further in.
    ///
    /// The bytes are not a shape and never will be: an upload is a body whose
    /// kind is decided by reading it, and describing it as an object would be
    /// describing it wrongly.
    #[must_use]
    pub fn undescribed(&self) -> Vec<&'static str> {
        let described: BTreeSet<&str> = self.shapes.iter().map(|shape| shape.named).collect();

        let referred = self
            .endpoints
            .iter()
            .flat_map(|endpoint| endpoint.takes.into_iter().chain(endpoint.answers.body()))
            .chain(self.shapes.iter().flat_map(Shape::refers_to));

        let mut missing: Vec<&'static str> = referred
            .filter(|named| *named != crate::THE_BYTES && !described.contains(named))
            .collect();

        missing.sort_unstable();
        missing.dedup();

        missing
    }

    /// The refusals every caller of this endpoint can receive without the
    /// endpoint asking for them: what the guard itself answers.
    ///
    /// These are added to the description rather than left for a client to
    /// discover, which is the whole of what was wrong before.
    #[must_use]
    pub fn floor(endpoint: &Endpoint) -> Vec<Code> {
        let mut floor = vec![Code::TooMany, Code::Internal];

        if endpoint.who != Who::Anybody {
            floor.push(Code::Unauthenticated);
            floor.push(Code::Forbidden);
        }

        if endpoint.takes.is_some() || !endpoint.parameters.is_empty() {
            floor.push(Code::Invalid);
        }

        floor
    }

    /// What is still missing, named. Empty is the only acceptable answer, and
    /// a test asserts it — so "the description is complete" is measured rather
    /// than believed.
    #[must_use]
    pub fn holes(&self) -> Vec<Hole> {
        let mut holes = Vec::new();

        for endpoint in &self.endpoints {
            let named = endpoint.named;

            if named.is_empty() {
                holes.push(Hole {
                    named: endpoint.path,
                    missing: "a name a generator can make a method from",
                });
            }

            if endpoint.about.is_empty() {
                holes.push(Hole {
                    named,
                    missing: "a sentence saying what it is for",
                });
            }

            // Every `{hole}` in the path is a parameter, and every declared
            // path parameter is a hole. Either half missing is a client that
            // cannot build the URL.
            for hole in holes_in(endpoint.path) {
                if !endpoint
                    .parameters
                    .iter()
                    .any(|p| p.carried == In::Path && p.name == hole)
                {
                    holes.push(Hole {
                        named,
                        missing: "a path parameter the path names",
                    });
                }
            }

            for parameter in &endpoint.parameters {
                if parameter.carried == In::Path
                    && !holes_in(endpoint.path).contains(&parameter.name)
                {
                    holes.push(Hole {
                        named,
                        missing: "a path parameter the path does not have",
                    });
                }

                if parameter.about.is_empty() {
                    holes.push(Hole {
                        named,
                        missing: "a sentence saying what a parameter is",
                    });
                }
            }
        }

        holes
    }

    /// Endpoints that cannot both exist, found by comparing rather than by
    /// remembering.
    ///
    /// Two kinds. The same method and path declared twice — one of them is
    /// unreachable, and which one depends on the order they were mounted in.
    /// And the one nobody sees coming: two paths of the same shape naming
    /// their hole differently. `/api/forms/{slug}/submissions` beside
    /// `/api/forms/{id}/submissions` reads as two endpoints in two crates and
    /// is one route, which a router is entitled to refuse outright at the
    /// moment the process starts.
    #[must_use]
    pub fn clashes(&self) -> Vec<Clash> {
        let mut clashes = Vec::new();

        for (at, one) in self.endpoints.iter().enumerate() {
            for other in &self.endpoints[at + 1..] {
                if one.method != other.method {
                    continue;
                }

                let why = if one.path == other.path {
                    "the same method and path twice"
                } else if same_shape(one.path, other.path) {
                    "one path, two names for the same hole"
                } else {
                    continue;
                };

                clashes.push(Clash {
                    named: one.named,
                    with: other.named,
                    why,
                });
            }
        }

        clashes
    }
}

/// Whether two paths differ only in what they call their holes.
fn same_shape(one: &str, other: &str) -> bool {
    let shape = |path: &str| {
        path.split('/')
            .map(|part| {
                if part.starts_with('{') && part.ends_with('}') {
                    "{}"
                } else {
                    part
                }
            })
            .collect::<Vec<_>>()
            .join("/")
    };

    shape(one) == shape(other)
}

/// Two endpoints that cannot both be mounted, or that are the same endpoint
/// twice.
#[derive(Debug, PartialEq, Eq)]
pub struct Clash {
    pub named: &'static str,
    pub with: &'static str,
    pub why: &'static str,
}

/// The `{names}` a path carries.
#[must_use]
pub fn holes_in(path: &'static str) -> Vec<&'static str> {
    path.split('/')
        .filter_map(|part| part.strip_prefix('{')?.strip_suffix('}'))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_reading() -> Endpoint {
        Endpoint {
            method: Method::Get,
            path: "/api/posts/{id}",
            named: "posts.read",
            about: "One post, whatever kind it is.",
            who: Who::AnAccount,
            parameters: vec![Parameter::path("id", Is::Id, "Which post.")],
            takes: None,
            answers: Answers::With("Post"),
            refuses: &[Code::NotFound],
            changes: false,
        }
    }

    #[test]
    fn an_endpoint_that_says_everything_has_no_holes() {
        assert_eq!(Api::of(vec![a_reading()]).holes(), Vec::new());
    }

    #[test]
    fn a_path_with_a_hole_nobody_described_is_a_hole() {
        let mut missing = a_reading();
        missing.parameters = Vec::new();

        let holes = Api::of(vec![missing]).holes();

        assert_eq!(holes.len(), 1, "{holes:?}");
        assert_eq!(holes[0].missing, "a path parameter the path names");
    }

    #[test]
    fn a_parameter_for_a_hole_the_path_does_not_have_is_also_a_hole() {
        // The other direction, which is the one that survives a rename: the
        // path stops saying `{id}` and the parameter is left behind.
        let mut stale = a_reading();
        stale.path = "/api/posts";

        let holes = Api::of(vec![stale]).holes();

        assert_eq!(holes.len(), 1, "{holes:?}");
        assert_eq!(holes[0].missing, "a path parameter the path does not have");
    }

    #[test]
    fn what_the_guard_answers_is_part_of_what_is_described() {
        let floor = Api::floor(&a_reading());

        // The four an account-guarded endpoint answers without asking. Before,
        // every operation described one response and inherited these silently.
        assert!(floor.contains(&Code::Unauthenticated));
        assert!(floor.contains(&Code::Forbidden));
        assert!(floor.contains(&Code::TooMany));
        assert!(floor.contains(&Code::Internal));

        let public = Endpoint {
            who: Who::Anybody,
            parameters: Vec::new(),
            ..a_reading()
        };

        assert!(!Api::floor(&public).contains(&Code::Unauthenticated));
    }

    #[test]
    fn a_status_is_what_the_answer_says_it_is() {
        // Sixty-seven operations answered 201, 202 or 204 while describing
        // 200. Here the status comes from the same value that says whether
        // there is a body.
        assert_eq!(Answers::With("Post").status(), 200);
        assert_eq!(Answers::Made("Post").status(), 201);
        assert_eq!(Answers::Later.status(), 202);
        assert_eq!(Answers::Nothing.status(), 204);
        assert_eq!(Answers::Nothing.body(), None);
    }

    #[test]
    fn two_endpoints_that_cannot_both_be_mounted_are_found_by_comparing() {
        // The shape that reads as two endpoints and is one route: two crates
        // each describing the same path and calling its hole something of
        // their own. Nothing about either declaration looks wrong on its own,
        // which is why this is asked of the whole rather than of each.
        let by_slug = Endpoint {
            path: "/api/forms/{slug}/submissions",
            named: "forms.fill-in",
            parameters: vec![Parameter::path("slug", Is::Text, "Which form.")],
            ..a_reading()
        };
        let by_id = Endpoint {
            path: "/api/forms/{id}/submissions",
            named: "forms.submissions",
            parameters: vec![Parameter::path("id", Is::Id, "Which form.")],
            ..a_reading()
        };

        let clashes = Api::of(vec![by_slug, by_id]).clashes();

        assert_eq!(clashes.len(), 1, "{clashes:#?}");
        assert_eq!(clashes[0].why, "one path, two names for the same hole");
    }

    #[test]
    fn the_same_endpoint_twice_is_a_clash_and_two_verbs_on_one_path_are_not() {
        let read = a_reading();
        let same = Endpoint {
            named: "posts.read-again",
            ..a_reading()
        };
        let write = Endpoint {
            method: Method::Delete,
            named: "posts.remove",
            ..a_reading()
        };

        assert_eq!(Api::of(vec![read, same]).clashes().len(), 1);
        assert!(Api::of(vec![a_reading(), write]).clashes().is_empty());
    }
}
