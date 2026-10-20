//! Property tests of the turn machine and its queue (design 5.9.8), on
//! virtual time. Random sends, interrupts, queue resumes, prompt results
//! (late ones too), snapshots, attach replies, adapter exits, autonomous
//! turns, unrelated driver events and passing time go to a [`World`]: a model
//! of the agent session and the actor's ports that answers the machine's
//! actions and checks each one as it comes. Every run ends by answering
//! everything until the session is idle. Each property asserts its own rules
//! on top of the well-formedness every run checks.

use std::collections::{HashMap, HashSet};
use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;
use std::time::Duration;

use cupper_protocol::domain::FailureCode;
use cupper_protocol::event::EventBody;
use cupper_protocol::ids::{AdapterInstanceId, ItemId, QueueId, TurnId};
use cupper_protocol::interaction::ResolutionReason;
use cupper_protocol::item::Item;
use cupper_protocol::session::{
    PauseReason, SnapshotPhase, SnapshotReason, StopReason, TurnOrigin, TurnStatus,
};
use proptest::prelude::*;

use super::testkit::{agent_error, attached, machine, send, snapshot};
use super::turn::{Action, Disposition, Input, Reply, SendMode, TurnEnd, TurnMachine};
use crate::clock::MonoTime;
use crate::driver::api::{DriverCommand, DriverEvent, PromptTag, RunState};
use crate::pure::{CoreDriver, Step};

#[derive(Debug, Clone)]
enum Op {
    Send(SendMode),
    Interrupt,
    /// An input the world only passes on: a queue resume, a run state,
    /// the start or end of an autonomous turn.
    Plain(Box<Input>),
    /// Return the n-th prompt whose result can still come, unless it hangs
    /// and its turn runs: with a stop reason, or failed.
    Answer(u8, Option<StopReason>),
    /// The running prompt hangs, the user interrupts and waits 10 s; its
    /// result can only come late (K1).
    Stall,
    /// Finish the n-th snapshot asked for.
    Snapshot(u8),
    /// Reply to the pending attach, successfully when set.
    Attach(bool),
    Exit,
    Advance(u64),
}

/// Whose rules a run asserts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Prop {
    /// PROP-queue-fifo.
    #[default]
    Fifo,
    /// PROP-interrupt-10s.
    Deadline,
    /// PROP-turn-single-prompt.
    K1,
    /// PROP-prompt-inflight-no-end.
    NoEnd,
}
use Prop::{Deadline, Fifo, K1, NoEnd};

/// Codex's thread gone idle: it ends no turn (K12).
const IDLE: RunState = RunState {
    active: false,
    waiting_on_approval: false,
    waiting_on_user_input: false,
    system_error: false,
};

fn ops() -> impl Strategy<Value = Vec<Op>> {
    let plain = prop_oneof![
        Just(Input::QueueResume),
        Just(Input::Driver(DriverEvent::RunState(IDLE))),
        Just(Input::AutonomousStarted),
        Just(Input::AutonomousEnded),
    ];
    let stop = prop_oneof![
        Just(Some(StopReason::EndTurn)),
        Just(Some(StopReason::Cancelled)),
        Just(None)
    ];
    let op = prop_oneof![
        4 => prop_oneof![Just(SendMode::Queue), Just(SendMode::Steer)].prop_map(Op::Send),
        3 => Just(Op::Interrupt),
        3 => plain.prop_map(|input| Op::Plain(Box::new(input))),
        4 => (any::<u8>(), stop).prop_map(|(n, stop)| Op::Answer(n, stop)),
        1 => Just(Op::Stall),
        4 => any::<u8>().prop_map(Op::Snapshot),
        3 => prop_oneof![4 => Just(true), 1 => Just(false)].prop_map(Op::Attach),
        1 => Just(Op::Exit),
        4 => prop_oneof![0..500_u64, 2_500..3_500_u64, 9_000..12_000_u64].prop_map(Op::Advance),
    ];
    prop::collection::vec(op, 0..100)
}

struct Machine(CoreDriver<TurnMachine>);

impl Default for Machine {
    fn default() -> Self {
        Self(machine())
    }
}

#[derive(Default)]
struct World {
    prop: Prop,
    m: Machine,
    /// The adapter instance the next attach reaches.
    instance: u128,
    attaching: bool,
    /// Prompts running on the agent session.
    running: Vec<PromptTag>,
    /// Prompts whose result can still arrive: running ones and closed ones.
    answerable: Vec<PromptTag>,
    hung: HashSet<PromptTag>,
    winding_down: bool,
    prompt_turn: HashMap<PromptTag, TurnId>,
    snapshots: Vec<(TurnId, SnapshotPhase)>,
    current: Option<(TurnId, TurnOrigin)>,
    started_at: HashMap<TurnId, MonoTime>,
    items: HashMap<ItemId, TurnId>,
    start_recorded: HashSet<TurnId>,
    end_recorded: HashSet<TurnId>,
    prompted: HashSet<TurnId>,
    settled: HashSet<TurnId>,
    completed: HashSet<TurnId>,
    /// Turns the host interrupted, and the deadline of those it gave 10 s.
    interrupted: HashSet<TurnId>,
    deadlines: HashMap<TurnId, MonoTime>,
    /// Queued messages, as accepted and as they ran.
    accepted: Vec<QueueId>,
    ran: Vec<QueueId>,
    queue: Vec<QueueId>,
    paused: bool,
    paused_before: bool,
    /// Within a step: the pause the turn that just completed calls for, and
    /// whether messages were waiting then.
    pause_due: Option<(Option<PauseReason>, bool)>,
}

impl World {
    fn must(&self, prop: Prop, ok: bool, what: &str) {
        assert!(self.prop != prop || ok, "{prop:?}: {what}");
    }

    fn apply(&mut self, op: Op) {
        let before = self.current.clone().map(|(turn, _)| turn);
        let idle_paused = before.is_none() && self.paused;
        let idle_waiting = before.is_none() && !self.queue.is_empty();
        let mut answered = None;
        let input = match op.clone() {
            Op::Send(mode) => send("m", mode),
            Op::Interrupt => {
                // Whatever the turn's phase, unless its end is decided already.
                if let Some(turn) = before.clone().filter(|t| !self.settled.contains(t)) {
                    self.interrupted.insert(turn);
                }
                Input::Interrupt {
                    expected_turn: None,
                }
            }
            Op::Plain(input) => *input,
            Op::Stall => {
                self.hung.extend(self.running.last());
                self.apply(Op::Interrupt);
                return self.apply(Op::Advance(10_000));
            }
            Op::Answer(n, stop) => {
                // A hung prompt answers only late: once its turn has ended.
                let late = |tag: &PromptTag| self.settled.contains(&self.prompt_turn[tag]);
                let open: Vec<usize> = (0..self.answerable.len())
                    .filter(|i| {
                        let tag = &self.answerable[*i];
                        self.winding_down || !self.hung.contains(tag) || late(tag)
                    })
                    .collect();
                let Some(i) = open.get(usize::from(n) % open.len().max(1)) else {
                    return;
                };
                let tag = self.answerable.remove(*i);
                self.running.retain(|t| *t != tag);
                answered = Some(tag);
                Input::Driver(match stop {
                    Some(stop_reason) => DriverEvent::PromptDone {
                        tag,
                        stop_reason,
                        usage: None,
                    },
                    None => DriverEvent::PromptFailed {
                        tag,
                        error: agent_error(),
                    },
                })
            }
            Op::Snapshot(_) if self.snapshots.is_empty() => return,
            Op::Snapshot(n) => {
                let (turn, phase) = self.snapshots.remove(usize::from(n) % self.snapshots.len());
                snapshot(&turn, phase)
            }
            Op::Attach(_) if !self.attaching => return,
            Op::Attach(ok) => {
                self.attaching = false;
                let failed = Input::Attached(Err(agent_error()));
                if ok { attached(self.instance) } else { failed }
            }
            Op::Exit => {
                // A dead adapter answers nothing; the runtime starts another.
                self.running.clear();
                self.answerable.clear();
                self.instance += 1;
                Input::Driver(DriverEvent::AdapterExited {
                    instance: AdapterInstanceId::from_uuid(self.instance - 1),
                    status: ExitStatus::from_raw(9),
                    stderr: String::new(),
                })
            }
            Op::Advance(ms) => {
                self.feed(Step::Advance(Duration::from_millis(ms)), None);
                return;
            }
        };
        let actions = self.feed(Step::Input(input), answered);
        let started = |origin: TurnOrigin| {
            let started = |a: &Action| matches!(a, Action::Event(EventBody::TurnStarted(s)) if s.origin == origin);
            actions.iter().any(started)
        };
        match op {
            Op::Send(_) if idle_paused => {
                let ok = started(TurnOrigin::User) && !self.paused;
                self.must(
                    Fifo,
                    ok,
                    "a message sent while idle starts and ends the pause",
                );
            }
            Op::Plain(input) => match *input {
                Input::QueueResume if idle_waiting => {
                    self.must(Fifo, started(TurnOrigin::Queued), "a resume runs the head");
                }
                Input::Driver(_) => {
                    self.must(NoEnd, actions.is_empty(), "a run state ends no turn (K12)");
                }
                _ => {}
            },
            Op::Interrupt => {
                if let Some(turn) = before.filter(|t| !self.settled.contains(t)) {
                    let deadline = self.m.0.now().mono + Duration::from_secs(10);
                    self.deadlines.entry(turn).or_insert(deadline);
                }
            }
            _ => {}
        }
    }

    /// Runs one step and checks its actions; returns them.
    fn feed(&mut self, step: Step<Input>, answered: Option<PromptTag>) -> Vec<Action> {
        let transcript = self.m.0.run([step]);
        for (at, action) in &transcript {
            self.observe(*at, action, answered);
        }
        let now = self.m.0.now().mono;
        for (turn, deadline) in &self.deadlines {
            let ok = self.settled.contains(turn) || now < *deadline;
            self.must(Deadline, ok, "an interrupted turn ends by its deadline");
        }
        let ok = self.current.is_some() || self.queue.is_empty() || self.paused;
        self.must(Fifo, ok, "an idle session takes the next queued message");
        if let Some((Some(_), true)) = self.pause_due.take() {
            self.must(
                Fifo,
                self.paused,
                "a cancelled or failed turn pauses the queue",
            );
        }
        transcript.into_iter().map(|(_, action)| action).collect()
    }

    fn observe(&mut self, at: MonoTime, action: &Action, answered: Option<PromptTag>) {
        if let Some(turn) = self.turn_of(action) {
            let ok = !self.completed.contains(&turn);
            self.must(
                NoEnd,
                ok,
                "every event of a turn comes before turn.completed",
            );
        }
        match action {
            Action::Driver(DriverCommand::Prompt { tag, .. }) => {
                self.must(K1, self.running.is_empty(), "one prompt in flight at most");
                let (turn, _) = self.current.clone().expect("the running turn's prompt");
                assert!(
                    self.start_recorded.contains(&turn),
                    "start snapshot or 3 s first"
                );
                self.prompted.insert(turn.clone());
                self.prompt_turn.insert(*tag, turn);
                self.running.push(*tag);
                self.answerable.push(*tag);
            }
            Action::Driver(DriverCommand::Close { .. }) => self.running.clear(),
            Action::Attach(_) => {
                assert!(!self.attaching, "one attach at a time");
                self.attaching = true;
            }
            Action::Snapshot { turn_id, phase } => {
                let ok = *phase == SnapshotPhase::Start || self.prompted.contains(turn_id);
                assert!(ok, "only a prompted turn takes an end snapshot");
                self.snapshots.push((turn_id.clone(), phase.clone()));
            }
            Action::Event(EventBody::TurnStarted(started)) => {
                assert!(self.current.is_none(), "one turn at a time");
                if started.origin == TurnOrigin::Queued {
                    self.must(
                        Fifo,
                        !self.paused_before,
                        "no message leaves a paused queue",
                    );
                }
                self.current = Some((started.turn_id.clone(), started.origin.clone()));
                self.started_at.insert(started.turn_id.clone(), at);
            }
            // A notice without a turn (`agent_restarted`, ...).
            Action::Event(EventBody::ItemStarted(item)) if item.turn_id.is_none() => {
                assert!(
                    matches!(item.item, Item::Notice(_)),
                    "only notices go without a turn"
                );
            }
            Action::Event(EventBody::ItemStarted(item)) => {
                let turn = item.turn_id.clone().expect("the user message's turn");
                self.items.insert(item.item_id.clone(), turn);
                if let Item::UserMessage(message) = &item.item
                    && let Some(queue_id) = &message.queue_id
                {
                    self.ran.push(queue_id.clone());
                    let ok = self.accepted.starts_with(&self.ran);
                    self.must(Fifo, ok, "queued messages run in arrival order");
                }
            }
            Action::Event(EventBody::SnapshotRecorded(record)) => {
                let turn = &record.turn_id;
                if record.phase == SnapshotPhase::End {
                    self.end_recorded.insert(turn.clone());
                } else if record.reason == Some(SnapshotReason::Timeout) {
                    assert_eq!(at, self.started_at[turn] + Duration::from_secs(3));
                }
                if record.phase == SnapshotPhase::Start {
                    assert!(self.start_recorded.insert(turn.clone()), "one start record");
                }
            }
            Action::Event(EventBody::QueueUpdated(update)) => {
                let paused = update.paused.as_ref().map(|pause| pause.reason.clone());
                if let Some(reason) = &paused
                    && !self.paused
                {
                    let due = self.pause_due.as_ref().and_then(|(due, _)| due.as_ref());
                    self.must(
                        Fifo,
                        due == Some(reason),
                        "only a turn's end pauses the queue",
                    );
                }
                self.paused_before = self.paused;
                self.paused = paused.is_some();
                self.queue = update.entries.iter().map(|e| e.queue_id.clone()).collect();
            }
            Action::Settle { end, asks } => self.settle(at, end, asks, answered),
            Action::Complete(end) => {
                let turn = &end.turn_id;
                assert!(
                    self.settled.contains(turn),
                    "a turn settles, then completes"
                );
                assert!(self.completed.insert(turn.clone()), "a turn completes once");
                let snapped = self.end_recorded.contains(turn);
                assert_eq!(self.prompted.contains(turn), snapped, "end snapshot first");
                self.current = None;
                let due = match end.status {
                    TurnStatus::Failed => Some(PauseReason::TurnFailed),
                    TurnStatus::Cancelled if self.interrupted.contains(turn) => {
                        Some(PauseReason::TurnCancelled)
                    }
                    _ => None,
                };
                self.pause_due = Some((due, !self.queue.is_empty()));
            }
            Action::Reply(Reply::Send(Ok(sent))) if sent.disposition == Disposition::Queued => {
                self.accepted.extend(sent.queue_id.clone());
            }
            _ => {}
        }
    }

    fn settle(
        &mut self,
        at: MonoTime,
        end: &TurnEnd,
        asks: &ResolutionReason,
        answered: Option<PromptTag>,
    ) {
        let turn = &end.turn_id;
        let current = self.current.as_ref().map(|(t, _)| t);
        assert_eq!(current, Some(turn), "only the running turn ends");
        assert!(self.settled.insert(turn.clone()), "a turn ends once");
        assert_eq!(end.status == TurnStatus::Failed, end.error_code.is_some());
        let timeout = end.stop_reason == Some(StopReason::HostTimeout);
        let running = self
            .running
            .iter()
            .any(|tag| self.prompt_turn.get(tag) == Some(turn));
        self.must(
            NoEnd,
            !running || timeout,
            "no turn ends while its prompt runs",
        );
        if let Some(tag) = answered {
            let ok = self.prompt_turn.get(&tag) == Some(turn);
            self.must(NoEnd, ok, "a late result ends no other turn");
        }
        match self.deadlines.get(turn) {
            Some(deadline) => {
                self.must(
                    Deadline,
                    at <= *deadline,
                    "an interrupted turn ends within 10 s",
                );
                self.must(
                    Deadline,
                    !timeout || at == *deadline,
                    "host_timeout at the deadline",
                );
                let ok = matches!(end.status, TurnStatus::Cancelled | TurnStatus::Failed);
                self.must(
                    Deadline,
                    ok,
                    "an interrupted turn is cancelled unless it failed",
                );
            }
            None => self.must(Deadline, !timeout, "host_timeout only after an interrupt"),
        }
        let expected = if end.error_code == Some(FailureCode::AdapterExited) {
            ResolutionReason::AdapterExited
        } else if self.interrupted.contains(turn) {
            ResolutionReason::TurnCancelled
        } else {
            ResolutionReason::TurnEnded
        };
        assert_eq!(*asks, expected, "pending requests end as the turn did");
    }

    fn turn_of(&self, action: &Action) -> Option<TurnId> {
        match action {
            Action::Event(EventBody::TurnStarted(started)) => Some(started.turn_id.clone()),
            Action::Event(EventBody::ItemStarted(item)) => item.turn_id.clone(),
            Action::Event(EventBody::ItemCompleted(item)) => self.items.get(&item.item_id).cloned(),
            Action::Event(EventBody::SnapshotRecorded(record)) => Some(record.turn_id.clone()),
            Action::Snapshot { turn_id, .. } | Action::CancelAsks(turn_id) => Some(turn_id.clone()),
            Action::Settle { end, .. } | Action::Complete(end) => Some(end.turn_id.clone()),
            _ => None,
        }
    }

    /// Answers everything until the session is idle with an empty queue.
    fn quiesce(&mut self) {
        self.winding_down = true;
        for _ in 0..1_000 {
            let idle = self.current.is_none() && self.queue.is_empty();
            if idle && !self.attaching && self.answerable.is_empty() && self.snapshots.is_empty() {
                break;
            }
            self.apply(Op::Attach(true));
            while !self.answerable.is_empty() {
                self.apply(Op::Answer(0, Some(StopReason::EndTurn)));
            }
            while !self.snapshots.is_empty() {
                self.apply(Op::Snapshot(0));
            }
            if matches!(self.current, Some((_, TurnOrigin::Autonomous))) {
                self.apply(Op::Plain(Box::new(Input::AutonomousEnded)));
            }
            if self.current.is_none() {
                self.apply(Op::Plain(Box::new(Input::QueueResume)));
            }
            self.apply(Op::Advance(11_000));
        }
        assert!(
            self.current.is_none() && self.queue.is_empty(),
            "the session settles"
        );
        assert_eq!(
            self.completed.len(),
            self.started_at.len(),
            "every turn ends"
        );
        self.must(
            Fifo,
            self.ran == self.accepted,
            "every queued message runs once",
        );
    }
}

fn run(prop: Prop, ops: Vec<Op>) {
    let mut world = World {
        prop,
        instance: 1,
        ..World::default()
    };
    for op in ops {
        world.apply(op);
    }
    world.quiesce();
}

proptest! {
    #[test]
    fn prop_queue_fifo_messages_leave_in_order_and_not_while_paused(ops in ops()) {
        run(Fifo, ops);
    }

    #[test]
    fn prop_interrupt_10s_turn_ends_by_the_deadline(ops in ops()) {
        run(Deadline, ops);
    }

    #[test]
    fn prop_turn_single_prompt_at_most_one_in_flight(ops in ops()) {
        run(K1, ops);
    }

    #[test]
    fn prop_prompt_inflight_no_end_turn_waits_for_its_prompt(ops in ops()) {
        run(NoEnd, ops);
    }
}
