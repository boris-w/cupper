//! The quiescence barrier (design 5.2.16, plan 1.4).
//!
//! `test/clock_advance` returns only once everything a clock advance set off has
//! settled: fired timers handled, the resulting actions executed, writes
//! committed, events fanned out. To wait for that, the barrier has to know every
//! actor that could still be working. So each kind of actor is registered here
//! when the host is assembled, and each running actor holds a [`Participant`]
//! for as long as it runs. Starting an actor of a kind nobody registered fails a
//! debug assertion, so a new actor cannot silently escape the barrier.
//!
//! A participant is busy unless it waits in [`Participant::idle`], and only
//! while the future it waits on is pending: the waker it hands that future
//! marks it busy again before any wake goes through, so a message sent to it
//! or a timer fired for it counts from that moment (rulings before T047).
//! Work that runs apart from its starter, such as a spawned task or a job
//! queued for the writer thread, holds a [`Hold`] until it is done. Waits on
//! the outside world (a peer's socket, an agent's output) are idle: the
//! barrier is for timers, not for other processes (design 5.2.16).

use std::future::{Future, poll_fn};
use std::pin::pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Wake, Waker};

use tokio::sync::Notify;

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
    shared: Arc<Shared>,
}

#[derive(Debug, Default)]
struct Shared {
    state: Mutex<State>,
    /// Told whenever the count of busy participants and holds falls.
    calmer: Notify,
}

#[derive(Debug, Default)]
struct State {
    kinds: Vec<Registered>,
    /// Busy participants and live holds.
    busy: usize,
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
            let mut state = self.lock();
            let known = state.kinds.iter().any(|entry| entry.kind == kind);
            if !known {
                state.kinds.push(Registered { kind, running: 0 });
            }
            !known
        };
        debug_assert!(added, "actor kind `{}` is registered twice", kind.name());
    }

    /// The registered kinds, in registration order.
    pub fn kinds(&self) -> Vec<ActorKind> {
        self.lock().kinds.iter().map(|entry| entry.kind).collect()
    }

    /// How many actors of `kind` are running.
    pub fn running(&self, kind: ActorKind) -> usize {
        self.lock()
            .kinds
            .iter()
            .find(|entry| entry.kind == kind)
            .map_or(0, |entry| entry.running)
    }

    /// Called by an actor as it starts, busy; it keeps the result until it
    /// stops. In debug builds, panics if `kind` was never registered.
    pub fn enter(&self, kind: ActorKind) -> Participant {
        // The lock is released before the assertion can panic.
        let registered = {
            let mut state = self.lock();
            match state.kinds.iter_mut().find(|entry| entry.kind == kind) {
                Some(entry) => {
                    entry.running += 1;
                    state.busy += 1;
                    true
                }
                None => false,
            }
        };
        debug_assert!(
            registered,
            "actor kind `{}` started without being registered for the quiescence barrier",
            kind.name()
        );
        Participant {
            registry: self.clone(),
            kind,
            flag: Arc::new(Flag {
                registry: self.clone(),
                state: Mutex::new(FlagState {
                    busy: registered,
                    wakes: 0,
                    stopped: false,
                }),
            }),
            counted: registered,
        }
    }

    /// Keeps the barrier from settling until dropped: for work its starter
    /// does not wait on, taken before the work is handed over.
    pub fn hold(&self) -> Hold {
        self.lock().busy += 1;
        Hold {
            registry: self.clone(),
        }
    }

    /// Completes once every participant is idle and nothing is held.
    pub async fn settled(&self) {
        loop {
            let mut calmer = pin!(self.shared.calmer.notified());
            calmer.as_mut().enable();
            if self.lock().busy == 0 {
                return;
            }
            calmer.await;
        }
    }

    fn calmer(&self) {
        let mut state = self.lock();
        state.busy -= 1;
        drop(state);
        self.shared.calmer.notify_waiters();
    }

    /// Every critical section leaves the state consistent, so a poisoned
    /// lock is still usable.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// A running actor's place in the barrier; dropping it marks the actor stopped.
#[derive(Debug)]
#[must_use = "the actor counts as running only while it holds its participant"]
pub struct Participant {
    registry: Quiescence,
    kind: ActorKind,
    flag: Arc<Flag>,
    /// False for an unregistered kind in a release build, which is not counted.
    counted: bool,
}

/// Whether a participant is busy, shared with the wakers it hands out.
#[derive(Debug)]
struct Flag {
    registry: Quiescence,
    state: Mutex<FlagState>,
}

#[derive(Debug)]
struct FlagState {
    busy: bool,
    /// Wakes so far: a wait that a wake overtook stays busy.
    wakes: u64,
    /// The participant is gone; a late wake changes nothing.
    stopped: bool,
}

impl Flag {
    fn lock(&self) -> MutexGuard<'_, FlagState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn busy(&self) {
        let mut state = self.lock();
        state.wakes += 1;
        if !state.busy && !state.stopped {
            state.busy = true;
            self.registry.lock().busy += 1;
        }
    }

    /// Idle, unless woken since `wakes`.
    fn idle(&self, wakes: u64) {
        let mut state = self.lock();
        if state.busy && state.wakes == wakes {
            state.busy = false;
            drop(state);
            self.registry.calmer();
        }
    }
}

/// The waker a participant's wait gets: busy first, then the task's own wake.
struct Waking {
    flag: Arc<Flag>,
    task: Waker,
}

impl Wake for Waking {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.flag.busy();
        self.task.wake_by_ref();
    }
}

impl Participant {
    pub fn kind(&self) -> ActorKind {
        self.kind
    }

    /// Awaits `fut`, the actor's wait for its next work: idle while it is
    /// pending, busy from any wake on, and while it runs after.
    pub async fn idle<F: Future>(&self, fut: F) -> F::Output {
        let mut fut = pin!(fut);
        poll_fn(|cx| {
            if !self.counted {
                return fut.as_mut().poll(cx);
            }
            let wakes = self.flag.lock().wakes;
            let waker = Waker::from(Arc::new(Waking {
                flag: Arc::clone(&self.flag),
                task: cx.waker().clone(),
            }));
            match fut.as_mut().poll(&mut Context::from_waker(&waker)) {
                Poll::Ready(out) => Poll::Ready(out),
                Poll::Pending => {
                    self.flag.idle(wakes);
                    Poll::Pending
                }
            }
        })
        .await
    }

    /// As [`idle`](Self::idle), for a thread blocking in `wait`; whoever gives
    /// it work holds the barrier meanwhile.
    pub fn idle_blocking<T>(&self, wait: impl FnOnce() -> T) -> T {
        if self.counted {
            let wakes = self.flag.lock().wakes;
            self.flag.idle(wakes);
        }
        let out = wait();
        if self.counted {
            self.flag.busy();
        }
        out
    }
}

impl Drop for Participant {
    fn drop(&mut self) {
        if !self.counted {
            return;
        }
        let busy = {
            let mut flag = self.flag.lock();
            flag.stopped = true;
            flag.busy
        };
        let mut state = self.registry.lock();
        if let Some(entry) = state.kinds.iter_mut().find(|e| e.kind == self.kind) {
            entry.running -= 1;
        }
        drop(state);
        if busy {
            self.registry.calmer();
        }
    }
}

/// Work in flight apart from its starter; the barrier waits for it.
#[derive(Debug)]
#[must_use = "the work counts only while its hold lives"]
pub struct Hold {
    registry: Quiescence,
}

impl Drop for Hold {
    fn drop(&mut self) {
        self.registry.calmer();
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

    fn settled_now(registry: &Quiescence) -> bool {
        use std::task::Context;
        let settled = pin!(registry.settled());
        settled
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_ready()
    }

    /// A participant is busy until it waits, from the wake on that brings it
    /// work, and until the work is done; a hold is busy until dropped.
    #[tokio::test]
    async fn unit_quiescence_barrier_counts_busy_participants_wakes_and_holds() {
        let registry = Quiescence::new();
        registry.register(SESSION);
        let participant = registry.enter(SESSION);
        assert!(!settled_now(&registry), "busy as it starts");
        let (mail, mut inbox) = tokio::sync::mpsc::unbounded_channel::<u32>();
        let (done, mut handled) = tokio::sync::mpsc::unbounded_channel();
        let actor = tokio::spawn(async move {
            while let Some(n) = participant.idle(inbox.recv()).await {
                done.send(n).unwrap();
            }
        });
        registry.settled().await;
        mail.send(1).unwrap();
        assert!(!settled_now(&registry), "busy from the wake on");
        registry.settled().await;
        assert_eq!(handled.try_recv(), Ok(1));

        let hold = registry.hold();
        assert!(!settled_now(&registry));
        drop(hold);
        assert!(settled_now(&registry));

        drop(mail);
        actor.await.unwrap();
        assert_eq!(registry.running(SESSION), 0);
        assert!(settled_now(&registry));
    }

    /// A timer fired by a manual clock makes its waiter busy at once.
    #[tokio::test]
    async fn unit_quiescence_barrier_counts_fired_timers() {
        use std::time::Duration;

        use crate::clock::{Clock, ManualClock};

        let (registry, clock) = (Quiescence::new(), ManualClock::new());
        registry.register(INDEX);
        let participant = registry.enter(INDEX);
        let timer = clock.timer_at(clock.now_mono() + Duration::from_secs(1));
        let waiting = tokio::spawn(async move { participant.idle(timer).await });
        registry.settled().await;
        clock.advance(Duration::from_secs(1));
        assert!(!settled_now(&registry), "busy as the timer fires");
        waiting.await.unwrap();
        assert!(settled_now(&registry));
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
