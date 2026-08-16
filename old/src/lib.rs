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
pub mod domain;
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
pub mod recover;
pub mod reports;
pub mod retention;
pub mod setup;
pub mod shop;
pub mod site;
pub mod start;
#[cfg(feature = "testing")]
pub mod testing;
pub mod trash;
pub mod webhooks;

use kernel::http::{AppState, Endpoint};
use kernel::wiring::{Answers, Wiring};

/// What the kernel is handed: what serves a page, what takes work off the
/// queue, what follows an event, who a student's cookie belongs to, and what
/// describes the whole of it.
///
/// The kernel needs each of these to happen and knows what none of them are.
/// This is the one place that says which of this crate's modules does which —
/// so a domain is reached from above rather than reached for from below.
#[must_use]
pub fn wiring() -> Wiring {
    Wiring {
        // Anything that is not one of the endpoints is one of the site's pages.
        otherwise: Some(axum::Router::new().fallback(edge::serve)),
        takes_work: Some(takes_work),
        keeps_time: Some(keeps_time),
        follows_an_event: vec![webhooks::after_an_event, flows::after_an_event],
        names_an_event: Some(domain::of_event),
        a_student: Some(learning::a_student),
        describes: Some(openapi_with),
    }
}

fn takes_work<'a>(state: &'a AppState, worker: &'a str) -> Answers<'a, bool> {
    Box::pin(jobs::tick(state, worker))
}

fn keeps_time(state: &AppState) -> Answers<'_, ()> {
    Box::pin(jobs::schedule_due(state))
}

impl AppState {
    /// A state for whatever has nothing to hand in: the environment, read here,
    /// and this crate's own wiring.
    ///
    /// [`AppState::new_with`] is the one to build a state with where any of
    /// that is somebody else's to decide.
    #[must_use]
    pub fn new(db: kernel::db::Db) -> Self {
        // `start` says this in prose before anything reaches here; this is the
        // same refusal for whatever builds a state without going through it. A
        // key that was meant and mistyped is not a key to fall back from.
        let keyring = kernel::crypto::Keyring::from_the_environment()
            .unwrap_or_else(|why| panic!("MAVI_KEYS: {why}"));

        // The same distinction, for the same reason: an address that was meant
        // and mistyped is refused here, and one nobody gave falls back to the
        // obviously invented one. Not the silence #18 was — what a machine
        // anybody can reach runs through is `start`, and that refuses to come
        // up without a real one.
        let address = kernel::config::Address::from_the_environment()
            .unwrap_or_else(|why| panic!("MAVI_URL: {why}"))
            .unwrap_or_else(kernel::config::Address::invented);

        let mut state = Self::new_with(db, kernel::Config::from_env(keyring, address));
        state.wiring = std::sync::Arc::new(wiring());
        state
    }
}

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
        crate::domain::known(domain),
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
            !crate::domain::known(endpoint.domain()),
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
/// panel's generated types are built from when something is mounted on this
/// crate, so its endpoints are not simply missing from them.
#[must_use]
pub fn openapi_with(outside: &kernel::outside::Outside) -> utoipa::openapi::OpenApi {
    let mut all = endpoints();
    all.extend(outside_endpoints(outside));
    kernel::openapi::describe(&all)
}
