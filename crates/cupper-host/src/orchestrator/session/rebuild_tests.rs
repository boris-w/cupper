//! IT-orch-rebuild: what a session load builds again after the host
//! stopped (design 5.2.8, 5.2.13), on IT-orch-lifecycle's fixture: the
//! state only the view had, a first message that never started its turn,
//! and the items the running turn left open.

use cupper_protocol::ids::SessionId;
use cupper_protocol::item::ToolKind;
use cupper_protocol::session::StopReason;
use serde_json::json;

use super::tests::{Fixture, caps, create, data, done, on, send, tags};
use crate::driver::api::{
    AgentSessionId, AttachTarget, DriverCommand, DriverEvent, NormUpdate, PromptTag,
};
use crate::orchestrator::testkit::text;

#[tokio::test]
async fn it_orch_rebuild_a_discarded_checkpoint_gets_what_only_the_view_had() {
    let mut f = Fixture::new("discarded").await;
    let id = f.started().await;
    f.ok("session/send", send(&id, "s-1", "next")).await;
    f.ok("session/interrupt", on(&id, "i-1")).await;
    assert!(matches!(f.command().await, DriverCommand::Cancel { .. }));
    f.emit(done(PromptTag(1), StopReason::Cancelled));
    let before = f.events(&id, "turn.completed", 1).await;
    let queue = data(&before, "queue.updated").pop().unwrap();
    // A view of another version, and a log pruned up to the checkpoint.
    f.exec("UPDATE checkpoints SET view_v = view_v + ?1", "1".into());
    let pruned = "DELETE FROM events WHERE stream = ?1
        AND seq <= (SELECT at_seq FROM checkpoints WHERE stream = ?1)";
    f.exec(pruned, format!("session/{id}"));
    f.restart().await;

    let session = json!({"session_id": id});
    assert_eq!(f.ok("session/capabilities", session).await, json!(caps()));
    let bodies = f.events(&id, "queue.updated", 1).await;
    let expected = "session.configured session.capabilities queue.updated";
    assert_eq!(tags(&bodies), expected);
    assert_eq!(data(&bodies, "queue.updated")[0], queue);
    let configured = &data(&bodies, "session.configured")[0];
    assert_eq!(configured["project_config"], "loaded");
    assert_eq!(configured["workspace"]["path"], json!(f.project()));
    // The queue goes on from there.
    f.ok("session/queue_resume", on(&id, "r-1")).await;
    let resume = AttachTarget::Resume(AgentSessionId("agent-1".into()));
    assert_eq!(f.attach().await.target, resume);
    assert_eq!(f.prompt().await.1, text("next"));
}

#[tokio::test]
async fn it_orch_rebuild_a_first_message_that_never_started_runs_after_a_restart() {
    let mut f = Fixture::new("first").await;
    let id = f.started().await;
    // What the store holds when the host stops before the first turn.
    f.exec("DELETE FROM events WHERE seq > ?1", "2".into());
    f.exec("DELETE FROM turns WHERE turn_no >= ?1", "1".into());
    let first = "UPDATE sessions SET initial_prompt_json = ?1, agent_session_id = NULL";
    f.exec(first, json!(text("hi")).to_string());
    f.restart().await;

    // The host loads the session at once, and the message starts turn 1.
    assert_eq!(f.attach().await.target, AttachTarget::New);
    assert_eq!(f.prompt().await.1, text("hi"));
    let bodies = f.events(&id, "turn.started", 1).await;
    assert_eq!(data(&bodies, "turn.started")[0]["turn_no"], 1);
    let taken = "SELECT (initial_prompt_json IS NULL) || '' FROM sessions";
    assert_eq!(f.rows(taken), ["1"]);
}

#[tokio::test]
async fn it_orch_rebuild_a_first_message_the_check_queued_runs_once() {
    let mut f = Fixture::new("queued-first").await;
    // Without its workspace the check fails: the message waits in the queue.
    std::fs::remove_dir(f.project()).unwrap();
    let created = f.ok("session/create", create("c-1")).await;
    let id: SessionId = serde_json::from_value(created["session_id"].clone()).unwrap();
    f.events(&id, "queue.updated", 2).await;
    let taken = "SELECT (initial_prompt_json IS NULL) || '' FROM sessions";
    assert_eq!(f.rows(taken), ["1"]);
    f.restart().await;

    f.ok("session/get", json!({"session_id": id})).await;
    let entries = "SELECT count(*) || '' FROM queue_entries";
    assert_eq!(f.rows(entries), ["1"]);
    let bodies = f.events(&id, "queue.updated", 2).await;
    assert!(!tags(&bodies).contains("turn.started"));
    assert!(f.quiet(), "nothing went to the agent");
}

#[tokio::test]
async fn it_orch_rebuild_the_interrupted_turn_ends_its_open_items_first() {
    let mut f = Fixture::new("open-items").await;
    let created = f.ok("session/create", create("c-1")).await;
    let id: SessionId = serde_json::from_value(created["session_id"].clone()).unwrap();
    let mut capabilities = caps();
    capabilities.turn.interrupt_stops_commands = false;
    f.attach_with(capabilities).await;
    f.prompt().await;
    let start = NormUpdate::ToolStart {
        tool_call_id: "t1".into(),
        tool_kind: ToolKind::Execute,
        title: "make".into(),
        paths: Vec::new(),
        command: Some("make".into()),
        cwd: None,
    };
    f.emit(DriverEvent::Update(start));
    let (message_id, text) = (Some("m1".into()), "building".into());
    f.emit(DriverEvent::Update(NormUpdate::AgentText {
        message_id,
        text,
    }));
    f.events(&id, "item.started", 3).await;
    f.restart().await;

    f.ok("session/get", json!({"session_id": id})).await;
    let bodies = f.events(&id, "turn.completed", 1).await;
    assert!(tags(&bodies).ends_with("item.completed item.completed turn.completed"));
    let ended: Vec<_> = (data(&bodies, "item.completed").iter().rev().take(2))
        .map(|i| (i["status"].clone(), i["may_still_run"].clone()))
        .collect();
    let (abandoned, completed) = (json!("abandoned"), json!("completed"));
    assert_eq!(ended, [(completed, json!(null)), (abandoned, json!(true))]);
}
