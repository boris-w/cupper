//! PROP-interaction (design 5.9.8) on virtual time: random requests
//! (permissions, plan confirmations, forms), answers from each principal
//! that holds `approve` (stale digests, other options, the window's keys,
//! early and late), withdrawals, interrupts, turn ends and adapter exits.
//! A `World` checks each action as it comes, and each reply against its own
//! run of 5.2.7's checks.

use std::collections::HashMap;
use std::time::Duration;

use cupper_protocol::domain::Tier;
use cupper_protocol::event::EventBody;
use cupper_protocol::ids::{AdapterInstanceId, RequestId, TurnId};
use cupper_protocol::interaction::{
    InteractionOutcome as O, InteractionRequest as R, InteractionRequested, InteractionResolved,
    InteractionStatus as S, ResolutionReason, request_digest,
};
use proptest::prelude::*;
use serde_json::json;

use super::question::AnswerError;
use super::tests::{answer, approval, ask, broker, form, respond};
use super::{Action, Input, InteractionBroker, Rejection};
use crate::clock::MonoTime;
use crate::driver::api::{AskAnswer, AskId, AskRequest, DriverCommand};
use crate::policy::testkit::{option, permission, selected, with_mode};
use crate::policy::{self, Pending};
use crate::pure::CoreDriver;

use ResolutionReason::{AdapterExited, TurnCancelled, TurnEnded, Withdrawn};

/// The principals that reach the broker: the method takes `approve`.
const WHO: &str = "UI control,UI window,cli + approve,test_harness,remote_device";

#[derive(Debug, Clone)]
enum Op {
    Ask(u8),
    /// A request, mostly a pending one (or one the session never had); a
    /// principal; an outcome and a window key. The digest goes stale when set.
    Respond(u8, u8, u8, bool),
    Withdraw(u8),
    Interrupt,
    /// The turn ends, cancelled when set; the next turn starts when set.
    Settle(bool, bool),
    /// The adapter exits; the turn ends and the next one has a new instance.
    Exit,
    Advance(u64),
}

fn ops() -> impl Strategy<Value = Vec<Op>> {
    let n = || any::<u8>();
    let respond = (n(), n(), n(), prop::bool::weighted(0.1));
    let op = prop_oneof![
        4 => (0..5_u8).prop_map(Op::Ask),
        10 => respond.prop_map(|(i, who, how, stale)| Op::Respond(i, who, how, stale)),
        1 => n().prop_map(Op::Withdraw),
        1 => Just(Op::Interrupt),
        3 => (any::<bool>(), prop::bool::weighted(0.9)).prop_map(|(c, next)| Op::Settle(c, next)),
        1 => Just(Op::Exit),
        3 => prop_oneof![0..400_u64, 400..2_000_u64].prop_map(Op::Advance),
    ];
    prop::collection::vec(op, 0..80)
}

/// Asks of kind `k`: an approval, a privileged-only one, a plan, a form, a form it cannot show.
fn request(k: u8) -> AskRequest {
    let options = |options| AskRequest::Classified(permission(options));
    match k {
        0 => approval(),
        1 => options(vec![
            option("always", "allow_always", "persistent", "privileged"),
            option("mystery", "allow_maybe", "unknown", "privileged"),
        ]),
        2 => {
            let plan = json!({"type": "plan_confirm", "plan": {"type": "text", "text": "p"}, "options": [
                option("once", "allow_once", "plan_accept", "approve"),
                with_mode(option("always", "allow_always", "mode_switch", "privileged"), "auto", "elevated"),
                option("no", "reject_once", "plan_revise", "approve")]});
            AskRequest::Classified(serde_json::from_value(plan).unwrap())
        }
        3 => form(None),
        _ => form(Some(json!([]))),
    }
}

fn outcome(m: u8) -> O {
    match m {
        0 => O::Dismissed,
        1..=3 | 6 => selected(["once", "always", "no", "", "", "", "mystery"][usize::from(m)]),
        4 => answer(json!({"question_0": "red"})),
        _ => O::Opened,
    }
}

struct Entry {
    requested: InteractionRequested,
    ask: u64,
    instance: u128,
    entered: MonoTime,
    ended: Option<InteractionResolved>,
}

struct World {
    b: CoreDriver<InteractionBroker>,
    turn: u128,
    instance: u128,
    asks: u64,
    closed: Option<(u128, ResolutionReason)>,
    entries: Vec<Entry>,
    /// Asks the agent heard an answer to, and whether it was `Cancelled`.
    heard: HashMap<u64, bool>,
}

impl World {
    fn apply(&mut self, op: Op) {
        let turn = self.turn;
        let in_turn = move |e: &Entry| e.requested.turn_id == Some(TurnId::from_uuid(turn));
        match op {
            Op::Ask(k) => {
                self.asks += 1;
                let (n, closed) = (self.asks, self.closed.clone().filter(|(t, _)| *t == turn));
                let cause = closed.clone().map(|(_, reason)| reason);
                let which = move |e: &Entry| closed.is_some() && e.ask == n;
                self.step(ask(n, self.instance, turn, request(k)), cause, None, which);
            }
            Op::Respond(..) if self.entries.is_empty() => {}
            Op::Respond(i, who, how, stale) => {
                let (i, all) = (usize::from(i), self.entries.len());
                let pending: Vec<_> = self.entries.iter().filter(|e| e.ended.is_none()).collect();
                let mut target = match (i % 4, pending.len()) {
                    (1.., 1..) => pending[i % pending.len()].requested.clone(),
                    _ => self.entries[i % all].requested.clone(),
                };
                if i % 16 == 0 {
                    target.request_id = RequestId::from_uuid(0);
                }
                if stale {
                    target.digest = format!("d_{}", "0".repeat(64)).parse().unwrap();
                }
                let keys = json!([null, {"kind": "click"}, {"kind": "key", "key": "enter"},
                    {"kind": "key", "key": "escape"}, {"kind": "key", "key": "digit", "digit": 1},
                    {"kind": "key", "key": "digit", "digit": 3}]);
                let key = Some(keys[usize::from(how / 7 % 6)].clone()).filter(|k| !k.is_null());
                let who = WHO.split(',').nth(usize::from(who % 5)).unwrap();
                let input = respond(who, &target, outcome(how % 7), key);
                let expected = self.oracle(&input);
                let (id, ok) = (target.request_id, expected.is_ok());
                let which = move |e: &Entry| ok && e.requested.request_id == id;
                self.step(input, None, Some(expected), which);
            }
            Op::Withdraw(n) => {
                let n = u64::from(n) % (self.asks + 1);
                let which = move |e: &Entry| e.ask == n;
                self.step(Input::Withdrawn(AskId(n)), Some(Withdrawn), None, which);
            }
            Op::Interrupt => self.close(None, TurnCancelled, in_turn),
            Op::Settle(cancelled, next) => {
                let asks = if cancelled { TurnCancelled } else { TurnEnded };
                self.close(Some(asks.clone()), asks, in_turn);
                self.turn += u128::from(next);
            }
            Op::Exit => {
                let instance = self.instance;
                let exited = Input::AdapterExited(AdapterInstanceId::from_uuid(instance));
                let which = move |e: &Entry| e.instance == instance;
                self.step(exited, Some(AdapterExited), None, which);
                self.close(Some(AdapterExited), AdapterExited, in_turn);
                (self.turn, self.instance) = (turn + 1, instance + 1);
            }
            Op::Advance(ms) => assert!(self.b.advance(Duration::from_millis(ms)).is_empty()),
        }
    }

    /// `TurnMachine`'s `Settle` with `asks`, or else its `CancelAsks`.
    fn close(
        &mut self,
        asks: Option<ResolutionReason>,
        why: ResolutionReason,
        which: impl Fn(&Entry) -> bool,
    ) {
        let turn_id = TurnId::from_uuid(self.turn);
        let input = match asks {
            Some(asks) => Input::Settle { turn_id, asks },
            None => Input::CancelAsks(turn_id),
        };
        self.closed = Some((self.turn, why.clone()));
        self.step(input, Some(why), None, which);
    }

    /// 5.2.7's checks, in order, by the model.
    fn oracle(&self, input: &Input) -> Result<(), Rejection> {
        let Input::Respond(respond) = input else {
            unreachable!()
        };
        let (who, params) = (&respond.principal, &respond.params);
        let entry = self
            .entries
            .iter()
            .find(|e| e.requested.request_id == params.request_id);
        let Some(entry) = entry else {
            return Err(Rejection::Unknown);
        };
        match &entry.ended {
            Some(ended) if ended.status == S::Expired => {
                return Err(Rejection::Expired(ended.reason.clone()));
            }
            Some(ended) => return Err(Rejection::AlreadyResolved(ended.resolved_by.clone())),
            None => {}
        }
        let requested = &entry.requested;
        if params.request_digest != requested.digest {
            return Err(Rejection::DigestMismatch);
        }
        if !requested.actionable_by.contains(&who.kind) {
            return Err(Rejection::NotActionable);
        }
        let (request, entered, tier) = (&requested.request, entry.entered, Some(&Tier::Normal));
        let pending = Pending {
            request,
            entered,
            tier,
        };
        policy::respond(who, &pending, params, self.b.now().mono)?;
        match (&requested.request, &params.outcome) {
            (_, O::Dismissed) | (R::Permission(_) | R::PlanConfirm(_), O::Selected(_)) => Ok(()),
            (R::Question(question), O::Answer(_)) if !question.unsupported_schema => Ok(()),
            _ => Err(AnswerError::Outcome.into()),
        }
    }

    /// Steps `input`, which ends the pending requests `which` picks and no
    /// others, for `cause` (`None`: answered), and replies `reply` if any.
    fn step(
        &mut self,
        input: Input,
        cause: Option<ResolutionReason>,
        reply: Option<Result<(), Rejection>>,
        which: impl Fn(&Entry) -> bool,
    ) {
        let now = self.b.now().mono;
        let mut answers = HashMap::new();
        for action in self.b.input(input) {
            match action {
                Action::Event(EventBody::InteractionRequested(requested)) => {
                    let turn = TurnId::from_uuid(self.turn);
                    let (id, request) = (&requested.request_id, &requested.request);
                    let digest = request_digest(&"s_1".parse().unwrap(), id, Some(&turn), request);
                    assert_eq!(requested.digest, digest.unwrap());
                    assert_eq!(requested.turn_id, Some(turn));
                    let new = self.entries.iter().all(|e| e.requested.request_id != *id);
                    // Only the privileged-only kind starts with "always".
                    let cli =
                        !matches!(request, R::Permission(p) if p.options[0].option_id == "always");
                    let who = ["desktop_ui", "cli", "test_harness"]
                        .into_iter()
                        .filter(|w| cli || *w != "cli");
                    let who: Vec<_> = who.map(Into::into).collect();
                    assert!(
                        new && requested.actionable_by == who,
                        "a new ID, actionable_by"
                    );
                    let (ask, instance, ended) = (self.asks, self.instance, None);
                    self.entries.push(Entry {
                        requested,
                        ask,
                        instance,
                        entered: now,
                        ended,
                    });
                }
                Action::Driver(DriverCommand::Answer { ask, answer }) => {
                    let cancelled = matches!(answer, AskAnswer::Cancelled);
                    let first = self.heard.insert(ask.0, cancelled).is_none();
                    assert!(
                        first && answers.insert(ask.0, cancelled).is_none(),
                        "one answer per ask"
                    );
                }
                Action::Event(EventBody::InteractionResolved(resolved)) => {
                    let id = &resolved.request_id;
                    let entry = self
                        .entries
                        .iter_mut()
                        .find(|e| e.requested.request_id == *id);
                    let entry = entry.unwrap();
                    let once = entry.ended.is_none() && which(entry);
                    assert!(once, "only what this input ends, once");
                    assert_eq!(resolved.reason, cause, "ended for what happened");
                    let status = match &resolved.reason {
                        None => S::Resolved,
                        Some(AdapterExited) => S::Expired,
                        Some(_) => S::Cancelled,
                    };
                    assert_eq!(resolved.status, status, "5.3.11's status for the reason");
                    assert_eq!(resolved.resolved_by.is_some(), status == S::Resolved);
                    // The agent hears an answer first, `Cancelled` when cancelled;
                    // a dead adapter instance hears nothing.
                    let heard = answers.remove(&entry.ask);
                    let expected = (status != S::Expired).then_some(status == S::Cancelled);
                    assert_eq!(heard, expected, "{resolved:?}");
                    entry.ended = Some(resolved);
                }
                Action::Reply(got) => {
                    let expected = reply.clone().expect("a reply to an answer only");
                    assert_eq!(got.map(|_| ()), expected, "the checks of 5.2.7, in order");
                }
                other => panic!("{other:?}"),
            }
        }
        assert!(answers.is_empty(), "answers only to requests that end");
        let left = self.entries.iter().any(|e| e.ended.is_none() && which(e));
        assert!(!left, "{cause:?} ends every request it is about");
    }
}

proptest! {
    #[test]
    fn prop_interaction_every_request_ends_once_and_answers_pass_the_checks(ops in ops()) {
        let mut world = World { b: broker(), turn: 1, instance: 1, asks: 0, closed: None,
            entries: Vec::new(), heard: HashMap::new() };
        for op in ops {
            world.apply(op);
        }
        world.apply(Op::Settle(false, false));
        prop_assert!(world.entries.iter().all(|e| e.ended.is_some()), "every request ends");
        for entry in &world.entries {
            let expired = entry.ended.as_ref().is_some_and(|e| e.status == S::Expired);
            prop_assert_eq!(world.heard.contains_key(&entry.ask), !expired);
        }
    }
}
