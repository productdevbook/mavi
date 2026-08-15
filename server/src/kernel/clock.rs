//! What the time is, asked rather than taken.
//!
//! A test that needs yesterday sets it; everything else asks. Code that calls
//! `now()` directly is code no test can put in the past, which is how a
//! scheduled post becomes a thing nobody can test.
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use chrono::{DateTime, Duration, TimeZone, Utc};

pub trait Clock: Send + Sync + std::fmt::Debug {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// A clock a test moves by hand, so "what happens in two days" is a test that
/// runs in a millisecond.
#[derive(Clone, Debug)]
pub struct TestClock(Arc<AtomicI64>);

impl TestClock {
    #[must_use]
    pub fn at(moment: DateTime<Utc>) -> Self {
        Self(Arc::new(AtomicI64::new(moment.timestamp_millis())))
    }

    pub fn advance(&self, by: Duration) {
        self.0.fetch_add(by.num_milliseconds(), Ordering::SeqCst);
    }
}

impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        Utc.timestamp_millis_opt(self.0.load(Ordering::SeqCst))
            .single()
            .expect("a moment this clock was set to")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_test_clock_moves_only_when_it_is_moved() {
        let start = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
        let clock = TestClock::at(start);

        assert_eq!(clock.now(), start);

        clock.advance(Duration::days(2));

        assert_eq!(clock.now(), start + Duration::days(2));
    }
}
