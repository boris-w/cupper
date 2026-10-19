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

use std::mem;
use std::sync::Arc;
use std::time::Duration;

use cupper_protocol::annotations::{DurationMs, TimestampMs};
use cupper_protocol::domain::{FailureCode, PromptContent};
use cupper_protocol::event::EventBody;
use cupper_protocol::ids::{AdapterInstanceId, ItemId, PrincipalId, QueueId, RequestId, TurnId};
use cupper_protocol::interaction::ResolutionReason;
use cupper_protocol::item::{Item, ItemCompleted, ItemPatch, ItemStarted, ItemStatus, UserMessage};
use cupper_protocol::session::{
    PauseReason, QueueEntry, SnapshotPhase, SnapshotReason, SnapshotRecorded, SnapshotStatus,
    StopReason, TurnOrigin, TurnStarted, TurnStatus, TurnUsage,
};
use thiserror::Error;

use super::queue::{QueueCore, QueueEditError, QueueFull, QueueOp, preview};
use crate::clock::{IdGen, Now};
use crate::driver::api::{
    AgentError, AgentSessionId, AttachInfo, AttachTarget, DriverCommand, DriverEvent, PromptTag,
};
use crate::durations::{Durations, INTERRUPT_FALLBACK, TURN_PREPARE};
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
    Expired(TurnTimer),
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
    /// Write `turn.completed`, adding the final answer (T041).
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
        }
    }

    fn send(&mut self, send: SendRequest, now: Now, out: &mut Vec<Action>) {
        let fallback = (send.mode == SendMode::Steer).then_some(SteerFallback::Unsupported);
        if self.turn.is_none() {
            // A new message ends a pause the last turn left (design 5.2.6).
            if let Some(update) = self.queue.resume() {
                out.push(Action::Event(EventBody::QueueUpdated(update)));
            }
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
            return;
        }
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
        });
        id
    }

    /// Moves things along after every step: an idle session takes the next
    /// queued message, and a preparing turn connects, closes and resumes a
    /// session with a prompt still in flight (K1), or sends its prompt.
    fn advance(&mut self, now: Now, out: &mut Vec<Action>) {
        if self.turn.is_none()
            && let Some((entry, update)) = self.queue.pop()
        {
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
            Link::Attached { session, .. } if *snapshot_done => {
                self.last_tag += 1;
                let tag = PromptTag(self.last_tag);
                out.push(Action::Driver(DriverCommand::Prompt {
                    session: session.clone(),
                    tag,
                    content: mem::take(content),
                }));
                self.inflight = Some(tag);
                if let Some(turn) = &mut self.turn {
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
        } else if outcome.pause == Some(PauseReason::TurnCancelled) {
            ResolutionReason::TurnCancelled
        } else {
            ResolutionReason::TurnEnded
        };
        out.push(Action::Settle {
            end: end.clone(),
            asks,
        });
        if matches!(turn.phase, Phase::Prompting(_) | Phase::Cancelling(_)) {
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
            Input::QueueResume => {
                if let Some(update) = self.queue.resume() {
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
            Input::Expired(timer) => self.expired(timer, now, &mut out),
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
}
