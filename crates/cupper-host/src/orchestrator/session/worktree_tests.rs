//! IT-worktree-create (CP-5): `session/create` in a new worktree (design
//! 5.2.10 "worktree", 5.5.2) of a project's repository, under the data
//! directory's `worktrees` as the fixture's settings have it. The worktree
//! is added, locked for the session and refreshed a second later, before
//! the agent attaches in it; its branch takes a suffix when the name is
//! taken; a name that is no branch's is refused; a `required` filter fails
//! it, and the first message waits in the paused queue.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use cupper_protocol::event::EventBody;
use cupper_protocol::ids::{ProjectId, SessionId};
use cupper_protocol::session::StopReason;
use serde_json::{Value, json};

use super::commit_tests::gits;
use super::hunk_tests::Case;
use super::tests::{code, data, done, harness, send, tags};
use crate::orchestrator::testkit::text;

/// `session/create` in a worktree from `main`, its branch named `hint`.
fn create(n: u32, hint: &str) -> Value {
    let worktree = json!({"kind": "worktree", "base_ref": "main", "branch_hint": hint});
    json!({"command_id": format!("w-{n}"), "agent": "claude", "workspace": worktree,
        "project_id": ProjectId::from_uuid(1), "message": text("hi")})
}

/// A repository on `main` with `a.txt` committed.
async fn repo(name: &str) -> Case {
    let c = Case::new(name).await;
    c.write("a.txt", "a\n");
    gits(&c, "checkout -q -b main\nadd -A\ncommit -qm base").await;
    c
}

async fn created(c: &Case, n: u32, hint: &str) -> SessionId {
    let created = c.f.ok("session/create", create(n, hint)).await;
    serde_json::from_value(created["session_id"].clone()).unwrap()
}

#[tokio::test]
async fn it_worktree_create_adds_locks_and_refreshes_then_attaches_there() {
    let mut c = repo("worktree").await;
    gits(&c, "branch cupper/x").await;
    let session = created(&c, 1, "x").await;
    let preparing = c.f.events(&session, "workspace.preparing", 2).await;
    let phase = |b: &EventBody| json!(b)["data"]["phase"].as_str().map(str::to_owned);
    let phases: Vec<String> = preparing.iter().filter_map(phase).collect();
    assert_eq!(phases.join(" "), "worktree_add refresh");
    // A message sent meanwhile waits for the first, and queues behind it.
    let two = send(&session, "w-2", "two");
    let sent = c.f.call(harness(), "session/send", two);
    let ((), sent) = tokio::join!(
        async {
            while c.f.clock.next_deadline().is_none() {
                tokio::task::yield_now().await;
            }
            let so_far = c.f.events(&session, "workspace.preparing", 2).await;
            assert!(!tags(&so_far).contains("workspace.ready"));
            c.f.clock.advance(Duration::from_secs(1));
        },
        sent
    );
    assert_eq!(sent.unwrap()["disposition"], "queued");

    let root = c.f.project().parent().unwrap().join("worktrees");
    let attach = c.f.attach().await;
    let (dir, name) = (attach.cwd.parent().unwrap(), attach.cwd.file_name());
    let at = (dir.parent().unwrap(), name);
    assert_eq!(at, (&*root, Some("x-2".as_ref())));
    let repo_dir = dir.file_name().unwrap().to_string_lossy().into_owned();
    let named = repo_dir.starts_with("app-") && repo_dir.len() == 12;
    assert!(named, "{repo_dir}");
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!((mode(&root), mode(dir)), (0o700, 0o700));
    assert!(attach.cwd.join("a.txt").is_file());
    let events = c.f.events(&session, "workspace.ready", 1).await;
    let ready = events.iter().find(|b| b.tag() == "workspace.ready");
    let path = attach.cwd.display().to_string();
    let want = json!({"path": path, "branch": "cupper/x-2"});
    assert_eq!(json!(ready.unwrap())["data"], want);
    let listed = c.git(&["worktree", "list", "--porcelain"], None).await;
    let listed = String::from_utf8(listed).unwrap();
    let head = String::from_utf8(c.git(&["rev-parse", "main"], None).await).unwrap();
    let branch = "branch refs/heads/cupper/x-2";
    let entry = format!("worktree {path}\nHEAD {head}{branch}\nlocked cupper session {session}\n");
    assert!(listed.contains(&entry), "{listed}");

    // The turn runs there; its diff has the session's changes only (GT-1).
    let (tag, content) = c.f.prompt().await;
    assert_eq!(content, text("hi"));
    std::fs::write(attach.cwd.join("b.txt"), "b\n").unwrap();
    c.f.emit(done(tag, StopReason::EndTurn));
    c.f.events(&session, "turn.completed", 1).await;
    let diff = json!({"session_id": session, "scope": "last_turn"});
    let diff = c.f.ok("git/diff", diff).await;
    assert_eq!(diff["files"][0]["path"], "b.txt");
    assert!(!json!(diff["caveats"]).to_string().contains("mixed_changes"));
    assert_eq!(c.f.prompt().await.1, text("two"));
}

#[tokio::test]
async fn it_worktree_create_refuses_a_bad_name_and_a_required_filter_fails_it() {
    let mut c = repo("worktree-filter").await;
    for hint in ["a/b", "x..y"] {
        let refused = c.f.call(harness(), "session/create", create(1, hint)).await;
        assert_eq!(refused.unwrap_err()["code"], -32602, "{hint}");
    }
    assert!(c.f.rows("SELECT session_id FROM sessions").is_empty());
    c.write(".gitattributes", "*.bin filter=keep\n");
    c.write("a.bin", "bytes\n");
    let filter = "add -A
        config filter.keep.required true
        config filter.keep.smudge cat
        config filter.keep.clean cat
        commit -qm filtered";
    gits(&c, filter).await;

    let session = created(&c, 2, "f").await;
    let events = c.f.events(&session, "workspace.failed", 1).await;
    let failed = events.iter().find(|b| b.tag() == "workspace.failed");
    let want = json!({"code": "git_error", "reason": "filter_unsupported"});
    assert_eq!(json!(failed.unwrap())["data"], want);
    for restarted in [false, true] {
        if restarted {
            c.f.restart().await;
        }
        let got = c.f.ok("session/get", json!({"session_id": session})).await;
        assert_eq!(got["summary"]["status"], "idle", "{restarted}");
        let events = c.f.events(&session, "queue.updated", 1).await;
        let queue = events.iter().rev().find(|b| b.tag() == "queue.updated");
        let queue = json!(queue.unwrap())["data"].clone();
        assert_eq!(queue["paused"]["reason"], "workspace_failed", "{restarted}");
        assert_eq!(
            queue["entries"][0]["content"],
            json!(text("hi")),
            "{restarted}"
        );
        let resume = json!({"command_id": format!("r-{restarted}"), "session_id": session});
        let resumed = c.f.call(harness(), "session/queue_resume", resume).await;
        assert_eq!(code(resumed.unwrap_err()), "unavailable ", "{restarted}");
    }
    let first = "SELECT session_id FROM sessions WHERE initial_prompt_json IS NOT NULL";
    assert!(c.f.rows(first).is_empty());
}

/// What a crash left is taken over (design 5.2.13): the worktree added and
/// locked, the host gone as the session waited to refresh it; or, when
/// `branch_only`, the branch the add made and no worktree.
async fn taken_over(branch_only: bool) {
    let (mut c, session, path) = waiting(&format!("worktree-over-{branch_only}")).await;
    if branch_only {
        gits(
            &c,
            &format!("worktree unlock {path}\nworktree remove --force {path}"),
        )
        .await;
    }
    // The first run's wait stays; the next run's comes after its refresh.
    c.f.restart().await;
    c.f.events(&session, "workspace.preparing", 3).await;
    while c.f.clock.pending() < 2 {
        tokio::task::yield_now().await;
    }
    c.f.clock.advance(Duration::from_secs(1));
    assert_eq!(c.f.attach().await.cwd, Path::new(&path));
    assert_eq!(c.f.prompt().await.1, text("hi"));
    let listed = c.git(&["worktree", "list", "--porcelain"], None).await;
    let listed = String::from_utf8(listed).unwrap();
    let locked = format!("branch refs/heads/cupper/x\nlocked cupper session {session}\n");
    assert_eq!(listed.matches("worktree ").count(), 2, "{listed}");
    assert!(listed.contains(&locked), "{listed}");
    let heads = c.git(
        &["for-each-ref", "--format=%(refname)", "refs/heads/cupper/"],
        None,
    );
    assert_eq!(heads.await, b"refs/heads/cupper/x\n");
}

/// A session whose worktree is added and locked, as it waits to refresh it;
/// the worktree's path.
async fn waiting(name: &str) -> (Case, SessionId, String) {
    let c = repo(name).await;
    let session = created(&c, 1, "x").await;
    c.f.events(&session, "workspace.preparing", 2).await;
    while c.f.clock.pending() == 0 {
        tokio::task::yield_now().await;
    }
    let path = c.f.rows("SELECT workspace_path FROM sessions").remove(0);
    (c, session, path)
}

/// A worktree that git still has but whose directory is gone fails, and
/// stays for the user to prune: the first message waits in the queue.
#[tokio::test]
async fn it_worktree_create_fails_once_the_worktree_a_crash_left_is_gone() {
    let (mut c, session, path) = waiting("worktree-gone").await;
    std::fs::remove_dir_all(&path).unwrap();
    c.f.restart().await;
    let events = c.f.events(&session, "workspace.failed", 1).await;
    assert_eq!(
        data(&events, "workspace.failed"),
        [json!({"code": "not_found"})]
    );
    let queue = data(&events, "queue.updated").pop().unwrap();
    assert_eq!(queue["paused"]["reason"], "workspace_failed");
    assert_eq!(queue["entries"][0]["content"], json!(text("hi")));
    let listed = c.git(&["worktree", "list", "--porcelain"], None).await;
    let listed = String::from_utf8(listed).unwrap();
    assert!(listed.contains(&format!("worktree {path}\n")), "{listed}");
}

#[tokio::test]
async fn it_worktree_create_takes_over_the_worktree_a_crash_left() {
    taken_over(false).await;
}

#[tokio::test]
async fn it_worktree_create_takes_over_the_branch_a_crash_left() {
    taken_over(true).await;
}
