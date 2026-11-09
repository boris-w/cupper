//! The driver's known problems (design 5.6.9, the K list) on a real host
//! process with the fake agent, Claude's or Codex's: each case runs what the
//! agent does and asserts what the host made of it, by the events it stored,
//! the frames it sent and the calls the fake agent recorded (`--calls`).
//! IT-degrade-plan and IT-degrade-thinking run it `--without` either.

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::*;
use cupper_cli::client::ClientError;
use cupper_cli::client::{self, Client};
use cupper_host::driver::runtime::launch;
use cupper_host::gateway::testkit::TempDir;
use cupper_protocol::error::{APP_ERROR, INVALID_PARAMS};
use cupper_protocol::ids::{ProjectId, SessionId};
use cupper_protocol::methods::session::{
    SessionCapabilities, SessionCapabilitiesParams, SessionCreate, SessionInterrupt, SessionSend,
    SessionSetConfig,
};
use serde_json::{Value, json};

/// A host with Claude's fake agent run with `args` and recording its calls,
/// more of `test_config` in `config`; the data directory, the host, the
/// client and the calls file.
async fn host(dir: &TempDir, args: &[&str], config: Value) -> (PathBuf, Running, Client, PathBuf) {
    host_of("claude", dir, args, config).await
}

/// [`host`] with the fake agent of `agent`'s persona as that agent.
async fn host_of(
    agent: &str,
    dir: &TempDir,
    args: &[&str],
    config: Value,
) -> (PathBuf, Running, Client, PathBuf) {
    let calls = dir.path().join("calls.jsonl");
    let persona = ["--persona", agent];
    let mut args: Vec<Value> = persona.iter().chain(args).map(|arg| json!(arg)).collect();
    args.extend([json!("--calls"), json!(calls)]);
    let mut command = json!({"program": launch::fake_agent(), "args": args});
    if let Some(version) = config.get("cli_version") {
        command["cli_version"] = version.clone();
    }
    let mut config = config;
    config.as_object_mut().unwrap().remove("cli_version");
    config["agents"] = json!({agent: command});
    let (data_dir, host, client) = fake_agent_host(dir, config).await;
    (data_dir, host, client, calls)
}

/// A Codex session whose first message is `text`.
async fn codex_session(client: &Client, text: &str) -> SessionId {
    let params = json!({"command_id": client::command_id(), "agent": "codex",
        "project_id": ProjectId::from_uuid(1), "workspace": {"kind": "local"},
        "message": [{"type": "text", "text": text}]});
    client
        .request::<SessionCreate>(typed(params))
        .await
        .unwrap()
        .session_id
}

/// The items of `kind` the session started so far, once there are `n`.
async fn items(data_dir: &Path, session: &SessionId, kind: &str, n: usize) -> Vec<Value> {
    let sql = format!(
        "SELECT json_extract(json, '$.data') FROM events WHERE stream = 'session/{session}'
         AND type = 'item.started' AND json_extract(json, '$.data.kind') = '{kind}' ORDER BY seq"
    );
    let rows = || query(&data_dir.join("host.db"), &sql);
    let count = format!("SELECT CAST((SELECT count(*) FROM ({sql})) >= {n} AS TEXT)");
    until(data_dir, "host.db", &count, &["1"]).await;
    rows()
        .iter()
        .map(|row| serde_json::from_str(row).unwrap())
        .collect()
}

/// The calls the fake agent recorded: `(method, text of a prompt)`.
fn calls(path: &Path) -> Vec<(String, String)> {
    let text = fs::read_to_string(path).unwrap_or_default();
    let line = |line: &str| {
        let call: Value = serde_json::from_str(line).unwrap();
        let method = call["method"].as_str().unwrap_or_default().to_owned();
        (method, call["text"].as_str().unwrap_or_default().to_owned())
    };
    text.lines().map(line).collect()
}

/// How often `method` came with prompt text `text`.
fn sent(path: &Path, method: &str, text: &str) -> usize {
    let calls = calls(path);
    calls
        .iter()
        .filter(|(m, t)| m == method && t == text)
        .count()
}

/// Waits until the agent said `needle` in the session.
async fn said(data_dir: &Path, session: &SessionId, needle: &str) {
    let sql = format!(
        "SELECT count(*) > 0 FROM events WHERE stream = 'session/{session}'
         AND type = 'item.delta' AND json_extract(json, '$.data.text') LIKE '%{needle}%'"
    );
    until(
        data_dir,
        "host.db",
        &format!("SELECT CAST(({sql}) AS TEXT)"),
        &["1"],
    )
    .await;
}

async fn interrupt(client: &Client, session: &SessionId) {
    let params = json!({"command_id": client::command_id(), "session_id": session});
    client
        .request::<SessionInterrupt>(typed(params))
        .await
        .unwrap();
}

/// `session/send` of `text` as a steer of `turn`; the result.
async fn steer(client: &Client, session: &SessionId, text: &str, turn: &Value) -> Value {
    let params = json!({"command_id": client::command_id(), "session_id": session,
        "mode": "steer", "expected_turn_id": turn, "content": [{"type": "text", "text": text}]});
    json!(client.request::<SessionSend>(typed(params)).await.unwrap())
}

/// The ID of the session's latest turn, once there are `n`.
async fn turn(data_dir: &Path, session: &SessionId, n: usize) -> Value {
    let started = events(data_dir, session, "turn.started", n).await;
    started[n - 1]["turn_id"].clone()
}

/// How many `_session/steering` requests the agent got.
fn steerings(path: &Path) -> usize {
    let calls = calls(path);
    calls
        .iter()
        .filter(|(m, _)| m == "_session/steering")
        .count()
}

/// How many processes the host has below it, itself included.
fn tree(host: u32) -> usize {
    let out = Command::new("ps")
        .args(["-x", "-o", "pid=,ppid="])
        .output()
        .unwrap();
    let table: Vec<(u32, u32)> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace().map(|f| f.parse().ok());
            Some((fields.next()??, fields.next()??))
        })
        .collect();
    let mut found = vec![host];
    let mut at = 0;
    while let Some(&parent) = found.get(at) {
        found.extend(
            table
                .iter()
                .filter(|(_, ppid)| *ppid == parent)
                .map(|(pid, _)| *pid),
        );
        at += 1;
    }
    found.len()
}

#[tokio::test(flavor = "multi_thread")]
async fn it_k1_close_resume_after_a_prompt_that_never_returns() {
    let dir = TempDir::new("k1");
    let config = json!({"durations": {"interrupt_fallback_ms": 300}});
    let (data_dir, _host, client, path) = host(&dir, &[], config).await;
    let session = create(&client, "say stuck; hang").await;
    said(&data_dir, &session, "stuck").await;
    interrupt(&client, &session).await;
    let ended = events(&data_dir, &session, "turn.completed", 1).await;
    assert_eq!(ended[0]["status"], "cancelled");
    send(&client, &session, "whoami").await;
    said(&data_dir, &session, "resumed=true").await;
    // Explicitly closed, then resumed, before the next prompt (K1).
    let methods: Vec<String> = calls(&path).into_iter().map(|(m, _)| m).collect();
    let after = methods.iter().rposition(|m| m == "session/cancel").unwrap();
    assert_eq!(
        methods[after + 1..],
        ["session/close", "session/resume", "session/prompt"]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn it_k5_withdraw_by_the_agent_and_on_interrupt() {
    let dir = TempDir::new("k5");
    let (data_dir, _host, client, _) = host(&dir, &[], json!({})).await;
    // Withdrawn by the agent: `$/cancel_request` ends it (K5).
    let session = create(
        &client,
        "withdraw_after 300; permission p1; sleep 300; say after",
    )
    .await;
    events(&data_dir, &session, "interaction.requested", 1).await;
    let resolved = events(&data_dir, &session, "interaction.resolved", 1).await;
    assert_eq!(resolved[0]["status"], "cancelled");
    assert_eq!(resolved[0]["reason"], "withdrawn");
    events(&data_dir, &session, "turn.completed", 1).await;
    // Interrupted: the host ends it first; Claude's withdrawal that follows
    // ends nothing more.
    send(&client, &session, "permission p2").await;
    events(&data_dir, &session, "interaction.requested", 2).await;
    interrupt(&client, &session).await;
    events(&data_dir, &session, "turn.completed", 2).await;
    let resolved = events(&data_dir, &session, "interaction.resolved", 2).await;
    assert_eq!(resolved.len(), 2, "each request ends once");
    assert_eq!(resolved[1]["status"], "cancelled");
    assert_eq!(resolved[1]["reason"], "turn_cancelled");
}

#[tokio::test(flavor = "multi_thread")]
async fn it_k6_crash_resume_sends_once_more_and_the_host_keeps_no_more_processes() {
    let dir = TempDir::new("k6");
    let (data_dir, host, client, path) = host(&dir, &[], json!({})).await;
    let session = create(&client, "say first").await;
    events(&data_dir, &session, "turn.completed", 1).await;
    let processes = tree(host.0.id());
    for round in 1..=5 {
        // The CLI dies: the turn fails, and the session is gone in the agent.
        send(&client, &session, "crash_cli").await;
        let ended = events(&data_dir, &session, "turn.completed", 2 * round).await;
        assert_eq!(ended[2 * round - 1]["status"], "failed");
        // The next prompt finds it gone before any output: resumed, sent again.
        send(&client, &session, "whoami").await;
        let ended = events(&data_dir, &session, "turn.completed", 2 * round + 1).await;
        assert_eq!(ended[2 * round]["status"], "completed");
        assert_eq!(sent(&path, "session/prompt", "whoami"), 2 * round);
        assert_eq!(sent(&path, "session/resume", ""), round);
        // Q43: resuming leaves no process behind on the host's side.
        assert_eq!(tree(host.0.id()), processes, "round {round}");
    }
    said(&data_dir, &session, "resumed=true").await;
    assert_eq!(
        sent(&path, "session/prompt", "crash_cli"),
        5,
        "never sent again"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn it_k6_output_then_crash_is_not_sent_again() {
    let dir = TempDir::new("k6-output");
    let (data_dir, _host, client, path) = host(&dir, &[], json!({})).await;
    let session = create(&client, "output_then_crash").await;
    let ended = events(&data_dir, &session, "turn.completed", 1).await;
    assert_eq!(ended[0]["status"], "failed");
    said(&data_dir, &session, "output before the crash").await;
    send(&client, &session, "say next").await;
    let ended = events(&data_dir, &session, "turn.completed", 2).await;
    assert_eq!(ended[1]["status"], "completed");
    assert_eq!(sent(&path, "session/prompt", "output_then_crash"), 1);
    assert_eq!(sent(&path, "session/resume", ""), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn it_k9_autonomous_turns_after_the_prompt_and_after_silence() {
    let dir = TempDir::new("k9");
    let config = json!({"durations": {"autonomous_silence_ms": 300}});
    let (data_dir, _host, client, _) = host(&dir, &[], config).await;
    let script = "say first; autonomous 100; say later; sleep 1000; say again";
    let session = create(&client, script).await;
    let started = events(&data_dir, &session, "turn.started", 3).await;
    let origins: Vec<&Value> = started.iter().map(|turn| &turn["origin"]).collect();
    assert_eq!(origins, ["user", "autonomous", "autonomous"]);
    said(&data_dir, &session, "again").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn it_k14_old_cli_auth_fails_as_cli_too_old() {
    let dir = TempDir::new("k14");
    let config = json!({"cli_version": "1.0.0"});
    let (data_dir, _host, client, _) = host(&dir, &[], config).await;
    let session = create(&client, "auth_none").await;
    let ended = events(&data_dir, &session, "turn.completed", 1).await;
    assert_eq!(ended[0]["error_code"], "cli_too_old");
}

#[tokio::test(flavor = "multi_thread")]
async fn it_auth_none_fails_as_not_logged_in() {
    let dir = TempDir::new("auth-none");
    let (data_dir, _host, client, _) = host(&dir, &["--auth", "logged-out"], json!({})).await;
    let session = create(&client, "auth_none").await;
    let ended = events(&data_dir, &session, "turn.completed", 1).await;
    assert_eq!(ended[0]["error_code"], "not_logged_in");
    let auth = "SELECT json_extract(json, '$.data.agents[0].auth.state') FROM events
                WHERE stream = 'index' AND type = 'agents.updated' ORDER BY seq DESC LIMIT 1";
    until(&data_dir, "host.db", auth, &["logged_out"]).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn it_k19_unknown_kind_privileged() {
    let dir = TempDir::new("k19");
    let (data_dir, _host, client, _) = host(&dir, &[], json!({})).await;
    let options = json!([{"optionId": "allow-once", "name": "Yes", "kind": "allow_once"},
        {"optionId": "maybe", "name": "Maybe", "kind": "allow_maybe"}]);
    let session = create(&client, &format!("permission_options {options}")).await;
    let asked = events(&data_dir, &session, "interaction.requested", 1).await;
    let options = &asked[0]["request"]["options"];
    assert_eq!(options[0]["requires"], "approve");
    // The kind nobody knows is taken at its strictest (K19).
    assert_eq!(options[1]["requires"], "privileged");
    assert_eq!(options[1]["effect"]["scope"], "unknown");
    let outcome = json!({"type": "selected", "option_id": "allow-once"});
    respond(&client, &session, &asked[0], outcome)
        .await
        .unwrap();
    said(&data_dir, &session, "permission:allow-once").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn it_thought_claude_streams_a_thought_item() {
    let dir = TempDir::new("thought");
    let (data_dir, _host, client, _) = host(&dir, &[], json!({})).await;
    let session = create(&client, "think pondering the question; say done").await;
    events(&data_dir, &session, "turn.completed", 1).await;
    let thoughts = "SELECT json_extract(json, '$.data.kind') FROM events
        WHERE type = 'item.started' AND json_extract(json, '$.data.kind') = 'thought'";
    until(&data_dir, "host.db", thoughts, &["thought"]).await;
    said(&data_dir, &session, "pondering the question").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn it_plan_entries_one_plan_item_follows_the_updates() {
    let dir = TempDir::new("plan");
    let (data_dir, _host, client, _) = host(&dir, &[], json!({})).await;
    let script = "plan_update 1. read\\n2. write; plan_update 1. read\\n2. write\\n3. test";
    let session = create(&client, script).await;
    events(&data_dir, &session, "turn.completed", 1).await;
    let plans = "SELECT json_extract(json, '$.data.kind') FROM events
        WHERE type = 'item.started' AND json_extract(json, '$.data.kind') = 'plan'";
    until(&data_dir, "host.db", plans, &["plan"]).await;
    let markdown = "SELECT json_extract(json, '$.data') FROM events
        WHERE type IN ('item.started', 'item.updated', 'item.completed') ORDER BY seq";
    let all = query(&data_dir.join("host.db"), markdown).join("\n");
    assert!(all.contains("3. test"), "{all}");
}

/// IT-degrade-plan, IT-degrade-thinking: the fake agent of `agent`'s
/// persona `--without` `what`. The option, or Claude's `plan` value, is gone
/// from the session's capabilities, the capability that follows it false
/// and the other one not; setting it is refused and never reaches the
/// agent; the turn has no plan, no thought and no request.
async fn degraded(agent: &str, what: &str) {
    let dir = TempDir::new(&format!("degrade-{agent}-{what}"));
    let args = ["--without", what];
    let (data_dir, _host, client, recorded) = host_of(agent, &dir, &args, json!({})).await;
    let script = match what {
        "plan" => "plan_update 1. read; plan_confirm; say done",
        _ => "think pondering; say done",
    };
    let session = match agent {
        "codex" => codex_session(&client, script).await,
        _ => create(&client, script).await,
    };
    events(&data_dir, &session, "turn.completed", 1).await;
    let asked = SessionCapabilitiesParams {
        session_id: session.clone(),
    };
    let caps = client.request::<SessionCapabilities>(asked).await.unwrap();
    let (option, value) = match (agent, what) {
        ("claude", "plan") => ("permission_mode", "plan"),
        (_, "plan") => ("plan_mode", "plan"),
        _ => ("reasoning_effort", "high"),
    };
    let offered = caps.config.iter().find(|o| o.id.as_str() == option);
    assert_eq!(offered.is_some(), (agent, what) == ("claude", "plan"));
    assert!(offered.is_none_or(|o| o.values.iter().all(|v| v.value != value)));
    let flags = (caps.interactions.plan_confirm, caps.extraction.thinking);
    assert_eq!(flags, (what != "plan", what != "thinking"));
    let set = json!({"command_id": client::command_id(), "session_id": session,
        "option": option, "value": value});
    let set = client.request::<SessionSetConfig>(typed(set)).await;
    let Err(ClientError::Rpc(err)) = set else {
        panic!("{set:?}")
    };
    let data = err.data.as_deref();
    let got = (
        err.code,
        data.map(|d| d.code.to_string()),
        data.and_then(|d| d.capability.clone()),
    );
    let unsupported = (APP_ERROR, Some("unsupported".into()), Some(option.into()));
    let want = if offered.is_some() {
        (INVALID_PARAMS, None, None)
    } else {
        unsupported
    };
    assert_eq!(got, want);
    assert!(
        calls(&recorded)
            .iter()
            .all(|(method, _)| method != "session/set_config_option")
    );
    let none = format!(
        "SELECT CAST(count(*) AS TEXT) FROM events WHERE stream = 'session/{session}'
         AND (type = 'interaction.requested' OR json_extract(json, '$.data.kind') IN ('plan', 'thought'))"
    );
    assert_eq!(query(&data_dir.join("host.db"), &none), ["0"]);
    said(&data_dir, &session, "done").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn it_degrade_plan_claude() {
    degraded("claude", "plan").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn it_degrade_plan_codex() {
    degraded("codex", "plan").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn it_degrade_thinking_claude() {
    degraded("claude", "thinking").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn it_degrade_thinking_codex() {
    degraded("codex", "thinking").await;
}

/// IT-K2-reject-ends-turn: Codex's only rejection is `cancel`, which ends
/// the whole turn as `cancelled` (#556); the option says so, and a turn the
/// user ended so does not pause the queue: the message queued meanwhile runs.
#[tokio::test(flavor = "multi_thread")]
async fn it_k2_reject_ends_turn_and_the_queue_goes_on() {
    let dir = TempDir::new("k2");
    let (data_dir, _host, client, _) = host_of("codex", &dir, &[], json!({})).await;
    let session = codex_session(&client, "permission t1 execute").await;
    let asked = events(&data_dir, &session, "interaction.requested", 1).await;
    let options = asked[0]["request"]["options"].as_array().unwrap();
    let reject = options.iter().find(|o| o["option_id"] == "cancel").unwrap();
    assert_eq!(reject["effect"]["ends_turn"], true, "{reject}");
    send(&client, &session, "say next").await;
    let outcome = json!({"type": "selected", "option_id": "cancel"});
    respond(&client, &session, &asked[0], outcome)
        .await
        .unwrap();
    let ended = events(&data_dir, &session, "turn.completed", 2).await;
    assert_eq!(
        (&ended[0]["status"], &ended[1]["status"]),
        (&json!("cancelled"), &json!("completed"))
    );
    let started = events(&data_dir, &session, "turn.started", 2).await;
    assert_eq!(started[1]["origin"], "queued");
}

/// IT-K3-late-output: interrupting does not stop Codex's command. It ends
/// `abandoned` and `may_still_run`; its output and end come later, as a
/// `late_tool_output` item of the turn running then, an autonomous one,
/// and the interrupted turn stays as it was.
#[tokio::test(flavor = "multi_thread")]
async fn it_k3_late_output_after_an_interrupt() {
    let dir = TempDir::new("k3");
    let (data_dir, _host, client, _) = host_of("codex", &dir, &[], json!({})).await;
    let session = codex_session(&client, "late_tool t1 300").await;
    let tool = items(&data_dir, &session, "tool_call", 1).await.remove(0);
    interrupt(&client, &session).await;
    let ended = events(&data_dir, &session, "turn.completed", 1).await;
    assert_eq!(ended[0]["status"], "cancelled");
    let late = items(&data_dir, &session, "late_tool_output", 1)
        .await
        .remove(0);
    assert_eq!(late["ref"]["item_id"], tool["item_id"], "{late}");
    let started = events(&data_dir, &session, "turn.started", 2).await;
    assert_eq!(
        (&late["turn_id"], &started[1]["origin"]),
        (&started[1]["turn_id"], &json!("autonomous"))
    );
    let ended = events(&data_dir, &session, "turn.completed", 2).await;
    assert_eq!(ended[1]["status"], "completed");
    let tool_ends = format!(
        "SELECT json_extract(json, '$.data.status') || ' ' || coalesce(json_extract(json, '$.data.may_still_run'), 0)
         FROM events WHERE stream = 'session/{session}' AND type = 'item.completed'
         AND json_extract(json, '$.data.item_id') = '{}'", tool["item_id"].as_str().unwrap()
    );
    assert_eq!(
        query(&data_dir.join("host.db"), &tool_ends),
        ["abandoned 1"]
    );
}

/// IT-K4-cancel-pending-approval: Codex does not take its request back on
/// a cancel; the host ends it as `cancelled` and answers it so for the user.
#[tokio::test(flavor = "multi_thread")]
async fn it_k4_cancel_pending_approval_is_answered_cancelled() {
    let dir = TempDir::new("k4");
    let (data_dir, _host, client, _) = host_of("codex", &dir, &[], json!({})).await;
    let session = codex_session(&client, "permission t1 execute").await;
    events(&data_dir, &session, "interaction.requested", 1).await;
    interrupt(&client, &session).await;
    let resolved = events(&data_dir, &session, "interaction.resolved", 1).await;
    assert_eq!(
        (&resolved[0]["status"], &resolved[0]["reason"]),
        (&json!("cancelled"), &json!("turn_cancelled"))
    );
    let ended = events(&data_dir, &session, "turn.completed", 1).await;
    assert_eq!(ended[0]["status"], "cancelled");
    // Raw frames hold no barrier: wait for the answer to land.
    let answered = "SELECT json_extract(frame, '$.result.outcome.outcome') FROM raw_frames
        WHERE dir = 'send' AND json_extract(frame, '$.result.outcome') IS NOT NULL";
    until(&data_dir, "raw.db", answered, &["cancelled"]).await;
}

/// IT-K7-restartable: Codex's app-server was killed (`1001`, `restartable`):
/// the turn fails with an `error` item that says to try again, and the next
/// prompt runs, the adapter having started it again.
#[tokio::test(flavor = "multi_thread")]
async fn it_k7_restartable_fails_the_turn_and_the_next_one_runs() {
    let dir = TempDir::new("k7");
    let (data_dir, _host, client, _) = host_of("codex", &dir, &[], json!({})).await;
    let session = codex_session(&client, "restartable").await;
    let ended = events(&data_dir, &session, "turn.completed", 1).await;
    assert_eq!(
        (&ended[0]["status"], &ended[0]["error_code"]),
        (&json!("failed"), &json!("restartable"))
    );
    let error = items(&data_dir, &session, "error", 1).await.remove(0);
    assert_eq!(
        (&error["guidance"], &error["agent_code"]),
        (&json!("retry"), &json!("1001")),
        "{error}"
    );
    send(&client, &session, "say again").await;
    let ended = events(&data_dir, &session, "turn.completed", 2).await;
    assert_eq!(ended[1]["status"], "completed");
    said(&data_dir, &session, "again").await;
}

/// IT-K12-plan-confirm-idle: Codex's thread is idle by the time it asks to
/// confirm its plan. The prompt is in flight, so its answer ends the turn:
/// the request stays, and the plan goes on once confirmed.
#[tokio::test(flavor = "multi_thread")]
async fn it_k12_plan_confirm_while_the_thread_is_idle() {
    let dir = TempDir::new("k12");
    let (data_dir, _host, client, _) = host_of("codex", &dir, &[], json!({})).await;
    let session = codex_session(&client, "plan_update 1. read; plan_confirm; say built").await;
    let asked = events(&data_dir, &session, "interaction.requested", 1).await;
    assert_eq!(asked[0]["request"]["type"], "plan_confirm", "{}", asked[0]);
    // The idle came first and ended nothing.
    let turns = format!(
        "SELECT count(*) || '' FROM events WHERE stream = 'session/{session}' AND type = 'turn.completed'"
    );
    assert_eq!(query(&data_dir.join("host.db"), &turns), ["0"]);
    let outcome = json!({"type": "selected", "option_id": "implement_plan"});
    respond(&client, &session, &asked[0], outcome)
        .await
        .unwrap();
    said(&data_dir, &session, "built").await;
    let ended = events(&data_dir, &session, "turn.completed", 1).await;
    assert_eq!(ended[0]["status"], "completed");
}

/// IT-K17-text-error: a failed thread (`systemError`) puts its error in a
/// message without an ID and still ends `end_turn`. The turn fails with the
/// error's class, its `error` item carries the words, and they are no
/// message of the agent's.
#[tokio::test(flavor = "multi_thread")]
async fn it_k17_text_error_fails_the_turn_with_its_words() {
    let dir = TempDir::new("k17");
    let (data_dir, _host, client, _) = host_of("codex", &dir, &[], json!({})).await;
    let words = "The 'gpt-x' model is not supported when using Codex with a ChatGPT account.";
    let session = codex_session(&client, &format!("thread_status systemError; say {words}")).await;
    let ended = events(&data_dir, &session, "turn.completed", 1).await;
    assert_eq!(
        (&ended[0]["status"], &ended[0]["error_code"]),
        (&json!("failed"), &json!("model_unsupported"))
    );
    let error = items(&data_dir, &session, "error", 1).await.remove(0);
    assert_eq!(
        (&error["raw"], &error["guidance"]),
        (&json!(words), &json!("change_model")),
        "{error}"
    );
    let errors = items(&data_dir, &session, "error", 1).await.len();
    let messages = items(&data_dir, &session, "agent_message", 0).await.len();
    assert_eq!((errors, messages), (1, 0));
}

/// K14 on opening (T063's handoff): Codex says it needs a login as the
/// session opens. With a CLI older than the verified range that is
/// `cli_too_old`; otherwise `not_logged_in`, and the agent is logged out.
#[tokio::test(flavor = "multi_thread")]
async fn it_k14_old_cli_auth_codex_fails_on_opening() {
    let dir = TempDir::new("k14-codex");
    let config = json!({"cli_version": "codex-cli 0.120.0"});
    let (data_dir, _host, client, _) =
        host_of("codex", &dir, &["--auth", "logged-out"], config).await;
    let session = codex_session(&client, "say hi").await;
    let ended = events(&data_dir, &session, "turn.completed", 1).await;
    assert_eq!(ended[0]["error_code"], "cli_too_old");

    let dir = TempDir::new("k14-codex-current");
    let (data_dir, _host, client, _) =
        host_of("codex", &dir, &["--auth", "logged-out"], json!({})).await;
    let session = codex_session(&client, "say hi").await;
    let ended = events(&data_dir, &session, "turn.completed", 1).await;
    assert_eq!(ended[0]["error_code"], "not_logged_in");
    let auth = "SELECT json_extract(value, '$.auth.state') FROM events,
                json_each(events.json, '$.data.agents') WHERE stream = 'index'
                AND events.type = 'agents.updated' AND json_extract(value, '$.agent') = 'codex'
                ORDER BY seq DESC LIMIT 1";
    until(&data_dir, "host.db", auth, &["logged_out"]).await;
}

/// IT-K18-upstream-retry: Codex retries an overloaded upstream by itself.
/// The turn has one `upstream_retry` notice: started at the first attempt,
/// updated with each, ended with the turn.
#[tokio::test(flavor = "multi_thread")]
async fn it_k18_upstream_retry_one_notice_follows_the_attempts() {
    let dir = TempDir::new("k18");
    let (data_dir, _host, client, _) = host_of("codex", &dir, &[], json!({})).await;
    let session = codex_session(&client, "upstream_retry 3; say through").await;
    let ended = events(&data_dir, &session, "turn.completed", 1).await;
    assert_eq!(ended[0]["status"], "completed");
    let notices = items(&data_dir, &session, "notice", 1).await;
    let [notice] = &notices[..] else {
        panic!("{notices:?}")
    };
    assert_eq!(notice["code"], "upstream_retry");
    assert_eq!(notice["turn_id"], ended[0]["turn_id"]);
    let of_it = |kind: &str, field: &str| {
        let sql = format!(
            "SELECT coalesce(json_extract(json, '$.data.{field}'), '-') || '' FROM events
             WHERE stream = 'session/{session}' AND type = '{kind}'
             AND json_extract(json, '$.data.item_id') = '{}' ORDER BY seq",
            notice["item_id"].as_str().unwrap()
        );
        query(&data_dir.join("host.db"), &sql)
    };
    let attempts = [&notice["params"]["attempt"]];
    let updated = of_it("item.updated", "params.attempt");
    assert_eq!(
        (attempts, &notice["params"]["max"]),
        ([&json!(1)], &json!(5))
    );
    assert_eq!(updated, ["2", "3"]);
    assert_eq!(of_it("item.completed", "status"), ["completed"]);
}

/// IT-K10-steer-explicit-only: while Claude's turn runs, a `queue` message
/// never steers; a `steer` naming the turn goes in as `steered`.
#[tokio::test(flavor = "multi_thread")]
async fn it_k10_steer_explicit_only_a_queued_message_never_steers() {
    let dir = TempDir::new("k10");
    let (data_dir, _host, client, path) = host(&dir, &[], json!({})).await;
    let session = create(&client, "say running; wait_cancel").await;
    said(&data_dir, &session, "running").await;
    let running = turn(&data_dir, &session, 1).await;
    send(&client, &session, "for later").await;
    assert_eq!(steerings(&path), 0, "a queued message steers");
    let steered = steer(&client, &session, "now", &running).await;
    let want = json!({"disposition": "steered", "turn_id": running});
    assert_eq!(steered, want);
    assert_eq!(sent(&path, "_session/steering", "now"), 1);
    let messages = items(&data_dir, &session, "user_message", 2).await;
    assert_eq!(
        (&messages[1]["steered"], &messages[1]["turn_id"]),
        (&json!(true), &running)
    );
    said(&data_dir, &session, "steered:now").await;
    interrupt(&client, &session).await;
}

/// IT-K11-steer-idle-queued: Codex ignores `idleBehavior`, so the host
/// steers only a prompting turn. A steer of a turn being cancelled is
/// queued; one while idle starts a turn of its own. No steering goes out.
#[tokio::test(flavor = "multi_thread")]
async fn it_k11_steer_idle_queued_outside_a_prompt() {
    let dir = TempDir::new("k11");
    let config = json!({"durations": {"interrupt_fallback_ms": 1000}});
    let (data_dir, _host, client, path) = host_of("codex", &dir, &[], config).await;
    let session = codex_session(&client, "say stuck; hang").await;
    said(&data_dir, &session, "stuck").await;
    let cancelling = turn(&data_dir, &session, 1).await;
    interrupt(&client, &session).await;
    let queued = steer(&client, &session, "while cancelling", &cancelling).await;
    let reason = (&queued["disposition"], &queued["fallback_reason"]);
    assert_eq!(reason, (&json!("queued"), &json!("not_prompting")));
    events(&data_dir, &session, "turn.completed", 1).await;
    let idle = steer(&client, &session, "say idle", &cancelling).await;
    assert_eq!(idle["disposition"], "started");
    said(&data_dir, &session, "idle").await;
    assert_eq!(steerings(&path), 0);
}

/// IT-degrade-steering: an agent without steering shows
/// `steer.supported: false`, and a steer is queued as `unsupported`.
#[tokio::test(flavor = "multi_thread")]
async fn it_degrade_steering_unsupported_is_queued() {
    let dir = TempDir::new("degrade-steering");
    let (data_dir, _host, client, path) = host(&dir, &["--fault", "no_steering"], json!({})).await;
    let session = create(&client, "say running; wait_cancel").await;
    said(&data_dir, &session, "running").await;
    let params = json!({"session_id": session});
    let caps = client
        .request::<SessionCapabilities>(typed(params))
        .await
        .unwrap();
    assert!(!caps.turn.steer.supported);
    let running = turn(&data_dir, &session, 1).await;
    let queued = steer(&client, &session, "now", &running).await;
    let reason = (&queued["disposition"], &queued["fallback_reason"]);
    assert_eq!(reason, (&json!("queued"), &json!("unsupported")));
    assert_eq!(steerings(&path), 0);
    interrupt(&client, &session).await;
}
