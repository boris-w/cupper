//! Participants of the quiescence barrier (design 5.2.16, plan 1.4).
//!
//! `test/clock_advance` returns only once everything a clock advance set off has
//! settled: fired timers handled, the resulting actions executed, writes
//! committed, events fanned out. To wait for that, the barrier has to know every
//! actor that could still be working. So each kind of actor is registered here
//! when the host is assembled, and each running actor holds a [`Participant`]
//! for as long as it runs. Starting an actor of a kind nobody registered fails a
//! debug assertion, so a new actor cannot silently escape the barrier.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// The name of a kind of actor, e.g. `ActorKind::new("session")`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ActorKind(&'static str);

impl ActorKind {
    pub const fn new(name: &'static str) -> Self {
        Self(name)
    }

    pub const fn name(self) -> &'static str {
        self.0
    }
}

/// The registry of barrier participants. Clones share it; the host has one.
#[derive(Debug, Clone, Default)]
pub struct Quiescence {
    shared: Arc<Mutex<Vec<Registered>>>,
}

#[derive(Debug)]
struct Registered {
    kind: ActorKind,
    running: usize,
}

impl Quiescence {
    pub fn new() -> Self {
        Self::default()
    }

    /// Declares that actors of `kind` take part in the barrier. Registering a
    /// kind twice fails a debug assertion: two actors would share a name.
    pub fn register(&self, kind: ActorKind) {
        let added = {
            let mut kinds = self.lock();
            let known = kinds.iter().any(|entry| entry.kind == kind);
            if !known {
                kinds.push(Registered { kind, running: 0 });
            }
            !known
        };
        debug_assert!(added, "actor kind `{}` is registered twice", kind.name());
    }

    /// The registered kinds, in registration order.
    pub fn kinds(&self) -> Vec<ActorKind> {
        self.lock().iter().map(|entry| entry.kind).collect()
    }

    /// How many actors of `kind` are running.
    pub fn running(&self, kind: ActorKind) -> usize {
        self.lock()
            .iter()
            .find(|entry| entry.kind == kind)
            .map_or(0, |entry| entry.running)
    }

    /// Called by an actor as it starts; it keeps the result until it stops. In
    /// debug builds, panics if `kind` was never registered.
    pub fn enter(&self, kind: ActorKind) -> Participant {
        // The lock is released before the assertion can panic.
        let registered = match self.lock().iter_mut().find(|entry| entry.kind == kind) {
            Some(entry) => {
                entry.running += 1;
                true
            }
            None => false,
        };
        debug_assert!(
            registered,
            "actor kind `{}` started without being registered for the quiescence barrier",
            kind.name()
        );
        Participant {
            registry: self.clone(),
            kind,
            counted: registered,
        }
    }

    /// Every critical section leaves the list consistent, so a poisoned lock
    /// is still usable.
    fn lock(&self) -> MutexGuard<'_, Vec<Registered>> {
        self.shared.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A running actor's place in the barrier; dropping it marks the actor stopped.
#[derive(Debug)]
#[must_use = "the actor counts as running only while it holds its participant"]
pub struct Participant {
    registry: Quiescence,
    kind: ActorKind,
    /// False for an unregistered kind in a release build, which is not counted.
    counted: bool,
}

impl Participant {
    pub fn kind(&self) -> ActorKind {
        self.kind
    }
}

impl Drop for Participant {
    fn drop(&mut self) {
        if !self.counted {
            return;
        }
        if let Some(entry) = self
            .registry
            .lock()
            .iter_mut()
            .find(|entry| entry.kind == self.kind)
        {
            entry.running -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SESSION: ActorKind = ActorKind::new("session");
    const INDEX: ActorKind = ActorKind::new("index");

    #[test]
    fn unit_quiescence_registry_enumerates_registered_kinds() {
        let registry = Quiescence::new();
        assert!(registry.kinds().is_empty());
        registry.register(SESSION);
        registry.register(INDEX);
        assert_eq!(registry.kinds(), [SESSION, INDEX]);

        let first = registry.enter(SESSION);
        let second = registry.clone().enter(SESSION);
        assert_eq!(first.kind(), SESSION);
        assert_eq!(registry.running(SESSION), 2);
        assert_eq!(registry.running(INDEX), 0);
        drop(first);
        assert_eq!(registry.running(SESSION), 1);
        drop(second);
        assert_eq!(registry.running(SESSION), 0);
        assert_eq!(registry.kinds(), [SESSION, INDEX]);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "actor kind `index` started without being registered")]
    fn unit_quiescence_registry_unregistered_start_fails_debug_assertion() {
        let registry = Quiescence::new();
        registry.register(SESSION);
        let _participant = registry.enter(INDEX);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "actor kind `session` is registered twice")]
    fn unit_quiescence_registry_duplicate_kind_fails_debug_assertion() {
        let registry = Quiescence::new();
        registry.register(SESSION);
        registry.register(SESSION);
    }
}
