//! IT-orch-trust-gating: the project trust rescans of a session (design
//! 5.2.6) on IT-orch-lifecycle's fixture, with its project in a temp dir and
//! an in-process agent on `ManualClock`. A project is trusted with the
//! digest of what its range holds when the test says so.

use std::fs;
use std::path::Path;
use std::time::Duration;

use cupper_protocol::ids::SessionId;
use cupper_protocol::session::StopReason;
use cupper_sys::{Command, Env};
use serde_json::{Value, json};

use super::gating_tests::changed;
use super::tests::{Fixture, create, data, done, on, send};
use crate::clock::ManualClock;
use crate::driver::api::{
    AgentSessionId, AttachTarget, DriverCommand, ProjectConfigPolicy, PromptTag,
};
use crate::git::GitTool;
use crate::git::run::run;
use crate::orchestrator::testkit::text;
use crate::store::testutil::TempDir;
use crate::trust;

/// Trusts the project with what its range holds now, `root` its git root.
fn trust_now(f: &Fixture, root: Option<&Path>) {
    let digest = trust::scan(&f.project(), root).unwrap().digest;
    f.exec("UPDATE projects SET trust_digest = ?1", digest.to_string());
}

/// Puts project configuration into the range.
fn change(dir: &Path) {
    fs::create_dir_all(dir.join(".claude")).unwrap();
    fs::write(dir.join(".claude/settings.json"), "{\"hooks\": {}}").unwrap();
}

async fn created(f: &Fixture) -> SessionId {
    let created = f.ok("session/create", create("c-1")).await;
    serde_json::from_value(created["session_id"].clone()).unwrap()
}

/// The notices so far, once there are `n` items, each with its reason.
async fn notices(f: &Fixture, session: &SessionId, n: usize) -> Vec<(String, Value)> {
    let items = data(&f.events(session, "item.started", n).await, "item.started");
    let notices = items.iter().filter(|i| i["kind"] == "notice");
    notices
        .map(|i| {
            (
                i["code"].as_str().unwrap().into(),
                i["params"]["reason"].clone(),
            )
        })
        .collect()
}

/// The reason the queue is paused for, once `n` updates were written.
async fn paused(f: &Fixture, session: &SessionId, n: usize) -> Value {
    let updates = data(
        &f.events(session, "queue.updated", n).await,
        "queue.updated",
    );
    updates.last().unwrap()["paused"]["reason"].clone()
}

#[tokio::test]
async fn it_orch_trust_gating_a_change_before_a_prompt_holds_the_message() {
    let mut f = Fixture::new("trust-before").await;
    trust_now(&f, None);
    let session = f.started().await;
    f.emit(done(PromptTag(1), StopReason::EndTurn));
    f.events(&session, "turn.completed", 1).await;
    change(&f.project());
    let sent = f.ok("session/send", send(&session, "s-1", "next")).await;
    assert_eq!(sent["disposition"], "queued");
    assert_eq!(paused(&f, &session, 2).await, "project_config_changed");
    let found = notices(&f, &session, 2).await;
    assert_eq!(found, [("project_config_changed".into(), Value::Null)]);
    assert_eq!(
        data(&f.events(&session, "turn.started", 1).await, "turn.started").len(),
        1
    );
    assert!(
        !f.index.1.lock().unwrap().is_empty(),
        "the index heard of the change"
    );
    assert!(f.quiet(), "no prompt went out");
    // Trusted again: resuming the queue rescans, and the message runs.
    trust_now(&f, None);
    f.ok("session/queue_resume", on(&session, "r-1")).await;
    assert_eq!(f.prompt().await.1, text("next"));
}

#[tokio::test]
async fn it_orch_trust_gating_a_change_during_a_turn_and_going_on_without_it() {
    let mut f = Fixture::new("trust-during").await;
    trust_now(&f, None);
    let session = f.started().await;
    change(&f.project());
    f.emit(done(PromptTag(1), StopReason::EndTurn));
    let found = notices(&f, &session, 2).await;
    assert_eq!(found, [("project_config_changed".into(), Value::Null)]);
    let sent = f.ok("session/send", send(&session, "s-1", "next")).await;
    assert_eq!(sent["disposition"], "queued");
    // Going on without the project's configuration resumes without it.
    f.manager.ignore_project_config(&session).await.unwrap();
    assert!(matches!(f.command().await, DriverCommand::Close { .. }));
    let attach = f.attach().await;
    let resume = AttachTarget::Resume(AgentSessionId("agent-1".into()));
    assert_eq!(
        (attach.target, attach.project_config),
        (resume, ProjectConfigPolicy::Ignore)
    );
    assert_eq!(f.prompt().await.1, text("next"));
    let configured = data(
        &f.events(&session, "session.configured", 2).await,
        "session.configured",
    );
    assert_eq!(configured.last().unwrap()["project_config"], "ignored");
    // The choice outlives the host (T089).
    let ignored = "SELECT project_config_ignored || '' FROM sessions";
    assert_eq!(f.rows(ignored), ["1"]);
}

#[tokio::test]
async fn it_orch_trust_gating_untrusted_and_unreadable_projects() {
    // Untrusted: the session runs, without the project's configuration.
    let mut f = Fixture::new("trust-untrusted").await;
    f.exec("UPDATE projects SET trust_state = ?1", "untrusted".into());
    let session = created(&f).await;
    assert_eq!(f.attach().await.project_config, ProjectConfigPolicy::Ignore);
    assert_eq!(f.prompt().await.1, text("hi"));
    let configured = data(
        &f.events(&session, "session.configured", 1).await,
        "session.configured",
    );
    assert_eq!(configured[0]["project_config"], "ignored");
    // A rescan that fails counts as a change, with its reason.
    let f = Fixture::new("trust-missing").await;
    fs::remove_dir(f.project()).unwrap();
    let session = created(&f).await;
    let found = notices(&f, &session, 1).await;
    assert_eq!(
        found,
        [("project_config_changed".into(), json!("workspace_missing"))]
    );
    assert_eq!(paused(&f, &session, 2).await, "project_config_changed");
}

#[tokio::test]
async fn it_orch_trust_gating_two_reasons_hold_until_both_go() {
    for confirm_first in [true, false] {
        let mut f = Fixture::new("trust-two").await;
        trust_now(&f, None);
        let session = f.started().await;
        change(&f.project());
        f.emit(done(PromptTag(1), StopReason::EndTurn));
        notices(&f, &session, 2).await;
        // Idle, the agent goes to full access by itself as well.
        f.emit(changed("bypass"));
        let bodies = f.events(&session, "session.configured", 3).await;
        let configured = data(&bodies, "session.configured").pop().unwrap();
        let mut confirm = on(&session, "k-1");
        confirm["config_digest"] = configured["confirmation_required"]["config_digest"].clone();
        f.ok("session/send", send(&session, "s-1", "next")).await;
        for (n, confirming) in [confirm_first, !confirm_first].into_iter().enumerate() {
            if confirming {
                f.ok("session/confirm_config", confirm.clone()).await;
            } else {
                trust_now(&f, None);
                f.ok("session/queue_resume", on(&session, "r-1")).await;
                if n == 0 {
                    assert_eq!(paused(&f, &session, 4).await, "config_unconfirmed");
                }
            }
            // With one reason left, a message sent while idle is queued.
            if n == 0 {
                let probe = f.ok("session/send", send(&session, "s-2", "more")).await;
                assert_eq!(probe["disposition"], "queued", "{confirm_first}");
            }
        }
        assert_eq!(f.prompt().await.1, text("next"), "{confirm_first}");
    }
}

#[tokio::test]
async fn it_orch_trust_gating_the_git_root_bounds_the_range() {
    let path = std::env::var_os("PATH").unwrap();
    let git = std::env::split_paths(&path)
        .map(|d| d.join("git"))
        .find(|g| g.is_file())
        .unwrap();
    let home = TempDir::new("trust-git-home");
    let trusted = [
        git.parent().unwrap().into(),
        "/usr/bin".into(),
        "/bin".into(),
    ];
    let home_dir = Some(home.path().to_path_buf());
    let tool = GitTool::new(git.clone(), None, &trusted, home_dir, None, home.path()).unwrap();
    let tool = tool.with_test_env("GIT_CONFIG_NOSYSTEM", "1");
    let mut f = Fixture::with_git("trust-git", Some(tool)).await;
    // The project is a directory of a repository whose root has a `.claude/`.
    let root = f.project().parent().unwrap().to_path_buf();
    let mut env = Env::new();
    env.set("HOME", home.path()).unwrap();
    env.set("GIT_CONFIG_NOSYSTEM", "1").unwrap();
    let mut init = Command::new(&git);
    init.args(["init", "-q"]).current_dir(&root).env(env);
    let clock = ManualClock::new();
    let out = run(&clock, init, None, Duration::from_secs(30))
        .await
        .unwrap();
    assert!(out.status.success());
    trust_now(&f, None);
    change(&root);
    let session = created(&f).await;
    let found = notices(&f, &session, 1).await;
    assert_eq!(found, [("project_config_changed".into(), Value::Null)]);
    assert!(f.quiet(), "the root's configuration is in the range");
}

/// IT-git-snapshot-parity at a turn: a git project's turn takes its start
/// and end snapshots in the snapshot tier, and its row keeps both trees.
#[tokio::test]
async fn it_git_snapshot_parity_a_turn_takes_its_start_and_end_snapshots() {
    let path = std::env::var_os("PATH").unwrap();
    let mut found = std::env::split_paths(&path).map(|d| d.join("git"));
    let git = found.find(|g| g.is_file()).unwrap();
    let home = TempDir::new("snapshot-turn-home");
    let trusted = [
        git.parent().unwrap().into(),
        "/usr/bin".into(),
        "/bin".into(),
    ];
    let home_dir = Some(home.path().to_path_buf());
    let tool = GitTool::new(git.clone(), None, &trusted, home_dir, None, home.path()).unwrap();
    let tool = tool.with_test_env("GIT_CONFIG_NOSYSTEM", "1");
    let mut f = Fixture::with_git("snapshot-turn", Some(tool)).await;
    let mut env = Env::new();
    env.set("HOME", home.path()).unwrap();
    env.set("GIT_CONFIG_NOSYSTEM", "1").unwrap();
    let mut init = Command::new(&git);
    init.args(["init", "-q"]).current_dir(f.project()).env(env);
    let clock = ManualClock::new();
    let out = run(&clock, init, None, Duration::from_secs(30)).await;
    assert!(out.unwrap().status.success());
    fs::write(f.project().join("a.txt"), "a\n").unwrap();

    let session = f.started().await;
    fs::write(f.project().join("b.txt"), "b\n").unwrap();
    f.emit(done(PromptTag(1), StopReason::EndTurn));
    let bodies = f.events(&session, "turn.completed", 1).await;
    let shots = data(&bodies, "snapshot.recorded");
    let shown: Vec<_> = shots.iter().map(|s| (&s["phase"], &s["status"])).collect();
    let (start, end, ok) = (json!("start"), json!("end"), json!("ok"));
    assert_eq!(shown, [(&start, &ok), (&end, &ok)]);
    let stat = json!({"files": 1, "added": 1, "removed": 0});
    assert_eq!(shots[1]["stat"], stat);
    let row = "SELECT (snapshot_start IS NOT NULL) || (snapshot_end IS NOT NULL) || ' ' || \
        snapshot_status FROM turns";
    assert_eq!(f.rows(row), ["11 ok"]);
}
