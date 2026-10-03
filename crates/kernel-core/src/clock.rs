//! Time and identity sources.
//!
//! The kernel never reads the wall clock or a random generator directly. In
//! experiments both are logical and seeded, which makes event logs
//! byte-for-byte reproducible. Interactive sessions use the system clock and
//! random identifiers.

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

pub trait Clock: Send + Sync {
    fn now_ms(&self) -> i64;
}

/// Wall-clock time.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        chrono::Utc::now().timestamp_millis()
    }
}

/// Logical time set explicitly by a simulation driver.
#[derive(Debug, Default)]
pub struct ManualClock {
    now: AtomicI64,
}

impl ManualClock {
    pub fn new(start_ms: i64) -> Self {
        Self {
            now: AtomicI64::new(start_ms),
        }
    }

    pub fn set(&self, now_ms: i64) {
        self.now.store(now_ms, Ordering::SeqCst);
    }

    pub fn advance(&self, delta_ms: i64) -> i64 {
        self.now.fetch_add(delta_ms, Ordering::SeqCst) + delta_ms
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> i64 {
        self.now.load(Ordering::SeqCst)
    }
}

pub trait IdSource: Send + Sync {
    /// A new identifier such as `evt-000042`. `kind` is a short lowercase prefix.
    fn next_id(&self, kind: &str) -> String;
}

/// Random identifiers for interactive sessions.
#[derive(Debug, Default, Clone, Copy)]
pub struct RandomIds;

impl IdSource for RandomIds {
    fn next_id(&self, kind: &str) -> String {
        format!("{kind}-{}", uuid::Uuid::new_v4())
    }
}

/// One counter shared by every kind, so identifiers also show creation order.
#[derive(Debug, Default)]
pub struct SequentialIds {
    counter: AtomicU64,
}

impl SequentialIds {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn issued(&self) -> u64 {
        self.counter.load(Ordering::SeqCst)
    }
}

impl IdSource for SequentialIds {
    fn next_id(&self, kind: &str) -> String {
        let n = self.counter.fetch_add(1, Ordering::SeqCst) + 1;
        format!("{kind}-{n:06}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequential_ids_are_ordered_and_reproducible() {
        let ids = SequentialIds::new();
        assert_eq!(ids.next_id("evt"), "evt-000001");
        assert_eq!(ids.next_id("jdg"), "jdg-000002");
        let again = SequentialIds::new();
        assert_eq!(again.next_id("evt"), "evt-000001");
    }

    #[test]
    fn manual_clock_only_moves_when_told() {
        let clock = ManualClock::new(1_000);
        assert_eq!(clock.now_ms(), 1_000);
        assert_eq!(clock.advance(250), 1_250);
        clock.set(5);
        assert_eq!(clock.now_ms(), 5);
    }
}
