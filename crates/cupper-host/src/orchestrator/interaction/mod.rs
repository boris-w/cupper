//! `InteractionBroker` (design 5.2.7, 5.2.16): what the agent asks of the
//! user, from `interaction.requested` to `interaction.resolved`.
//!
//! - A request comes in with a host request ID, its digest, who may answer
//!   it (`actionable_by`) and the time the window's 400 ms count from. A form
//!   becomes a question ([`question`]).
//! - An answer passes 5.2.7's checks in order: the request is pending, the
//!   digest matches, the principal is in `actionable_by`, the policy's
//!   [`respond`](crate::policy::respond) (option, scope, tier, input evidence,
//!   400 ms), and the outcome answers this request. Only then does the agent
//!   hear it. The first answer wins: the actor steps one input at a time.
//! - Every request ends exactly once. Withdrawn by the agent (K5), or its
//!   turn interrupted or ended: `cancelled`, and the agent hears `Cancelled`.
//!   Its adapter instance exited or the host restarted: `expired` (SEC-10),
//!   and nothing goes to the dead instance. No request times out.
//! - A request belongs to a turn (design 5.3.11, T014's rulings). Once a turn
//!   is interrupted or settled, a request that still arrives for it ends at
//!   once, the same way.

pub mod question;

#[cfg(test)]
mod props;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;

use cupper_protocol::annotations::TimestampMs;
use cupper_protocol::domain::{PrincipalKind, ResolvedBy, Tier};
use cupper_protocol::error::{ErrorCode, ErrorReason, INVALID_PARAMS, RpcError};
use cupper_protocol::event::EventBody;
use cupper_protocol::ids::{AdapterInstanceId, ItemId, RequestId, SessionId, TurnId};
use cupper_protocol::interaction::{
    InteractionOption, InteractionOutcome, InteractionRequest, InteractionRequested,
    InteractionResolved, InteractionStatus, ResolutionReason, request_digest,
};
use cupper_protocol::methods::handshake::Principal;
use cupper_protocol::methods::interaction::{InteractionRespondParams, InteractionRespondResult};
use cupper_protocol::rpc::Scope;

use self::question::{AnswerError, Layout};
use crate::clock::{IdGen, MonoTime, Now};
use crate::driver::api::{AskAnswer, AskId, AskRequest, DriverCommand};
use crate::policy::{self, Denial};
use crate::pure::{Core, TimerRequest};

#[derive(Debug)]
pub struct InteractionBroker {
    session_id: SessionId,
    ids: Arc<IdGen>,
    /// In arrival order.
    pending: Vec<Pending>,
    ended: HashMap<RequestId, InteractionResolved>,
    /// The last turn whose requests were cancelled or settled, and why.
    closed: Option<(TurnId, ResolutionReason)>,
}

#[derive(Debug)]
struct Pending {
    requested: InteractionRequested,
    ask: AskId,
    instance: AdapterInstanceId,
    layout: Layout,
    entered: MonoTime,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    Ask(Box<Ask>),
    /// The agent took the request back (`$/cancel_request`, K5).
    Withdrawn(AskId),
    /// `interaction/respond`.
    Respond(Box<Respond>),
    /// `TurnMachine`'s `CancelAsks`: the user interrupted the turn.
    CancelAsks(TurnId),
    /// `TurnMachine`'s `Settle`: the turn's end is decided.
    Settle {
        turn_id: TurnId,
        asks: ResolutionReason,
    },
    AdapterExited(AdapterInstanceId),
    /// The session's requests that ended before this broker started, from
    /// the store: after a host restart they answer `expired` or
    /// `already_resolved`, not "unknown".
    Ended(Vec<InteractionResolved>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ask {
    pub ask: AskId,
    pub instance: AdapterInstanceId,
    /// The running turn, or the autonomous turn the actor opened for it.
    pub turn_id: TurnId,
    /// The tool call the request is about.
    pub item_id: Option<ItemId>,
    pub request: AskRequest,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Respond {
    pub principal: Principal,
    pub resolved_by: ResolvedBy,
    /// The session's tier now, for an option with `mode_after`; `None` while unknown.
    pub tier: Option<Tier>,
    pub params: InteractionRespondParams,
}

#[derive(Debug, Clone, PartialEq)]
#[expect(
    clippy::large_enum_variant,
    reason = "a step's few actions live only until the actor carries them out"
)]
pub enum Action {
    /// Append to the session stream.
    Event(EventBody),
    /// Always a `DriverCommand::Answer`.
    Driver(DriverCommand),
    /// The reply to [`Input::Respond`].
    Reply(Reply),
}

pub type Reply = Result<InteractionRespondResult, Rejection>;

/// Why an answer is refused, in 5.2.7's order of checks.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Rejection {
    #[error("no such request in the session")]
    Unknown,
    #[error("the request is no longer pending")]
    AlreadyResolved(Option<ResolvedBy>),
    #[error("the request expired")]
    Expired(Option<ResolutionReason>),
    #[error("the digest does not match the request")]
    DigestMismatch,
    #[error("this principal kind is not in actionable_by")]
    NotActionable,
    #[error(transparent)]
    Policy(#[from] Denial),
    #[error(transparent)]
    Answer(#[from] AnswerError),
}

impl From<Rejection> for RpcError {
    fn from(rejection: Rejection) -> Self {
        let message = rejection.to_string();
        let app = |code, reason| RpcError::app(code, &message).with_reason(reason);
        match rejection {
            Rejection::Unknown | Rejection::Answer(_) => RpcError::new(INVALID_PARAMS, message),
            Rejection::AlreadyResolved(by) => {
                app(ErrorCode::Conflict, ErrorReason::AlreadyResolved)
                    .with_data(|data| data.resolved_by = by)
            }
            Rejection::Expired(Some(ResolutionReason::AdapterExited)) => {
                app(ErrorCode::Expired, ErrorReason::AdapterExited)
            }
            Rejection::Expired(Some(ResolutionReason::HostRestarted)) => {
                app(ErrorCode::Expired, ErrorReason::HostRestarted)
            }
            Rejection::Expired(_) => RpcError::app(ErrorCode::Expired, message),
            Rejection::DigestMismatch => app(ErrorCode::Conflict, ErrorReason::DigestMismatch),
            Rejection::NotActionable => app(ErrorCode::Forbidden, ErrorReason::Role),
            Rejection::Policy(denial) => denial.into(),
        }
    }
}

impl InteractionBroker {
    pub fn new(session_id: SessionId, ids: Arc<IdGen>) -> Self {
        Self {
            session_id,
            ids,
            pending: Vec::new(),
            ended: HashMap::new(),
            closed: None,
        }
    }

    fn ask(&mut self, ask: Ask, now: Now, out: &mut Vec<Action>) {
        let (request, layout) = match ask.request {
            AskRequest::Classified(request) => (request, Layout::default()),
            AskRequest::Form(form) => {
                let (question, layout) = question::map(&form);
                (InteractionRequest::Question(question), layout)
            }
        };
        let id = RequestId::from_uuid(self.ids.new_id_at(now.wall_ms).as_u128());
        let Ok(digest) = request_digest(&self.session_id, &id, Some(&ask.turn_id), &request) else {
            // An integer beyond the protocol's bound: it cannot go on the wire.
            out.push(answer(ask.ask, AskAnswer::Cancelled));
            return;
        };
        let requested = InteractionRequested {
            request_id: id,
            turn_id: Some(ask.turn_id.clone()),
            item_id: ask.item_id,
            created_at: TimestampMs(now.wall_ms),
            digest,
            actionable_by: actionable_by(&request),
            request,
        };
        out.push(Action::Event(EventBody::InteractionRequested(
            requested.clone(),
        )));
        let (ask, instance, entered) = (ask.ask, ask.instance, now.mono);
        let pending = Pending {
            requested,
            ask,
            instance,
            layout,
            entered,
        };
        match self.closed.clone() {
            Some((turn, reason)) if pending.in_turn(&turn) => self.end(pending, &reason, out),
            _ => self.pending.push(pending),
        }
    }

    fn respond(&mut self, respond: Respond, now: Now, out: &mut Vec<Action>) -> Reply {
        let id = &respond.params.request_id;
        let Some(index) = self
            .pending
            .iter()
            .position(|p| p.requested.request_id == *id)
        else {
            return Err(match self.ended.get(id) {
                Some(ended) if ended.status == InteractionStatus::Expired => {
                    Rejection::Expired(ended.reason.clone())
                }
                Some(ended) => Rejection::AlreadyResolved(ended.resolved_by.clone()),
                None => Rejection::Unknown,
            });
        };
        let Pending {
            requested,
            layout,
            entered,
            ..
        } = &self.pending[index];
        if respond.params.request_digest != requested.digest {
            return Err(Rejection::DigestMismatch);
        }
        if !requested.actionable_by.contains(&respond.principal.kind) {
            return Err(Rejection::NotActionable);
        }
        let (request, tier) = (&requested.request, respond.tier.as_ref());
        let checked = policy::Pending {
            request,
            entered: *entered,
            tier,
        };
        policy::respond(&respond.principal, &checked, &respond.params, now.mono)?;
        let outcome = fit(request, layout, &respond.params.outcome)?;
        let pending = self.pending.remove(index);
        out.push(answer(pending.ask, AskAnswer::Outcome(outcome)));
        let request_id = pending.requested.request_id;
        let resolved = InteractionResolved {
            request_id: request_id.clone(),
            status: InteractionStatus::Resolved,
            outcome: Some(respond.params.outcome),
            resolved_by: Some(respond.resolved_by),
            reason: None,
        };
        self.record(resolved, out);
        Ok(InteractionRespondResult { request_id })
    }

    /// Ends the turn's pending requests for `reason`, and those still to come.
    fn close(&mut self, turn_id: TurnId, reason: ResolutionReason, out: &mut Vec<Action>) {
        self.end_all(|p| p.in_turn(&turn_id), &reason, out);
        self.closed = Some((turn_id, reason));
    }

    fn end_all(
        &mut self,
        which: impl Fn(&Pending) -> bool,
        reason: &ResolutionReason,
        out: &mut Vec<Action>,
    ) {
        let ending: Vec<Pending> = self.pending.extract_if(.., |p| which(p)).collect();
        for pending in ending {
            self.end(pending, reason, out);
        }
    }

    /// Ends `pending` without an answer: `expired` when its adapter instance
    /// is gone, else `cancelled`, and the agent hears `Cancelled`.
    fn end(&mut self, pending: Pending, reason: &ResolutionReason, out: &mut Vec<Action>) {
        use InteractionStatus::{Cancelled, Expired};
        use ResolutionReason::{AdapterExited, HostRestarted};
        let expired = matches!(reason, AdapterExited | HostRestarted);
        if !expired {
            out.push(answer(pending.ask, AskAnswer::Cancelled));
        }
        let status = if expired { Expired } else { Cancelled };
        let ended = InteractionResolved {
            request_id: pending.requested.request_id,
            status,
            outcome: None,
            resolved_by: None,
            reason: Some(reason.clone()),
        };
        self.record(ended, out);
    }

    fn record(&mut self, resolved: InteractionResolved, out: &mut Vec<Action>) {
        self.ended
            .insert(resolved.request_id.clone(), resolved.clone());
        out.push(Action::Event(EventBody::InteractionResolved(resolved)));
    }
}

impl Pending {
    fn in_turn(&self, turn_id: &TurnId) -> bool {
        self.requested.turn_id.as_ref() == Some(turn_id)
    }
}

impl Core for InteractionBroker {
    type Input = Input;
    type Action = Action;
    type TimerKey = Infallible;

    fn step(&mut self, input: Input, now: Now) -> Vec<Action> {
        use InteractionStatus::Pending;
        let mut out = Vec::new();
        match input {
            Input::Ask(ask) => self.ask(*ask, now, &mut out),
            Input::Withdrawn(ask) => {
                self.end_all(|p| p.ask == ask, &ResolutionReason::Withdrawn, &mut out)
            }
            Input::Respond(respond) => {
                let reply = self.respond(*respond, now, &mut out);
                out.push(Action::Reply(reply));
            }
            Input::CancelAsks(turn_id) => {
                self.close(turn_id, ResolutionReason::TurnCancelled, &mut out)
            }
            Input::Settle { turn_id, asks } => self.close(turn_id, asks, &mut out),
            Input::AdapterExited(instance) => {
                let exited = ResolutionReason::AdapterExited;
                self.end_all(|p| p.instance == instance, &exited, &mut out);
            }
            Input::Ended(requests) => {
                let ended = requests.into_iter().filter(|r| r.status != Pending);
                self.ended.extend(ended.map(|r| (r.request_id.clone(), r)));
            }
        }
        out
    }

    fn timer_request(_: &Action) -> Option<TimerRequest<Infallible>> {
        None
    }

    fn timer_expired(key: Infallible) -> Input {
        match key {}
    }
}

fn answer(ask: AskId, answer: AskAnswer) -> Action {
    Action::Driver(DriverCommand::Answer { ask, answer })
}

/// Who may answer `request` (design 5.3.6): the desktop and the test
/// harness, plus an elevated CLI unless every option takes `privileged`.
/// The test harness holds every scope (design 5.8.3); see T042's notes.
pub fn actionable_by(request: &InteractionRequest) -> Vec<PrincipalKind> {
    let options = match request {
        InteractionRequest::Permission(request) => request.options.as_slice(),
        InteractionRequest::PlanConfirm(request) => request.options.as_slice(),
        _ => &[],
    };
    use PrincipalKind::{Cli, DesktopUi, TestHarness};
    let privileged = |option: &InteractionOption| policy::requires(option) == Scope::Privileged;
    if !options.is_empty() && options.iter().all(privileged) {
        vec![DesktopUi, TestHarness]
    } else {
        vec![DesktopUi, Cli, TestHarness]
    }
}

/// What the agent hears for `outcome`, if it answers `request`: an option of
/// a permission or a plan, a question's fields, `opened` for a sign-in URL,
/// or `dismissed` for any.
fn fit(
    request: &InteractionRequest,
    layout: &Layout,
    outcome: &InteractionOutcome,
) -> Result<InteractionOutcome, AnswerError> {
    use InteractionOutcome as O;
    use InteractionRequest as R;
    match (request, outcome) {
        (_, O::Dismissed)
        | (R::Permission(_) | R::PlanConfirm(_), O::Selected(_))
        | (R::AuthUrl(_), O::Opened) => Ok(outcome.clone()),
        (R::Question(question), O::Answer(answer)) => {
            question::merge(question, layout, answer).map(O::Answer)
        }
        _ => Err(AnswerError::Outcome),
    }
}
