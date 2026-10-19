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
//! - A request from the agent goes to the judge ([`Input::Asked`]) and to the
//!   coalescer (`Interject`) before it is written; each resolution follows as
//!   [`Input::Resolved`]. The requests of a turn resolve at its Settle. T088
//!   routes the agent's requests here through the interaction broker.

use std::mem;
use std::sync::Arc;

use cupper_protocol::event::EventBody;
use cupper_protocol::ids::TurnId;

use crate::clock::{IdGen, Now};
use crate::driver::api::DriverEvent;
use crate::durations::Durations;
use crate::orchestrator::autonomous::{self, AutonomousJudge, JudgeTimer};
use crate::orchestrator::coalescer::{self, Coalescer, FlushTimer};
use crate::orchestrator::turn::{self, TurnMachine, TurnSignal, TurnTimer};
use crate::pure::{Core, TimerRequest};

#[derive(Debug)]
pub struct SessionCore {
    machine: TurnMachine,
    judge: AutonomousJudge,
    coalescer: Coalescer,
    /// Requests not yet resolved.
    asks: usize,
}

#[derive(Debug, Clone)]
pub enum Input {
    Machine(turn::Input),
    Agent(DriverEvent),
    /// A request from the agent, before it is written.
    Asked,
    /// A request was resolved.
    Resolved,
    Expired(Key),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    Turn(TurnTimer),
    Judge(JudgeTimer),
    Flush(FlushTimer),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Append to the session stream.
    Event(EventBody),
    /// The turn machine's other actions; never `Event`, `Timer` or `Complete`.
    Machine(turn::Action),
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
    pub fn new(ids: Arc<IdGen>, durations: &Durations, stops_commands: bool) -> Self {
        Self {
            machine: TurnMachine::new(Arc::clone(&ids), durations),
            judge: AutonomousJudge::new(durations),
            coalescer: Coalescer::new(ids, durations, stops_commands),
            asks: 0,
        }
    }

    /// The agent's capabilities came in with an attach.
    pub fn set_stops_commands(&mut self, stops_commands: bool) {
        self.coalescer.set_stops_commands(stops_commands);
    }

    /// The running turn, from `turn.started` until it completes.
    pub fn turn_id(&self) -> Option<&TurnId> {
        self.machine.turn_id()
    }

    /// A turn's signals go to the coalescer at once, and to the judge after
    /// the machine's step; the judge's actions go back to the machine.
    fn machine(&mut self, input: turn::Input, now: Now, out: &mut Vec<Action>) {
        let mut judge = Vec::new();
        for action in self.machine.step(input, now) {
            let signal = TurnSignal::of(&action);
            match action {
                turn::Action::Event(body) => out.push(Action::Event(body)),
                turn::Action::Timer(request) => out.push(timer(request, Key::Turn)),
                // The coalescer writes `turn.completed`.
                turn::Action::Complete(_) => {}
                action => out.push(Action::Machine(action)),
            }
            if let Some(signal) = signal {
                self.coalesce(coalescer::Input::Turn(signal.clone()), now, out);
                if let TurnSignal::Settled { .. } = signal {
                    judge.extend(
                        (0..mem::take(&mut self.asks)).map(|_| autonomous::Input::Resolved),
                    );
                }
                judge.push(autonomous::Input::Turn(signal));
            }
        }
        for input in judge {
            self.judge(input, now, out);
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
            // The judge first, so that a turn is open for the coalescer.
            Input::Agent(event) => {
                self.judge(autonomous::Input::Driver(event.clone()), now, &mut out);
                self.machine(turn::Input::Driver(event.clone()), now, &mut out);
                if let DriverEvent::Update(update) = event {
                    self.coalesce(coalescer::Input::Update(update), now, &mut out);
                }
            }
            Input::Asked => {
                self.asks += 1;
                self.judge(autonomous::Input::Asked, now, &mut out);
                self.coalesce(coalescer::Input::Interject, now, &mut out);
            }
            Input::Resolved if self.asks > 0 => {
                self.asks -= 1;
                self.judge(autonomous::Input::Resolved, now, &mut out);
            }
            Input::Resolved => {}
            Input::Expired(Key::Turn(key)) => {
                self.machine(turn::Input::Expired(key), now, &mut out)
            }
            Input::Expired(Key::Judge(key)) => {
                self.judge(autonomous::Input::Expired(key), now, &mut out)
            }
            Input::Expired(Key::Flush(key)) => {
                self.coalesce(coalescer::Input::Expired(key), now, &mut out)
            }
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
