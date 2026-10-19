//! The production clock: the only code in the host that reads real time.
#![expect(
    clippy::disallowed_methods,
    reason = "SystemClock is the host's single reader of the real clock (design 5.2.16)"
)]

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::{Clock, MonoTime, Timer, TimerKind};

/// Where a deadline too far out to represent is parked, as tokio does.
const FAR_FUTURE: Duration = Duration::from_secs(86_400 * 365 * 30);

/// Real time. Its monotonic origin is the moment it was created, so the host
/// creates one and shares it.
#[derive(Debug, Clone, Copy)]
pub struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now_mono(&self) -> MonoTime {
        MonoTime::from_origin(self.origin.elapsed())
    }

    fn now_wall_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| {
                u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
            })
    }

    /// Must be called inside the tokio runtime, whose time driver runs the timer.
    fn timer_at(&self, deadline: MonoTime) -> Timer {
        let at = self
            .origin
            .checked_add(deadline.since_origin())
            .unwrap_or_else(|| self.origin + FAR_FUTURE);
        Timer {
            deadline,
            kind: TimerKind::System(Box::pin(tokio::time::sleep_until(at.into()))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clock_reads_real_time_and_fires_due_timers() {
        let clock = SystemClock::new();
        let first = clock.now_mono();
        assert!(clock.now_mono() >= first);
        // 2026-01-01T00:00:00Z: the clock is not stuck at the epoch.
        assert!(clock.now_wall_ms() > 1_767_225_600_000);

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("build a runtime");
        // A deadline in the past completes without waiting.
        runtime.block_on(async { clock.timer_at(first).await });
    }
}
