//! The supervisor's watch (design 5.2.13): when the host stops taking new
//! sessions and turns, and when it exits on its own.
//!
//! - **Idle**: no client, no running turn, no pending interaction, no queued
//!   message and the installer idle, for 10 minutes.
//! - **Orphaned** (`dev-auth`): the process that started the host exited. No
//!   new sessions or turns; the exit once no turn runs.
//! - **Pending upgrade**: the code on disk is not what runs ([`SelfCheck`],
//!   or the installer's `Failed(BundleChanged)` through [`Watch`]). No new
//!   sessions or turns; the exit once no turn runs, no interaction waits and
//!   nothing is queued, whatever the clients (ruled before T037).
//!
//! An upgrade comes before the orphaned state, which comes before idleness.
//! Either exit waits for a look taken after new sessions and turns were
//! closed: an earlier one may miss a turn that started since.
//! [`WatchCore`] decides; the actor looks at the clients, the work and the
//! parent every `supervisor_observe` and carries the decisions out. The same
//! actor holds the power assertion ([`power`](super::power)).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use cupper_protocol::error::{ErrorCode, ErrorReason, RpcError};
use cupper_sys::Cdhash;
use tokio::sync::mpsc;

use super::power::{self, Holder, Power};
use super::{Activity, Exit, Part};
use crate::clock::{Clock, MonoTime, Now};
use crate::durations::{Durations, SUPERVISOR_IDLE, SUPERVISOR_OBSERVE};
use crate::gateway::{AuditEvent, AuditRecord, Gateway, HostEvent};
use crate::index::IndexHandle;
use crate::pure::{Alarm, Core, TimerRequest, TimerTable};
use crate::quiescence::{ActorKind, Participant, Quiescence};

/// The supervisor's kind in the quiescence barrier.
pub const SUPERVISOR: ActorKind = ActorKind::new("supervisor");

/// What the supervisor saw.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Observed {
    pub clients: usize,
    pub activity: Activity,
    pub installer_busy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Observed(Observed),
    /// The process that started the host exited.
    Orphaned,
    /// The code on disk is not what runs.
    BundleChanged,
    Expired(WatchTimer),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WatchTimer {
    Idle,
}

/// Why new sessions and turns are refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Closed {
    Orphaned = 1,
    PendingUpgrade = 2,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Close(Closed),
    Exit(Exit),
    Timer(TimerRequest<WatchTimer>),
}

#[derive(Debug)]
pub struct WatchCore {
    idle_for: Duration,
    seen: Option<Observed>,
    orphaned: bool,
    upgrade: bool,
    /// The idle timer runs.
    idle: bool,
    exited: bool,
}

impl WatchCore {
    pub fn new(durations: &Durations) -> Self {
        Self {
            idle_for: durations.get(SUPERVISOR_IDLE),
            seen: None,
            orphaned: false,
            upgrade: false,
            idle: false,
            exited: false,
        }
    }

    fn decide(&mut self, now: MonoTime, out: &mut Vec<Action>) {
        let Some(seen) = self.seen else {
            return;
        };
        let work = seen.activity;
        let settled = work.running_turns == 0 && work.pending_interactions == 0;
        let settled = settled && work.queued_messages == 0;
        if self.upgrade && settled {
            return self.exit(Exit::Upgrade, out);
        }
        if self.orphaned && work.running_turns == 0 {
            return self.exit(Exit::Orphaned, out);
        }
        let idle = seen.clients == 0 && settled && !seen.installer_busy;
        if idle != self.idle {
            self.idle = idle;
            let key = WatchTimer::Idle;
            out.push(Action::Timer(match idle {
                true => TimerRequest::Set {
                    key,
                    at: now + self.idle_for,
                },
                false => TimerRequest::Cancel(key),
            }));
        }
    }

    fn exit(&mut self, exit: Exit, out: &mut Vec<Action>) {
        self.exited = true;
        if self.idle {
            out.push(Action::Timer(TimerRequest::Cancel(WatchTimer::Idle)));
        }
        out.push(Action::Exit(exit));
    }
}

impl Core for WatchCore {
    type Input = Input;
    type Action = Action;
    type TimerKey = WatchTimer;

    fn step(&mut self, input: Input, now: Now) -> Vec<Action> {
        let mut out = Vec::new();
        if self.exited {
            return out;
        }
        match input {
            Input::Observed(seen) => self.seen = Some(seen),
            Input::Orphaned if !self.orphaned => {
                self.orphaned = true;
                self.seen = None;
                if !self.upgrade {
                    out.push(Action::Close(Closed::Orphaned));
                }
            }
            Input::BundleChanged if !self.upgrade => {
                self.upgrade = true;
                self.seen = None;
                out.push(Action::Close(Closed::PendingUpgrade));
            }
            Input::Orphaned | Input::BundleChanged => {}
            Input::Expired(WatchTimer::Idle) => {
                self.idle = false;
                self.exit(Exit::Idle, &mut out);
                return out;
            }
        }
        self.decide(now.mono, &mut out);
        out
    }

    fn timer_request(action: &Action) -> Option<TimerRequest<WatchTimer>> {
        match action {
            Action::Timer(request) => Some(request.clone()),
            _ => None,
        }
    }

    fn timer_expired(key: WatchTimer) -> Input {
        Input::Expired(key)
    }
}

/// Whether new sessions and turns are accepted; the session manager asks at
/// each `session/create` and `session/send`.
#[derive(Debug, Default)]
pub struct Admission(AtomicU8);

impl Admission {
    pub fn refusal(&self) -> Option<RpcError> {
        let unavailable = |message| RpcError::app(ErrorCode::Unavailable, message);
        match self.0.load(Ordering::Relaxed) {
            0 => None,
            // `dev-auth` only: no reason of its own (ruled before T059).
            1 => Some(unavailable("the process that started the host exited")),
            _ => Some(
                unavailable("the host waits for an upgrade")
                    .with_reason(ErrorReason::PendingUpgrade),
            ),
        }
    }

    pub fn pending_upgrade(&self) -> bool {
        self.0.load(Ordering::Relaxed) == Closed::PendingUpgrade as u8
    }

    fn close(&self, closed: Closed) {
        self.0.fetch_max(closed as u8, Ordering::Relaxed);
    }
}

/// The way into the supervisor's watch. Clones share it.
#[derive(Debug, Clone)]
pub struct Watch(mpsc::UnboundedSender<Input>);

/// The watch and the power assertion until [`Watching::start`]; what comes
/// before waits.
#[derive(Debug)]
pub struct Watching(
    mpsc::UnboundedReceiver<Input>,
    mpsc::UnboundedReceiver<power::Mail>,
);

impl Watch {
    pub fn new() -> (Self, Power, Watching) {
        let (mail, inbox) = mpsc::unbounded_channel();
        let (power, powers) = Power::new();
        (Self(mail), power, Watching(inbox, powers))
    }

    /// The code on disk is not what runs: an upgrade waits. The runtime
    /// installer (T129) calls it on `Failed(BundleChanged)`.
    pub fn bundle_changed(&self) {
        let _ = self.0.send(Input::BundleChanged);
    }
}

/// The self-check (design 5.2.13, ADR-01): the cdhash of the executable on
/// disk against the one this process runs.
#[derive(Debug, Clone)]
pub struct SelfCheck {
    exe: PathBuf,
    own: Cdhash,
    watch: Watch,
}

impl SelfCheck {
    /// `None` when this process's cdhash cannot be read: no self-check then
    /// (ruled before T037).
    pub fn new(watch: Watch) -> Option<Self> {
        let own = cupper_sys::own_cdhash().ok()?;
        let exe = std::env::current_exe().ok()?;
        Some(Self { exe, own, watch })
    }

    /// On each new connection and before an adapter starts. Reading the
    /// code on disk can take long, so it runs off the async threads (T138);
    /// a check that cannot finish counts as a changed executable.
    pub async fn check(&self) {
        let (exe, own) = (self.exe.clone(), self.own);
        let same =
            tokio::task::spawn_blocking(move || cupper_sys::file_cdhash(&exe).ok() == Some(own));
        if !same.await.unwrap_or(false) {
            self.watch.bundle_changed();
        }
    }
}

pub struct WatchDeps {
    pub clock: Arc<dyn Clock>,
    pub durations: Durations,
    pub quiescence: Quiescence,
    pub parts: Arc<[Arc<dyn Part>]>,
    pub gateway: Weak<Gateway>,
    pub index: IndexHandle,
    pub admission: Arc<Admission>,
    pub exits: mpsc::UnboundedSender<Exit>,
    /// `dev-auth`: the parent at startup; the host is orphaned once it changes.
    pub parent: Option<u32>,
    /// Set while the runtime installer installs.
    pub installing: Arc<AtomicBool>,
    /// For `host.pending_upgrade`.
    pub audit: mpsc::UnboundedSender<AuditRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Due {
    Observe,
    Core(WatchTimer),
}

enum Next {
    Input(Input),
    Power(power::Mail),
    Due,
}

impl Watching {
    pub fn start(self, deps: WatchDeps) {
        deps.quiescence.register(SUPERVISOR);
        let participant = deps.quiescence.enter(SUPERVISOR);
        let watcher = Watcher {
            core: WatchCore::new(&deps.durations),
            timers: TimerTable::new(),
            alarm: Alarm::new(Arc::clone(&deps.clock)),
            power: Holder::default(),
            deps,
        };
        tokio::spawn(watcher.run(self, participant));
    }
}

struct Watcher {
    core: WatchCore,
    timers: TimerTable<Due>,
    alarm: Alarm,
    power: Holder,
    deps: WatchDeps,
}

impl Watcher {
    /// Runs as long as the host: the assertion lives here, with or without
    /// a [`Watch`] left.
    async fn run(mut self, Watching(mut inbox, mut powers): Watching, participant: Participant) {
        self.observe();
        loop {
            self.alarm.follow(&self.timers);
            let next = participant.idle(async {
                tokio::select! {
                    Some(input) = inbox.recv() => Next::Input(input),
                    Some(mail) = powers.recv() => Next::Power(mail),
                    () = self.alarm.rung() => Next::Due,
                }
            });
            match next.await {
                Next::Input(input) => self.step(input),
                Next::Power(mail) => {
                    let now = self.deps.clock.now();
                    self.power.handle(mail, now, &self.deps.index);
                }
                Next::Due => {
                    let now = self.deps.clock.now_mono();
                    while let Some(due) = self.timers.pop_due(now) {
                        match due {
                            Due::Observe => self.observe(),
                            Due::Core(key) => self.step(WatchCore::timer_expired(key)),
                        }
                    }
                }
            }
        }
    }

    /// Looks at the clients, the work and the parent, and again later.
    fn observe(&mut self) {
        let parent = self.deps.parent;
        if parent.is_some_and(|parent| parent != std::os::unix::process::parent_id()) {
            self.step(Input::Orphaned);
        }
        let gateway = self.deps.gateway.upgrade();
        self.step(Input::Observed(Observed {
            clients: gateway.map_or(0, |gateway| gateway.clients().len()),
            activity: self.deps.parts.iter().map(|part| part.activity()).sum(),
            installer_busy: self.deps.installing.load(Ordering::Relaxed),
        }));
        let at = self.deps.clock.now_mono() + self.deps.durations.get(SUPERVISOR_OBSERVE);
        self.timers.apply(TimerRequest::Set {
            key: Due::Observe,
            at,
        });
    }

    fn step(&mut self, input: Input) {
        for action in self.core.step(input, self.deps.clock.now()) {
            match action {
                Action::Timer(TimerRequest::Set { key, at }) => {
                    let key = Due::Core(key);
                    self.timers.apply(TimerRequest::Set { key, at });
                }
                Action::Timer(TimerRequest::Cancel(key)) => {
                    self.timers.apply(TimerRequest::Cancel(Due::Core(key)));
                }
                Action::Close(closed) => {
                    tracing::warn!(?closed, "no new sessions or turns");
                    self.deps.admission.close(closed);
                    if closed == Closed::PendingUpgrade {
                        self.deps.index.pending_upgrade();
                        let event = AuditEvent::Host(HostEvent::PendingUpgrade);
                        let (at_ms, conn) = (self.deps.clock.now_wall_ms(), None);
                        let _ = self.deps.audit.send(AuditRecord { at_ms, conn, event });
                    }
                }
                Action::Exit(exit) => {
                    let _ = self.deps.exits.send(exit);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
