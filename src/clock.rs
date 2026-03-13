//! Logical clock and timestamp used to order Accord transactions.
//!
//! Accord timestamps are ordered as `(time, seq, node)`, where `time` is a per-process monotonic
//! value that loosely tracks wall time, `seq` breaks ties when the wall time does not advance, and
//! `node` identifies the originating node.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::node::NodeID;

/// A timestamp used to impose a total order on conflicting transactions.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Timestamp {
    /// Monotonic wall-clock component in nanoseconds since the UNIX epoch.
    pub time: u64,
    /// Logical sequence number used when [`Self::time`] does not advance.
    pub seq: u32,
    /// The node that generated the timestamp.
    pub node: NodeID,
}

/// A logical clock that produces monotonically increasing timestamps.
pub struct Clock<S: TimeSource = SystemTimeSource> {
    /// Node ID associated with this clock. [`Self::now`] timestamps always have this node ID.
    node: NodeID,
    /// Time source used to seed new logical timestamps.
    time_source: S,
    /// Latest timestamp seen by this clock.
    ///
    /// INVARIANT: monotonically increasing.
    latest: Mutex<Timestamp>,
}

impl Clock<SystemTimeSource> {
    /// Creates a clock for the given node ID.
    #[must_use]
    pub fn new(node: NodeID) -> Self {
        Self::with_time_source(node, SystemTimeSource)
    }
}

impl<S: TimeSource> Clock<S> {
    /// Creates a clock using the given time source.
    fn with_time_source(node: NodeID, time_source: S) -> Self {
        Self { node, time_source, latest: Mutex::default() }
    }

    /// Generates the next local timestamp.
    ///
    /// INVARIANT: the returned timestamp is greater than any previously observed and returned
    /// timestamp.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned or if the logical sequence overflows `u32`.
    pub fn now(&self) -> Timestamp {
        let mut latest = self.latest.lock().expect("clock mutex poisoned");
        let time = self.time_source.now();

        if time > latest.time {
            latest.time = time;
            latest.seq = 0;
        } else {
            latest.seq = latest.seq.checked_add(1).expect("logical clock overflow");
        }
        latest.node = self.node;

        *latest
    }

    /// Observes a timestamp, advancing the clock such that future timestamps sort after it.
    ///
    /// TODO: this should not be far into the future, since it may exhaust the logical sequence.
    /// Consider checking against the time source and panicking if it's too far ahead.
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    pub fn observe(&self, timestamp: Timestamp) {
        let mut latest = self.latest.lock().expect("clock mutex poisoned");

        if timestamp > *latest {
            *latest = timestamp;
        }
    }
}

/// Source of time for seeding logical timestamps.
pub trait TimeSource: Send + Sync {
    /// Returns the current time in nanoseconds since the UNIX epoch.
    fn now(&self) -> u64;
}

/// Time source backed by the system clock.
pub struct SystemTimeSource;

impl TimeSource for SystemTimeSource {
    fn now(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before UNIX epoch")
            .as_nanos()
            .try_into()
            .expect("system time overflow")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use itertools::{Itertools as _, iproduct};
    use rand::seq::SliceRandom as _;
    use rand::thread_rng;

    use super::*;

    const NODE: NodeID = 3;
    const PEER: NodeID = 5;

    /// Deterministic time source that yields queued times in order.
    struct QueuedTimeSource {
        timestamps: Mutex<VecDeque<u64>>,
    }

    impl QueuedTimeSource {
        /// Creates a deterministic time source from an ordered sequence of times.
        fn new(timestamps: impl IntoIterator<Item = u64>) -> Self {
            Self { timestamps: Mutex::new(timestamps.into_iter().collect()) }
        }
    }

    impl TimeSource for QueuedTimeSource {
        fn now(&self) -> u64 {
            self.timestamps.lock().expect("mutex poisoned").pop_front().expect("exhausted")
        }
    }

    /// Tests that timestamps are ordered correctly by time, seq, node.
    #[test]
    fn timestamp_order() {
        // Generate timestamps with all combinations of time, seq, and node in [1, 3] in order.
        let original = iproduct!(1..=3, 1..=3, 1..=3)
            .map(|(time, seq, node)| Timestamp { time, seq, node })
            .collect_vec();

        // Create a randomized copy of the timestamps, and sort them.
        let mut timestamps = original.clone();
        timestamps.shuffle(&mut thread_rng());
        timestamps.sort();

        // They should be in the same order as the original generated list.
        assert_eq!(timestamps, original);
    }

    /// Tests that [`Clock::now`] uses the time source when it advances.
    #[test]
    fn clock_now_time_source_ticks() {
        let clock = Clock::with_time_source(NODE, QueuedTimeSource::new([1, 2]));

        assert_eq!(clock.now(), Timestamp { time: 1, seq: 0, node: NODE });
        assert_eq!(clock.now(), Timestamp { time: 2, seq: 0, node: NODE });
    }

    /// Tests that [`Clock::now`] increments the sequence number when the time source does not
    /// advance. It then resets the sequence number when the time source advances again.
    #[test]
    fn clock_now_time_source_stalls() {
        let clock = Clock::with_time_source(NODE, QueuedTimeSource::new([1, 1, 1, 2, 2, 3]));

        assert_eq!(clock.now(), Timestamp { time: 1, seq: 0, node: NODE });
        assert_eq!(clock.now(), Timestamp { time: 1, seq: 1, node: NODE });
        assert_eq!(clock.now(), Timestamp { time: 1, seq: 2, node: NODE });
        assert_eq!(clock.now(), Timestamp { time: 2, seq: 0, node: NODE });
        assert_eq!(clock.now(), Timestamp { time: 2, seq: 1, node: NODE });
        assert_eq!(clock.now(), Timestamp { time: 3, seq: 0, node: NODE });
    }

    /// Tests that [`Clock::observe`] advances the clock such that future timestamps sort after it.
    #[test]
    fn clock_observe_advances() {
        let clock = Clock::with_time_source(NODE, QueuedTimeSource::new([1, 2, 3, 4, 5]));

        assert_eq!(clock.now(), Timestamp { time: 1, seq: 0, node: NODE });

        clock.observe(Timestamp { time: 3, seq: 5, node: PEER });

        assert_eq!(clock.now(), Timestamp { time: 3, seq: 6, node: NODE });
        assert_eq!(clock.now(), Timestamp { time: 3, seq: 7, node: NODE });
        assert_eq!(clock.now(), Timestamp { time: 4, seq: 0, node: NODE });
    }

    /// Tests that [`Clock::observe`] ignores timestamps older than the last timestamp emitted by
    /// [`Clock::now`].
    #[test]
    fn clock_observe_ignores_older() {
        let clock = Clock::with_time_source(NODE, QueuedTimeSource::new([1, 2, 3]));

        assert_eq!(clock.now(), Timestamp { time: 1, seq: 0, node: NODE });
        assert_eq!(clock.now(), Timestamp { time: 2, seq: 0, node: NODE });

        clock.observe(Timestamp { time: 1, seq: 2, node: PEER });

        assert_eq!(clock.now(), Timestamp { time: 3, seq: 0, node: NODE });
    }

    /// Tests that [`Clock::observe`] ignores timestamps older than the last observed timestamp.
    #[test]
    fn clock_observe_ignores_older_observed() {
        let clock = Clock::with_time_source(NODE, QueuedTimeSource::new([1, 2, 3]));

        clock.observe(Timestamp { time: 2, seq: 5, node: PEER });
        clock.observe(Timestamp { time: 1, seq: 10, node: PEER });

        assert_eq!(clock.now(), Timestamp { time: 2, seq: 6, node: NODE });
        assert_eq!(clock.now(), Timestamp { time: 2, seq: 7, node: NODE });
        assert_eq!(clock.now(), Timestamp { time: 3, seq: 0, node: NODE });
    }
}
