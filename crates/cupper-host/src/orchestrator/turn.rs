//! `TurnMachine` (design 5.2.6, 5.2.16): a session's turns, from the message
//! that starts one to `turn.completed`, and the queue they drain.
//!
//! - A turn is `Preparing` until its start snapshot is in (or 3 s passed) and
//!   the agent is attached; then `Prompting` until the prompt returns. An
//!   interrupt sends `session/cancel` and gives the agent 10 s (`Cancelling`);
//!   then the host ends the turn as `host_timeout` and ignores the prompt's
//!   late result.
//! - At most one prompt is in flight on the agent session (K1). A prompt
//!   abandoned at the 10 s deadline stays in flight until it returns; the
//!   next turn closes and resumes the agent session before its own prompt.
//! - Once a turn's end is decided, [`Action::Settle`] closes what is still
//!   open in it, the end snapshot follows (only for a turn that sent its
//!   prompt), and only then [`Action::Complete`]: every event about a turn
//!   comes before its `turn.completed` (design 5.3.11).
//! - A turn that ends normally takes the next queued message; one cancelled
//!   or failed pauses the queue, except a cancel the agent made by itself
//!   (Codex #556, K2). A message sent while idle starts a turn at once and
//!   ends such a pause; an empty queue is never paused.
//! - While the session's gate is shut, for any of its reasons
//!   (`config_unconfirmed`, `project_config_changed`), every message is
//!   queued, a preparing turn does not send its prompt, and only reopening
//!   the gate resumes the queue.
//! - With checks required, a message turns into a turn only after
//!   [`Action::Check`] passed (the project trust rescan before every prompt,
//!   design 5.2.6): a message sent while idle waits outside the queue and its
//!   reply waits too; one that fails the check goes into the queue, which the
//!   gate holds.
//! - A prompt that fails with "Session not found" before any update is sent
//!   once more after a resume (K6); an exited adapter or CLI leaves an
//!   `agent_restarted` notice (K7).

use std::mem;
use std::sync::Arc;
use std::time::Duration;

use cupper_protocol::annotations::{DurationMs, TimestampMs};
use cupper_protocol::domain::{FailureCode, PromptContent};
use cupper_protocol::event::EventBody;
use cupper_protocol::ids::{AdapterInstanceId, ItemId, PrincipalId, QueueId, RequestId, TurnId};
use cupper_protocol::interaction::ResolutionReason;
use cupper_protocol::item::{
    Item, ItemCompleted, ItemPatch, ItemStarted, ItemStatus, NoticeCode, UserMessage,
};
use cupper_protocol::session::{
    PauseReason, QueueEntry, QueueUpdated, SnapshotPhase, SnapshotReason, SnapshotRecorded,
    SnapshotStatus, StopReason, TurnOrigin, TurnStarted, TurnStatus, TurnUsage,
};
use thiserror::Error;

use super::queue::{QueueCore, QueueEditError, QueueFull, QueueOp, preview};
use crate::clock::{IdGen, Now};
use crate::driver::api::{
    AgentError, AgentErrorClass, AgentSessionId, AttachInfo, AttachTarget, DriverCommand,
    DriverEvent, PromptTag,
};
use crate::durations::{Durations, INTERRUPT_FALLBACK, TURN_PREPARE};
use crate::orchestrator::notice;
use crate::pure::{Core, TimerRequest};

#[derive(Debug)]
pub struct TurnMachine {
    ids: Arc<IdGen>,
    prepare_for: Duration,
    cancel_for: Duration,
    link: Link,
    /// The prompt the agent session has not answered. It outlives its turn
    /// when the turn ended at the interrupt deadline.
    inflight: Option<PromptTag>,
    last_tag: u64,
    last_turn_no: u32,
    turn: Option<Turn>,
    queue: QueueCore,
    /// Set by the autonomous judge's [`Input::HoldQueue`].
    hold: bool,
    /// The reasons the gate is shut for, in the order they came.
    gate: Vec<PauseReason>,
    /// Turns from messages wait for [`Action::Check`].
    checks: bool,
    check: Check,
    /// A message sent while idle, waiting for its check.
    held: Option<(SendRequest, Option<SteerFallback>)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Check {
    None,
    Pending,
    Passed,
}

#[derive(Debug)]
enum Link {
    /// With the agent session to resume, once there is one.
    Detached(Option<AgentSessionId>),
    Attaching(Option<AgentSessionId>),
    Attached {
        session: AgentSessionId,
        instance: AdapterInstanceId,
    },
}

#[derive(Debug)]
struct Turn {
    id: TurnId,
    started: Now,
    phase: Phase,
    /// The prompt in flight, until an update comes (K6).
    resend: Option<PromptContent>,
    /// The prompt was sent again once already.
    resent: bool,
}

#[derive(Debug)]
enum Phase {
    Preparing {
        content: PromptContent,
        snapshot_done: bool,
    },
    Prompting(PromptTag),
    Cancelling(PromptTag),
    /// Updates outside any prompt (K9); T041 decides when it starts and ends.
    Autonomous,
    /// Decided; waiting for the end snapshot.
    Ending {
        end: TurnEnd,
        pause: Option<PauseReason>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    Send(SendRequest),
    Interrupt {
        expected_turn: Option<TurnId>,
    },
    QueueResume,
    QueueEdit {
        expected_version: u64,
        ops: Vec<QueueOp>,
    },
    /// The reply to [`Action::Attach`].
    Attached(Result<AttachInfo, AgentError>),
    /// A snapshot [`Action::Snapshot`] asked for.
    Snapshot(SnapshotRecorded),
    /// Every driver event; the machine acts on prompt results and adapter
    /// exits, the normaliser, the broker and the judge on the rest.
    Driver(DriverEvent),
    /// The autonomous turn's judge (T041) saw updates outside any prompt.
    AutonomousStarted,
    AutonomousEnded,
    /// The judge's dequeue guard: while held, an idle session takes no
    /// queued message; a message sent while idle still starts its turn.
    HoldQueue(bool),
    /// Shuts the session's gate for a reason, or takes the reason away.
    Gate {
        reason: PauseReason,
        shut: bool,
    },
    /// The reply to [`Action::Check`]; a failed check shuts the gate for
    /// `project_config_changed`, a passed one takes that reason away.
    Checked(bool),
    /// Closes the idle agent session, so that the next turn resumes it with
    /// the attach's new settings.
    Detach,
    Expired(TurnTimer),
    /// The host's forced exit (design 5.2.13): the running turn completes
    /// `interrupted` at once, its requests expire (`host_restarted`), and no
    /// queued message starts.
    HostExit,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SendRequest {
    pub content: PromptContent,
    pub mode: SendMode,
    pub by: PrincipalId,
    pub reply_to: Option<RequestId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendMode {
    Queue,
    /// Sent as `Queue` until T140 brings steering, with
    /// [`SteerFallback::Unsupported`] in the reply.
    Steer,
}

#[derive(Debug, Clone, PartialEq)]
#[expect(
    clippy::large_enum_variant,
    reason = "a step's few actions live only until the actor carries them out"
)]
pub enum Action {
    /// Append to the session stream.
    Event(EventBody),
    Driver(DriverCommand),
    /// Attach to the agent; the actor adds the workspace, the configuration
    /// and the project's trust state, and replies with [`Input::Attached`].
    Attach(AttachTarget),
    /// Take a git snapshot and report it as [`Input::Snapshot`].
    Snapshot {
        turn_id: TurnId,
        phase: SnapshotPhase,
    },
    /// Check whether a message may start a turn, and reply with
    /// [`Input::Checked`].
    Check,
    /// The turn was interrupted: cancel its pending requests and answer the
    /// agent `Cancelled` (T042; Codex does not withdraw them, K4).
    CancelAsks(TurnId),
    /// The turn's end is decided: flush its text, complete its open items as
    /// `abandoned` and resolve its pending requests (T041, T042), all before
    /// `turn.completed`. The requests end `expired` for `adapter_exited`, and
    /// `cancelled` for the other reasons (design 5.3.11).
    Settle {
        end: TurnEnd,
        asks: ResolutionReason,
    },
    /// The turn is over: the coalescer writes `turn.completed` with its final
    /// answer.
    Complete(TurnEnd),
    Timer(TimerRequest<TurnTimer>),
    /// The reply to the command input of this step.
    Reply(Reply),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TurnEnd {
    pub turn_id: TurnId,
    pub status: TurnStatus,
    pub stop_reason: Option<StopReason>,
    pub error_code: Option<FailureCode>,
    pub usage: Option<TurnUsage>,
    pub ended_at: TimestampMs,
    pub duration_ms: DurationMs,
}

/// How the session's other cores (the coalescer, the autonomous judge)
/// follow its turns: what an action of the machine tells them.
#[derive(Debug, Clone, PartialEq)]
pub enum TurnSignal {
    /// `turn.started`.
    Started { turn_id: TurnId, origin: TurnOrigin },
    /// [`Action::Settle`]: the end is decided; `interrupted` when the host
    /// interrupted the turn.
    Settled { interrupted: bool },
    /// [`Action::Complete`].
    Completed(TurnEnd),
}

impl TurnSignal {
    pub fn of(action: &Action) -> Option<Self> {
        match action {
            Action::Event(EventBody::TurnStarted(started)) => Some(Self::Started {
                turn_id: started.turn_id.clone(),
                origin: started.origin.clone(),
            }),
            Action::Settle { asks, .. } => Some(Self::Settled {
                interrupted: *asks == ResolutionReason::TurnCancelled,
            }),
            Action::Complete(end) => Some(Self::Completed(end.clone())),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TurnTimer {
    /// The start snapshot's 3 s.
    Prepare(TurnId),
    /// The interrupt's 10 s.
    CancelDeadline(TurnId),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Send(Result<Sent, QueueFull>),
    Interrupt(Result<Option<TurnId>, TurnMismatch>),
    QueueResume,
    QueueEdit(Result<QueueEdited, QueueEditError>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sent {
    pub disposition: Disposition,
    pub turn_id: Option<TurnId>,
    pub queue_id: Option<QueueId>,
    pub fallback: Option<SteerFallback>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    Started,
    Queued,
}

/// Why a steer went to the queue instead (`fallback_reason`, design 5.3.11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SteerFallback {
    Unsupported,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueueEdited {
    pub version: u64,
    /// Set when the edit was a steer, which stays in the queue.
    pub steer_fallback: Option<SteerFallback>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("the interrupt names a turn that is not running")]
pub struct TurnMismatch;

struct Outcome {
    status: TurnStatus,
    stop_reason: Option<StopReason>,
    error_code: Option<FailureCode>,
    usage: Option<TurnUsage>,
    pause: Option<PauseReason>,
}

impl Outcome {
    fn new(status: TurnStatus, pause: Option<PauseReason>) -> Self {
        Self {
            status,
            stop_reason: None,
            error_code: None,
            usage: None,
            pause,
        }
    }

    fn failed(code: FailureCode) -> Self {
        Self {
            error_code: Some(code),
            ..Self::new(TurnStatus::Failed, Some(PauseReason::TurnFailed))
        }
    }
}

struct Message {
    content: PromptContent,
    queue_id: Option<QueueId>,
    reply_to: Option<RequestId>,
}

impl TurnMachine {
    pub fn new(ids: Arc<IdGen>, durations: &Durations) -> Self {
        Self {
            ids,
            prepare_for: durations.get(TURN_PREPARE),
            cancel_for: durations.get(INTERRUPT_FALLBACK),
            link: Link::Detached(None),
            inflight: None,
            last_tag: 0,
            last_turn_no: 0,
            turn: None,
            queue: QueueCore::default(),
            hold: false,
            gate: Vec::new(),
            checks: false,
            check: Check::None,
            held: None,
        }
    }

    /// The running turn, until it completes.
    pub fn turn_id(&self) -> Option<&TurnId> {
        self.turn.as_ref().map(|turn| &turn.id)
    }

    /// The queued messages that will run: none while the queue is paused,
    /// which waits for the user (design 5.2.13's idle exit).
    pub fn queued(&self) -> u32 {
        if self.queue.paused().is_some() {
            0
        } else {
            self.queue.count()
        }
    }

    /// A message sent while idle waits for its check.
    pub fn holding(&self) -> bool {
        self.held.is_some()
    }

    /// The attached agent session.
    pub fn agent_session(&self) -> Option<&AgentSessionId> {
        match &self.link {
            Link::Attached { session, .. } => Some(session),
            _ => None,
        }
    }

    /// A session loaded from the store (T089): the agent session to resume,
    /// the last turn's number, the queue and the reasons the gate is shut
    /// for. No turn runs, so a queue with messages pauses for
    /// `turn_interrupted`: after a restart nothing starts before the user
    /// says so.
    pub fn restore(
        &mut self,
        agent_session: Option<AgentSessionId>,
        last_turn_no: u32,
        queue: QueueCore,
        gate: Vec<PauseReason>,
    ) -> Option<QueueUpdated> {
        self.link = Link::Detached(agent_session);
        self.last_turn_no = last_turn_no;
        (self.queue, self.gate) = (queue, gate);
        self.queue.pause(PauseReason::TurnInterrupted)
    }

    /// Messages wait for [`Action::Check`] from now on.
    pub fn require_checks(&mut self) {
        self.checks = true;
    }

    /// The queue shows the first reason still there.
    fn gate(&mut self, reason: PauseReason, shut: bool, out: &mut Vec<Action>) {
        let had = self.gate.contains(&reason);
        let mut updates = Vec::new();
        if shut && !had {
            updates.push(self.queue.pause(reason.clone()));
            self.gate.push(reason);
        } else if !shut && had {
            self.gate.retain(|r| *r != reason);
            if self.queue.paused() == Some(&reason) {
                updates.push(self.queue.resume());
                let next = self.gate.first().cloned();
                updates.extend(next.map(|next| self.queue.pause(next)));
            }
        }
        let updates = updates.into_iter().flatten();
        out.extend(updates.map(|u| Action::Event(EventBody::QueueUpdated(u))));
    }

    fn checked(&mut self, passed: bool, now: Now, out: &mut Vec<Action>) {
        self.check = Check::None;
        self.gate(PauseReason::ProjectConfigChanged, !passed, out);
        match self.held.take() {
            Some((send, fallback)) if passed && self.turn.is_none() => {
                self.start_sent(send, fallback, now, out)
            }
            Some((send, fallback)) => self.enqueue(send, fallback, now, out),
            None if passed => self.check = Check::Passed,
            None => {}
        }
    }

    fn request_check(&mut self, out: &mut Vec<Action>) {
        if self.check != Check::Pending {
            self.check = Check::Pending;
            out.push(Action::Check);
        }
    }

    fn send(&mut self, send: SendRequest, now: Now, out: &mut Vec<Action>) {
        let fallback = (send.mode == SendMode::Steer).then_some(SteerFallback::Unsupported);
        if self.turn.is_none() && self.gate.is_empty() && self.held.is_none() {
            // A new message ends a pause the last turn left (design 5.2.6).
            if let Some(update) = self.queue.resume() {
                out.push(Action::Event(EventBody::QueueUpdated(update)));
            }
            if self.checks {
                self.held = Some((send, fallback));
                return self.request_check(out);
            }
            return self.start_sent(send, fallback, now, out);
        }
        self.enqueue(send, fallback, now, out);
    }

    /// A turn for a message sent while idle, and the reply.
    fn start_sent(
        &mut self,
        send: SendRequest,
        fallback: Option<SteerFallback>,
        now: Now,
        out: &mut Vec<Action>,
    ) {
        let message = Message {
            content: send.content,
            queue_id: None,
            reply_to: send.reply_to,
        };
        let turn_id = self.start(TurnOrigin::User, Some(message), now, out);
        out.push(Action::Reply(Reply::Send(Ok(Sent {
            disposition: Disposition::Started,
            turn_id: Some(turn_id),
            queue_id: None,
            fallback,
        }))));
    }

    fn enqueue(
        &mut self,
        send: SendRequest,
        fallback: Option<SteerFallback>,
        now: Now,
        out: &mut Vec<Action>,
    ) {
        let queue_id = QueueId::from_uuid(self.ids.new_id_at(now.wall_ms).as_u128());
        let entry = QueueEntry {
            queue_id: queue_id.clone(),
            preview: preview(&send.content),
            content: send.content,
            reply_to: send.reply_to,
            created_at: TimestampMs(now.wall_ms),
            created_by: send.by,
        };
        let sent = self.queue.push(entry).map(|update| {
            out.push(Action::Event(EventBody::QueueUpdated(update)));
            let paused = (self.gate.first().cloned()).and_then(|reason| self.queue.pause(reason));
            out.extend(paused.map(|u| Action::Event(EventBody::QueueUpdated(u))));
            Sent {
                disposition: Disposition::Queued,
                turn_id: None,
                queue_id: Some(queue_id),
                fallback,
            }
        });
        out.push(Action::Reply(Reply::Send(sent)));
    }

    /// Writes `turn.started` and, for a message, the user message; asks for
    /// the start snapshot and sets its timer.
    fn start(
        &mut self,
        origin: TurnOrigin,
        message: Option<Message>,
        now: Now,
        out: &mut Vec<Action>,
    ) -> TurnId {
        let id = TurnId::from_uuid(self.ids.new_id_at(now.wall_ms).as_u128());
        let at = TimestampMs(now.wall_ms);
        self.last_turn_no += 1;
        // A check counts for one turn.
        self.check = Check::None;
        out.push(Action::Event(EventBody::TurnStarted(TurnStarted {
            turn_id: id.clone(),
            turn_no: self.last_turn_no,
            origin,
            started_at: at,
        })));
        let phase = match message {
            None => Phase::Autonomous,
            Some(message) => {
                let item_id = ItemId::from_uuid(self.ids.new_id_at(now.wall_ms).as_u128());
                out.push(Action::Event(EventBody::ItemStarted(ItemStarted {
                    item_id: item_id.clone(),
                    turn_id: Some(id.clone()),
                    started_at: at,
                    group_id: None,
                    item: Item::UserMessage(UserMessage {
                        content: message.content.clone(),
                        steered: false,
                        queue_id: message.queue_id,
                        reply_to: message.reply_to,
                    }),
                })));
                out.push(Action::Event(EventBody::ItemCompleted(ItemCompleted {
                    item_id,
                    status: ItemStatus::Completed,
                    ended_at: at,
                    duration_ms: DurationMs(0),
                    may_still_run: false,
                    patch: ItemPatch::default(),
                })));
                out.push(Action::Snapshot {
                    turn_id: id.clone(),
                    phase: SnapshotPhase::Start,
                });
                out.push(Action::Timer(TimerRequest::Set {
                    key: TurnTimer::Prepare(id.clone()),
                    at: now.mono + self.prepare_for,
                }));
                Phase::Preparing {
                    content: message.content,
                    snapshot_done: false,
                }
            }
        };
        self.turn = Some(Turn {
            id: id.clone(),
            started: now,
            phase,
            resend: None,
            resent: false,
        });
        id
    }

    /// Moves things along after every step: an idle session takes the next
    /// queued message unless the queue is held, and a preparing turn
    /// connects, closes and resumes a session with a prompt still in flight
    /// (K1), or sends its prompt.
    fn advance(&mut self, now: Now, out: &mut Vec<Action>) {
        let ready =
            self.turn.is_none() && !self.hold && self.gate.is_empty() && self.held.is_none();
        if ready && self.checks && self.check != Check::Passed && self.queue.ready() {
            self.request_check(out);
        } else if ready && let Some((entry, update)) = self.queue.pop() {
            out.push(Action::Event(EventBody::QueueUpdated(update)));
            let message = Message {
                content: entry.content,
                queue_id: Some(entry.queue_id),
                reply_to: entry.reply_to,
            };
            self.start(TurnOrigin::Queued, Some(message), now, out);
        }
        let Some(Turn {
            phase:
                Phase::Preparing {
                    content,
                    snapshot_done,
                },
            ..
        }) = &mut self.turn
        else {
            return;
        };
        match &self.link {
            Link::Detached(resume) => {
                let resume = resume.clone();
                out.push(Action::Attach(
                    resume
                        .clone()
                        .map_or(AttachTarget::New, AttachTarget::Resume),
                ));
                self.link = Link::Attaching(resume);
            }
            Link::Attached { session, .. } if self.inflight.is_some() => {
                let session = session.clone();
                out.push(Action::Driver(DriverCommand::Close {
                    session: session.clone(),
                }));
                out.push(Action::Attach(AttachTarget::Resume(session.clone())));
                self.inflight = None;
                self.link = Link::Attaching(Some(session));
            }
            Link::Attached { session, .. } if *snapshot_done && self.gate.is_empty() => {
                self.last_tag += 1;
                let tag = PromptTag(self.last_tag);
                let content = mem::take(content);
                let resend = content.clone();
                out.push(Action::Driver(DriverCommand::Prompt {
                    session: session.clone(),
                    tag,
                    content,
                }));
                self.inflight = Some(tag);
                if let Some(turn) = &mut self.turn {
                    turn.resend = (!turn.resent).then_some(resend);
                    turn.phase = Phase::Prompting(tag);
                }
            }
            Link::Attaching(_) | Link::Attached { .. } => {}
        }
    }

    fn interrupt(&mut self, expected: Option<TurnId>, now: Now, out: &mut Vec<Action>) {
        let current = self.turn.as_ref().map(|turn| turn.id.clone());
        if expected.is_some() && expected != current {
            out.push(Action::Reply(Reply::Interrupt(Err(TurnMismatch))));
            return;
        }
        let cancel = match &self.link {
            Link::Attached { session, .. } => Some(Action::Driver(DriverCommand::Cancel {
                session: session.clone(),
            })),
            _ => None,
        };
        let cancelled = || Outcome::new(TurnStatus::Cancelled, Some(PauseReason::TurnCancelled));
        if let Some(turn) = &mut self.turn {
            match turn.phase {
                Phase::Preparing { .. } => self.end(cancelled(), now, out),
                Phase::Prompting(tag) => {
                    turn.phase = Phase::Cancelling(tag);
                    out.extend(cancel);
                    out.push(Action::CancelAsks(turn.id.clone()));
                    out.push(Action::Timer(TimerRequest::Set {
                        key: TurnTimer::CancelDeadline(turn.id.clone()),
                        at: now.mono + self.cancel_for,
                    }));
                }
                Phase::Autonomous => {
                    out.extend(cancel);
                    self.end(cancelled(), now, out);
                }
                Phase::Cancelling(_) | Phase::Ending { .. } => {}
            }
        }
        out.push(Action::Reply(Reply::Interrupt(Ok(current))));
    }

    fn attached(
        &mut self,
        result: Result<AttachInfo, AgentError>,
        now: Now,
        out: &mut Vec<Action>,
    ) {
        let Link::Attaching(resume) = &self.link else {
            return;
        };
        match result {
            Ok(info) => {
                self.link = Link::Attached {
                    session: info.session,
                    instance: info.instance,
                }
            }
            Err(error) => {
                self.link = Link::Detached(resume.clone());
                if let Some(Turn {
                    phase: Phase::Preparing { .. },
                    ..
                }) = self.turn
                {
                    self.end(Outcome::failed(error.class.failure_code()), now, out);
                }
            }
        }
    }

    fn snapshot(&mut self, snapshot: SnapshotRecorded, out: &mut Vec<Action>) {
        let Some(turn) = &mut self.turn else {
            return;
        };
        if turn.id != snapshot.turn_id {
            return;
        }
        match (&mut turn.phase, &snapshot.phase) {
            (Phase::Preparing { snapshot_done, .. }, SnapshotPhase::Start) if !*snapshot_done => {
                *snapshot_done = true;
                out.push(Action::Timer(TimerRequest::Cancel(TurnTimer::Prepare(
                    turn.id.clone(),
                ))));
                out.push(Action::Event(EventBody::SnapshotRecorded(snapshot)));
            }
            (Phase::Ending { .. }, SnapshotPhase::End) => {
                out.push(Action::Event(EventBody::SnapshotRecorded(snapshot)));
                if let Some(Turn {
                    phase: Phase::Ending { end, pause },
                    ..
                }) = self.turn.take()
                {
                    self.complete(end, pause, out);
                }
            }
            _ => {}
        }
    }

    fn prompt_returned(
        &mut self,
        tag: PromptTag,
        result: Result<(StopReason, Option<TurnUsage>), AgentError>,
        now: Now,
        out: &mut Vec<Action>,
    ) {
        if self.inflight != Some(tag) {
            return;
        }
        self.inflight = None;
        if let Err(error) = &result
            && self.resume_and_resend(tag, error, now, out)
        {
            return;
        }
        let restarted = result.as_ref().err().is_some_and(|e| {
            matches!(
                e.class,
                AgentErrorClass::ProcessExited | AgentErrorClass::Restartable
            )
        });
        let phase = self.turn.as_ref().map(|turn| &turn.phase);
        let outcome = match (phase, result) {
            (Some(Phase::Prompting(t)), Ok((stop_reason, usage))) if *t == tag => {
                // Cancelled without an interrupt: the user rejected an
                // approval (Codex #556), and the queue goes on.
                let status = match stop_reason {
                    StopReason::Cancelled => TurnStatus::Cancelled,
                    _ => TurnStatus::Completed,
                };
                Outcome {
                    stop_reason: Some(stop_reason),
                    usage,
                    ..Outcome::new(status, None)
                }
            }
            (Some(Phase::Cancelling(t)), Ok((stop_reason, usage))) if *t == tag => Outcome {
                stop_reason: Some(stop_reason),
                usage,
                ..Outcome::new(TurnStatus::Cancelled, Some(PauseReason::TurnCancelled))
            },
            (Some(Phase::Prompting(t) | Phase::Cancelling(t)), Err(error)) if *t == tag => {
                Outcome::failed(error.class.failure_code())
            }
            // The prompt of a turn that ended at its interrupt deadline.
            _ => return,
        };
        self.end(outcome, now, out);
        if restarted {
            out.extend(notice(&self.ids, now, NoticeCode::AgentRestarted).map(Action::Event));
        }
    }

    /// K6: the agent lost its session (its CLI crashed) before the prompt
    /// brought anything. Resumes the session and sends the prompt once more.
    fn resume_and_resend(
        &mut self,
        tag: PromptTag,
        error: &AgentError,
        now: Now,
        out: &mut Vec<Action>,
    ) -> bool {
        let (Some(turn), Link::Attached { session, .. }) = (&mut self.turn, &self.link) else {
            return false;
        };
        let prompting = matches!(turn.phase, Phase::Prompting(t) if t == tag);
        if error.class != AgentErrorClass::SessionNotFound || !prompting {
            return false;
        }
        let Some(content) = turn.resend.take() else {
            return false;
        };
        turn.resent = true;
        let snapshot_done = true;
        turn.phase = Phase::Preparing {
            content,
            snapshot_done,
        };
        self.link = Link::Detached(Some(session.clone()));
        out.extend(notice(&self.ids, now, NoticeCode::SessionResumed).map(Action::Event));
        true
    }

    fn adapter_exited(&mut self, instance: AdapterInstanceId, now: Now, out: &mut Vec<Action>) {
        let Link::Attached {
            session,
            instance: current,
        } = &self.link
        else {
            return;
        };
        if *current != instance {
            return;
        }
        self.link = Link::Detached(Some(session.clone()));
        self.inflight = None;
        if let Some(Turn {
            phase: Phase::Prompting(_) | Phase::Cancelling(_) | Phase::Autonomous,
            ..
        }) = self.turn
        {
            self.end(Outcome::failed(FailureCode::AdapterExited), now, out);
        }
        out.extend(notice(&self.ids, now, NoticeCode::AgentRestarted).map(Action::Event));
    }

    fn expired(&mut self, timer: TurnTimer, now: Now, out: &mut Vec<Action>) {
        let Some(turn) = &mut self.turn else {
            return;
        };
        match (timer, &mut turn.phase) {
            (TurnTimer::Prepare(id), Phase::Preparing { snapshot_done, .. })
                if id == turn.id && !*snapshot_done =>
            {
                *snapshot_done = true;
                out.push(Action::Event(EventBody::SnapshotRecorded(
                    SnapshotRecorded {
                        turn_id: id,
                        phase: SnapshotPhase::Start,
                        status: SnapshotStatus::Failed,
                        reason: Some(SnapshotReason::Timeout),
                        excluded: Vec::new(),
                        stat: None,
                    },
                )));
            }
            (TurnTimer::CancelDeadline(id), Phase::Cancelling(_)) if id == turn.id => self.end(
                Outcome {
                    stop_reason: Some(StopReason::HostTimeout),
                    ..Outcome::new(TurnStatus::Cancelled, Some(PauseReason::TurnCancelled))
                },
                now,
                out,
            ),
            _ => {}
        }
    }

    /// Decides the running turn's end. A turn that sent its prompt waits for
    /// its end snapshot; any other completes at once.
    fn end(&mut self, outcome: Outcome, now: Now, out: &mut Vec<Action>) {
        let Some(turn) = &mut self.turn else {
            return;
        };
        match &turn.phase {
            Phase::Preparing { .. } => out.push(Action::Timer(TimerRequest::Cancel(
                TurnTimer::Prepare(turn.id.clone()),
            ))),
            Phase::Cancelling(_) => out.push(Action::Timer(TimerRequest::Cancel(
                TurnTimer::CancelDeadline(turn.id.clone()),
            ))),
            _ => {}
        }
        let elapsed = now.mono.saturating_duration_since(turn.started.mono);
        let end = TurnEnd {
            turn_id: turn.id.clone(),
            status: outcome.status,
            stop_reason: outcome.stop_reason,
            error_code: outcome.error_code,
            usage: outcome.usage,
            ended_at: TimestampMs(now.wall_ms),
            duration_ms: DurationMs(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)),
        };
        let asks = if end.error_code == Some(FailureCode::AdapterExited) {
            ResolutionReason::AdapterExited
        } else if end.status == TurnStatus::Interrupted {
            ResolutionReason::HostRestarted
        } else if matches!(turn.phase, Phase::Cancelling(_))
            || outcome.pause == Some(PauseReason::TurnCancelled)
        {
            // Interrupted by the host, whatever the prompt's result.
            ResolutionReason::TurnCancelled
        } else {
            ResolutionReason::TurnEnded
        };
        out.push(Action::Settle {
            end: end.clone(),
            asks,
        });
        let interrupted = end.status == TurnStatus::Interrupted;
        if matches!(turn.phase, Phase::Prompting(_) | Phase::Cancelling(_)) && !interrupted {
            out.push(Action::Snapshot {
                turn_id: turn.id.clone(),
                phase: SnapshotPhase::End,
            });
            turn.phase = Phase::Ending {
                end,
                pause: outcome.pause,
            };
        } else {
            self.turn = None;
            self.complete(end, outcome.pause, out);
        }
    }

    fn complete(&mut self, end: TurnEnd, pause: Option<PauseReason>, out: &mut Vec<Action>) {
        out.push(Action::Complete(end));
        if let Some(update) = pause.and_then(|reason| self.queue.pause(reason)) {
            out.push(Action::Event(EventBody::QueueUpdated(update)));
        }
    }
}

impl Core for TurnMachine {
    type Input = Input;
    type Action = Action;
    type TimerKey = TurnTimer;

    fn step(&mut self, input: Input, now: Now) -> Vec<Action> {
        let mut out = Vec::new();
        match input {
            Input::Send(send) => self.send(send, now, &mut out),
            Input::Interrupt { expected_turn } => self.interrupt(expected_turn, now, &mut out),
            // A shut gate keeps the queue paused.
            Input::QueueResume => {
                let resumed = self.gate.is_empty().then(|| self.queue.resume()).flatten();
                if let Some(update) = resumed {
                    out.push(Action::Event(EventBody::QueueUpdated(update)));
                }
                out.push(Action::Reply(Reply::QueueResume));
            }
            Input::QueueEdit {
                expected_version,
                ops,
            } => {
                let steer = matches!(ops.as_slice(), [QueueOp::Steer { .. }]);
                let edited = self.queue.edit(expected_version, &ops).map(|update| {
                    out.extend(update.map(|u| Action::Event(EventBody::QueueUpdated(u))));
                    QueueEdited {
                        version: self.queue.version(),
                        steer_fallback: steer.then_some(SteerFallback::Unsupported),
                    }
                });
                out.push(Action::Reply(Reply::QueueEdit(edited)));
            }
            Input::Attached(result) => self.attached(result, now, &mut out),
            Input::Snapshot(snapshot) => self.snapshot(snapshot, &mut out),
            Input::Driver(DriverEvent::PromptDone {
                tag,
                stop_reason,
                usage,
            }) => self.prompt_returned(tag, Ok((stop_reason, usage)), now, &mut out),
            Input::Driver(DriverEvent::PromptFailed { tag, error }) => {
                self.prompt_returned(tag, Err(error), now, &mut out)
            }
            Input::Driver(DriverEvent::AdapterExited { instance, .. }) => {
                self.adapter_exited(instance, now, &mut out)
            }
            // Output came: the prompt is not sent again (K6).
            Input::Driver(DriverEvent::Update(_)) => {
                if let Some(turn) = &mut self.turn {
                    turn.resend = None;
                }
            }
            Input::Driver(_) => {}
            Input::AutonomousStarted => {
                if self.turn.is_none() {
                    self.start(TurnOrigin::Autonomous, None, now, &mut out);
                }
            }
            Input::AutonomousEnded => {
                if let Some(Turn {
                    phase: Phase::Autonomous,
                    ..
                }) = self.turn
                {
                    self.end(Outcome::new(TurnStatus::Completed, None), now, &mut out);
                }
            }
            Input::HoldQueue(hold) => self.hold = hold,
            Input::Gate { reason, shut } => self.gate(reason, shut, &mut out),
            Input::Checked(passed) => self.checked(passed, now, &mut out),
            Input::Detach => {
                if let (None, Link::Attached { session, .. }) = (&self.turn, &self.link) {
                    let session = session.clone();
                    out.push(Action::Driver(DriverCommand::Close {
                        session: session.clone(),
                    }));
                    self.link = Link::Detached(Some(session));
                }
            }
            Input::Expired(timer) => self.expired(timer, now, &mut out),
            Input::HostExit => {
                self.hold = true;
                let interrupted = Some(PauseReason::TurnInterrupted);
                let outcome = Outcome::new(TurnStatus::Interrupted, interrupted);
                self.end(outcome, now, &mut out);
            }
        }
        self.advance(now, &mut out);
        out
    }

    fn timer_request(action: &Action) -> Option<TimerRequest<TurnTimer>> {
        match action {
            Action::Timer(request) => Some(request.clone()),
            _ => None,
        }
    }

    fn timer_expired(key: TurnTimer) -> Input {
        Input::Expired(key)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::orchestrator::testkit::*;

    #[test]
    fn turn_machine_runs_a_turn_from_send_to_completed() {
        let mut m = machine();
        let actions = m.input(send("hi", SendMode::Queue));
        let turn = started(&actions);
        assert_eq!(
            kinds(&actions),
            [
                "turn.started",
                "item.started",
                "item.completed",
                "snapshot",
                "timer",
                "reply",
                "attach"
            ]
        );
        assert_eq!(kinds(&m.input(attached(1))), [""; 0]);
        assert_eq!(
            kinds(&m.input(snapshot(&turn, SnapshotPhase::Start))),
            ["untimer", "snapshot.recorded", "prompt"]
        );
        assert_eq!(
            kinds(&m.input(done(1, StopReason::EndTurn))),
            ["settle", "snapshot"]
        );
        let end = m.input(snapshot(&turn, SnapshotPhase::End));
        assert_eq!(kinds(&end), ["snapshot.recorded", "complete"]);
        let Action::Complete(end) = &end[1] else {
            panic!("{end:?}")
        };
        assert_eq!(end.status, TurnStatus::Completed);
        assert_eq!(
            kinds(&m.input(Input::Attached(Err(agent_error())))),
            [""; 0]
        );
        assert!(m.advance(Duration::from_secs(60)).is_empty());
    }

    #[test]
    fn turn_machine_prompts_after_3_s_without_its_start_snapshot() {
        let mut m = machine();
        let turn = started(&m.input(send("hi", SendMode::Queue)));
        m.input(attached(1));
        assert!(m.advance(Duration::from_millis(2_999)).is_empty());
        let late = m.advance(Duration::from_millis(1));
        assert_eq!(kinds(&late), ["snapshot.recorded", "prompt"]);
        let Action::Event(EventBody::SnapshotRecorded(record)) = &late[0] else {
            panic!("{late:?}")
        };
        assert_eq!(
            (&record.status, &record.reason),
            (&SnapshotStatus::Failed, &Some(SnapshotReason::Timeout))
        );
        assert!(m.input(snapshot(&turn, SnapshotPhase::Start)).is_empty());
    }

    #[test]
    fn turn_machine_ends_at_the_deadline_then_closes_and_resumes() {
        let mut m = machine();
        let first = started(&m.input(send("a", SendMode::Queue)));
        m.input(attached(1));
        m.input(snapshot(&first, SnapshotPhase::Start));
        let queued = m.input(send("b", SendMode::Steer));
        assert_eq!(kinds(&queued), ["queue.updated", "reply"]);
        let Action::Reply(Reply::Send(Ok(sent))) = &queued[1] else {
            panic!("{queued:?}")
        };
        assert_eq!(
            (sent.disposition, sent.fallback),
            (Disposition::Queued, Some(SteerFallback::Unsupported))
        );
        let interrupt = Input::Interrupt {
            expected_turn: None,
        };
        assert_eq!(
            kinds(&m.input(interrupt)),
            ["cancel", "cancel_asks", "timer", "reply"]
        );
        assert!(m.advance(Duration::from_millis(9_999)).is_empty());
        let deadline = m.advance(Duration::from_millis(1));
        assert_eq!(kinds(&deadline), ["untimer", "settle", "snapshot"]);
        let Action::Settle { end, asks } = &deadline[1] else {
            panic!("{deadline:?}")
        };
        assert_eq!(
            (&end.status, &end.stop_reason, asks),
            (
                &TurnStatus::Cancelled,
                &Some(StopReason::HostTimeout),
                &ResolutionReason::TurnCancelled
            )
        );
        let end = m.input(snapshot(&first, SnapshotPhase::End));
        assert_eq!(
            kinds(&end),
            ["snapshot.recorded", "complete", "queue.updated"]
        );
        // The queue is paused; a message sent while idle runs at once, ends
        // the pause, and makes the session close and resume first (K1).
        let next = m.input(send("c", SendMode::Queue));
        assert_eq!(kinds(&next)[..2], ["queue.updated", "turn.started"]);
        assert_eq!(kinds(&next)[next.len() - 2..], ["close", "resume"]);
        assert!(m.input(done(1, StopReason::Cancelled)).is_empty());
        m.input(snapshot(&started(&next), SnapshotPhase::Start));
        assert_eq!(kinds(&m.input(attached(1))), ["prompt"]);
    }

    #[test]
    fn turn_machine_interrupts_the_running_turn_as_the_host_exits() {
        let mut m = machine();
        let turn = started(&m.input(send("a", SendMode::Queue)));
        m.input(attached(1));
        m.input(snapshot(&turn, SnapshotPhase::Start));
        m.input(send("b", SendMode::Queue));
        let exit = m.input(Input::HostExit);
        assert_eq!(kinds(&exit), ["settle", "complete", "queue.updated"]);
        let Action::Settle { end, asks } = &exit[0] else {
            panic!("{exit:?}")
        };
        let ended = (&end.status, asks);
        assert_eq!(
            ended,
            (&TurnStatus::Interrupted, &ResolutionReason::HostRestarted)
        );
        assert!(m.input(Input::HostExit).is_empty());
    }
}
