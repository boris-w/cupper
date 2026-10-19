//! `AutonomousJudge` (design 5.2.6, Q10, K9; the rulings at the end of
//! 5.3.11): when the agent's work outside any prompt makes an autonomous
//! turn, when that turn ends, and when the queue may move on after it.
//!
//! - Output (an update or a request) while no turn runs starts an autonomous
//!   turn. Output after a turn's prompt returned, while the turn waits for
//!   its end snapshot, holds the queue and starts an autonomous turn as soon
//!   as that turn completes, before any queued message. Output while a turn
//!   runs belongs to it.
//! - An autonomous turn ends once no tool runs and no request is pending, at
//!   the first of: a `usage_update` with `cost` (Claude); the thread going
//!   idle (Codex); a tool finishing while the thread is idle (late output,
//!   K3); 10 s without updates, unless the thread is known to be active.
//! - A turn ended on silence keeps the queue held until the agent has been
//!   quiet for another 30 s (K1); a turn the user starts while idle ends the
//!   hold. Only the queue is held: a message sent while idle runs at once.
//!
//! The session's actor wires the judge between its other cores. A driver
//! event goes to the judge first, then to the turn machine, then (an update)
//! to the coalescer. A request goes to the judge as [`Input::Asked`] before
//! the broker writes it, and each resolution follows as [`Input::Resolved`].
//! Every [`TurnSignal`] of the turn machine's actions goes to the coalescer,
//! then to the judge. The judge's actions reach the turn machine as
//! [`Action::turn_input`], after the machine's current step.

use std::collections::BTreeSet;
use std::mem;
use std::time::Duration;

use cupper_protocol::session::TurnOrigin;

use super::turn::{self, TurnSignal};
use crate::clock::Now;
use crate::driver::api::{DriverEvent, NormUpdate, ToolStatus};
use crate::durations::{AUTONOMOUS_SILENCE, DEQUEUE_GUARD, Durations};
use crate::pure::{Core, TimerRequest};

#[derive(Debug)]
pub struct AutonomousJudge {
    silence: Duration,
    guard: Duration,
    phase: Phase,
    /// Output came while the turn was ending: start one when it completes.
    start_next: bool,
    /// The judge ends the autonomous turn on silence.
    silent_end: bool,
    held: bool,
    /// Tool calls running, by ACP `toolCallId`.
    running: BTreeSet<String>,
    /// Requests not yet resolved.
    asks: usize,
    /// The thread's state, from an agent that reports it (Codex).
    thread_active: Option<bool>,
}

/// The turn machine's turn, as its signals tell it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Prompted,
    Autonomous,
    /// The end is decided; `turn.completed` is still to come.
    Ending,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    Turn(TurnSignal),
    Driver(DriverEvent),
    /// A request from the agent, before the broker writes it.
    Asked,
    /// The broker resolved a request.
    Resolved,
    Expired(JudgeTimer),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Start an autonomous turn.
    Start,
    /// End the autonomous turn.
    End,
    /// Hold the queue, or let it move again.
    Hold(bool),
    Timer(TimerRequest<JudgeTimer>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JudgeTimer {
    /// 10 s without updates.
    Silence,
    /// 30 s of quiet after a turn ended on silence.
    Guard,
}

impl Action {
    /// The turn machine's input for this action; `None` for a timer.
    pub fn turn_input(&self) -> Option<turn::Input> {
        match self {
            Self::Start => Some(turn::Input::AutonomousStarted),
            Self::End => Some(turn::Input::AutonomousEnded),
            Self::Hold(hold) => Some(turn::Input::HoldQueue(*hold)),
            Self::Timer(_) => None,
        }
    }
}

impl AutonomousJudge {
    pub fn new(durations: &Durations) -> Self {
        Self {
            silence: durations.get(AUTONOMOUS_SILENCE),
            guard: durations.get(DEQUEUE_GUARD),
            phase: Phase::Idle,
            start_next: false,
            silent_end: false,
            held: false,
            running: BTreeSet::new(),
            asks: 0,
            thread_active: None,
        }
    }

    fn arm(&self, key: JudgeTimer, now: Now) -> Action {
        let after = match key {
            JudgeTimer::Silence => self.silence,
            JudgeTimer::Guard => self.guard,
        };
        Action::Timer(TimerRequest::Set {
            key,
            at: now.mono + after,
        })
    }

    fn hold(&mut self, out: &mut Vec<Action>) {
        if !mem::replace(&mut self.held, true) {
            out.push(Action::Hold(true));
        }
    }

    fn release(&mut self, out: &mut Vec<Action>) {
        if mem::take(&mut self.held) {
            out.push(Action::Timer(TimerRequest::Cancel(JudgeTimer::Guard)));
            out.push(Action::Hold(false));
        }
    }

    fn turn(&mut self, signal: TurnSignal, now: Now, out: &mut Vec<Action>) {
        match signal {
            TurnSignal::Started { origin, .. } => {
                self.silent_end = false;
                if origin == TurnOrigin::Autonomous {
                    self.phase = Phase::Autonomous;
                    out.push(self.arm(JudgeTimer::Silence, now));
                } else {
                    self.phase = Phase::Prompted;
                    self.release(out);
                }
            }
            TurnSignal::Settled { .. } => {
                if self.phase == Phase::Autonomous {
                    out.push(Action::Timer(TimerRequest::Cancel(JudgeTimer::Silence)));
                }
                // Its tool calls are abandoned; late output starts them again.
                self.running.clear();
                self.phase = Phase::Ending;
            }
            TurnSignal::Completed(_) => {
                self.phase = Phase::Idle;
                if mem::take(&mut self.start_next) {
                    out.push(Action::Start);
                } else if mem::take(&mut self.silent_end) {
                    out.push(self.arm(JudgeTimer::Guard, now));
                } else {
                    self.release(out);
                }
            }
        }
    }

    /// The agent produced output: it opens a turn when none runs.
    fn output(&mut self, now: Now, out: &mut Vec<Action>) {
        match self.phase {
            Phase::Idle => out.push(Action::Start),
            Phase::Ending => {
                self.start_next = true;
                self.hold(out);
            }
            Phase::Autonomous => out.push(self.arm(JudgeTimer::Silence, now)),
            Phase::Prompted => {}
        }
    }

    /// The agent is busy without output: the quiet starts over.
    fn activity(&mut self, now: Now, out: &mut Vec<Action>) {
        match self.phase {
            Phase::Autonomous => out.push(self.arm(JudgeTimer::Silence, now)),
            Phase::Idle if self.held => out.push(self.arm(JudgeTimer::Guard, now)),
            _ => {}
        }
    }

    fn end(&mut self, silent: bool, out: &mut Vec<Action>) {
        if self.phase != Phase::Autonomous || !self.running.is_empty() || self.asks > 0 {
            return;
        }
        if silent {
            self.silent_end = true;
            self.hold(out);
        }
        out.push(Action::End);
    }

    fn driver(&mut self, event: DriverEvent, now: Now, out: &mut Vec<Action>) {
        match event {
            DriverEvent::Update(update) => {
                let finished = match update {
                    NormUpdate::ToolUpdate {
                        tool_call_id,
                        status: Some(ToolStatus::Completed | ToolStatus::Failed),
                        ..
                    } => {
                        self.running.remove(&tool_call_id);
                        true
                    }
                    NormUpdate::ToolStart { tool_call_id, .. }
                    | NormUpdate::ToolUpdate { tool_call_id, .. } => {
                        self.running.insert(tool_call_id);
                        false
                    }
                    _ => false,
                };
                self.output(now, out);
                if finished && self.thread_active == Some(false) {
                    self.end(false, out);
                }
            }
            DriverEvent::Usage(usage) if usage.cost.is_some() => self.end(false, out),
            DriverEvent::RunState(state) => {
                self.thread_active = Some(state.active);
                if state.active {
                    self.activity(now, out);
                } else {
                    self.end(false, out);
                }
            }
            DriverEvent::UpstreamRetry { .. } => self.activity(now, out),
            DriverEvent::AdapterExited { .. } => self.thread_active = None,
            _ => {}
        }
    }
}

impl Core for AutonomousJudge {
    type Input = Input;
    type Action = Action;
    type TimerKey = JudgeTimer;

    fn step(&mut self, input: Input, now: Now) -> Vec<Action> {
        let mut out = Vec::new();
        match input {
            Input::Turn(signal) => self.turn(signal, now, &mut out),
            Input::Driver(event) => self.driver(event, now, &mut out),
            Input::Asked => {
                self.asks += 1;
                self.output(now, &mut out);
            }
            Input::Resolved => {
                self.asks = self.asks.saturating_sub(1);
                if self.phase == Phase::Autonomous {
                    out.push(self.arm(JudgeTimer::Silence, now));
                }
            }
            Input::Expired(JudgeTimer::Silence) if self.thread_active != Some(true) => {
                self.end(true, &mut out)
            }
            Input::Expired(JudgeTimer::Guard) if self.phase == Phase::Idle => {
                self.release(&mut out)
            }
            Input::Expired(_) => {}
        }
        out
    }

    fn timer_request(action: &Action) -> Option<TimerRequest<JudgeTimer>> {
        match action {
            Action::Timer(request) => Some(request.clone()),
            _ => None,
        }
    }

    fn timer_expired(key: JudgeTimer) -> Input {
        Input::Expired(key)
    }
}

#[cfg(test)]
mod tests {
    use cupper_protocol::annotations::{DurationMs, TimestampMs};
    use cupper_protocol::domain::Cost;
    use cupper_protocol::ids::TurnId;
    use cupper_protocol::session::{TurnStatus, UsageUpdated};

    use super::*;
    use crate::driver::api::RunState;
    use crate::pure::CoreDriver;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn started(origin: TurnOrigin) -> Input {
        let turn_id = TurnId::from_uuid(1);
        Input::Turn(TurnSignal::Started { turn_id, origin })
    }

    fn settled() -> Input {
        Input::Turn(TurnSignal::Settled { interrupted: false })
    }

    fn completed() -> Input {
        Input::Turn(TurnSignal::Completed(turn::TurnEnd {
            turn_id: TurnId::from_uuid(1),
            status: TurnStatus::Completed,
            stop_reason: None,
            error_code: None,
            usage: None,
            ended_at: TimestampMs(0),
            duration_ms: DurationMs(0),
        }))
    }

    fn update(update: NormUpdate) -> Input {
        Input::Driver(DriverEvent::Update(update))
    }

    fn text() -> Input {
        update(NormUpdate::Thought { text: "…".into() })
    }

    fn tool(status: Option<ToolStatus>) -> Input {
        let (tool_call_id, title, output, exit_code, diffs) = ("t1".into(), None, None, None, None);
        update(NormUpdate::ToolUpdate {
            tool_call_id,
            title,
            status,
            output,
            exit_code,
            diffs,
        })
    }

    fn thread(active: bool) -> Input {
        let (waiting_on_approval, waiting_on_user_input, system_error) = (false, false, false);
        let state = RunState {
            active,
            waiting_on_approval,
            waiting_on_user_input,
            system_error,
        };
        Input::Driver(DriverEvent::RunState(state))
    }

    fn kinds(actions: Vec<Action>) -> Vec<&'static str> {
        let kind = |action: &Action| match action {
            Action::Start => "start",
            Action::End => "end",
            Action::Hold(true) => "hold",
            Action::Hold(false) => "release",
            Action::Timer(TimerRequest::Set {
                key: JudgeTimer::Silence,
                ..
            }) => "silence",
            Action::Timer(TimerRequest::Set {
                key: JudgeTimer::Guard,
                ..
            }) => "guard",
            Action::Timer(TimerRequest::Cancel(_)) => "untimer",
        };
        actions.iter().map(kind).collect()
    }

    #[test]
    fn autonomous_judge_ends_on_silence_then_holds_the_queue_for_30_s_of_quiet() {
        let mut j = CoreDriver::new(AutonomousJudge::new(&Durations::default()));
        assert_eq!(kinds(j.input(text())), ["start"]);
        assert_eq!(kinds(j.input(started(TurnOrigin::Autonomous))), ["silence"]);
        j.advance(ms(9_000));
        assert_eq!(kinds(j.input(text())), ["silence"]);
        assert!(j.advance(ms(9_999)).is_empty());
        assert_eq!(kinds(j.advance(ms(1))), ["hold", "end"]);
        assert_eq!(kinds(j.input(settled())), ["untimer"]);
        assert_eq!(kinds(j.input(completed())), ["guard"]);
        assert!(j.advance(ms(20_000)).is_empty());
        let retry = Input::Driver(DriverEvent::UpstreamRetry { attempt: 1, max: 5 });
        assert_eq!(kinds(j.input(retry)), ["guard"], "the quiet starts over");
        assert!(j.advance(ms(29_999)).is_empty());
        assert_eq!(kinds(j.advance(ms(1))), ["untimer", "release"]);
    }

    #[test]
    fn autonomous_judge_takes_output_after_a_prompt_and_waits_for_running_tools() {
        let mut j = CoreDriver::new(AutonomousJudge::new(&Durations::default()));
        j.input(started(TurnOrigin::User));
        j.input(settled());
        assert_eq!(
            kinds(j.input(text())),
            ["hold"],
            "output while the turn ends"
        );
        assert_eq!(
            kinds(j.input(completed())),
            ["start"],
            "before any queued message"
        );
        j.input(started(TurnOrigin::Autonomous));
        j.input(tool(None));
        j.input(thread(false));
        let cost = Some(Cost {
            amount: 0.1,
            currency: "USD".into(),
        });
        let usage = UsageUpdated {
            context_used: 1,
            context_size: 2,
            cost,
        };
        assert!(j.input(Input::Driver(DriverEvent::Usage(usage))).is_empty());
        assert!(j.advance(ms(60_000)).is_empty(), "no end while a tool runs");
        let done = j.input(tool(Some(ToolStatus::Completed)));
        assert_eq!(
            kinds(done),
            ["silence", "end"],
            "a tool finishing while the thread is idle"
        );
        j.input(settled());
        assert_eq!(kinds(j.input(completed())), ["untimer", "release"]);
    }
}
