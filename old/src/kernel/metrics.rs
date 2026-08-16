//! What is counted, for whoever is looking at a graph.
//!
//! Prometheus at `/metrics`: what each domain answered and how long it took,
//! how deep the queue is, and what has gone to the dead letter. Nothing here
//! counts a person — a metric with an address in it is a log with an address
//! in it.
use std::sync::LazyLock;
use std::time::Instant;

use axum::extract::{MatchedPath, Request};
use axum::middleware::Next;
use axum::response::Response;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

/// Seconds. The buckets are the answers somebody actually asks for — is the
/// panel quick, is anything taking whole seconds — rather than a default ladder.
const LATENCY_BUCKETS: [f64; 9] = [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 5.0];

static HANDLE: LazyLock<PrometheusHandle> = LazyLock::new(|| {
    PrometheusBuilder::new()
        .set_buckets(&LATENCY_BUCKETS)
        .expect("the buckets are in order")
        .install_recorder()
        .expect("nothing else installed a recorder")
});

#[must_use]
pub fn render() -> String {
    HANDLE.render()
}

/// Nothing is counted until something is installed to count it, and the first
/// thing to ask for the numbers is usually whatever is scraping them — by
/// which time everything before it went nowhere. So every counter here makes
/// sure there is a recorder first.
fn counting() {
    LazyLock::force(&HANDLE);
}

/// Counts by the route's pattern rather than its path, so a thousand posts are
/// one line rather than a thousand.
pub async fn observe(request: Request, next: Next) -> Response {
    counting();

    let route = request.extensions().get::<MatchedPath>().map_or_else(
        || "unmatched".to_owned(),
        |matched| matched.as_str().to_owned(),
    );

    let method = request.method().to_string();
    let started = Instant::now();

    let response = next.run(request).await;
    let status = response.status().as_u16().to_string();

    metrics::counter!(
        "http_requests_total",
        "route" => route.clone(),
        "method" => method.clone(),
        "status" => status
    )
    .increment(1);

    metrics::histogram!(
        "http_request_duration_seconds",
        "route" => route,
        "method" => method
    )
    .record(started.elapsed().as_secs_f64());

    response
}

/// One line per domain rather than one per path: "is the shop refusing things"
/// is the question somebody has, and a hundred routes do not answer it.
pub fn domain_answered(domain: &'static str, allowed: bool) {
    counting();

    metrics::counter!(
        "domain_requests_total",
        "domain" => domain,
        "outcome" => if allowed { "allowed" } else { "refused" },
    )
    .increment(1);
}

/// Something a domain said happened, counted where it was said. What is in the
/// outbox is what a site's webhooks carry, so a domain that has gone quiet
/// shows up here before anybody's receiver notices.
pub fn domain_emitted(domain: &'static str, event: &str) {
    counting();

    metrics::counter!(
        "domain_events_total",
        "domain" => domain,
        "event" => event.to_owned(),
    )
    .increment(1);
}

/// A job, and whether it did what it was for.
pub fn job_ran(kind: &str, done: bool) {
    counting();

    metrics::counter!(
        "jobs_run_total",
        "kind" => kind.to_owned(),
        "outcome" => if done { "done" } else { "failed" },
    )
    .increment(1);
}

/// The last time a scheduled job finished. A cron that has quietly stopped is
/// worse than one that was never written, and this is what an alert reads.
pub fn job_finished(kind: &str, at: chrono::DateTime<chrono::Utc>) {
    counting();

    // Milliseconds since the epoch fit in an f64 exactly for another
    // quarter of a million years.
    #[expect(
        clippy::cast_precision_loss,
        reason = "a timestamp is far inside f64's exact range"
    )]
    let seconds = at.timestamp_millis() as f64 / 1000.0;

    metrics::gauge!("job_last_finished_timestamp", "kind" => kind.to_owned()).set(seconds);
}
