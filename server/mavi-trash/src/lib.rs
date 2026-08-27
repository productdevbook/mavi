//! Backwards-compatible package name for the application-owned trash use case.
//!
//! New runtime code imports this module from `mavi_application`. Keeping this
//! re-export avoids breaking migration tooling and downstream integration
//! tests while the cross-domain coordinator has one canonical owner.

pub use mavi_application::trash::*;
