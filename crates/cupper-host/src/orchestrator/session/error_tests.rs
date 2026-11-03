//! IT-error-item: a failed turn's `error` item (MS-5), with the guidance its
//! code maps to, when the attach is refused (design 5.5.2), when the prompt
//! fails, and when the adapter exits, with the end of its stderr: one item a
//! turn, right before the turn's end, with the agent's words redacted.

use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;

use cupper_protocol::ids::{AdapterInstanceId, SessionId};
use serde_json::{Value, json};

use super::tests::{Fixture, HOME, create, data, send, tags};
use crate::driver::api::{AgentError, AgentErrorClass as C, AgentErrorForm, DriverEvent};

fn failed(class: C, form: AgentErrorForm, message: String) -> AgentError {
    AgentError {
        class,
        form,
        message,
    }
}

#[tokio::test]
async fn it_error_item_for_each_way_a_turn_fails() {
    let mut f = Fixture::new("error-item").await;
    let created = f.ok("session/create", create("c-1")).await;
    let session: SessionId = serde_json::from_value(created["session_id"].clone()).unwrap();
    let missing = format!("no CLI in {HOME}/bin");
    f.refuse(failed(C::CliMissing, AgentErrorForm::Text, missing))
        .await;
    f.events(&session, "turn.completed", 1).await;
    let got = f.ok("session/get", json!({"session_id": session})).await;
    let summary = [&got["summary"]["status"], &got["summary"]["error_code"]];
    assert_eq!(summary, ["error", "cli_missing"]);

    f.ok("session/send", send(&session, "s-1", "two")).await;
    f.attach().await;
    let (tag, _) = f.prompt().await;
    let rpc = AgentErrorForm::Rpc { agent_code: -32000 };
    let error = failed(C::NotLoggedIn, rpc, format!("Not logged in as {HOME}"));
    f.emit(DriverEvent::PromptFailed { tag, error });
    f.events(&session, "turn.completed", 2).await;
    let exited = |stderr: &str| DriverEvent::AdapterExited {
        instance: AdapterInstanceId::from_uuid(1),
        status: ExitStatus::from_raw(256),
        stderr: stderr.into(),
        reclaimed: false,
    };
    // The adapter exits under the third turn; under the fourth it exits
    // after the prompt failed, which wrote the turn's item already.
    f.ok("session/send", send(&session, "s-2", "three")).await;
    f.prompt().await;
    f.emit(exited("starting\nboom\n"));
    f.events(&session, "turn.completed", 3).await;
    f.ok("session/send", send(&session, "s-3", "four")).await;
    f.attach().await;
    let (tag, _) = f.prompt().await;
    let error = failed(C::Other, AgentErrorForm::Text, "oops".into());
    f.emit(DriverEvent::PromptFailed { tag, error });
    f.emit(exited("gone\n"));

    // Four messages, four errors, two `agent_restarted` notices.
    let bodies = f.events(&session, "item.started", 10).await;
    let items = data(&bodies, "item.started");
    let errors: Vec<&Value> = items
        .iter()
        .filter(|item| item["kind"] == "error")
        .collect();
    let fields = ["code", "agent_code", "guidance", "retryable", "raw"];
    let shown = errors
        .iter()
        .map(|e| fields.map(|k| e[k].to_string()).join(" "));
    let expected = [
        r#""cli_missing" null "install_cli" false "no CLI in ~/bin""#,
        r#""not_logged_in" "-32000" "run_login_in_terminal" false "Not logged in as ~""#,
        r#""adapter_exited" null "retry" true "starting\nboom""#,
        r#""other" null "none" false "oops""#,
    ];
    assert_eq!(shown.collect::<Vec<_>>(), expected);
    let turns = errors.iter().map(|e| &e["turn_id"]);
    let ended = data(&bodies, "turn.completed");
    assert!(
        turns.eq(ended.iter().map(|t| &t["turn_id"])),
        "each in its turn"
    );
    // Only the end snapshot comes between an error item and its turn's end.
    let all = tags(&bodies);
    for turn in all.split_inclusive("turn.completed").take(4) {
        let end = turn.trim_end_matches(" turn.completed");
        let end = end.trim_end_matches(" snapshot.recorded");
        assert!(end.ends_with("item.started item.completed"), "{turn}");
    }
}
