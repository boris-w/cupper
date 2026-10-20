//! Interactions, project trust and the approval tier on a host process
//! (design 5.9.8): IT-form, IT-plan-confirm, IT-interaction-expire,
//! IT-config-unconfirmed, IT-trust-stale, IT-trust-rescan-before-prompt and
//! IT-trust-settingsources. The agent is the claude-shaped fake agent, the
//! project a directory of the test's own, and the tests talk to the host as
//! the CLI's `harness` client.

mod common;

use std::path::{Path, PathBuf};

use cupper_cli::client::{self, Client};
use cupper_host::clock::{Clock, SystemClock};
use cupper_host::driver::runtime::launch;
use cupper_host::gateway::testkit::TempDir;
use cupper_protocol::ids::{ProjectId, SessionId};
use cupper_protocol::methods::project::{
    ProjectAdd, ProjectTrust, ProjectTrustInfo, ProjectTrustInfoParams,
};
use cupper_protocol::methods::session::{SessionConfirmConfig, SessionCreate, SessionQueueResume};
use serde_json::{Value, json};

use common::*;

/// A host with the fake agent, run with `args`, and `dir/app` added as a
/// project, trusted when `trusted`.
async fn host(
    dir: &TempDir,
    args: &[&str],
    trusted: bool,
    config: Value,
) -> (PathBuf, Running, Client, ProjectId) {
    let data_dir = dir.path().join("data");
    let agent = json!({"program": launch::fake_agent(), "args": args});
    let mut test_config = json!({"agents": {"claude": agent}, "login_env": {}});
    let extra = config.as_object().unwrap().clone();
    test_config.as_object_mut().unwrap().extend(extra);
    let host = Running(spawn_with(&data_dir, test_config));
    let client = connect(&data_dir).await;
    let app = dir.path().join("app");
    std::fs::create_dir_all(&app).unwrap();
    let path = app.to_str().unwrap();
    let add = json!({"command_id": client::command_id(), "path": path});
    let added = client.request::<ProjectAdd>(typed(add)).await.unwrap();
    if trusted {
        trust(&client, &added.project_id).await;
    }
    (data_dir, host, client, added.project_id)
}

/// Trusts `project` with what its range holds now.
async fn trust(client: &Client, project: &ProjectId) {
    let info = ProjectTrustInfoParams {
        project_id: project.clone(),
    };
    let info = client.request::<ProjectTrustInfo>(info).await.unwrap();
    let params = json!({"command_id": client::command_id(), "project_id": project,
        "trust_digest": info.trust_digest});
    client.request::<ProjectTrust>(typed(params)).await.unwrap();
}

/// A session in `project` whose first message is `text`.
async fn start(client: &Client, project: &ProjectId, text: &str) -> SessionId {
    let params = json!({"command_id": client::command_id(), "agent": "claude",
        "project_id": project, "workspace": {"kind": "local"},
        "message": [{"type": "text", "text": text}]});
    let created = client.request::<SessionCreate>(typed(params)).await;
    created.unwrap().session_id
}

/// Waits until the agent has said `text` in the session.
async fn said(data_dir: &Path, session: &SessionId, text: &str) {
    let sql = format!(
        "SELECT coalesce(group_concat(json_extract(json, '$.data.text'), ''), '') FROM events
         WHERE stream = 'session/{session}' AND type = 'item.delta'"
    );
    let clock = SystemClock::new();
    let give_up = clock.now_mono() + PATIENCE;
    while !query(&data_dir.join("host.db"), &sql)[0].contains(text) {
        assert!(clock.now_mono() < give_up, "not said: {text}");
        wait(&clock).await;
    }
}

/// The session's status and pending kind in the index, once they are `want`.
async fn status(data_dir: &Path, session: &SessionId, want: (&str, &str)) {
    let sql = format!(
        "SELECT json_extract(json, '$.data.status') || ' ' ||
                coalesce(json_extract(json, '$.data.pending_kind'), '-')
         FROM events WHERE stream = 'index' AND type = 'session.upserted'
           AND json_extract(json, '$.data.session_id') = '{session}'
         ORDER BY seq DESC LIMIT 1"
    );
    let want = format!("{} {}", want.0, want.1);
    until(data_dir, "host.db", &sql, &[want.as_str()]).await;
}

/// The latest notice of the session: its code and reason.
async fn notice(data_dir: &Path, session: &SessionId, n: usize) -> (String, Value) {
    let items = events(data_dir, session, "item.started", n).await;
    let notice = items.iter().rev().find(|item| item["kind"] == "notice");
    let notice = notice.expect("a notice");
    (
        notice["code"].as_str().unwrap().into(),
        notice["params"]["reason"].clone(),
    )
}

/// Puts project configuration into `dir`'s `.claude/`.
fn configure(dir: &Path, hooks: &str) {
    std::fs::create_dir_all(dir.join(".claude")).unwrap();
    let settings = format!(
        r#"{{"hooks": {{"Stop": [{{"hooks": [{{"type": "command", "command": "{hooks}"}}]}}]}}}}"#
    );
    std::fs::write(dir.join(".claude/settings.json"), settings).unwrap();
}

/// IT-form (AP-5): a form's request panel, answered by any client; the
/// agent hears the answer.
#[tokio::test]
async fn it_form_a_form_is_asked_answered_and_heard() {
    let dir = TempDir::new("form");
    let (data_dir, _host, client, project) = host(&dir, &[], true, json!({})).await;
    let session = start(&client, &project, "form color=red|blue").await;
    let asked = events(&data_dir, &session, "interaction.requested", 1).await;
    let request = &asked[0]["request"];
    assert_eq!(
        (&request["type"], &request["fields"][0]["field_id"]),
        (&json!("question"), &json!("question_0"))
    );
    status(&data_dir, &session, ("needs_action", "input")).await;
    let answer = json!({"type": "answer", "values": {"question_0": "blue"}});
    respond(&client, &session, &asked[0], answer).await.unwrap();
    said(&data_dir, &session, r#"form:{"question_0":"blue"}"#).await;
    let resolved = events(&data_dir, &session, "interaction.resolved", 1).await;
    assert_eq!(resolved[0]["status"], "resolved");
    until(
        &data_dir,
        "host.db",
        "SELECT status FROM turns",
        &["completed"],
    )
    .await;
    let responded = "'interaction.responded'";
    audited(&data_dir, responded, &["interaction.responded pr_test"]).await;
}

/// IT-plan-confirm (AP-5): a plan to confirm; the option chosen sets the
/// agent's mode, and the turn goes on.
#[tokio::test]
async fn it_plan_confirm_the_chosen_option_goes_back_to_the_agent() {
    let dir = TempDir::new("plan");
    let (data_dir, _host, client, project) = host(&dir, &[], true, json!({})).await;
    let script = "plan_update # Plan\\n- one step; plan_confirm; say after";
    let session = start(&client, &project, script).await;
    let asked = events(&data_dir, &session, "interaction.requested", 1).await;
    let request = &asked[0]["request"];
    assert_eq!(
        (&request["type"], &request["plan"]["text"]),
        (&json!("plan_confirm"), &json!("# Plan\n- one step"))
    );
    status(&data_dir, &session, ("needs_action", "approval")).await;
    // Auto mode, which the harness may choose: the agent switches to it.
    let chosen = json!({"type": "selected", "option_id": "exit-plan-auto"});
    respond(&client, &session, &asked[0], chosen).await.unwrap();
    said(&data_dir, &session, "plan:exit-plan-auto").await;
    said(&data_dir, &session, "after").await;
    let configured = events(&data_dir, &session, "session.configured", 2).await;
    let last = configured.last().unwrap();
    assert_eq!(last["config"]["permission_mode"], "auto");
    assert!(last.get("confirmation_required").is_none(), "{last}");
}

/// IT-interaction-expire (SEC-10): an answer counts only for the request and
/// the content it was given for, and a request its adapter left behind has
/// expired.
#[tokio::test]
async fn it_interaction_expire_an_answer_fits_its_request_and_its_adapter() {
    let dir = TempDir::new("expire");
    let (data_dir, _host, client, project) = host(&dir, &[], true, json!({})).await;
    let session = start(&client, &project, "permission t1 execute").await;
    let mut asked = events(&data_dir, &session, "interaction.requested", 1).await;
    let once = json!({"type": "selected", "option_id": "allow_once"});
    let digest = asked[0]["digest"].clone();
    asked[0]["digest"] = json!(format!("d_{}", "0".repeat(64)));
    let wrong = respond(&client, &session, &asked[0], once.clone()).await;
    assert_eq!(
        refused(wrong),
        ("conflict".into(), "digest_mismatch".into())
    );
    // The adapter dies while the request waits.
    let pid = query(
        &data_dir.join("host.db"),
        "SELECT CAST(pid AS TEXT) FROM adapters",
    );
    let killed = std::process::Command::new("/bin/kill")
        .args(["-KILL", &pid[0]])
        .status();
    assert!(killed.unwrap().success());
    let resolved = events(&data_dir, &session, "interaction.resolved", 1).await;
    assert_eq!(
        (&resolved[0]["status"], &resolved[0]["reason"]),
        (&json!("expired"), &json!("adapter_exited"))
    );
    asked[0]["digest"] = digest;
    let late = respond(&client, &session, &asked[0], once).await;
    assert_eq!(refused(late).0, "expired");
    let ended = events(&data_dir, &session, "turn.completed", 1).await;
    assert_eq!(
        (&ended[0]["status"], &ended[0]["error_code"]),
        (&json!("failed"), &json!("adapter_exited"))
    );
    status(&data_dir, &session, ("error", "-")).await;
}

/// The fake agent's script line that raises its own mode to bypass.
const RAISE: &str = r#"raw {"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"${session}","update":{"sessionUpdate":"current_mode_update","currentModeId":"bypassPermissions"}}}"#;

/// IT-config-unconfirmed (CP-4, design 5.9.8): the agent raises its mode
/// above what its setter may set during a turn; the host sets it back, or
/// interrupts the turn when it cannot, and the session waits in
/// `config_unconfirmed` until `session/confirm_config`.
#[tokio::test]
async fn it_config_unconfirmed_a_raise_is_set_back_and_waits_for_a_confirmation() {
    let dir = TempDir::new("unconfirmed");
    let (data_dir, _host, client, project) = host(&dir, &[], true, json!({})).await;
    let session = start(&client, &project, &format!("{RAISE}; sleep 300; say done")).await;
    said(&data_dir, &session, "done").await;
    let configured = events(&data_dir, &session, "session.configured", 3).await;
    let last = configured.last().unwrap();
    let required = &last["confirmation_required"];
    assert_eq!(
        (&last["config"]["permission_mode"], &required["reason"]),
        (&json!("default"), &json!("full_access_unconfirmed"))
    );
    // The next message waits for the confirmation.
    send(&client, &session, "say next").await;
    let queue = events(&data_dir, &session, "queue.updated", 2).await;
    assert_eq!(
        queue.last().unwrap()["paused"]["reason"],
        "config_unconfirmed"
    );
    let params = json!({"command_id": client::command_id(), "session_id": session,
        "config_digest": required["config_digest"]});
    client
        .request::<SessionConfirmConfig>(typed(params))
        .await
        .unwrap();
    said(&data_dir, &session, "next").await;
    let config = "'config.unconfirmed', 'config.confirmed'";
    audited(
        &data_dir,
        config,
        &["config.unconfirmed pr_test", "config.confirmed pr_test"],
    )
    .await;

    // An agent that refuses the setting back: the turn is interrupted.
    let dir = TempDir::new("unconfirmed-refused");
    let fault = ["--fault", "set_config_error"];
    let (data_dir, _host, client, project) = host(&dir, &fault, true, json!({})).await;
    let session = start(&client, &project, &format!("{RAISE}; wait_cancel")).await;
    let ended = events(&data_dir, &session, "turn.completed", 1).await;
    assert_eq!(ended[0]["status"], "cancelled");
    let configured = events(&data_dir, &session, "session.configured", 2).await;
    let required = &configured.last().unwrap()["confirmation_required"];
    assert_eq!(required["reason"], "full_access_unconfirmed");
}

/// The reason the session's queue is paused for, as the store has it now.
fn paused(data_dir: &Path, session: &SessionId) -> Vec<String> {
    let sql = format!(
        "SELECT coalesce(json_extract(json, '$.data.paused.reason'), '-') FROM events
         WHERE stream = 'session/{session}' AND type = 'queue.updated'
         ORDER BY seq DESC LIMIT 1"
    );
    query(&data_dir.join("host.db"), &sql)
}

/// IT-trust-stale (PS-2, design 5.9.8): the project's settings are a link
/// out of the workspace; pointed elsewhere, the project turns `stale` at the
/// rescan before the next prompt, which waits until it is trusted again. On
/// the manual clock, `test/clock_advance` returns only once that rescan is
/// done (T047's barrier).
#[tokio::test]
async fn it_trust_stale_a_retargeted_link_holds_the_next_prompt() {
    let dir = TempDir::new("stale");
    let (app, outside) = (dir.path().join("app"), dir.path().join("outside"));
    configure(&outside, "acme-a");
    // Enough to read that the rescan takes a while.
    std::fs::create_dir_all(app.join(".claude/agents")).unwrap();
    for n in 0..3 {
        let big = "x".repeat(4 << 20);
        std::fs::write(app.join(format!(".claude/agents/agent-{n}.md")), big).unwrap();
    }
    let link = app.join(".claude/settings.json");
    std::os::unix::fs::symlink(outside.join(".claude/settings.json"), &link).unwrap();
    let (data_dir, _host, client, project) =
        host(&dir, &[], true, json!({"clock": "manual"})).await;
    let script = "say started; sleep 300; say one";
    let session = start(&client, &project, script).await;
    said(&data_dir, &session, "started").await;
    configure(&dir.path().join("elsewhere"), "acme-b");
    std::fs::remove_file(&link).unwrap();
    let elsewhere = dir.path().join("elsewhere/.claude/settings.json");
    std::os::unix::fs::symlink(elsewhere, &link).unwrap();
    // The rescan at the turn's end finds it: once the clock has settled.
    let turns = "SELECT status FROM turns";
    until(&data_dir, "host.db", turns, &["completed"]).await;
    advance(&client, 0).await;
    let notices = format!(
        "SELECT json_extract(json, '$.data.code') FROM events
         WHERE stream = 'session/{session}' AND type = 'item.started'
           AND json_extract(json, '$.data.kind') = 'notice'"
    );
    let changed = ["project_config_changed"];
    assert_eq!(query(&data_dir.join("host.db"), &notices), changed);
    send(&client, &session, "say two").await;
    assert_eq!(paused(&data_dir, &session), changed);
    let state = "SELECT json_extract(json, '$.data.trust_state') FROM events
                 WHERE stream = 'index' AND type = 'project.upserted' ORDER BY seq DESC LIMIT 1";
    until(&data_dir, "host.db", state, &["stale"]).await;
    let trust_events = "'project.trust.trusted', 'project.trust.stale'";
    audited(
        &data_dir,
        trust_events,
        &["project.trust.trusted pr_test", "project.trust.stale -"],
    )
    .await;
    // Trusted again, the queue goes on.
    trust(&client, &project).await;
    let resume = json!({"command_id": client::command_id(), "session_id": session});
    client
        .request::<SessionQueueResume>(typed(resume))
        .await
        .unwrap();
    said(&data_dir, &session, "two").await;
}

/// IT-trust-rescan-before-prompt (design 5.2.6, 5.9.8): the project is a
/// directory of a git repository, whose root's `.claude/` counts too. A
/// change during a turn holds the message queued meanwhile, at the rescan
/// before its prompt; a link that leads nowhere fails the rescan, with why.
#[tokio::test]
async fn it_trust_rescan_before_prompt_a_change_during_a_turn_holds_the_next() {
    let dir = TempDir::new("rescan");
    let git = std::process::Command::new("/usr/bin/git")
        .args(["init", "-q"])
        .arg(dir.path())
        .status();
    assert!(git.unwrap().success());
    configure(dir.path(), "acme-a");
    let login = json!({"login_env": {"PATH": "/usr/bin:/bin"}, "clock": "manual"});
    let (data_dir, _host, client, project) = host(&dir, &[], true, login).await;
    let session = start(&client, &project, "say started; sleep 400; say one").await;
    said(&data_dir, &session, "started").await;
    configure(dir.path(), "acme-b");
    send(&client, &session, "say two").await;
    said(&data_dir, &session, "one").await;
    let changed = ("project_config_changed".into(), Value::Null);
    assert_eq!(notice(&data_dir, &session, 3).await, changed);
    let turns = "SELECT CAST(count(*) AS TEXT) FROM turns";
    assert_eq!(query(&data_dir.join("host.db"), turns), ["1"]);
    // Trusted again, it goes; then a link that leads nowhere fails the
    // rescan, and says why.
    trust(&client, &project).await;
    let resume = json!({"command_id": client::command_id(), "session_id": session});
    client
        .request::<SessionQueueResume>(typed(resume))
        .await
        .unwrap();
    said(&data_dir, &session, "two").await;
    let ended = "SELECT status FROM turns ORDER BY turn_no";
    until(&data_dir, "host.db", ended, &["completed", "completed"]).await;
    advance(&client, 0).await;
    let mcp = dir.path().join("app/.mcp.json");
    std::os::unix::fs::symlink(dir.path().join("nowhere"), mcp).unwrap();
    send(&client, &session, "say three").await;
    let failed = ("project_config_changed".into(), json!("dangling_link"));
    assert_eq!(notice(&data_dir, &session, 6).await, failed);
    audited(
        &data_dir,
        "'project.trust.scan_failed'",
        &["project.trust.scan_failed -"],
    )
    .await;
    assert_eq!(query(&data_dir.join("host.db"), turns), ["2"]);
}

/// IT-trust-settingsources (design 5.2.6): an untrusted project's session
/// runs with the user's settings only, `settingSources: ["user"]`; a trusted
/// one's loads the project's.
#[tokio::test]
async fn it_trust_settingsources_untrusted_projects_load_the_user_s_settings_only() {
    let dir = TempDir::new("sources");
    configure(&dir.path().join("app"), "acme-a");
    let (data_dir, _host, client, project) = host(&dir, &[], false, json!({})).await;
    let untrusted = start(&client, &project, "echo_meta").await;
    let user = json!({"claudeCode": {"options": {"settingSources": ["user"]}}});
    said(&data_dir, &untrusted, &format!("meta={user}")).await;
    let configured = events(&data_dir, &untrusted, "session.configured", 1).await;
    assert_eq!(configured.last().unwrap()["project_config"], "ignored");
    trust(&client, &project).await;
    let trusted = start(&client, &project, "echo_meta").await;
    said(&data_dir, &trusted, "meta=null").await;
    let configured = events(&data_dir, &trusted, "session.configured", 1).await;
    assert_eq!(configured.last().unwrap()["project_config"], "loaded");
}
