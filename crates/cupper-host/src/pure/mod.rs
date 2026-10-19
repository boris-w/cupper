//! The shape of the host's pure cores (design 5.2.16).
//!
//! Stateful decisions (`TurnMachine`, `QueueCore`, `InteractionBroker`,
//! `AutonomousJudge`, `Coalescer`) are written as a [`Core`]: a state machine that
//! takes one input at a time with the current time and returns the actions to
//! take. A core does no I/O, never awaits and never reads the clock, so tests
//! drive it on virtual time (see `CoreDriver`, built for tests and
//! `test-harness`) and get the same actions on every run.
//!
//! The actor that owns a core is an adapter around this loop:
//!
//! 1. turn a mailbox message into an input and call [`Core::step`] with
//!    [`Clock::now`](crate::clock::Clock::now);
//! 2. hand every action that [`Core::timer_request`] recognises to a
//!    [`TimerTable`], and execute the rest (append events, call the driver,
//!    reply to a command), reporting results back as new inputs;
//! 3. keep one [`Clock::timer_at`](crate::clock::Clock::timer_at) armed for
//!    [`TimerTable::next_deadline`]; when it fires, feed every
//!    [`TimerTable::pop_due`] key back through [`Core::timer_expired`].
//!
//! A timer cancelled or moved by a step never reaches the core afterwards, so a
//! core sees an expiry only for a timer it still has set. A core that needs IDs
//! mints them with [`IdGen::new_id_at`](crate::clock::IdGen::new_id_at) at
//! `now.wall_ms`, so they match the time of the step.

#[cfg(any(test, feature = "test-harness"))]
mod driver;

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::hash::Hash;

use crate::clock::{MonoTime, Now};

#[cfg(any(test, feature = "test-harness"))]
pub use driver::{CoreDriver, Step};

/// A pure core: `step(&mut self, input, now) -> Vec<Action>`.
pub trait Core {
    type Input;
    type Action;
    /// Names a timer the core can set; setting a name again moves that timer.
    /// A core without timers uses [`std::convert::Infallible`].
    type TimerKey: Clone + Eq + Hash + fmt::Debug;

    /// Decides what `input`, arriving at `now`, leads to.
    fn step(&mut self, input: Self::Input, now: Now) -> Vec<Self::Action>;

    /// The timer request carried by `action`, if it is one.
    fn timer_request(action: &Self::Action) -> Option<TimerRequest<Self::TimerKey>>;

    /// The input that tells the core its timer `key` expired.
    fn timer_expired(key: Self::TimerKey) -> Self::Input;
}

/// An action that sets or cancels one of a core's timers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimerRequest<K> {
    /// Fire `key` at `at`, replacing any deadline it had.
    Set {
        key: K,
        at: MonoTime,
    },
    Cancel(K),
}

/// The timers a core has set and not yet seen expire, earliest first; timers
/// with the same deadline in the order they were set.
pub struct TimerTable<K> {
    by_key: HashMap<K, (MonoTime, u64)>,
    by_due: BTreeMap<(MonoTime, u64), K>,
    next_seq: u64,
}

impl<K: Clone + Eq + Hash> TimerTable<K> {
    pub fn new() -> Self {
        Self {
            by_key: HashMap::new(),
            by_due: BTreeMap::new(),
            next_seq: 0,
        }
    }

    pub fn apply(&mut self, request: TimerRequest<K>) {
        match request {
            TimerRequest::Set { key, at } => {
                let due = (at, self.next_seq);
                self.next_seq += 1;
                if let Some(old) = self.by_key.insert(key.clone(), due) {
                    self.by_due.remove(&old);
                }
                self.by_due.insert(due, key);
            }
            TimerRequest::Cancel(key) => {
                if let Some(old) = self.by_key.remove(&key) {
                    self.by_due.remove(&old);
                }
            }
        }
    }

    pub fn next_deadline(&self) -> Option<MonoTime> {
        self.by_due.keys().next().map(|(at, _)| *at)
    }

    /// Removes and returns the earliest timer due at `now`.
    pub fn pop_due(&mut self, now: MonoTime) -> Option<K> {
        let entry = self.by_due.first_entry()?;
        if entry.key().0 > now {
            return None;
        }
        let key = entry.remove();
        self.by_key.remove(&key);
        Some(key)
    }

    /// The pending timers, earliest first.
    pub fn iter(&self) -> impl Iterator<Item = (MonoTime, &K)> {
        self.by_due.iter().map(|((at, _), key)| (*at, key))
    }

    pub fn is_empty(&self) -> bool {
        self.by_due.is_empty()
    }
}

impl<K: Clone + Eq + Hash> Default for TimerTable<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: fmt::Debug> fmt::Debug for TimerTable<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map()
            .entries(self.by_due.iter().map(|((at, _), key)| (key, at)))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn at(ms: u64) -> MonoTime {
        MonoTime::from_origin(Duration::from_millis(ms))
    }

    #[test]
    fn timer_table_orders_by_deadline_then_by_setting() {
        let mut table = TimerTable::new();
        for (key, ms) in [("c", 30), ("a", 10), ("b1", 20), ("b2", 20), ("gone", 5)] {
            table.apply(TimerRequest::Set { key, at: at(ms) });
        }
        table.apply(TimerRequest::Cancel("gone"));
        table.apply(TimerRequest::Set {
            key: "a",
            at: at(25),
        });
        assert_eq!(table.next_deadline(), Some(at(20)));
        let pending: Vec<_> = table.iter().map(|(_, key)| *key).collect();
        assert_eq!(pending, ["b1", "b2", "a", "c"]);

        assert_eq!(table.pop_due(at(19)), None);
        let mut fired = Vec::new();
        while let Some(key) = table.pop_due(at(25)) {
            fired.push(key);
        }
        assert_eq!(fired, ["b1", "b2", "a"]);
        assert_eq!(table.pop_due(at(100)), Some("c"));
        assert!(table.is_empty());
    }
}
