//! Fixed cases of PROP-interaction: an answer, each way a request ends, and
//! the wire codes of refusals. Helpers shared with `props`.

use std::sync::Arc;
use std::time::Duration;

use cupper_protocol::domain::{ResolvedBy, Tier};
use cupper_protocol::error::RpcError;
use cupper_protocol::event::EventBody;
use cupper_protocol::ids::{AdapterInstanceId, RequestId, TurnId};
use cupper_protocol::interaction::{
    InteractionOutcome, InteractionRequested, InteractionResolved, ResolutionReason, request_digest,
};

use serde_json::{Value, json};

use super::question::AnswerError;
use super::{Action, Ask, Input, InteractionBroker, Rejection, Respond};
use crate::clock::{IdGen, ManualClock};
use crate::driver::api::{AskAnswer, AskId, AskRequest, DriverCommand, FormAsk};
use crate::policy::Denial;
use crate::policy::testkit::{column, option, permission, respond_params, selected};
use crate::pure::CoreDriver;

pub fn broker() -> CoreDriver<InteractionBroker> {
    let clock = ManualClock::new();
    let ids = Arc::new(IdGen::seeded(Arc::new(clock.clone()), 1));
    let session = "s_1".parse().unwrap();
    CoreDriver::with_clock(InteractionBroker::new(session, ids), clock)
}

pub fn ask(ask: u64, instance: u128, turn: u128, request: AskRequest) -> Input {
    Input::Ask(Box::new(Ask {
        ask: AskId(ask),
        instance: AdapterInstanceId::from_uuid(instance),
        turn_id: TurnId::from_uuid(turn),
        item_id: None,
        request,
    }))
}

/// Allow once, allow always (privileged), reject.
pub fn approval() -> AskRequest {
    AskRequest::Classified(permission(vec![
        option("once", "allow_once", "once", "approve"),
        option("always", "allow_always", "persistent", "privileged"),
        option("no", "reject_once", "reject", "approve"),
    ]))
}

/// A form of `schema`; without one, Claude's one question with its custom box.
pub fn form(schema: Option<Value>) -> AskRequest {
    let choice = json!({"type": "string", "oneOf": [{"const": "red"}, {"const": "blue"}]});
    let claude = json!({"type": "object", "properties": {"question_0": choice,
        "question_0_custom": {"type": "string"}}});
    let (message, schema) = ("Asked".into(), schema.unwrap_or(claude));
    AskRequest::Form(FormAsk { message, schema })
}

pub fn answer(values: Value) -> InteractionOutcome {
    serde_json::from_value(json!({"type": "answer", "values": values})).unwrap()
}

pub fn by(principal: &str) -> ResolvedBy {
    let by =
        json!({"principal_id": column(principal).id, "client_kind": "test", "client_name": "t"});
    serde_json::from_value(by).unwrap()
}

/// `principal`'s answer to `requested`, with the window's `input`.
pub fn respond(
    principal: &str,
    requested: &InteractionRequested,
    outcome: InteractionOutcome,
    input: Option<Value>,
) -> Input {
    let mut params = respond_params(outcome, input);
    params.request_id = requested.request_id.clone();
    params.request_digest = requested.digest.clone();
    let (resolved_by, tier) = (by(principal), Some(Tier::Normal));
    let principal = column(principal);
    Input::Respond(Box::new(Respond {
        principal,
        resolved_by,
        tier,
        params,
    }))
}

pub fn requested(actions: &[Action]) -> InteractionRequested {
    let requested = actions.iter().find_map(|action| match action {
        Action::Event(EventBody::InteractionRequested(requested)) => Some(requested.clone()),
        _ => None,
    });
    requested.expect("a request")
}

/// One short name per action.
fn kinds(actions: &[Action]) -> Vec<String> {
    let kind = |action: &Action| match action {
        Action::Event(EventBody::InteractionResolved(r)) => {
            let reason = r.reason.as_ref().map_or("by", |reason| reason.as_str());
            format!("{} {reason}", r.status)
        }
        Action::Event(body) => body.tag().to_owned(),
        Action::Driver(DriverCommand::Answer { ask, answer }) => match answer {
            AskAnswer::Cancelled => format!("answer {} cancelled", ask.0),
            AskAnswer::Outcome(outcome) => format!("answer {} {}", ask.0, outcome.tag()),
        },
        Action::Driver(_) => "driver".into(),
        Action::Reply(Ok(_)) => "ok".into(),
        Action::Reply(Err(rejection)) => format!("{rejection:?}"),
    };
    actions.iter().map(kind).collect()
}

fn reply(actions: &[Action]) -> Result<(), Rejection> {
    match actions.last() {
        Some(Action::Reply(reply)) => reply.clone().map(|_| ()),
        _ => panic!("no reply: {actions:?}"),
    }
}

#[test]
fn prop_interaction_case_an_answer_resolves_the_request_once() {
    let mut b = broker();
    let asked = b.input(ask(7, 1, 1, approval()));
    assert_eq!(kinds(&asked), ["interaction.requested"]);
    let request = requested(&asked);
    let (session, turn) = ("s_1".parse().unwrap(), TurnId::from_uuid(1));
    let digest = request_digest(&session, &request.request_id, Some(&turn), &request.request);
    assert_eq!(
        (&request.turn_id, &request.digest),
        (&Some(turn), &digest.unwrap())
    );
    let all = ["desktop_ui", "cli", "test_harness"].map(Into::into);
    assert_eq!(request.actionable_by, all);

    // The window waits 400 ms from the request, on the host's clock.
    let click = || Some(json!({"kind": "click"}));
    let early = b.input(respond("UI window", &request, selected("once"), click()));
    assert_eq!(reply(&early), Err(Denial::TooEarly.into()));
    b.advance(Duration::from_millis(400));
    let answered = b.input(respond("UI window", &request, selected("once"), click()));
    assert_eq!(kinds(&answered), ["answer 7 selected", "resolved by", "ok"]);
    let Action::Event(EventBody::InteractionResolved(resolved)) = &answered[1] else {
        panic!()
    };
    let (outcome, by_window) = (Some(selected("once")), Some(by("UI window")));
    assert_eq!(
        (&resolved.outcome, &resolved.resolved_by),
        (&outcome, &by_window)
    );
    // The first answer wins; the agent cannot take it back any more.
    let again = b.input(respond("UI control", &request, selected("no"), None));
    assert_eq!(reply(&again), Err(Rejection::AlreadyResolved(by_window)));
    assert_eq!(b.input(Input::Withdrawn(AskId(7))), []);
}

#[test]
fn prop_interaction_case_privileged_only_requests_leave_out_the_cli() {
    let mut b = broker();
    let privileged = AskRequest::Classified(permission(vec![
        option("always", "allow_always", "once", "approve"),
        option("mystery", "allow_maybe", "unknown", "privileged"),
    ]));
    let request = requested(&b.input(ask(1, 1, 1, privileged)));
    assert_eq!(
        request.actionable_by,
        ["desktop_ui", "test_harness"].map(Into::into)
    );
    let dismiss = respond(
        "cli + approve",
        &request,
        InteractionOutcome::Dismissed,
        None,
    );
    assert_eq!(reply(&b.input(dismiss)), Err(Rejection::NotActionable));
    let chosen = b.input(respond("test_harness", &request, selected("mystery"), None));
    assert_eq!(kinds(&chosen), ["answer 1 selected", "resolved by", "ok"]);
}

#[test]
fn prop_interaction_case_forms_are_answered_in_the_agents_fields() {
    let mut b = broker();
    let request = requested(&b.input(ask(1, 1, 1, form(None))));
    let green = answer(json!({"question_0": "green"}));
    let answered = b.input(respond("test_harness", &request, green.clone(), None));
    let content = AskAnswer::Outcome(answer(json!({"question_0_custom": "green"})));
    let to_agent = DriverCommand::Answer {
        ask: AskId(1),
        answer: content,
    };
    assert_eq!(answered[0], Action::Driver(to_agent));
    let Action::Event(EventBody::InteractionResolved(resolved)) = &answered[1] else {
        panic!()
    };
    assert_eq!(resolved.outcome, Some(green));

    let unsupported = form(Some(json!([])));
    let request = requested(&b.input(ask(2, 1, 1, unsupported)));
    let refused = b.input(respond("test_harness", &request, answer(json!({})), None));
    assert_eq!(reply(&refused), Err(AnswerError::Outcome.into()));
    let skip = respond(
        "test_harness",
        &request,
        InteractionOutcome::Dismissed,
        None,
    );
    assert_eq!(
        kinds(&b.input(skip)),
        ["answer 2 dismissed", "resolved by", "ok"]
    );
}

#[test]
fn prop_interaction_case_requests_end_with_their_turn_agent_or_host() {
    use ResolutionReason::{AdapterExited, HostRestarted, TurnCancelled, TurnEnded};
    let mut b = broker();
    let first = requested(&b.input(ask(1, 1, 1, approval())));
    b.input(ask(2, 1, 1, form(None)));
    let withdrawn = kinds(&b.input(Input::Withdrawn(AskId(1))));
    assert_eq!(withdrawn, ["answer 1 cancelled", "cancelled withdrawn"]);
    let interrupted = kinds(&b.input(Input::CancelAsks(TurnId::from_uuid(1))));
    assert_eq!(
        interrupted,
        ["answer 2 cancelled", "cancelled turn_cancelled"]
    );
    // While the turn is being cancelled, a new request is cancelled at once.
    let late = kinds(&b.input(ask(3, 1, 1, approval())));
    assert_eq!(
        late[1..],
        ["answer 3 cancelled", "cancelled turn_cancelled"]
    );
    let settle = |turn, asks| Input::Settle {
        turn_id: TurnId::from_uuid(turn),
        asks,
    };
    assert_eq!(b.input(settle(1, TurnCancelled)), []);

    b.input(ask(4, 1, 2, approval()));
    let ended = kinds(&b.input(settle(2, TurnEnded)));
    assert_eq!(ended, ["answer 4 cancelled", "cancelled turn_ended"]);
    let after = kinds(&b.input(ask(5, 1, 2, approval())));
    assert_eq!(after[1..], ["answer 5 cancelled", "cancelled turn_ended"]);

    // Nothing goes to an adapter instance that exited.
    let gone = requested(&b.input(ask(6, 1, 3, approval())));
    b.input(ask(7, 2, 3, approval()));
    let exited = b.input(Input::AdapterExited(AdapterInstanceId::from_uuid(1)));
    assert_eq!(kinds(&exited), ["expired adapter_exited"]);
    assert_eq!(
        kinds(&b.input(settle(3, AdapterExited))),
        ["expired adapter_exited"]
    );
    let mut answer = |n: Option<u128>| {
        let mut request = if n.is_some() {
            gone.clone()
        } else {
            first.clone()
        };
        request.request_id = n.map_or(request.request_id, RequestId::from_uuid);
        reply(&b.input(respond("UI control", &request, selected("once"), None)))
    };
    let expired = |reason| Err(Rejection::Expired(Some(reason)));
    assert_eq!(answer(None), Err(Rejection::AlreadyResolved(None)));
    assert_eq!(answer(Some(0)), Err(Rejection::Unknown));

    // After a restart the store tells what ended before.
    let ended = |n, status: &str, resolved_by: Option<&str>, reason: Option<&str>| {
        let (by, reason) = (resolved_by.map(by), reason.map(ResolutionReason::from));
        let id = RequestId::from_uuid(n);
        let (outcome, status) = (None, status.into());
        InteractionResolved {
            request_id: id,
            status,
            outcome,
            resolved_by: by,
            reason,
        }
    };
    let mut b = broker();
    b.input(Input::Ended(vec![
        ended(1, "expired", None, Some("host_restarted")),
        ended(2, "resolved", Some("cli + approve"), None),
        ended(3, "pending", None, None),
    ]));
    let mut answer = |n| {
        let mut request = gone.clone();
        request.request_id = RequestId::from_uuid(n);
        reply(&b.input(respond("UI control", &request, selected("once"), None)))
    };
    assert_eq!(answer(1), expired(HostRestarted));
    assert_eq!(
        answer(2),
        Err(Rejection::AlreadyResolved(Some(by("cli + approve"))))
    );
    assert_eq!(answer(3), Err(Rejection::Unknown));
}

#[test]
fn prop_interaction_case_rejections_are_semantic_codes() {
    use ResolutionReason::{AdapterExited, HostRestarted, Withdrawn};
    let wire = |rejection: Rejection| {
        let error = serde_json::to_value(RpcError::from(rejection)).unwrap();
        let data = &error["data"];
        format!("{} {} {}", error["code"], data["code"], data["reason"]).replace('"', "")
    };
    let resolved = RpcError::from(Rejection::AlreadyResolved(Some(by("UI window"))));
    assert_eq!(resolved.data.unwrap().resolved_by, Some(by("UI window")));
    for (rejection, expected) in [
        (
            Rejection::AlreadyResolved(None),
            "-32000 conflict already_resolved",
        ),
        (
            Rejection::Expired(Some(AdapterExited)),
            "-32000 expired adapter_exited",
        ),
        (
            Rejection::Expired(Some(HostRestarted)),
            "-32000 expired host_restarted",
        ),
        (Rejection::Expired(Some(Withdrawn)), "-32000 expired null"),
        (Rejection::DigestMismatch, "-32000 conflict digest_mismatch"),
        (Rejection::NotActionable, "-32000 forbidden role"),
        (Denial::TooEarly.into(), "-32000 conflict too_early"),
        (Rejection::Unknown, "-32602 null null"),
        (AnswerError::Outcome.into(), "-32602 null null"),
    ] {
        assert_eq!(wire(rejection.clone()), expected, "{rejection:?}");
    }
}
