use std::sync::{Arc, Mutex};

pub type NodeID = u32;

/// A hybrid logical clock.
pub struct Clock {
    last: Arc<Mutex<Timestamp>>,
}

impl Clock {
    /// Creates a new HLC.
    pub fn new(node_id: NodeID) -> Self {
        let last = Timestamp { walltime: 0, logical: 0, node_id };
        Self { last: Arc::new(Mutex::new(last)) }
    }

    /// Returns the current HLC time.
    pub fn now(&self) -> Timestamp {
        let mut last = self.last.lock().expect("mutex failed");
        let walltime = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("invalid timestamp before UNIX epoch")
            .as_nanos()
            .try_into()
            .expect("invalid timestamp overflowed u64"); // in 500,000 years
        if walltime <= last.walltime {
            last.logical += 1;
        } else {
            last.walltime = walltime;
            last.logical = 0;
        }
        *last
    }

    /// Updates the HLC with a remote event timestamp.
    pub fn update(&mut self, event: Timestamp) {
        let mut last = self.last.lock().expect("mutex failed");
        if event > *last {
            last.walltime = event.walltime;
            last.logical = event.logical;
        }
    }
}

/// An HLC timestamp.
#[derive(Copy, Clone, Default, PartialEq, PartialOrd)]
pub struct Timestamp {
    /// Wall time in nanoseconds since UNIX epoch.
    walltime: u64,
    /// The logical time.
    logical: u32,
    /// The node ID on which the timestamp originated.
    node_id: NodeID,
}
