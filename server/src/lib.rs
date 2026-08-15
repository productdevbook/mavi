//! Mavi CMS: one binary serving many sites.
//!
//! A request is resolved from its `Host` header to a site, and every table
//! carrying a site's data is behind row-level security the database enforces.
//! `kernel` is what every domain is built out of; each other module is one
//! thing a site does.
pub mod analytics;
pub mod assistant;
pub mod audit;
pub mod auth;
pub mod boards;
pub mod building;
pub mod content;
pub mod edge;
pub mod flows;
pub mod forms;
pub mod health;
pub mod housekeeping;
pub mod jobs;
pub mod kernel;
pub mod learning;
pub mod mail;
pub mod mcp;
pub mod media;
pub mod pages;
pub mod people;
pub mod plugins;
pub mod portable;
pub mod publishing;
pub mod reports;
pub mod setup;
pub mod shop;
pub mod site;
pub mod trash;
pub mod webhooks;

use kernel::http::{AppState, Endpoint};

/// Everything reachable. A domain not in this list is not served.
#[must_use]
pub fn endpoints() -> Vec<Endpoint> {
    let mut all = within("analytics", analytics::endpoints());
    all.extend(within("audit", audit::endpoints()));
    all.extend(within("auth", auth::endpoints()));
    all.extend(within("boards", boards::endpoints()));
    all.extend(within("content", content::endpoints()));
    all.extend(within("flows", flows::endpoints()));
    all.extend(within("forms", forms::endpoints()));
    all.extend(within("health", health::endpoints()));
    all.extend(within("learning", learning::endpoints()));
    all.extend(within("mail", mail::endpoints()));
    all.extend(within("mcp", mcp::endpoints()));
    all.extend(within("media", media::all_endpoints()));
    all.extend(within("pages", pages::endpoints()));
    all.extend(within("people", people::endpoints()));
    all.extend(within("plugins", plugins::endpoints()));
    all.extend(within("portable", portable::endpoints()));
    all.extend(within("publishing", publishing::endpoints()));
    all.extend(within("reports", reports::endpoints()));
    all.extend(within("setup", setup::endpoints()));
    all.extend(within("shop", shop::endpoints()));
    all.extend(within("site", site::endpoints()));
    all.extend(within("assistant", assistant::endpoints()));
    all.extend(within("trash", trash::endpoints()));
    all
}

/// Says which domain a module's endpoints belong to, once for the module
/// rather than once for each of them.
fn within(domain: &'static str, endpoints: Vec<Endpoint>) -> Vec<Endpoint> {
    assert!(
        kernel::domain::known(domain),
        "{domain} is not one of the domains this is made of"
    );

    endpoints
        .into_iter()
        .map(|endpoint| endpoint.within(domain))
        .collect()
}

/// Everything reachable, plus whatever `state` carries in from outside this
/// crate — mounted the same way, so an outside endpoint goes through the same
/// [`kernel::http::Guard`] and the same audit rule as one of this crate's own.
pub fn router(state: AppState) -> axum::Router {
    let mut all = endpoints();
    all.extend(outside_endpoints(&state.outside));
    kernel::http::mount(state, all)
}

/// The endpoints something outside this crate hands in, refusing any that
/// claims a domain this crate already answers under — its spans and its
/// `domain_answered` metric would land under a name that is not its own, and
/// nothing would say so. Two outside endpoints may share a domain with each
/// other; neither may take one of ours.
fn outside_endpoints(outside: &kernel::outside::Outside) -> Vec<Endpoint> {
    for endpoint in &outside.endpoints {
        assert!(
            !endpoint.domain().is_empty(),
            "{} {} came from outside this crate without saying which domain it belongs to",
            endpoint.method(),
            endpoint.path()
        );

        assert!(
            !kernel::domain::known(endpoint.domain()),
            "{} is one of this crate's own domains and cannot be claimed from outside it",
            endpoint.domain()
        );
    }

    outside.endpoints.clone()
}

/// The description of this API, built from the list of endpoints rather than
/// written beside it.
///
/// Describes this crate's own endpoints only — kept stable for the snapshot
/// test regardless of what an outside crate mounts. [`openapi_with`] is what
/// the server actually answers `/openapi.json` with.
#[must_use]
pub fn openapi() -> utoipa::openapi::OpenApi {
    kernel::openapi::describe(&endpoints())
}

/// The description of this API plus whatever `outside` hands in — what the
/// panel's generated types are built from when the operator's own half is
/// mounted, so its endpoints are not simply missing from them.
#[must_use]
pub fn openapi_with(outside: &kernel::outside::Outside) -> utoipa::openapi::OpenApi {
    let mut all = endpoints();
    all.extend(outside_endpoints(outside));
    kernel::openapi::describe(&all)
}
