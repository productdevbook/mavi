//! Counting what a request actually asks the database.
//!
//! sqlx emits one tracing event per query, so a subscriber that counts those is
//! a query counter — and a listing that quietly does one query per row is a
//! test that fails rather than a page that is slow in a year's time.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use std::fmt::Write as _;
use tracing::subscriber::DefaultGuard;
use tracing::{Event, Metadata, Subscriber};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;

use tracing_subscriber::{Layer, Registry};

#[derive(Clone, Default)]
pub struct Counter(Arc<AtomicUsize>);

impl Counter {
    #[must_use]
    pub fn count(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

struct CountQueries(Counter);

/// What the statement was, so that the ones a pool runs when it opens a
/// connection can be left out. They are not what a request costs, and a pool
/// that grows mid-measurement would otherwise look like a query per row.
#[derive(Default)]
struct Statement(String);

impl tracing::field::Visit for Statement {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "summary" || field.name() == "db.statement" {
            let _ = write!(self.0, "{value:?}");
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "summary" || field.name() == "db.statement" {
            self.0.push_str(value);
        }
    }
}

/// What a connection runs before it is a connection anybody asked for.
///
/// `pg_catalog.pg_type` and `pg_catalog.pg_enum` are here for the same reason
/// as `set role`, not because they look alike: the first time a physical
/// connection decodes a Postgres enum it has not seen before — `course_state`,
/// say — sqlx asks the catalog what it is and caches the answer on that
/// connection for as long as it lives (`sqlx-postgres`'s
/// `maybe_fetch_type_info_by_oid`/`fetch_enum_by_oid`). That cache is
/// per-connection, not per-process, so which request pays for it depends on
/// which of the pool's connections happens to serve it — the same request
/// twice in a row can differ by exactly two queries for a reason no warm-up
/// request can reliably fix, since it might land on a different connection
/// than the one it is meant to warm. It is connection setup, not row cost, so
/// it is excluded here the same way `set role` is.
const SETTING_UP: [&str; 4] = [
    "set role",
    "set_config",
    "pg_catalog.pg_type",
    "pg_catalog.pg_enum",
];

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for CountQueries {
    fn enabled(&self, metadata: &Metadata<'_>, _: Context<'_, S>) -> bool {
        metadata.target() == "sqlx::query"
    }

    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        if event.metadata().target() != "sqlx::query" {
            return;
        }

        let mut statement = Statement::default();
        event.record(&mut statement);

        let setup = SETTING_UP
            .iter()
            .any(|prefix| statement.0.to_lowercase().contains(prefix));

        if !setup {
            self.0.0.fetch_add(1, Ordering::SeqCst);
        }
    }
}

/// Counts for as long as the guard is held. Held on one thread, so a test that
/// counts runs its request on the same one.
#[must_use]
pub fn counting() -> (Counter, DefaultGuard) {
    let counter = Counter::default();
    let subscriber = Registry::default().with(CountQueries(counter.clone()));

    (
        counter.clone(),
        tracing::subscriber::set_default(subscriber),
    )
}
