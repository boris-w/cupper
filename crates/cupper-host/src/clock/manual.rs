//! Virtual time for tests: it moves only when told to.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use super::{Clock, MonoTime, Timer, TimerKind};

/// A clock that stands still until [`advance`](Self::advance) or
/// [`advance_to`](Self::advance_to) moves it. The wall clock moves with the
/// monotonic clock, millisecond for millisecond. Clones share one timeline.
///
/// Advancing wakes every timer that came due, in deadline order (ties in the
/// order the timers were created), after the new time is in place: a woken task
/// reads the time it was advanced to. To act on each deadline before the next
/// one fires, step through [`next_deadline`](Self::next_deadline) with
/// `advance_to` and wait for the work in between.
#[derive(Clone)]
pub struct ManualClock {
    shared: Arc<Mutex<State>>,
}

struct State {
    now: MonoTime,
    start_wall_ms: u64,
    next_seq: u64,
    /// Timers not yet due, by `(deadline, creation order)`, with the waker of the
    /// task waiting on each.
    pending: BTreeMap<TimerKey, Option<Waker>>,
}

type TimerKey = (MonoTime, u64);

impl ManualClock {
    /// 2026-01-01T00:00:00Z.
    pub const DEFAULT_START_WALL_MS: u64 = 1_767_225_600_000;

    /// Monotonic time at [`MonoTime::ZERO`], wall time at
    /// [`DEFAULT_START_WALL_MS`](Self::DEFAULT_START_WALL_MS).
    pub fn new() -> Self {
        Self::starting_at(Self::DEFAULT_START_WALL_MS)
    }

    pub fn starting_at(wall_ms: u64) -> Self {
        Self {
            shared: Arc::new(Mutex::new(State {
                now: MonoTime::ZERO,
                start_wall_ms: wall_ms,
                next_seq: 0,
                pending: BTreeMap::new(),
            })),
        }
    }

    pub fn advance(&self, by: Duration) {
        self.fire(|now| now + by);
    }

    /// Panics if `to` is in the past: time never goes back.
    pub fn advance_to(&self, to: MonoTime) {
        self.fire(|now| {
            assert!(
                to >= now,
                "ManualClock cannot go back from {now:?} to {to:?}"
            );
            to
        });
    }

    /// The earliest deadline of a timer that has not fired or been dropped.
    pub fn next_deadline(&self) -> Option<MonoTime> {
        lock(&self.shared)
            .pending
            .first_key_value()
            .map(|((deadline, _), _)| *deadline)
    }

    fn fire(&self, target: impl FnOnce(MonoTime) -> MonoTime) {
        let mut due = Vec::new();
        {
            let mut state = lock(&self.shared);
            let now = target(state.now);
            state.now = now;
            while let Some(entry) = state.pending.first_entry() {
                if entry.key().0 > now {
                    break;
                }
                due.push(entry.remove());
            }
        }
        // Outside the lock: a waker may run arbitrary code.
        for waker in due.into_iter().flatten() {
            waker.wake();
        }
    }
}

impl Default for ManualClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for ManualClock {
    fn now_mono(&self) -> MonoTime {
        lock(&self.shared).now
    }

    fn now_wall_ms(&self) -> u64 {
        lock(&self.shared).wall_ms()
    }

    fn now(&self) -> super::Now {
        let state = lock(&self.shared);
        super::Now {
            mono: state.now,
            wall_ms: state.wall_ms(),
        }
    }

    fn timer_at(&self, deadline: MonoTime) -> Timer {
        let mut state = lock(&self.shared);
        let key = (deadline > state.now).then(|| {
            let key = (deadline, state.next_seq);
            state.next_seq += 1;
            state.pending.insert(key, None);
            key
        });
        Timer {
            deadline,
            kind: TimerKind::Manual(ManualTimer {
                shared: Arc::clone(&self.shared),
                key,
            }),
        }
    }
}

impl fmt::Debug for ManualClock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = lock(&self.shared);
        f.debug_struct("ManualClock")
            .field("now", &state.now)
            .field("wall_ms", &state.wall_ms())
            .field("pending_timers", &state.pending.len())
            .finish()
    }
}

impl State {
    fn wall_ms(&self) -> u64 {
        let elapsed = u64::try_from(self.now.since_origin().as_millis()).unwrap_or(u64::MAX);
        self.start_wall_ms.saturating_add(elapsed)
    }
}

/// The [`Timer`] of a [`ManualClock`]. `key` is `None` when it was due on creation.
pub(super) struct ManualTimer {
    shared: Arc<Mutex<State>>,
    key: Option<TimerKey>,
}

impl ManualTimer {
    pub(super) fn poll(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        let Some(key) = self.key else {
            return Poll::Ready(());
        };
        let mut state = lock(&self.shared);
        if key.0 <= state.now {
            state.pending.remove(&key);
            self.key = None;
            return Poll::Ready(());
        }
        if let Some(waker) = state.pending.get_mut(&key) {
            match waker {
                Some(waker) => waker.clone_from(cx.waker()),
                None => *waker = Some(cx.waker().clone()),
            }
        }
        Poll::Pending
    }
}

impl Drop for ManualTimer {
    fn drop(&mut self) {
        if let Some(key) = self.key {
            lock(&self.shared).pending.remove(&key);
        }
    }
}

/// The state stays consistent through a panic in a critical section, so a
/// poisoned lock is still usable.
fn lock(shared: &Mutex<State>) -> MutexGuard<'_, State> {
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::Pin;
    use std::task::Wake;

    use super::*;

    /// Records, by label, which timers' wakers fired and in what order.
    #[derive(Default)]
    struct WakeLog(Mutex<Vec<&'static str>>);

    struct LabelledWaker {
        label: &'static str,
        log: Arc<WakeLog>,
    }

    impl Wake for LabelledWaker {
        fn wake(self: Arc<Self>) {
            self.log.0.lock().unwrap().push(self.label);
        }
    }

    /// Polls `timer` once with a waker that logs `label`; true if it completed.
    fn poll_once(timer: &mut Timer, label: &'static str, log: &Arc<WakeLog>) -> bool {
        let waker = Waker::from(Arc::new(LabelledWaker {
            label,
            log: Arc::clone(log),
        }));
        Pin::new(timer)
            .poll(&mut Context::from_waker(&waker))
            .is_ready()
    }

    fn at(ms: u64) -> MonoTime {
        MonoTime::from_origin(Duration::from_millis(ms))
    }

    #[test]
    fn unit_clock_manual_fires_due_timers_in_deadline_order() {
        let clock = ManualClock::new();
        let log = Arc::new(WakeLog::default());
        let mut timers = [
            ("c", clock.timer_at(at(30))),
            ("a", clock.timer_at(at(10))),
            ("b1", clock.timer_at(at(20))),
            ("b2", clock.timer_at(at(20))),
            ("late", clock.timer_at(at(1_000))),
        ];
        for (label, timer) in &mut timers {
            assert!(!poll_once(timer, label, &log), "{label} completed early");
        }
        let dropped = clock.timer_at(at(15));
        drop(dropped);
        assert_eq!(clock.next_deadline(), Some(at(10)));

        clock.advance(Duration::from_millis(25));
        assert_eq!(*log.0.lock().unwrap(), ["a", "b1", "b2"]);
        assert_eq!(clock.next_deadline(), Some(at(30)));

        clock.advance_to(at(30));
        assert_eq!(*log.0.lock().unwrap(), ["a", "b1", "b2", "c"]);
        for (label, timer) in &mut timers[..4] {
            assert!(poll_once(timer, label, &log), "{label} did not complete");
        }
        let (label, late) = &mut timers[4];
        assert!(!poll_once(late, label, &log));
        assert_eq!(clock.next_deadline(), Some(at(1_000)));
    }

    #[test]
    fn unit_clock_manual_due_timer_completes_at_once() {
        let clock = ManualClock::new();
        clock.advance(Duration::from_secs(1));
        let log = Arc::new(WakeLog::default());
        for deadline in [at(0), at(1_000)] {
            let mut timer = clock.timer_at(deadline);
            assert!(poll_once(&mut timer, "due", &log));
        }
        assert_eq!(clock.next_deadline(), None);
        assert!(log.0.lock().unwrap().is_empty());
    }

    #[test]
    fn unit_clock_manual_wall_clock_moves_with_monotonic_clock() {
        let clock = ManualClock::starting_at(5_000);
        let start = clock.now();
        assert_eq!(start.mono, MonoTime::ZERO);
        assert_eq!(start.wall_ms, 5_000);

        for step in [1_500_u64, 999, 1, 60_000] {
            clock.advance(Duration::from_micros(step));
            let now = clock.now();
            assert_eq!(now.mono, clock.now_mono());
            assert_eq!(now.wall_ms, clock.now_wall_ms());
            let elapsed = now.mono.saturating_duration_since(start.mono);
            assert_eq!(u128::from(now.wall_ms - start.wall_ms), elapsed.as_millis());
        }
        assert_eq!(
            clock.now_mono(),
            MonoTime::from_origin(Duration::from_micros(62_500))
        );
        assert_eq!(clock.now_wall_ms(), 5_062);
    }

    #[test]
    #[should_panic(expected = "cannot go back")]
    fn unit_clock_manual_never_goes_back() {
        let clock = ManualClock::new();
        clock.advance(Duration::from_millis(10));
        clock.advance_to(at(5));
    }

    /// The runtime has no time driver: a real tokio timer would panic, a real
    /// sleep of an hour would hang.
    #[test]
    fn unit_clock_manual_wakes_waiting_tasks_without_real_sleep() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("build a runtime");
        runtime.block_on(async {
            let clock = ManualClock::new();
            let (done_tx, mut done_rx) = tokio::sync::mpsc::unbounded_channel();
            for (label, secs) in [("hour", 3_600), ("minute", 60)] {
                let timer = clock.timer_at(MonoTime::ZERO + Duration::from_secs(secs));
                let done_tx = done_tx.clone();
                tokio::spawn(async move {
                    timer.await;
                    done_tx.send(label).expect("the test is waiting");
                });
            }
            tokio::task::yield_now().await;
            assert!(done_rx.try_recv().is_err());

            clock.advance(Duration::from_secs(3_600));
            assert_eq!(done_rx.recv().await, Some("minute"));
            assert_eq!(done_rx.recv().await, Some("hour"));
        });
    }
}
