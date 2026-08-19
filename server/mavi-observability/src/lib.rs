//! Process-local observability primitives shared by the runtime composition
//! roots and domain workers.
//!
//! The registry deliberately has no HTTP, database or exporter dependency.
//! The HTTP crate owns the `/metrics` operational endpoint and renders this
//! stable snapshot as Prometheus text. Self-host and cloud runtimes can
//! therefore share one counter registry without making worker execution
//! depend on an HTTP transport.

use std::{
    fmt::Write as _,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

/// Counters owned by one Mavi process.
#[derive(Clone, Debug, Default)]
pub struct RuntimeMetrics {
    inner: Arc<RuntimeMetricCounters>,
}

#[derive(Debug, Default)]
struct RuntimeMetricCounters {
    http: HttpMetricCounters,
    worker: Arc<WorkerMetricCounters>,
}

#[derive(Debug, Default)]
struct HttpMetricCounters {
    requests: AtomicU64,
    responses_2xx: AtomicU64,
    responses_3xx: AtomicU64,
    responses_4xx: AtomicU64,
    responses_5xx: AtomicU64,
}

/// A copyable view of HTTP activity.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HttpMetricsSnapshot {
    pub requests: u64,
    pub responses_2xx: u64,
    pub responses_3xx: u64,
    pub responses_4xx: u64,
    pub responses_5xx: u64,
}

/// A handle to the worker counters in a [`RuntimeMetrics`] registry.
///
/// The handle is intentionally separate from the runtime registry so the
/// worker crate can record job activity without knowing about HTTP or the
/// Prometheus transport.
#[derive(Clone, Debug, Default)]
pub struct WorkerMetrics {
    inner: Arc<WorkerMetricCounters>,
}

#[derive(Debug, Default)]
struct WorkerMetricCounters {
    polls: AtomicU64,
    claims: AtomicU64,
    completed: AtomicU64,
    failed: AtomicU64,
    deferred: AtomicU64,
    lost_leases: AtomicU64,
    errors: AtomicU64,
}

/// A consistent, copyable view of worker activity.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkerMetricsSnapshot {
    pub polls: u64,
    pub claims: u64,
    pub completed: u64,
    pub failed: u64,
    pub deferred: u64,
    pub lost_leases: u64,
    pub errors: u64,
}

/// A consistent view of all metrics exported by one process.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeMetricsSnapshot {
    pub http: HttpMetricsSnapshot,
    pub worker: WorkerMetricsSnapshot,
}

impl RuntimeMetrics {
    /// Returns the worker handle backed by this registry.
    #[must_use]
    pub fn worker_metrics(&self) -> WorkerMetrics {
        WorkerMetrics {
            inner: Arc::clone(&self.inner.worker),
        }
    }

    /// Records one completed HTTP response.
    pub fn record_http_response(&self, status: u16) {
        increment(&self.inner.http.requests);
        match status {
            200..=299 => increment(&self.inner.http.responses_2xx),
            300..=399 => increment(&self.inner.http.responses_3xx),
            400..=499 => increment(&self.inner.http.responses_4xx),
            _ => increment(&self.inner.http.responses_5xx),
        }
    }

    /// Returns one consistent-enough process-local snapshot. Individual
    /// counters are atomic; the snapshot is intended for telemetry rather
    /// than transactional accounting.
    #[must_use]
    pub fn snapshot(&self) -> RuntimeMetricsSnapshot {
        RuntimeMetricsSnapshot {
            http: HttpMetricsSnapshot {
                requests: load(&self.inner.http.requests),
                responses_2xx: load(&self.inner.http.responses_2xx),
                responses_3xx: load(&self.inner.http.responses_3xx),
                responses_4xx: load(&self.inner.http.responses_4xx),
                responses_5xx: load(&self.inner.http.responses_5xx),
            },
            worker: self.worker_metrics().snapshot(),
        }
    }

    /// Renders the process snapshot in the Prometheus text exposition format.
    #[must_use]
    pub fn prometheus(&self) -> String {
        let snapshot = self.snapshot();
        let mut output = String::new();

        counter(
            &mut output,
            "mavi_http_requests_total",
            "Completed HTTP requests.",
            snapshot.http.requests,
        );
        let _ = writeln!(
            output,
            "# HELP mavi_http_responses_total Completed HTTP responses by status class.\n# TYPE mavi_http_responses_total counter\nmavi_http_responses_total{{status_class=\"2xx\"}} {}\nmavi_http_responses_total{{status_class=\"3xx\"}} {}\nmavi_http_responses_total{{status_class=\"4xx\"}} {}\nmavi_http_responses_total{{status_class=\"5xx\"}} {}",
            snapshot.http.responses_2xx,
            snapshot.http.responses_3xx,
            snapshot.http.responses_4xx,
            snapshot.http.responses_5xx,
        );
        counter(
            &mut output,
            "mavi_worker_polls_total",
            "Worker site polling attempts.",
            snapshot.worker.polls,
        );
        counter(
            &mut output,
            "mavi_worker_claims_total",
            "Worker job claims.",
            snapshot.worker.claims,
        );
        counter(
            &mut output,
            "mavi_worker_completed_total",
            "Worker jobs completed or safely skipped.",
            snapshot.worker.completed,
        );
        counter(
            &mut output,
            "mavi_worker_failed_total",
            "Worker jobs moved to a failed state.",
            snapshot.worker.failed,
        );
        counter(
            &mut output,
            "mavi_worker_deferred_total",
            "Worker jobs deferred because they were not due yet.",
            snapshot.worker.deferred,
        );
        counter(
            &mut output,
            "mavi_worker_lost_leases_total",
            "Worker mutations rejected because their lease was lost.",
            snapshot.worker.lost_leases,
        );
        counter(
            &mut output,
            "mavi_worker_errors_total",
            "Worker polling or execution errors.",
            snapshot.worker.errors,
        );

        output
    }
}

impl WorkerMetrics {
    /// Returns the current worker counter snapshot.
    #[must_use]
    pub fn snapshot(&self) -> WorkerMetricsSnapshot {
        WorkerMetricsSnapshot {
            polls: load(&self.inner.polls),
            claims: load(&self.inner.claims),
            completed: load(&self.inner.completed),
            failed: load(&self.inner.failed),
            deferred: load(&self.inner.deferred),
            lost_leases: load(&self.inner.lost_leases),
            errors: load(&self.inner.errors),
        }
    }

    pub fn record_poll(&self) {
        increment(&self.inner.polls);
    }

    pub fn record_claim(&self) {
        increment(&self.inner.claims);
    }

    pub fn record_completed(&self) {
        increment(&self.inner.completed);
    }

    pub fn record_failed(&self) {
        increment(&self.inner.failed);
    }

    pub fn record_deferred(&self) {
        increment(&self.inner.deferred);
    }

    pub fn record_lost_lease(&self) {
        increment(&self.inner.lost_leases);
    }

    pub fn record_error(&self) {
        increment(&self.inner.errors);
    }
}

fn increment(counter: &AtomicU64) {
    counter.fetch_add(1, Ordering::Relaxed);
}

fn load(counter: &AtomicU64) -> u64 {
    counter.load(Ordering::Relaxed)
}

fn counter(output: &mut String, name: &str, help: &str, value: u64) {
    let _ = writeln!(
        output,
        "# HELP {name} {help}\n# TYPE {name} counter\n{name} {value}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_statuses_are_counted_by_class() {
        let metrics = RuntimeMetrics::default();
        for status in [200, 201, 302, 404, 422, 500, 503] {
            metrics.record_http_response(status);
        }

        assert_eq!(
            metrics.snapshot().http,
            HttpMetricsSnapshot {
                requests: 7,
                responses_2xx: 2,
                responses_3xx: 1,
                responses_4xx: 2,
                responses_5xx: 2,
            }
        );
    }

    #[test]
    fn worker_handle_is_shared_with_the_runtime_registry() {
        let metrics = RuntimeMetrics::default();
        let worker = metrics.worker_metrics();
        worker.record_poll();
        worker.record_claim();
        worker.record_completed();

        assert_eq!(metrics.snapshot().worker.polls, 1);
        assert_eq!(metrics.snapshot().worker.claims, 1);
        assert_eq!(metrics.snapshot().worker.completed, 1);
    }

    #[test]
    fn prometheus_output_has_stable_counter_families() {
        let metrics = RuntimeMetrics::default();
        metrics.record_http_response(503);
        metrics.worker_metrics().record_error();

        let output = metrics.prometheus();
        assert!(output.contains("# TYPE mavi_http_requests_total counter"));
        assert!(output.contains("mavi_http_responses_total{status_class=\"5xx\"} 1"));
        assert!(output.contains("# TYPE mavi_worker_errors_total counter"));
        assert!(output.ends_with('\n'));
    }
}
