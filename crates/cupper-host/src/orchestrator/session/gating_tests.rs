//! IT-orch-gating: the session actor's tier checks, its gate,
//! `session/confirm_config`, crashes and late output, on IT-orch-lifecycle's
//! fixture with an in-process agent and `ManualClock`.

use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;
use std::time::Duration;

use cupper_protocol::domain::PrincipalKind;
use cupper_protocol::ids::{AdapterInstanceId, SessionId};
use cupper_protocol::item::ToolKind;
use cupper_protocol::rpc::Scope::{Approve, Read, Write};
use cupper_protocol::session::{ConfirmationReason, StopReason};
use serde_json::{Value, json};

use super::interaction_tests::{answer, answered, ask, asked, selected};
use super::tests::{Fixture, caps, create, data, done, harness, on, principal, send};
use crate::driver::api::{
    AgentError, AgentErrorClass, AgentErrorForm, AgentSessionId, AttachTarget, DriverCommand,
    DriverEvent, NormUpdate, PromptTag, ToolOutput, ToolStatus,
};
use crate::gateway::AuditEvent;
use crate::orchestrator::testkit::text;
use crate::policy::testkit::{option, with_mode};

/// The agent's options with the permission mode `mode`.
fn changed(mode: &str) -> DriverEvent {
    let mut options = caps().config;
    options[0].current = mode.into();
    DriverEvent::ConfigChanged(options)
}

/// A session `by` created, attached in `mode`.
async fn created_in(f: &mut Fixture, by: &str, mode: &str) -> SessionId {
    let by = match by {
        "cli" => principal("pr_cli", PrincipalKind::Cli, &[Read, Write]),
        _ => harness(),
    };
    let created = f.call(by, "session/create", create("c-1")).await.unwrap();
    let mut capabilities = caps();
    capabilities.config[0].current = mode.into();
    f.attach_with(capabilities).await;
    serde_json::from_value(created["session_id"].clone()).unwrap()
}

/// The last `session.configured`, once `n` were written.
async fn configured(f: &Fixture, session: &SessionId, n: usize) -> Value {
    let bodies = f.events(session, "session.configured", n).await;
    data(&bodies, "session.configured").pop().unwrap()
}

/// The codes of the notices written so far, once there are `n` items.
async fn notices(f: &Fixture, session: &SessionId, n: usize) -> Vec<String> {
    let items = data(&f.events(session, "item.started", n).await, "item.started");
    let notices = items.iter().filter(|i| i["kind"] == "notice");
    notices
        .map(|i| i["code"].as_str().unwrap().to_owned())
        .collect()
}

fn failed(class: AgentErrorClass) -> AgentError {
    let (form, message) = (AgentErrorForm::Text, "failed".into());
    AgentError {
        class,
        form,
        message,
    }
}

#[tokio::test]
async fn it_orch_gating_config_unconfirmed_holds_messages_until_confirmed() {
    let mut f = Fixture::new("unconfirmed").await;
    let session = created_in(&mut f, "harness", "bypass").await;
    let required = configured(&f, &session, 2).await["confirmation_required"].clone();
    assert_eq!(required["reason"], "full_access_unconfirmed");
    assert_eq!(notices(&f, &session, 2).await, ["config_unconfirmed"]);
    let record = f.audit.recv().await.unwrap();
    let AuditEvent::ConfigUnconfirmed { reason, mode, .. } = record.event else {
        panic!("{record:?}")
    };
    let expected = (
        None,
        ConfirmationReason::FullAccessUnconfirmed,
        Some("bypass".into()),
    );
    assert_eq!((record.conn, reason, mode), expected);

    // The first turn holds its prompt; once it is interrupted, a message sent
    // while idle is queued, and resuming the queue does not release it.
    f.ok("session/interrupt", on(&session, "i-1")).await;
    let sent = f.ok("session/send", send(&session, "s-1", "next")).await;
    assert_eq!(sent["disposition"], "queued");
    f.ok("session/queue_resume", on(&session, "r-1")).await;
    let queue = data(
        &f.events(&session, "queue.updated", 2).await,
        "queue.updated",
    );
    assert_eq!(
        queue.last().unwrap()["paused"]["reason"],
        "config_unconfirmed"
    );
    assert!(f.quiet(), "no prompt went out");

    // Confirmed with the digest of what was shown, the queue runs.
    let mut confirm = on(&session, "k-1");
    confirm["config_digest"] = json!(format!("d_{}", "0".repeat(64)));
    let stale = f.err("session/confirm_config", confirm.clone()).await;
    assert_eq!(stale, "conflict digest_mismatch");
    (confirm["command_id"], confirm["config_digest"]) =
        (json!("k-2"), required["config_digest"].clone());
    assert_eq!(f.ok("session/confirm_config", confirm).await, json!({}));
    assert_eq!(f.prompt().await.1, text("next"));
    assert!(configured(&f, &session, 3).await["confirmation_required"].is_null());
    let set = "SELECT mode_set_by || ' ' || (full_access_confirmed_at IS NOT NULL) FROM sessions";
    assert_eq!(f.rows(set), ["pr_test 1"]);

    // A writer may not leave a session in `elevated`.
    let mut f = Fixture::new("above").await;
    let session = created_in(&mut f, "cli", "auto").await;
    let required = configured(&f, &session, 2).await["confirmation_required"].clone();
    assert_eq!(required["reason"], "tier_above_setter");
    assert_eq!(f.rows("SELECT mode_set_by FROM sessions"), ["pr_cli"]);
}

#[tokio::test]
async fn it_orch_gating_a_raise_during_a_turn_is_set_back_or_interrupted() {
    for case in ["taken back", "deadline", "raised again"] {
        let mut f = Fixture::new("raise").await;
        let session = f.started().await;
        f.emit(changed("bypass"));
        let DriverCommand::SetConfig { option, value, .. } = f.command().await else {
            panic!("{case}: the mode is set back")
        };
        assert_eq!(
            (option.as_str(), value.as_str()),
            ("permission_mode", "default")
        );
        match case {
            "taken back" => f.emit(changed("default")),
            "deadline" => f.clock.advance(Duration::from_secs(2)),
            _ => f.emit(changed("bypass")),
        }
        let stop = if case == "taken back" {
            StopReason::EndTurn
        } else {
            let cancel = f.command().await;
            assert!(matches!(cancel, DriverCommand::Cancel { .. }), "{case}");
            StopReason::Cancelled
        };
        f.emit(done(PromptTag(1), stop));
        let bodies = f.events(&session, "turn.completed", 1).await;
        let status = ["completed", "cancelled"][usize::from(case != "taken back")];
        assert_eq!(
            data(&bodies, "turn.completed")[0]["status"],
            status,
            "{case}"
        );
        let configured = data(&bodies, "session.configured").pop().unwrap();
        let reason = &configured["confirmation_required"]["reason"];
        assert_eq!(reason, "full_access_unconfirmed", "{case}");
        let sent = f.ok("session/send", send(&session, "s-1", "next")).await;
        assert_eq!(sent["disposition"], "queued", "{case}");
    }
}

#[tokio::test]
async fn it_orch_gating_an_option_makes_its_chooser_the_setter() {
    let mut f = Fixture::new("chosen").await;
    let session = f.started().await;
    let auto = with_mode(
        option("auto", "allow_once", "once", "approve"),
        "auto",
        "elevated",
    );
    f.emit(ask(1, vec![auto]));
    let asked = asked(&f, &json!(session), 1).await;
    let cli = principal("pr_cli", PrincipalKind::Cli, &[Read, Write, Approve]);
    let respond = answer(&asked[0], selected("auto"), "r-1");
    f.call(cli, "interaction/respond", respond).await.unwrap();
    answered(&mut f).await;
    // The agent takes the mode: no revert, no gate.
    f.emit(changed("auto"));
    f.emit(done(PromptTag(1), StopReason::EndTurn));
    f.events(&session, "turn.completed", 1).await;
    let sent = f.ok("session/send", send(&session, "s-1", "next")).await;
    assert_eq!(sent["disposition"], "started");
    assert_eq!(f.prompt().await.1, text("next"));
    assert_eq!(f.rows("SELECT mode_set_by FROM sessions"), ["pr_cli"]);
}

#[tokio::test]
async fn it_orch_gating_crashes_restart_and_resume() {
    let mut f = Fixture::new("crash").await;
    let session = f.started().await;
    // The adapter exits: the turn fails; the next message resumes.
    let instance = AdapterInstanceId::from_uuid(1);
    let status = ExitStatus::from_raw(9);
    let stderr = String::new();
    f.emit(DriverEvent::AdapterExited {
        instance,
        status,
        stderr,
    });
    let bodies = f.events(&session, "turn.completed", 1).await;
    assert_eq!(
        data(&bodies, "turn.completed")[0]["error_code"],
        "adapter_exited"
    );
    f.ok("session/send", send(&session, "s-1", "two")).await;
    let resume = AttachTarget::Resume(AgentSessionId("agent-1".into()));
    assert_eq!(f.attach().await.target, resume);
    // Codex's app-server was killed (K7); the next prompt finds the session
    // gone and, with nothing written yet, is sent again once (K6).
    let (tag, _) = f.prompt().await;
    let error = failed(AgentErrorClass::Restartable);
    f.emit(DriverEvent::PromptFailed { tag, error });
    f.events(&session, "turn.completed", 2).await;
    f.ok("session/send", send(&session, "s-2", "three")).await;
    let (tag, _) = f.prompt().await;
    let error = failed(AgentErrorClass::SessionNotFound);
    f.emit(DriverEvent::PromptFailed { tag, error });
    assert_eq!(f.attach().await.target, resume);
    let (tag, content) = f.prompt().await;
    assert_eq!(content, text("three"));
    // After output, a lost session fails the turn instead.
    let (message_id, text_out) = (Some("m1".into()), "partial".into());
    let update = NormUpdate::AgentText {
        message_id,
        text: text_out,
    };
    f.emit(DriverEvent::Update(update));
    let error = failed(AgentErrorClass::SessionNotFound);
    f.emit(DriverEvent::PromptFailed { tag, error });
    let bodies = f.events(&session, "turn.completed", 3).await;
    let ends = data(&bodies, "turn.completed");
    let codes: Vec<&Value> = ends.iter().map(|t| &t["error_code"]).collect();
    assert_eq!(
        codes,
        ["adapter_exited", "restartable", "session_not_found"]
    );
    let notices = notices(&f, &session, 7).await;
    assert_eq!(
        notices,
        ["agent_restarted", "agent_restarted", "session_resumed"]
    );
    assert!(f.quiet(), "sent once more only");
}

#[tokio::test]
async fn it_orch_gating_late_tool_output_after_an_interrupt() {
    let mut f = Fixture::new("late").await;
    let created = f.ok("session/create", create("c-1")).await;
    let session: SessionId = serde_json::from_value(created["session_id"].clone()).unwrap();
    let mut capabilities = caps();
    capabilities.turn.interrupt_stops_commands = false;
    f.attach_with(capabilities).await;
    let (tag, _) = f.prompt().await;
    let (tool_call_id, title) = ("t1".to_owned(), "make".to_owned());
    let (paths, cwd, command) = (Vec::new(), None, Some("make".into()));
    let tool_kind = ToolKind::Execute;
    let start = NormUpdate::ToolStart {
        tool_call_id,
        tool_kind,
        title,
        paths,
        command,
        cwd,
    };
    f.emit(DriverEvent::Update(start));
    f.ok("session/interrupt", on(&session, "i-1")).await;
    assert!(matches!(f.command().await, DriverCommand::Cancel { .. }));
    f.emit(done(tag, StopReason::Cancelled));
    let bodies = f.events(&session, "turn.completed", 1).await;
    let abandoned = data(&bodies, "item.completed").pop().unwrap();
    assert_eq!(
        (&abandoned["status"], &abandoned["may_still_run"]),
        (&json!("abandoned"), &json!(true))
    );
    // The command goes on; its output and end come in a turn of their own.
    let late = NormUpdate::ToolUpdate {
        tool_call_id: "t1".into(),
        title: None,
        command: None,
        paths: None,
        status: Some(ToolStatus::Completed),
        output: Some(ToolOutput::Append("done\n".into())),
        exit_code: Some(0),
        diffs: None,
    };
    f.emit(DriverEvent::Update(late));
    let bodies = f.events(&session, "turn.started", 2).await;
    let turns = data(&bodies, "turn.started");
    assert_eq!(turns[1]["origin"], "autonomous");
    let items = data(&bodies, "item.started");
    let late = items
        .iter()
        .find(|i| i["kind"] == "late_tool_output")
        .expect("a late item");
    assert_eq!(late["turn_id"], turns[1]["turn_id"]);
    assert_eq!(late["ref"]["item_id"], abandoned["item_id"]);
}
