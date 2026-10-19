//! The host's only source of time (design 5.2.16).
//!
//! Every reading of the monotonic or the wall clock and every timer goes through a
//! [`Clock`], and every UUIDv7 through an [`IdGen`] on that clock, so tests can
//! run the host on virtual time. The clippy `disallowed-methods` list bans the std
//! and tokio time calls and `Uuid::now_v7` everywhere else; [`SystemClock`] is the
//! one place that reads real time.

mod id;
#[cfg(any(test, feature = "test-harness"))]
mod manual;
mod system;

use std::fmt;
use std::future::Future;
use std::ops::Add;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

pub use id::IdGen;
#[cfg(any(test, feature = "test-harness"))]
pub use manual::ManualClock;
pub use system::SystemClock;

/// A reading of a clock's monotonic time: how long after the clock's origin.
/// Readings of one clock compare and subtract; readings of different clocks
/// mean nothing to each other, so the host shares a single clock.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MonoTime(Duration);

impl MonoTime {
    /// The clock's origin.
    pub const ZERO: Self = Self(Duration::ZERO);

    pub const fn from_origin(elapsed: Duration) -> Self {
        Self(elapsed)
    }

    pub const fn since_origin(self) -> Duration {
        self.0
    }

    /// The time from `earlier` to `self`, or zero if `earlier` is later.
    pub fn saturating_duration_since(self, earlier: Self) -> Duration {
        self.0.saturating_sub(earlier.0)
    }
}

impl Add<Duration> for MonoTime {
    type Output = Self;

    fn add(self, rhs: Duration) -> Self {
        Self(self.0 + rhs)
    }
}

/// Both clock readings, taken together: what a pure core's `step` receives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Now {
    pub mono: MonoTime,
    /// Milliseconds since the Unix epoch.
    pub wall_ms: u64,
}

pub trait Clock: Send + Sync + fmt::Debug {
    fn now_mono(&self) -> MonoTime;

    /// Milliseconds since the Unix epoch.
    fn now_wall_ms(&self) -> u64;

    fn now(&self) -> Now {
        Now {
            mono: self.now_mono(),
            wall_ms: self.now_wall_ms(),
        }
    }

    /// A timer that completes once the monotonic time reaches `deadline`, at once
    /// if it already has. Dropping the timer cancels it.
    fn timer_at(&self, deadline: MonoTime) -> Timer;
}

/// A pending [`Clock::timer_at`]. Await it to wait; drop it to cancel.
#[must_use = "a timer does nothing unless awaited; dropping it cancels it"]
pub struct Timer {
    deadline: MonoTime,
    kind: TimerKind,
}

enum TimerKind {
    System(Pin<Box<tokio::time::Sleep>>),
    #[cfg(any(test, feature = "test-harness"))]
    Manual(manual::ManualTimer),
}

impl Timer {
    pub fn deadline(&self) -> MonoTime {
        self.deadline
    }
}

impl Future for Timer {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        match &mut self.get_mut().kind {
            TimerKind::System(sleep) => sleep.as_mut().poll(cx),
            #[cfg(any(test, feature = "test-harness"))]
            TimerKind::Manual(timer) => timer.poll(cx),
        }
    }
}

impl fmt::Debug for Timer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Timer")
            .field("deadline", &self.deadline)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    #[test]
    fn clock_module_is_the_only_disallowed_methods_exemption() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        for config in ["clippy.toml", ".clippy.toml"] {
            assert!(
                !crate_dir.join(config).exists(),
                "a crate-level {config} would replace the workspace ban list"
            );
        }
        let clock_dir = crate_dir.join("src").join("clock");
        let mut offenders = Vec::new();
        let mut dirs = vec![crate_dir.join("src"), crate_dir.join("tests")];
        while let Some(dir) = dirs.pop() {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for path in entries.map(|entry| entry.expect("read a directory entry").path()) {
                if path.is_dir() {
                    dirs.push(path);
                } else if is_rust_outside(&path, &clock_dir)
                    && fs::read_to_string(&path)
                        .expect("read a source file")
                        .contains("disallowed_methods")
                {
                    offenders.push(path);
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "only `clock` may lift `clippy::disallowed_methods`: {offenders:?}"
        );
    }

    fn is_rust_outside(path: &Path, dir: &Path) -> bool {
        path.extension().is_some_and(|ext| ext == "rs") && !path.starts_with(dir)
    }
}
