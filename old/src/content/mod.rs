//! What a site publishes.
//!
//! Posts and pages, the categories and tags they are filed under, the kinds of
//! thing a site invents for itself, and the languages it writes in. Four files
//! rather than one because they are four tables — but one module, because a
//! post without its terms and its language is not a post.
mod languages;
mod posts;
mod terms;
mod types;

pub use languages::{Language, NewLanguage};
pub use posts::{NewPost, Post, PostChanges, PublishDue, State, publish_due};
pub use terms::{NewTerm, Term, TermKind};
pub use types::{ContentType, Field, Kind, NewType};

use crate::kernel::authz::{Access, Capability, Needs};
use crate::kernel::http::Endpoint;

pub(crate) fn needs(access: Access) -> Needs {
    Needs::new(Capability::Content, access)
}

pub(crate) fn taxonomy(access: Access) -> Needs {
    Needs::new(Capability::Taxonomy, access)
}

#[must_use]
pub fn kinds() -> Vec<String> {
    use crate::kernel::queue::Task;

    vec![PublishDue::KIND.to_owned()]
}

/// Writing a post from a tool call rather than from a request.
///
/// The same rules either way: the language must be one the site writes in, a
/// kind of thing's fields are what it declares, and the audit row is written
/// before anything is answered. What is different is only where the arguments
/// came from.
pub async fn write_through_a_tool(
    state: &crate::kernel::http::AppState,
    caller: &crate::kernel::http::Caller,
    arguments: &serde_json::Value,
) -> crate::kernel::error::Result<serde_json::Value> {
    posts::write_through_a_tool(state, caller, arguments).await
}

/// Changing a post from a tool call rather than from a request.
///
/// Whose the post is, which state it may move to and what its kind of thing
/// allows are decided by the same code the route uses.
pub async fn change_through_a_tool(
    state: &crate::kernel::http::AppState,
    caller: &crate::kernel::http::Caller,
    arguments: &serde_json::Value,
) -> crate::kernel::error::Result<serde_json::Value> {
    posts::change_through_a_tool(state, caller, arguments).await
}

#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    let mut all = posts::endpoints();
    all.extend(terms::endpoints());
    all.extend(types::endpoints());
    all.extend(languages::endpoints());
    all
}
