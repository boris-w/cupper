//! Property tests of the session's cores together (design 5.9.8), on
//! virtual time, through the actor's own [`SessionCore`]: the turn machine,
//! the autonomous judge, the coalescer and the interaction broker. A
//! [`World`] models the agent and the actor's ports, answers the cores'
//! actions, and checks the log as it is written.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;
use std::sync::Arc;
use std::time::Duration;

use cupper_protocol::domain::Cost;
use cupper_protocol::event::EventBody;
use cupper_protocol::ids::{AdapterInstanceId, Digest, ItemId, RequestId, SessionId, TurnId};
use cupper_protocol::item::{Item, ItemStatus};
use cupper_protocol::session::{SnapshotPhase, StopReason, TurnOrigin, TurnStatus, UsageUpdated};
use proptest::prelude::*;
use serde_json::{from_value, json};

use super::session::core::{Action as Out, Input as In, SessionCore};
use super::testkit::{agent_error, attached, send, snapshot};
use super::turn::{self, SendMode};
use crate::clock::{IdGen, ManualClock, MonoTime};
use crate::driver::api::{
    AskId, AskRequest, DriverCommand, DriverEvent, NormUpdate, PromptTag, RunState, ToolOutput,
    ToolStatus,
};
use crate::durations::Durations;
use crate::orchestrator::interaction::Respond;
use crate::policy::testkit::{column, option, permission};
use crate::pure::{CoreDriver, Step};

#[derive(Debug, Clone)]
enum Op {
    Send,
    Interrupt,
    /// Reply to the pending attach, successfully when set.
    Attach(bool),
    /// Finish the n-th snapshot asked for.
    Snapshot(u8),
    /// Return the n-th running prompt, failed unless set.
    Answer(u8, bool),
    /// Agent output: a message fragment, a thought, a tool call starting,
    /// output or the end of the n-th unfinished tool call.
    Text(u8),
    Thought,
    ToolStart,
    Output(u8),
    Done(u8),
    /// A `usage_update` with `cost`.
    Cost,
    /// The thread's state, active when set.
    Thread(bool),
    Ask,
    Resolve,
    Exit,
    Advance(u64),
}

fn ops() -> impl Strategy<Value = Vec<Op>> {
    let op = prop_oneof![
        3 => Just(Op::Send),
        2 => Just(Op::Interrupt),
        3 => any::<bool>().prop_map(Op::Attach),
        3 => any::<u8>().prop_map(Op::Snapshot),
        3 => (any::<u8>(), prop::bool::weighted(0.8)).prop_map(|(n, ok)| Op::Answer(n, ok)),
        3 => any::<u8>().prop_map(Op::Text),
        1 => Just(Op::Thought),
        2 => Just(Op::ToolStart),
        3 => any::<u8>().prop_map(Op::Output),
        2 => any::<u8>().prop_map(Op::Done),
        1 => Just(Op::Cost),
        2 => any::<bool>().prop_map(Op::Thread),
        1 => Just(Op::Ask),
        1 => Just(Op::Resolve),
        1 => Just(Op::Exit),
        4 => prop_oneof![0..100_u64, 5_000..12_000_u64, 25_000..35_000_u64].prop_map(Op::Advance),
    ];
    prop::collection::vec(op, 0..100)
}

/// Whose rules a run asserts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Prop {
    /// PROP-autonomous-dequeue-guard.
    Guard,
    /// PROP-turn-snapshot-immutable.
    Immutable,
}

struct World {
    prop: Prop,
    d: CoreDriver<SessionCore>,
    // The agent and the ports.
    instance: u128,
    attaching: bool,
    prompts: Vec<PromptTag>,
    snapshots: Vec<(TurnId, SnapshotPhase)>,
    tools: Vec<String>,
    finished: HashSet<String>,
    /// An instance is attached: requests can be answered.
    attached: bool,
    // What the judge should know.
    running: BTreeSet<String>,
    thread_active: Option<bool>,
    /// Requests not resolved, in the order asked, with their request ID and
    /// digest once written.
    asks: Vec<(AskId, Option<(RequestId, Digest)>)>,
    next_ask: u64,
    /// Resolutions asked for so far; two in three answer, if they can.
    resolves: u64,
    // The log so far.
    current: Option<(TurnId, TurnOrigin, MonoTime)>,
    settled: HashSet<TurnId>,
    completed: HashSet<TurnId>,
    /// By item: its turn, whether it ended, whether it is a tool call.
    items: HashMap<ItemId, (TurnId, bool, bool)>,
    abandoned: HashSet<ItemId>,
    /// Notices without a turn (`agent_restarted`, ...).
    loose: HashSet<ItemId>,
    late: HashSet<(ItemId, TurnId)>,
    queue: (usize, bool),
    // The dequeue guard.
    advancing: bool,
    /// The agent's last output or activity, and the judge's last reason to
    /// start its 10 s over.
    quiet_since: MonoTime,
    poke: MonoTime,
    /// The end of an autonomous turn ended on silence, until the agent has
    /// been quiet for 30 s after it, a turn the user starts, or another end.
    guard: Option<MonoTime>,
    /// Output came while the turn was ending.
    after_prompt: bool,
}

impl World {
    fn new(prop: Prop) -> Self {
        let clock = ManualClock::new();
        let ids = Arc::new(IdGen::seeded(Arc::new(clock.clone()), 1));
        let durations = Durations::default();
        let setter = (crate::policy::testkit::column("UI control"), false);
        let session = SessionCore::new(SessionId::from_uuid(1), ids, &durations, false, setter);
        Self {
            prop,
            d: CoreDriver::with_clock(session, clock),
            instance: 1,
            attaching: false,
            prompts: Vec::new(),
            snapshots: Vec::new(),
            tools: Vec::new(),
            finished: HashSet::new(),
            running: BTreeSet::new(),
            thread_active: None,
            attached: false,
            asks: Vec::new(),
            next_ask: 0,
            resolves: 0,
            current: None,
            settled: HashSet::new(),
            completed: HashSet::new(),
            items: HashMap::new(),
            abandoned: HashSet::new(),
            loose: HashSet::new(),
            late: HashSet::new(),
            queue: (0, false),
            advancing: false,
            quiet_since: MonoTime::ZERO,
            poke: MonoTime::ZERO,
            guard: None,
            after_prompt: false,
        }
    }

    fn must(&self, prop: Prop, ok: bool, what: &str) {
        assert!(self.prop != prop || ok, "{prop:?}: {what}");
    }

    /// The n-th tool call that has not finished.
    fn unfinished(&self, n: u8) -> Option<String> {
        let open: Vec<&String> = self
            .tools
            .iter()
            .filter(|t| !self.finished.contains(*t))
            .collect();
        open.get(usize::from(n) % open.len().max(1))
            .map(|t| (*t).clone())
    }

    fn input(&mut self, op: &Op) -> Option<In> {
        let update = |update| Some(In::Agent(DriverEvent::Update(update)));
        let tool = |tool_call_id, output, status: Option<ToolStatus>| {
            let exit_code = status.map(|_| 0);
            let (title, diffs) = (None, None);
            update(NormUpdate::ToolUpdate {
                tool_call_id,
                title,
                command: None,
                paths: None,
                status,
                output,
                exit_code,
                diffs,
            })
        };
        Some(match op.clone() {
            Op::Send => In::Machine(send("m", SendMode::Queue)),
            Op::Interrupt => In::Machine(turn::Input::Interrupt {
                expected_turn: None,
            }),
            Op::Attach(_) if !self.attaching => return None,
            Op::Attach(ok) => {
                self.attaching = false;
                self.attached = ok;
                In::Machine(if ok {
                    attached(self.instance)
                } else {
                    turn::Input::Attached(Err(agent_error()))
                })
            }
            Op::Snapshot(_) if self.snapshots.is_empty() => return None,
            Op::Snapshot(n) => {
                let (turn, phase) = self.snapshots.remove(usize::from(n) % self.snapshots.len());
                In::Machine(snapshot(&turn, phase))
            }
            Op::Answer(_, _) if self.prompts.is_empty() => return None,
            Op::Answer(n, ok) => {
                let tag = self.prompts.remove(usize::from(n) % self.prompts.len());
                let error = agent_error();
                In::Agent(match ok {
                    true => DriverEvent::PromptDone {
                        tag,
                        stop_reason: StopReason::EndTurn,
                        usage: None,
                    },
                    false => DriverEvent::PromptFailed { tag, error },
                })
            }
            Op::Text(m) => {
                let (message_id, text) = (Some(format!("m{}", m % 2)), "t".into());
                return update(NormUpdate::AgentText { message_id, text });
            }
            Op::Thought => return update(NormUpdate::Thought { text: "…".into() }),
            Op::ToolStart => {
                self.tools.push(format!("t{}", self.tools.len()));
                self.running.extend(self.tools.last().cloned());
                return update(NormUpdate::ToolStart {
                    tool_call_id: self.tools[self.tools.len() - 1].clone(),
                    tool_kind: cupper_protocol::item::ToolKind::Execute,
                    title: "Run".into(),
                    paths: Vec::new(),
                    command: Some("make".into()),
                    cwd: Some(cupper_protocol::annotations::WirePath("/work".into())),
                });
            }
            Op::Output(n) => {
                let id = self.unfinished(n)?;
                self.running.insert(id.clone());
                return tool(id, Some(ToolOutput::Append("o".into())), None);
            }
            Op::Done(n) => {
                let id = self.unfinished(n)?;
                self.running.remove(&id);
                self.finished.insert(id.clone());
                return tool(id, None, Some(ToolStatus::Completed));
            }
            Op::Cost => {
                let cost = Some(Cost {
                    amount: 0.1,
                    currency: "USD".into(),
                });
                In::Agent(DriverEvent::Usage(UsageUpdated {
                    context_used: 1,
                    context_size: 2,
                    cost,
                }))
            }
            Op::Thread(active) => {
                self.thread_active = Some(active);
                let (waiting_on_approval, waiting_on_user_input, system_error) =
                    (false, false, false);
                let state = RunState {
                    active,
                    waiting_on_approval,
                    waiting_on_user_input,
                    system_error,
                };
                In::Agent(DriverEvent::RunState(state))
            }
            // Only an attached agent asks.
            Op::Ask if !self.attached => return None,
            Op::Ask => {
                self.next_ask += 1;
                let ask = AskId(self.next_ask);
                self.asks.push((ask, None));
                let option = option("allow", "allow_once", "once", "approve");
                let request = AskRequest::Classified(permission(vec![option]));
                In::Agent(DriverEvent::Ask {
                    ask,
                    tool_call_id: None,
                    request,
                })
            }
            Op::Resolve => {
                let (ask, written) = self.asks.first().cloned()?;
                self.resolves += 1;
                match written {
                    Some((request_id, digest)) if !self.resolves.is_multiple_of(3) => {
                        In::Respond(Box::new(dismiss(request_id, digest)))
                    }
                    // Withdrawn before it was written: no event tells.
                    None => {
                        self.asks.remove(0);
                        In::Agent(DriverEvent::AskWithdrawn(ask))
                    }
                    Some(_) => In::Agent(DriverEvent::AskWithdrawn(ask)),
                }
            }
            Op::Exit => {
                // Requests not written yet go with the instance.
                self.attached = false;
                self.asks.retain(|(_, written)| written.is_some());
                self.prompts.clear();
                self.thread_active = None;
                self.instance += 1;
                let instance = AdapterInstanceId::from_uuid(self.instance - 1);
                In::Agent(DriverEvent::AdapterExited {
                    instance,
                    status: ExitStatus::from_raw(9),
                    stderr: String::new(),
                })
            }
            Op::Advance(_) => return None,
        })
    }

    fn apply(&mut self, op: Op) {
        let now = self.d.now().mono;
        let output = matches!(
            op,
            Op::Text(_) | Op::Thought | Op::ToolStart | Op::Output(_) | Op::Done(_) | Op::Ask
        );
        let ending = self
            .current
            .as_ref()
            .is_some_and(|(t, ..)| self.settled.contains(t));
        let idle = self.current.is_none();
        // Signals that the agent is done: cost, the thread going idle, a tool
        // finishing while it is idle.
        let idle_thread = self.thread_active == Some(false);
        let done =
            matches!(op, Op::Cost | Op::Thread(false)) || matches!(op, Op::Done(_)) && idle_thread;
        let autonomous = matches!(self.current, Some((_, TurnOrigin::Autonomous, _)));
        let step = match op {
            Op::Advance(ms) => Step::Advance(Duration::from_millis(ms)),
            _ => match self.input(&op) {
                Some(input) => Step::Input(input),
                None => return,
            },
        };
        // Once the agent has been quiet for 30 s after a silent end, the
        // queue may move; activity after that does not hold it again
        // (design 5.2.6: "结束后再静默 30 s 才允许出队").
        let over = |end: MonoTime| now >= end.max(self.quiet_since) + Duration::from_secs(30);
        if self.guard.is_some_and(over) {
            self.guard = None;
        }
        if output || matches!(op, Op::Thread(true)) {
            self.quiet_since = now;
        }
        if output || matches!(op, Op::Thread(true) | Op::Resolve | Op::Exit) {
            self.poke = now;
        }
        self.after_prompt |= output && ending;
        self.advancing = matches!(op, Op::Advance(_));
        for (at, out) in self.d.run([step]) {
            self.observe(at, out);
        }
        let free = self.running.is_empty() && self.asks.is_empty();
        if done && autonomous && free {
            let ended = !matches!(self.current, Some((_, TurnOrigin::Autonomous, _)));
            self.must(
                Prop::Guard,
                ended,
                "the agent being done ends a free autonomous turn at once",
            );
        }
        if output && idle {
            assert!(
                self.current.is_some(),
                "output outside a turn starts one (K9)"
            );
        }
        self.check_liveness();
    }

    fn observe(&mut self, at: MonoTime, out: Out) {
        match out {
            Out::Driver(DriverCommand::Prompt { tag, .. }) => self.prompts.push(tag),
            Out::Driver(DriverCommand::Close { .. }) => self.prompts.clear(),
            Out::Machine(turn::Action::Attach(_)) => self.attaching = true,
            Out::Machine(turn::Action::Snapshot { turn_id, phase }) => {
                self.snapshots.push((turn_id, phase))
            }
            Out::Machine(turn::Action::Settle { end, .. }) => {
                let judged = matches!(self.current, Some((_, TurnOrigin::Autonomous, _)))
                    && end.status == TurnStatus::Completed;
                let free = self.running.is_empty() && self.asks.is_empty();
                self.must(
                    Prop::Guard,
                    !judged || free,
                    "no end while a tool runs or a request waits",
                );
                self.settled.insert(end.turn_id);
                // Abandoned; the broker resolves the turn's requests.
                self.running.clear();
            }
            Out::Event(body) => self.event(at, body),
            Out::Machine(_)
            | Out::Driver(_)
            | Out::Answer(_)
            | Out::Timer(_)
            | Out::Tier(_)
            | Out::Spill(_) => {}
        }
    }

    fn event(&mut self, at: MonoTime, body: EventBody) {
        use Prop::{Guard, Immutable};
        let open = |w: &Self, turn: &TurnId| !w.completed.contains(turn);
        match body {
            EventBody::TurnStarted(started) => {
                assert!(self.current.is_none(), "one turn at a time");
                let origin = started.origin;
                if origin == TurnOrigin::Queued {
                    let quiet = self.guard.is_none_or(|end| {
                        at >= end + Duration::from_secs(30)
                            && at >= self.quiet_since + Duration::from_secs(30)
                    });
                    self.must(
                        Guard,
                        quiet,
                        "after a turn ended on silence, the queue waits for 30 s of quiet",
                    );
                    self.must(
                        Guard,
                        !self.after_prompt,
                        "output after a prompt goes to an autonomous turn first",
                    );
                }
                if origin != TurnOrigin::Autonomous {
                    self.guard = None;
                }
                self.after_prompt = false;
                self.current = Some((started.turn_id, origin, at));
            }
            EventBody::TurnCompleted(completed) => {
                let turn = completed.turn_id;
                let (current, origin, _) = self.current.take().expect("a running turn completes");
                assert_eq!(current, turn, "only the running turn completes");
                let once = self.settled.contains(&turn) && self.completed.insert(turn.clone());
                self.must(Immutable, once, "a turn settles, then completes once");
                let open_items = self
                    .items
                    .values()
                    .any(|(t, ended, _)| *t == turn && !ended);
                self.must(
                    Immutable,
                    !open_items,
                    "every item of a turn ends before turn.completed",
                );
                if origin == TurnOrigin::Autonomous {
                    let silent = self.advancing && completed.status == TurnStatus::Completed;
                    let quiet = !silent || self.thread_active != Some(true);
                    self.must(Guard, quiet, "no end on silence while the thread is active");
                    self.guard = silent.then_some(at);
                }
            }
            EventBody::ItemStarted(started) if started.turn_id.is_none() => {
                assert!(
                    matches!(started.item, Item::Notice(_)),
                    "only notices go without a turn"
                );
                self.loose.insert(started.item_id);
            }
            EventBody::ItemCompleted(item) if self.loose.contains(&item.item_id) => {}
            EventBody::ItemStarted(started) => {
                let turn = started.turn_id.expect("items go into a turn");
                let running = self.current.as_ref().is_some_and(|(t, ..)| *t == turn);
                self.must(
                    Immutable,
                    running && !self.settled.contains(&turn),
                    "items start in the running turn, before it settles",
                );
                if let Item::LateToolOutput(late) = &started.item {
                    let of = &late.of;
                    let original = self.abandoned.contains(&of.item_id)
                        && self.completed.contains(&of.turn_id);
                    self.must(
                        Immutable,
                        original,
                        "late output refers to a call abandoned in a completed turn",
                    );
                    let first = self.late.insert((of.item_id.clone(), turn.clone()));
                    self.must(Immutable, first, "one late output item per call and turn");
                }
                let tool = matches!(started.item, Item::ToolCall(_));
                self.items.insert(started.item_id, (turn, false, tool));
            }
            EventBody::ItemDelta(delta) => self.item_event(&delta.item_id, None),
            EventBody::ItemUpdated(updated) => self.item_event(&updated.item_id, None),
            EventBody::ItemCompleted(item) => self.item_event(&item.item_id, Some(item.status)),
            EventBody::SnapshotRecorded(record) => {
                self.must(
                    Immutable,
                    open(self, &record.turn_id),
                    "no snapshot after turn.completed",
                );
            }
            EventBody::QueueUpdated(update) => {
                self.queue = (update.entries.len(), update.paused.is_some())
            }
            EventBody::InteractionRequested(requested) => {
                let turn = requested.turn_id.expect("requests go into a turn");
                let running = self.current.as_ref().is_some_and(|(t, ..)| *t == turn);
                let ok = running && !self.settled.contains(&turn);
                self.must(
                    Immutable,
                    ok,
                    "requests go into the running turn, before it settles",
                );
                let written = (requested.request_id, requested.digest);
                let first = self.asks.iter_mut().find(|(_, w)| w.is_none());
                first.expect("a request asked for").1 = Some(written);
            }
            EventBody::InteractionResolved(resolved) => {
                let id = Some(&resolved.request_id);
                self.asks.retain(|(_, w)| w.as_ref().map(|(r, _)| r) != id);
            }
            _ => {}
        }
    }

    /// An event of an item: never after the item ended or its turn completed.
    fn item_event(&mut self, item: &ItemId, end: Option<ItemStatus>) {
        let (turn, ended, tool) = self.items.get(item).cloned().expect("a started item");
        let ok = !ended && !self.completed.contains(&turn);
        self.must(
            Prop::Immutable,
            ok,
            "nothing changes an ended item or a completed turn",
        );
        if let Some(status) = end {
            if tool && status == ItemStatus::Abandoned {
                self.abandoned.insert(item.clone());
            }
            self.items.insert(item.clone(), (turn, true, tool));
        }
    }

    /// The judge ends a quiet autonomous turn after 10 s; an idle session
    /// holds a waiting queue only after a turn ended on silence, and for 30 s
    /// of quiet at most.
    fn check_liveness(&self) {
        let now = self.d.now().mono;
        if let Some((_, TurnOrigin::Autonomous, started)) = &self.current {
            let free =
                self.running.is_empty() && self.asks.is_empty() && self.thread_active != Some(true);
            let due = (*started).max(self.poke) + Duration::from_secs(10);
            self.must(
                Prop::Guard,
                !(free && now >= due),
                "a quiet autonomous turn ends after 10 s",
            );
        }
        if let (None, (1.., false)) = (&self.current, self.queue) {
            let due = self
                .guard
                .map(|end| end.max(self.quiet_since) + Duration::from_secs(30));
            let held = due.is_some_and(|due| now < due);
            self.must(
                Prop::Guard,
                held,
                "the queue moves unless 30 s of quiet are not over",
            );
        }
    }

    /// Answers everything until the session is idle with an empty queue.
    fn quiesce(&mut self) {
        for _ in 0..200 {
            let done = self.current.is_none() && self.queue.0 == 0 && !self.attaching;
            if done && self.prompts.is_empty() && self.snapshots.is_empty() {
                break;
            }
            self.apply(Op::Attach(true));
            while !self.prompts.is_empty() {
                self.apply(Op::Answer(0, true));
            }
            while !self.snapshots.is_empty() {
                self.apply(Op::Snapshot(0));
            }
            while self.unfinished(0).is_some() {
                self.apply(Op::Done(0));
            }
            while !self.asks.is_empty() {
                self.apply(Op::Resolve);
            }
            if self.thread_active == Some(true) {
                self.apply(Op::Thread(false));
            }
            if self.current.is_none() {
                self.d
                    .input(In::Machine(turn::Input::QueueResume))
                    .into_iter()
                    .for_each(|out| self.observe(self.d.now().mono, out));
            }
            self.apply(Op::Advance(45_000));
        }
        assert!(
            self.current.is_none() && self.queue.0 == 0,
            "the session settles"
        );
        assert_eq!(
            self.completed.len(),
            self.settled.len(),
            "every turn completes"
        );
    }
}

/// The test harness dismisses a written request.
fn dismiss(request_id: RequestId, request_digest: Digest) -> Respond {
    let params = json!({"command_id": "c-1", "session_id": SessionId::from_uuid(1),
        "request_id": request_id, "request_digest": request_digest,
        "outcome": {"type": "dismissed"}});
    let by = json!({"principal_id": "pr_test", "client_kind": "test", "client_name": "props"});
    let (principal, tier) = (column("test_harness"), None);
    let (resolved_by, params) = (from_value(by).unwrap(), from_value(params).unwrap());
    Respond {
        principal,
        resolved_by,
        tier,
        params,
    }
}

fn run(prop: Prop, ops: Vec<Op>) {
    let mut world = World::new(prop);
    for op in ops {
        world.apply(op);
    }
    world.quiesce();
}

proptest! {
    #[test]
    fn prop_autonomous_dequeue_guard_queue_waits_while_the_agent_works(ops in ops()) {
        run(Prop::Guard, ops);
    }

    #[test]
    fn prop_turn_snapshot_immutable_no_event_after_turn_completed(ops in ops()) {
        run(Prop::Immutable, ops);
    }
}
