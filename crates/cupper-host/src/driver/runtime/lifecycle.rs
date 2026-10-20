//! An adapter's lifecycle (design 5.6.8, ADR-09): when a failed adapter may
//! start again, the process groups an instance leaves, and what its
//! processes use.

use std::collections::VecDeque;
use std::path::Path;
use std::time::Duration;

use cupper_sys::{ProcessGroup, ProcessInfo, pid_path};

use crate::clock::MonoTime;
use crate::durations::{ADAPTER_BACKOFF, ADAPTER_BACKOFF_MAX, ADAPTER_CRASH_WINDOW, Durations};

/// Failures within the window that make the agent `crashed`.
const CRASH_FAILURES: usize = 5;

/// When the next instance may start: after a failure, 1 s, doubling with
/// each failure in the last five minutes up to 30 s; never once five failed
/// within them (`crashed`), until [`clear`](Self::clear). A pure core.
#[derive(Debug)]
pub struct Restarts {
    failures: VecDeque<MonoTime>,
    crashed: bool,
    backoff: (Duration, Duration),
    window: Duration,
}

impl Restarts {
    pub fn new(durations: &Durations) -> Self {
        Self {
            failures: VecDeque::new(),
            crashed: false,
            backoff: (
                durations.get(ADAPTER_BACKOFF),
                durations.get(ADAPTER_BACKOFF_MAX),
            ),
            window: durations.get(ADAPTER_CRASH_WINDOW),
        }
    }

    /// An instance failed at `now`: it did not start, or ended unasked.
    pub fn failed(&mut self, now: MonoTime) {
        let window = self.window;
        let old = |at: &MonoTime| now.saturating_duration_since(*at) >= window;
        while self.failures.front().is_some_and(old) {
            self.failures.pop_front();
        }
        self.failures.push_back(now);
        self.crashed |= self.failures.len() >= CRASH_FAILURES;
    }

    /// The earliest time the next instance may start; `None` while crashed.
    pub fn next_start(&self) -> Option<MonoTime> {
        let Some(last) = self.failures.back().filter(|_| !self.crashed) else {
            return (!self.crashed).then_some(MonoTime::ZERO);
        };
        let doublings = u32::try_from(self.failures.len() - 1).unwrap_or(u32::MAX);
        let (base, max) = self.backoff;
        Some(*last + base.saturating_mul(2u32.saturating_pow(doublings)).min(max))
    }

    /// `agents/restart` (T111): the failures are forgotten.
    pub fn clear(&mut self) {
        (self.failures, self.crashed) = (VecDeque::new(), false);
    }
}

/// `groups` and those of `more` it does not have, by group id.
pub fn union(mut groups: Vec<ProcessGroup>, more: &[ProcessGroup]) -> Vec<ProcessGroup> {
    for group in more {
        if !groups.iter().any(|g| g.pgid == group.pgid) {
            groups.push(*group);
        }
    }
    groups
}

/// Sends `signal` to each group that is still the recorded one.
pub fn signal(groups: &[ProcessGroup], signal: cupper_sys::Signal) {
    for group in groups {
        let _ = group.signal(signal);
    }
}

/// One sample of what an agent's adapter uses (design 5.2.13, 5.6.8).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Resources {
    /// The resident memory of all its process groups, in bytes.
    pub rss_bytes: u64,
    /// The `claude` processes among them (Q43).
    pub claude_processes: usize,
}

/// Samples `group` and the groups of its descendants, which it returns too.
/// A `claude` process runs `cli`, the CLI found (its real path); `rss`
/// stands in for the measured memory (`test_config.rss_samples`).
pub fn sample(
    group: &ProcessGroup,
    cli: Option<&Path>,
    rss: Option<u64>,
) -> (Resources, Vec<ProcessGroup>) {
    let groups = group.with_descendants();
    let members: Vec<ProcessInfo> = groups.iter().flat_map(ProcessGroup::members).collect();
    let measured = members
        .iter()
        .filter_map(|m| cupper_sys::resident_size(m.pid))
        .sum();
    let runs_cli =
        |m: &&ProcessInfo| cli.is_some_and(|cli| pid_path(m.pid).is_ok_and(|p| p == cli));
    let claude_processes = members.iter().filter(runs_cli).count();
    let rss_bytes = rss.unwrap_or(measured);
    let resources = Resources {
        rss_bytes,
        claude_processes,
    };
    (resources, groups[1..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restarts_back_off_and_crash_after_five_failures_in_the_window() {
        let at = |s: u64| MonoTime::from_origin(Duration::from_secs(s));
        let mut restarts = Restarts::new(&Durations::default());
        assert_eq!(restarts.next_start(), Some(MonoTime::ZERO));
        // Each failure doubles the wait; one outside the window is forgotten.
        let steps = [
            (0, 1),
            (10, 12),
            (400, 401),
            (401, 403),
            (403, 407),
            (407, 415),
        ];
        for (failed, next) in steps {
            restarts.failed(at(failed));
            assert_eq!(restarts.next_start(), Some(at(next)), "failed at {failed}");
        }
        restarts.failed(at(415));
        assert_eq!(restarts.next_start(), None);
        restarts.clear();
        assert_eq!(restarts.next_start(), Some(MonoTime::ZERO));
    }
}
