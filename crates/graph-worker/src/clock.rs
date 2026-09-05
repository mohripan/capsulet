//! Time, supplied rather than read.
//!
//! The decision core takes `now` as a parameter so that deciding is replayable.
//! That guarantee only holds if the thing calling it is also honest about where
//! its time came from, which is why the worker holds a clock rather than
//! calling `SystemTime::now` at each site. A test can then drive a run through
//! a timer without waiting for one.

use std::time::{SystemTime, UNIX_EPOCH};

use capsulet_ir::correctness::evidence::RecordedTime;

/// Where the worker gets the current time.
pub trait Clock: Send + Sync {
    fn now(&self) -> RecordedTime;
}

/// The host's clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> RecordedTime {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_millis());
        RecordedTime(i64::try_from(millis).unwrap_or(i64::MAX))
    }
}
