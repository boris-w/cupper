//! The driver's known problems (design 5.6.9, the K list) on a real host
//! process with Claude's fake agent: each case runs what the agent does and
//! asserts what the host made of it, by the events it stored and the calls
//! the fake agent recorded (`--calls`).

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::*;
use cupper_cli::client::{self, Client};
use cupper_host::driver::runtime::launch;
use cupper_host::gateway::testkit::TempDir;
use cupper_protocol::ids::SessionId;
use cupper_protocol::methods::session::SessionInterrupt;
use serde_json::{Value, json};

/// A host with Claude's fake agent run with `args` and recording its calls,
/// more of `test_config` in `config`; the data directory, the host, the
/// client and the calls file.
async fn host(dir: &TempDir, args: &[&str], config: Value) -> (PathBuf, Running, Client, PathBuf) {
    let calls = dir.path().join("calls.jsonl");
    let mut args: Vec<Value> = args.iter().map(|arg| json!(arg)).collect();
    args.extend([json!("--calls"), json!(calls)]);
    let mut agent = json!({"program": launch::fake_agent(), "args": args});
    if let Some(version) = config.get("cli_version") {
        agent["cli_version"] = version.clone();
    }
    let mut config = config;
    config.as_object_mut().unwrap().remove("cli_version");
    config["agents"] = json!({"claude": agent});
    let (data_dir, host, client) = fake_agent_host(dir, config).await;
    (data_dir, host, client, calls)
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
