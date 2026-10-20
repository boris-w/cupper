//! IT-orch-interaction: the agent's requests through the session actor, and
//! `interaction/respond`, `interaction/get` and `reply_to` through the
//! gateway's dispatch, on IT-orch-lifecycle's fixture.

use std::time::Duration;

use cupper_protocol::domain::PrincipalKind;
use cupper_protocol::ids::RequestId;
use cupper_protocol::interaction::InteractionOption;
use cupper_protocol::item::ToolKind;
use cupper_protocol::rpc::Scope::{Approve, Read, UiLocal, Write};
use cupper_protocol::session::StopReason;
use serde_json::{Value, json};

use super::tests::{Fixture, code, data, done, harness, on, principal, send, tags};
use crate::driver::api::{
    AskAnswer, AskId, AskRequest, DriverCommand, DriverEvent, NormUpdate, PromptTag,
};
use crate::gateway::AuditEvent;
use crate::policy::testkit::{option, permission};

pub(super) fn ask(n: u64, options: Vec<InteractionOption>) -> DriverEvent {
    let request = AskRequest::Classified(permission(options));
    DriverEvent::Ask {
        ask: AskId(n),
        tool_call_id: None,
        request,
    }
}

pub(super) fn once() -> InteractionOption {
    option("once", "allow_once", "once", "approve")
}

fn always() -> InteractionOption {
    option("always", "allow_always", "always", "privileged")
}

/// `interaction/respond` params answering `asked` with `outcome`.
pub(super) fn answer(asked: &Value, outcome: Value, command: &str) -> Value {
    json!({"command_id": command, "session_id": asked["session"], "request_id":
        asked["request_id"], "request_digest": asked["digest"], "outcome": outcome})
}

pub(super) fn selected(option: &str) -> Value {
    json!({"type": "selected", "option_id": option})
}

/// The requests written so far, each with its session for [`answer`].
pub(super) async fn asked(f: &Fixture, session: &Value, n: usize) -> Vec<Value> {
    let session = serde_json::from_value(session.clone()).unwrap();
    let bodies = f.events(&session, "interaction.requested", n).await;
    let mut asked = data(&bodies, "interaction.requested");
    asked.iter_mut().for_each(|a| a["session"] = json!(session));
    asked
}

pub(super) async fn answered(f: &mut Fixture) -> (AskId, AskAnswer) {
    match f.command().await {
        DriverCommand::Answer { ask, answer } => (ask, answer),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn it_orch_interaction_answers_in_the_turn_and_the_first_wins() {
    let mut f = Fixture::new("answer").await;
    let session = json!(f.started().await);
    f.emit(ask(1, vec![once(), always()]));
    f.emit(ask(2, vec![always()]));
    let asked = asked(&f, &session, 2).await;
    let id = serde_json::from_value(session.clone()).unwrap();
    let turn = data(&f.events(&id, "turn.started", 1).await, "turn.started")[0]["turn_id"].clone();
    assert!(asked.iter().all(|a| a["turn_id"] == turn));
    assert_eq!(
        asked[1]["actionable_by"],
        json!(["desktop_ui", "test_harness"])
    );

    // 5.2.7's checks, each refusing.
    let cli = principal("pr_cli", PrincipalKind::Cli, &[Read, Write, Approve]);
    let window = principal(
        "pr_desktop",
        PrincipalKind::DesktopUi,
        &[Read, Write, Approve, UiLocal],
    );
    let mut wrong = answer(&asked[0], selected("once"), "r-1");
    wrong["request_digest"] = json!(format!("d_{}", "0".repeat(64)));
    let mut early = answer(&asked[0], selected("once"), "r-2");
    early["input"] = json!({"kind": "click"});
    let mut unknown = answer(&asked[0], selected("once"), "r-3");
    unknown["request_id"] = json!(RequestId::from_uuid(9));
    let refused = [
        (harness(), wrong, "conflict digest_mismatch"),
        (
            cli.clone(),
            answer(&asked[1], selected("always"), "r-4"),
            "forbidden role",
        ),
        (
            cli.clone(),
            answer(&asked[0], selected("always"), "r-5"),
            "forbidden scope",
        ),
        (window, early, "conflict too_early"),
        (harness(), unknown, "not_found "),
    ];
    for (n, (by, params, expected)) in refused.into_iter().enumerate() {
        let refused = f.call(by, "interaction/respond", params).await;
        assert_eq!(code(refused.unwrap_err()), expected, "{n}");
    }

    // The CLI's answer reaches the agent; a later one is too late.
    let params = answer(&asked[0], selected("once"), "r-6");
    let result = f.call(cli, "interaction/respond", params).await.unwrap();
    assert_eq!(result["request_id"], asked[0]["request_id"]);
    let (ask, answer_sent) = answered(&mut f).await;
    let AskAnswer::Outcome(outcome) = answer_sent else {
        panic!()
    };
    assert_eq!((ask, json!(outcome)), (AskId(1), selected("once")));
    let late = f.call(
        harness(),
        "interaction/respond",
        answer(&asked[0], selected("once"), "r-7"),
    );
    let late = late.await.unwrap_err();
    assert_eq!(late["data"]["resolved_by"]["principal_id"], "pr_cli");
    assert_eq!(code(late), "conflict already_resolved");
    let bodies = f.events(&id, "interaction.resolved", 1).await;
    let by = json!({"principal_id": "pr_cli", "client_kind": "cli", "client_name": "cli-peer"});
    assert_eq!(data(&bodies, "interaction.resolved")[0]["resolved_by"], by);

    // The audit, the index, the rows, and what the main process reads.
    let record = f.audit.recv().await.unwrap();
    let AuditEvent::InteractionResponded { principal, .. } = record.event else {
        panic!()
    };
    assert_eq!(principal.as_str(), "pr_cli");
    assert_eq!(*f.index.0.lock().unwrap(), [1, 2, 1]);
    let rows = "SELECT kind || ' ' || status || ' ' || (adapter_instance != '') FROM interactions \
        ORDER BY rowid";
    assert_eq!(
        f.rows(rows),
        ["permission resolved 1", "permission pending 1"]
    );
    let get = json!({"session_id": session, "request_id": asked[0]["request_id"]});
    assert_eq!(f.ok("interaction/get", get).await["status"], "resolved");
    let respond = answer(&asked[1], selected("always"), "r-8");
    let described = json!({"method": "interaction/respond", "params": respond});
    assert_eq!(
        f.ok("policy/describe", described).await["action"],
        "privileged_option"
    );
}

#[tokio::test]
async fn it_orch_interaction_outside_a_turn_opens_one_until_answered() {
    let mut f = Fixture::new("outside").await;
    let id = f.started().await;
    f.emit(done(PromptTag(1), StopReason::EndTurn));
    f.events(&id, "turn.completed", 1).await;
    f.emit(ask(1, vec![once()]));
    let asked = asked(&f, &json!(id), 1).await;
    let started = data(&f.events(&id, "turn.started", 2).await, "turn.started");
    assert_eq!(started[1]["origin"], "autonomous");
    assert_eq!(asked[0]["turn_id"], started[1]["turn_id"]);

    // Long quiet, but the request waits, and so does its turn: the answer
    // still finds it pending. The actor handles the timers before it reads
    // its mail (one thread, nothing to write).
    f.clock.advance(Duration::from_secs(30));
    tokio::task::yield_now().await;
    let dismissed = json!({"type": "dismissed"});
    f.ok("interaction/respond", answer(&asked[0], dismissed, "r-1"))
        .await;
    f.clock.advance(Duration::from_secs(10));
    let bodies = f.events(&id, "turn.completed", 2).await;
    let resolved = &data(&bodies, "interaction.resolved")[0];
    assert_eq!(resolved["status"], "resolved");
    let all = tags(&bodies);
    assert!(all.rfind("interaction.resolved") < all.rfind("turn.completed"));
}

/// A request about a tool call names the call's item; one about a call the
/// session has not seen names none.
#[tokio::test]
async fn it_orch_interaction_request_names_its_tool_calls_item() {
    let mut f = Fixture::new("item").await;
    let id = f.started().await;
    f.emit(DriverEvent::Update(NormUpdate::ToolStart {
        tool_call_id: "toolu_1".into(),
        tool_kind: ToolKind::Execute,
        title: "ls".into(),
        paths: Vec::new(),
        command: Some("ls".into()),
        cwd: None,
    }));
    for (n, call) in [(1, "toolu_1"), (2, "toolu_unknown")] {
        let DriverEvent::Ask { ask, request, .. } = ask(n, vec![once()]) else {
            unreachable!()
        };
        let tool_call_id = Some(call.to_owned());
        f.emit(DriverEvent::Ask {
            ask,
            tool_call_id,
            request,
        });
    }
    let asked = asked(&f, &json!(id), 2).await;
    let started = data(&f.events(&id, "item.started", 2).await, "item.started");
    let tool = started.iter().find(|item| item["kind"] == "tool_call");
    let tool = tool.expect("the tool call's item");
    assert_eq!(asked[0]["item_id"], tool["item_id"]);
    assert_eq!(asked[1].get("item_id"), None);
}

#[tokio::test]
async fn it_orch_interaction_interrupt_and_withdrawal_cancel_requests() {
    let mut f = Fixture::new("cancel").await;
    let id = f.started().await;
    f.emit(ask(1, vec![once()]));
    f.emit(ask(2, vec![once()]));
    asked(&f, &json!(id), 2).await;
    f.emit(DriverEvent::AskWithdrawn(AskId(2)));
    assert_eq!(answered(&mut f).await, (AskId(2), AskAnswer::Cancelled));
    // Codex does not withdraw on an interrupt: the host answers (K4).
    f.ok("session/interrupt", on(&id, "i-1")).await;
    assert!(matches!(f.command().await, DriverCommand::Cancel { .. }));
    assert_eq!(answered(&mut f).await, (AskId(1), AskAnswer::Cancelled));
    f.emit(done(PromptTag(1), StopReason::Cancelled));
    let bodies = f.events(&id, "turn.completed", 1).await;
    let resolved = data(&bodies, "interaction.resolved");
    let ends: Vec<_> = resolved
        .iter()
        .map(|r| (&r["status"], &r["reason"]))
        .collect();
    let cancelled = json!("cancelled");
    let (withdrawn, interrupted) = (json!("withdrawn"), json!("turn_cancelled"));
    assert_eq!(ends, [(&cancelled, &withdrawn), (&cancelled, &interrupted)]);
    let all = tags(&bodies);
    assert!(all.rfind("interaction.resolved") < all.find("turn.completed"));
}

#[tokio::test]
async fn it_orch_interaction_reply_to_names_an_answered_request() {
    let mut f = Fixture::new("reply").await;
    let id = f.started().await;
    f.emit(ask(1, vec![once()]));
    f.emit(ask(2, vec![once()]));
    let asked = asked(&f, &json!(id), 2).await;
    let dismissed = json!({"type": "dismissed"});
    f.ok("interaction/respond", answer(&asked[0], dismissed, "r-1"))
        .await;
    // A pending request and an unknown one are refused.
    for (n, reply_to) in [
        asked[1]["request_id"].clone(),
        json!(RequestId::from_uuid(9)),
    ]
    .into_iter()
    .enumerate()
    {
        let mut params = send(&id, &format!("s-{n}"), "no");
        params["reply_to"] = reply_to;
        let refused = f.call(harness(), "session/send", params).await.unwrap_err();
        assert_eq!(refused["code"], -32602, "{n}");
    }
    let mut params = send(&id, "s-ok", "rather this");
    params["reply_to"] = asked[0]["request_id"].clone();
    assert_eq!(f.ok("session/send", params).await["disposition"], "queued");
    let reply_to = asked[0]["request_id"].as_str().unwrap();
    assert_eq!(f.rows("SELECT reply_to FROM queue_entries"), [reply_to]);
    f.emit(done(PromptTag(1), StopReason::EndTurn));
    let bodies = f.events(&id, "turn.started", 2).await;
    let message = data(&bodies, "item.started").pop().unwrap();
    assert_eq!(message["reply_to"], reply_to);
}
