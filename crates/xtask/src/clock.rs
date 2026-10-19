//! The only place in xtask that reads the system clock. xtask has no `Clock`
//! (design 5.2.16), so the clippy `disallowed-methods` ban is lifted here alone.
#![expect(
    clippy::disallowed_methods,
    reason = "xtask's single time source: step durations and log directory names"
)]

use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub struct Stopwatch(Instant);

impl Stopwatch {
    pub fn start() -> Self {
        Self(Instant::now())
    }

    pub fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.0.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

/// Milliseconds since the Unix epoch, for unique log directory names.
pub fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis())
}
