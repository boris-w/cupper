//! `SessionCore`: the session's pure cores as one (design 5.2.16), wired the
//! way T058's composition test fixed (`session_props`, which drives this
//! core):
//!
//! - A driver event goes to the judge, then to the turn machine, then (an
//!   update) to the coalescer: the judge opens a turn for output first.
//! - Every [`TurnSignal`] of the machine's actions goes to the coalescer at
//!   once, and to the judge after the machine's step; the judge's `Start`,
//!   `End` and `Hold` go back to the machine as inputs.
//! - The coalescer writes `turn.completed`, with the final answer.
//! - A request from the agent goes to the judge (`Asked`), which opens a turn
//!   for it when none runs, then to the coalescer (`Interject`: the buffered
//!   text goes first and the activity group ends), and then the interaction
//!   broker writes it into the open turn (design 5.2.7). A request that comes
//!   while the running turn's end is decided waits for the turn the judge
//!   opens next. Each resolution goes to the judge (`Resolved`).
//! - The machine's `CancelAsks` and `Settle` go to the broker, so a turn's
//!   requests are resolved before its `turn.completed` (design 5.3.11).
//! - The tier guard hears of the permission mode; its interrupt and gate go
//!   to the machine, its revert to the agent, and `config_unconfirmed` comes
//!   with a notice.
//! - A session loaded from the store starts with [`Input::Restored`]: each
//!   core takes its part, and what the previous run left open in the stream
//!   is written (design 5.2.13).

use std::mem;
use std::sync::Arc;

use cupper_protocol::annotations::{DurationMs, TimestampMs};
use cupper_protocol::capabilities::ConfigOptionId;
use cupper_protocol::domain::Tier;
use cupper_protocol::event::EventBody;
use cupper_protocol::ids::{AdapterInstanceId, ItemId, PrincipalId, RequestId, SessionId, TurnId};
use cupper_protocol::interaction::{InteractionResolved, InteractionStatus, ResolutionReason};
use cupper_protocol::item::{ItemCompleted, ItemPatch, ItemStatus, NoticeCode};
use cupper_protocol::methods::handshake::Principal;
use cupper_protocol::session::{ConfirmationReason, PauseReason, TurnCompleted, TurnStatus};

use crate::clock::{IdGen, Now};
use crate::driver::api::{
    AgentSessionId, AskAnswer, AskId, AskRequest, DriverCommand, DriverEvent,
};
use crate::durations::Durations;
use crate::orchestrator::autonomous::{self, AutonomousJudge, JudgeTimer};
use crate::orchestrator::coalescer::{self, Coalescer, FlushTimer};
use crate::orchestrator::interaction::{self, InteractionBroker};
use crate::orchestrator::notice;
use crate::orchestrator::queue::QueueCore;
use crate::orchestrator::tier_guard::{self, RevertTimer, TierGuard};
use crate::orchestrator::turn::{self, SendRequest, TurnMachine, TurnSignal, TurnTimer};
use crate::pure::{Core, TimerRequest};

#[derive(Debug)]
pub struct SessionCore {
    machine: TurnMachine,
    judge: AutonomousJudge,
    coalescer: Coalescer,
    broker: InteractionBroker,
    guard: TierGuard,
    ids: Arc<IdGen>,
    /// The turn requests go into: from `turn.started` until its Settle.
    open: Option<TurnId>,
    /// Requests that came while no turn was open, for the next one.
    held: Vec<(AskId, AskRequest, AdapterInstanceId, Option<String>)>,
    /// The adapter instance of the last attach.
    instance: Option<AdapterInstanceId>,
}

#[derive(Debug, Clone)]
pub enum Input {
    Machine(turn::Input),
    /// Every driver event, requests and their withdrawals included.
    Agent(DriverEvent),
    /// `interaction/respond`.
    Respond(Box<interaction::Respond>),
    /// The tier guard's; a mode change counts as `during_turn` only while a
    /// turn runs.
    Tier(tier_guard::Input),
    Expired(Key),
    /// The session was loaded from the store; its first input.
    Restored(Box<Restored>),
}

/// A session as the store has it, and what the previous run left open in
/// its stream (T089).
#[derive(Debug, Clone)]
pub struct Restored {
    pub agent_session: Option<AgentSessionId>,
    pub last_turn_no: u32,
    pub queue: QueueCore,
    /// The reasons the gate was shut for, as far as the store tells.
    pub gate: Vec<PauseReason>,
    /// The last permission mode, with its tier.
    pub mode: Option<(String, Tier)>,
    pub unconfirmed: Option<ConfirmationReason>,
    /// The requests that ended, for the broker.
    pub ended: Vec<InteractionResolved>,
    /// The requests the stream still has pending: the host's exit ended them.
    pub expired: Vec<RequestId>,
    /// The turn the stream still has running: its ID, its start, and its end
    /// as the startup cleanup recorded it.
    pub interrupted: Option<(TurnId, TimestampMs, TimestampMs)>,
    /// What a view rebuilt from turn snapshots lacked, from the rows and the
    /// cache (T169).
    pub missing: Vec<EventBody>,
    /// The running turn's items that did not end, with their start, and,
    /// for a tool call, whether its command may still run.
    pub open: Vec<(ItemId, TimestampMs, Option<bool>)>,
    /// The first message, when the host stopped before it started a turn.
    pub first: Option<SendRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    Turn(TurnTimer),
    Judge(JudgeTimer),
    Flush(FlushTimer),
    Revert(RevertTimer),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Append to the session stream.
    Event(EventBody),
    Driver(DriverCommand),
    /// The turn machine's other actions: `Attach`, `Snapshot`, `Reply`, and
    /// `Settle`, which the core has carried out already.
    Machine(turn::Action),
    /// The reply to [`Input::Respond`].
    Answer(interaction::Reply),
    /// The guard's `Unconfirmed` and `SetBy`, which the actor records.
    Tier(tier_guard::Action),
    Timer(TimerRequest<Key>),
}

fn timer<K>(request: TimerRequest<K>, key: impl Fn(K) -> Key) -> Action {
    Action::Timer(match request {
        TimerRequest::Set { key: k, at } => TimerRequest::Set { key: key(k), at },
        TimerRequest::Cancel(k) => TimerRequest::Cancel(key(k)),
    })
}

impl SessionCore {
    /// `stops_commands`: the agent's `interrupt_stops_commands`, as far as
    /// known; see [`set_stops_commands`](Self::set_stops_commands).
    /// `setter` set the session's tier; `confirmed` when it is a confirmed
    /// `full_access`.
    pub fn new(
        session: SessionId,
        ids: Arc<IdGen>,
        durations: &Durations,
        stops_commands: bool,
        (setter, confirmed): (Principal, bool),
    ) -> Self {
        Self {
            guard: TierGuard::new(durations, setter, confirmed),
            ids: Arc::clone(&ids),
            machine: TurnMachine::new(Arc::clone(&ids), durations),
            judge: AutonomousJudge::new(durations),
            coalescer: Coalescer::new(Arc::clone(&ids), durations, stops_commands),
            broker: InteractionBroker::new(session, ids),
            open: None,
            held: Vec::new(),
            instance: None,
        }
    }

    /// The agent's capabilities came in with an attach.
    pub fn set_stops_commands(&mut self, stops_commands: bool) {
        self.coalescer.set_stops_commands(stops_commands);
    }

    pub fn ids(&self) -> &IdGen {
        &self.ids
    }

    /// Messages wait for the project trust rescan before their turns.
    pub fn require_trust_checks(&mut self) {
        self.machine.require_checks();
    }

    /// The running turn, from `turn.started` until it completes.
    /// Who set the session's tier.
    pub fn setter(&self) -> &PrincipalId {
        self.guard.setter()
    }

    pub fn turn_id(&self) -> Option<&TurnId> {
        self.machine.turn_id()
    }

    /// A message waits for its check before it turns into a turn.
    pub fn holding(&self) -> bool {
        self.machine.holding()
    }

    /// How many queued messages will run.
    pub fn queued(&self) -> u32 {
        self.machine.queued()
    }

    /// The adapter instance of the last attach.
    pub fn instance(&self) -> Option<&AdapterInstanceId> {
        self.instance.as_ref()
    }

    /// Request `id` of this session was answered (`session/send`'s
    /// `reply_to`, design 5.3.4).
    pub fn answered(&self, id: &RequestId) -> bool {
        let ended = self.broker.ended(id);
        ended.is_some_and(|ended| ended.status == InteractionStatus::Resolved)
    }

    /// A turn's signals go to the coalescer at once, and to the judge after
    /// the machine's step; the judge's actions go back to the machine.
    fn machine(&mut self, input: turn::Input, now: Now, out: &mut Vec<Action>) {
        if let turn::Input::Attached(Ok(info)) = &input {
            self.instance = Some(info.instance.clone());
        }
        let mut judge = Vec::new();
        for action in self.machine.step(input, now) {
            let signal = TurnSignal::of(&action);
            let mut settle = None;
            match action {
                turn::Action::Event(body) => out.push(Action::Event(body)),
                turn::Action::Timer(request) => out.push(timer(request, Key::Turn)),
                turn::Action::Driver(command) => out.push(Action::Driver(command)),
                // The coalescer writes `turn.completed`.
                turn::Action::Complete(_) => {}
                turn::Action::CancelAsks(turn_id) => {
                    let resolved = self.broker(interaction::Input::CancelAsks(turn_id), now, out);
                    judge.extend((0..resolved).map(|_| autonomous::Input::Resolved));
                }
                // Passed on as well: what the session's tests see of a Settle.
                turn::Action::Settle { end, asks } => {
                    settle = Some((end.turn_id.clone(), asks.clone()));
                    out.push(Action::Machine(turn::Action::Settle { end, asks }));
                }
                action => out.push(Action::Machine(action)),
            }
            let Some(signal) = signal else { continue };
            self.coalesce(coalescer::Input::Turn(signal.clone()), now, out);
            match &signal {
                TurnSignal::Started { turn_id, .. } => self.open = Some(turn_id.clone()),
                TurnSignal::Settled { .. } => self.open = None,
                TurnSignal::Completed(_) => {}
            }
            // The turn's open items end, then its requests, then
            // `turn.completed`.
            if let Some((turn_id, asks)) = settle {
                let resolved = self.broker(interaction::Input::Settle { turn_id, asks }, now, out);
                judge.extend((0..resolved).map(|_| autonomous::Input::Resolved));
            }
            judge.push(autonomous::Input::Turn(signal));
        }
        for input in judge {
            self.judge(input, now, out);
        }
        self.write_held(now, out);
    }

    /// A request from the agent: the judge first, so that a turn is open
    /// for it. Without an attached instance it cannot be answered later.
    fn ask(
        &mut self,
        ask: AskId,
        tool_call: Option<String>,
        request: AskRequest,
        now: Now,
        out: &mut Vec<Action>,
    ) {
        let Some(instance) = self.instance.clone() else {
            let answer = AskAnswer::Cancelled;
            return out.push(Action::Driver(DriverCommand::Answer { ask, answer }));
        };
        self.judge(autonomous::Input::Asked, now, out);
        self.held.push((ask, request, instance, tool_call));
        self.write_held(now, out);
    }

    /// Writes the held requests into the open turn, if there is one, each
    /// naming the item of its tool call when there is one.
    fn write_held(&mut self, now: Now, out: &mut Vec<Action>) {
        let Some(turn_id) = self.open.clone() else {
            return;
        };
        for (ask, request, instance, tool_call) in mem::take(&mut self.held) {
            self.coalesce(coalescer::Input::Interject, now, out);
            let item_id = tool_call.and_then(|call| self.coalescer.item_of(&call));
            let turn_id = turn_id.clone();
            let ask = interaction::Ask {
                ask,
                instance,
                turn_id,
                item_id,
                request,
            };
            self.resolve(interaction::Input::Ask(Box::new(ask)), now, out);
        }
    }

    /// Steps the broker; the judge hears of each request it resolves.
    fn resolve(&mut self, input: interaction::Input, now: Now, out: &mut Vec<Action>) {
        for _ in 0..self.broker(input, now, out) {
            self.judge(autonomous::Input::Resolved, now, out);
        }
    }

    /// Steps the broker; returns how many requests it resolved.
    fn broker(&mut self, input: interaction::Input, now: Now, out: &mut Vec<Action>) -> usize {
        let mut resolved = 0;
        for action in self.broker.step(input, now) {
            out.push(match action {
                interaction::Action::Event(body) => {
                    resolved += usize::from(matches!(body, EventBody::InteractionResolved(_)));
                    Action::Event(body)
                }
                interaction::Action::Driver(command) => Action::Driver(command),
                interaction::Action::Reply(reply) => Action::Answer(reply),
            });
        }
        resolved
    }

    /// Requests the agent took back, or that its exit ended, before they
    /// were written: the judge stops waiting for them.
    fn drop_held(
        &mut self,
        gone: impl Fn(&AskId, &AdapterInstanceId) -> bool,
        now: Now,
        out: &mut Vec<Action>,
    ) {
        let dropped = self
            .held
            .extract_if(.., |(ask, _, instance, _)| gone(ask, instance));
        for _ in dropped.collect::<Vec<_>>() {
            self.judge(autonomous::Input::Resolved, now, out);
        }
    }

    fn judge(&mut self, input: autonomous::Input, now: Now, out: &mut Vec<Action>) {
        let mut machine = Vec::new();
        for action in self.judge.step(input, now) {
            match (action.turn_input(), action) {
                (Some(input), _) => machine.push(input),
                (None, autonomous::Action::Timer(request)) => out.push(timer(request, Key::Judge)),
                (None, _) => {}
            }
        }
        for input in machine {
            self.machine(input, now, out);
        }
    }

    /// Each core takes its part of a loaded session. What the view lacked is
    /// written again; then, in the stream's order (design 5.2.13), the items
    /// the previous run left open end as a turn's end ends them, its pending
    /// requests expire and its running turn completes `interrupted`. A first
    /// message that never started a turn goes to the machine again.
    fn restore(&mut self, restored: Restored, now: Now, out: &mut Vec<Action>) {
        self.broker(interaction::Input::Ended(restored.ended), now, out);
        out.extend(restored.missing.into_iter().map(Action::Event));
        let ended_at = restored.interrupted.as_ref().map(|(.., at)| *at);
        for (item_id, started, command) in restored.open {
            let ended_at = ended_at.unwrap_or(started);
            let completed = ItemCompleted {
                item_id,
                status: match command {
                    Some(_) => ItemStatus::Abandoned,
                    None => ItemStatus::Completed,
                },
                ended_at,
                duration_ms: DurationMs(ended_at.0.saturating_sub(started.0)),
                may_still_run: command.unwrap_or(false),
                patch: ItemPatch::default(),
            };
            out.push(Action::Event(EventBody::ItemCompleted(completed)));
        }
        for request_id in restored.expired {
            let resolved = InteractionResolved {
                request_id,
                status: InteractionStatus::Expired,
                outcome: None,
                resolved_by: None,
                reason: Some(ResolutionReason::HostRestarted),
            };
            out.push(Action::Event(EventBody::InteractionResolved(resolved)));
        }
        if let Some((turn_id, started, ended_at)) = restored.interrupted {
            let completed = TurnCompleted {
                turn_id,
                status: TurnStatus::Interrupted,
                stop_reason: None,
                ended_at,
                duration_ms: DurationMs(ended_at.0.saturating_sub(started.0)),
                usage: None,
                error_code: None,
                final_item_ids: Vec::new(),
            };
            out.push(Action::Event(EventBody::TurnCompleted(completed)));
        }
        let (session, turn_no) = (restored.agent_session, restored.last_turn_no);
        let paused = (self.machine).restore(session, turn_no, restored.queue, restored.gate);
        out.extend(paused.map(|update| Action::Event(EventBody::QueueUpdated(update))));
        let guarded = self.guard.restore(restored.mode, restored.unconfirmed);
        self.guarded(guarded, now, out);
        if let Some(first) = restored.first {
            self.machine(turn::Input::Send(first), now, out);
        }
    }

    /// Steps the guard.
    fn tier(&mut self, input: tier_guard::Input, now: Now, out: &mut Vec<Action>) {
        let actions = self.guard.step(input, now);
        self.guarded(actions, now, out);
    }

    /// Carries out what the session's cores do for the guard.
    fn guarded(&mut self, actions: Vec<tier_guard::Action>, now: Now, out: &mut Vec<Action>) {
        for action in actions {
            match action {
                tier_guard::Action::Revert(value) => {
                    let Some(session) = self.machine.agent_session().cloned() else {
                        continue;
                    };
                    let option = ConfigOptionId::PermissionMode.to_string();
                    let command = DriverCommand::SetConfig {
                        session,
                        option,
                        value,
                    };
                    out.push(Action::Driver(command));
                }
                tier_guard::Action::Interrupt => {
                    let expected_turn = None;
                    self.machine(turn::Input::Interrupt { expected_turn }, now, out)
                }
                tier_guard::Action::Timer(request) => out.push(timer(request, Key::Revert)),
                tier_guard::Action::Unconfirmed(reason) => {
                    let (pause, shut) = (PauseReason::ConfigUnconfirmed, reason.is_some());
                    self.machine(
                        turn::Input::Gate {
                            reason: pause,
                            shut,
                        },
                        now,
                        out,
                    );
                    if shut {
                        let notice = notice(&self.ids, now, NoticeCode::ConfigUnconfirmed);
                        out.extend(notice.map(Action::Event));
                    }
                    out.push(Action::Tier(tier_guard::Action::Unconfirmed(reason)));
                }
                action @ tier_guard::Action::SetBy { .. } => out.push(Action::Tier(action)),
            }
        }
    }

    fn coalesce(&mut self, input: coalescer::Input, now: Now, out: &mut Vec<Action>) {
        for action in self.coalescer.step(input, now) {
            out.push(match action {
                coalescer::Action::Event(body) => Action::Event(body),
                coalescer::Action::Timer(request) => timer(request, Key::Flush),
            });
        }
    }
}

impl Core for SessionCore {
    type Input = Input;
    type Action = Action;
    type TimerKey = Key;

    fn step(&mut self, input: Input, now: Now) -> Vec<Action> {
        let mut out = Vec::new();
        match input {
            Input::Machine(input) => self.machine(input, now, &mut out),
            Input::Agent(DriverEvent::Ask {
                ask,
                tool_call_id,
                request,
            }) => self.ask(ask, tool_call_id, request, now, &mut out),
            Input::Agent(DriverEvent::AskWithdrawn(ask)) => {
                if self.held.iter().any(|(held, ..)| *held == ask) {
                    self.drop_held(|held, _| *held == ask, now, &mut out);
                    let answer = AskAnswer::Cancelled;
                    out.push(Action::Driver(DriverCommand::Answer { ask, answer }));
                } else {
                    self.resolve(interaction::Input::Withdrawn(ask), now, &mut out);
                }
            }
            // The judge first, so that a turn is open for the coalescer.
            Input::Agent(event) => {
                self.judge(autonomous::Input::Driver(event.clone()), now, &mut out);
                self.machine(turn::Input::Driver(event.clone()), now, &mut out);
                match event {
                    DriverEvent::Update(update) => {
                        self.coalesce(coalescer::Input::Update(update), now, &mut out)
                    }
                    // Its requests end `expired`; nothing goes to it.
                    DriverEvent::AdapterExited { instance, .. } => {
                        if self.instance.as_ref() == Some(&instance) {
                            self.instance = None;
                        }
                        self.drop_held(|_, held| *held == instance, now, &mut out);
                        let exited = interaction::Input::AdapterExited(instance);
                        self.resolve(exited, now, &mut out);
                    }
                    _ => {}
                }
            }
            Input::Respond(respond) => {
                self.resolve(interaction::Input::Respond(respond), now, &mut out)
            }
            Input::Tier(tier_guard::Input::Mode {
                mode,
                tier,
                during_turn,
            }) => {
                let during_turn = during_turn && self.machine.turn_id().is_some();
                let mode = tier_guard::Input::Mode {
                    mode,
                    tier,
                    during_turn,
                };
                self.tier(mode, now, &mut out)
            }
            Input::Tier(input) => self.tier(input, now, &mut out),
            Input::Expired(Key::Revert(key)) => {
                self.tier(tier_guard::Input::Expired(key), now, &mut out)
            }
            Input::Expired(Key::Turn(key)) => {
                self.machine(turn::Input::Expired(key), now, &mut out)
            }
            Input::Expired(Key::Judge(key)) => {
                self.judge(autonomous::Input::Expired(key), now, &mut out)
            }
            Input::Expired(Key::Flush(key)) => {
                self.coalesce(coalescer::Input::Expired(key), now, &mut out)
            }
            Input::Restored(restored) => self.restore(*restored, now, &mut out),
        }
        out
    }

    fn timer_request(action: &Action) -> Option<TimerRequest<Key>> {
        match action {
            Action::Timer(request) => Some(request.clone()),
            _ => None,
        }
    }

    fn timer_expired(key: Key) -> Input {
        Input::Expired(key)
    }
}
