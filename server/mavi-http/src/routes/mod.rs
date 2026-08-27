// Route modules intentionally share the private HTTP composition vocabulary;
// keeping the transport imports local to the parent prevents duplicating the
// generated domain DTO list in every plugin module.
#![allow(clippy::wildcard_imports)]

//! Plugin-owned HTTP route composition.
//!
//! Each module owns its route declarations and handlers. The parent HTTP
//! crate owns admission, state and transport middleware; these modules only
//! assemble the compiled product capabilities that the plugin registry gates
//! at runtime.

use super::*;

mod analytics;
mod automation;
mod boards;
mod commerce;
mod core;
mod forms;
mod governance;
mod learning;
mod messaging;
mod writing;

pub(super) fn api_routes() -> Router<HttpState> {
    Router::new()
        .route("/openapi.json", get(openapi_document))
        .merge(core::plugin_routes())
        .merge(core::identity_routes())
        .merge(core::settings_routes())
        .merge(writing::content_routes())
        .merge(writing::media_routes())
        .merge(core::credentials_routes())
        .merge(governance::audit_trash_routes())
        .merge(writing::design_routes())
        .merge(forms::form_routes())
        .merge(core::feedback_routes())
        .merge(messaging::mail_routes())
        .merge(learning::course_routes())
        .merge(commerce::shop_routes())
        .merge(automation::automation_routes())
        .merge(boards::board_routes())
        .merge(analytics::analytics_routes())
        .merge(governance::portable_routes())
        .route("/api/v1/runtime/manifest", get(core::runtime_manifest))
}
