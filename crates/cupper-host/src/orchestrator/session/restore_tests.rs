//! IT-session-restore (PS-3): `session/restore` (design 5.2.6) of archived
//! sessions in worktrees: the worktree kept, or added again from its branch;
//! read-only once the branch is gone; and one git lists without its
//! directory added again with `--force`, unless a lock not the session's or
//! another worktree with its branch out keeps it read-only. A read-only
//! session refuses writes.

use std::pin::pin;
use std::time::Duration;

use cupper_protocol::ids::SessionId;
use serde_json::{Value, json};

use super::archive_tests::{ended, on};
use super::commit_tests::gits;
use super::tests::{harness, send};
use super::worktree_tests::repo;
use crate::clock::Clock;
use crate::orchestrator::session::hunk_tests::Case;

/// `session/restore` of `session`, the clock moved on for the refresh of a
/// worktree added again.
async fn restored(c: &Case, session: &SessionId, n: &str) -> Value {
    let mut restore = pin!(c.f.call(harness(), "session/restore", on(session, n, json!({}))));
    let refresh = || Some(c.f.clock.now_mono() + Duration::from_secs(1));
    loop {
        tokio::select! {
            restored = &mut restore => return restored.unwrap(),
            () = tokio::task::yield_now() => if c.f.clock.next_deadline() == refresh() {
                c.f.clock.advance(Duration::from_secs(1));
            },
        }
    }
}

#[tokio::test]
async fn it_session_restore_brings_the_worktree_back_or_reads_only() {
    let mut c = repo("restore").await;
    let hints = "kept removed branchless gone foreign elsewhere".split(' ');
    let hints: Vec<&str> = hints.collect();
    let mut made = Vec::new();
    for (n, hint) in (1..).zip(&hints) {
        made.push(ended(&mut c, n, hint).await);
    }
    let other = c.path("../other").display().to_string();
    let setup = format!(
        "worktree unlock {f}\nworktree lock --reason mine {f}
        worktree add -q --force {other} cupper/elsewhere",
        f = made[4].1,
    );
    for (hint, (session, path)) in hints.iter().zip(&made) {
        if ["gone", "foreign", "elsewhere"].contains(hint) {
            std::fs::remove_dir_all(path).unwrap();
        }
        let worktree = if *hint == "kept" { "keep" } else { "remove" };
        c.f.ok(
            "session/archive",
            on(session, hint, json!({"worktree": worktree})),
        )
        .await;
    }
    gits(&c, &format!("{setup}\nbranch -D cupper/branchless")).await;
    let listed = c.git(&["worktree", "list", "--porcelain"], None).await;
    let before = String::from_utf8(listed).unwrap();

    let modes = "full full read_only full read_only read_only".split(' ');
    for ((hint, (session, path)), mode) in hints.iter().zip(&made).zip(modes) {
        let got = restored(&c, session, &format!("r-{hint}")).await;
        assert_eq!(got["mode"], mode, "{hint}");
        let there = std::path::Path::new(path).is_dir();
        assert_eq!(there, mode == "full", "{hint}");
        let get = c.f.ok("session/get", json!({"session_id": session})).await;
        assert!(get.get("archived_at").is_none(), "{hint}");
        assert_eq!(get["summary"]["read_only"], mode == "read_only", "{hint}");
    }
    let listed = c.git(&["worktree", "list", "--porcelain"], None).await;
    let listed = String::from_utf8(listed).unwrap();
    // Each worktree there again, once, locked for its session.
    for (session, path) in [0, 1, 3].map(|i| &made[i]) {
        let entry = format!("worktree {path}\nHEAD ");
        let locked = format!("locked cupper session {session}\n");
        assert_eq!(listed.matches(&entry).count(), 1, "{listed}");
        assert!(listed.contains(&locked), "{listed}");
    }
    assert_eq!(
        listed.matches("worktree ").count(),
        before.matches("worktree ").count() + 1
    );
    let sent =
        c.f.err("session/send", send(&made[2].0, "s-1", "more"))
            .await;
    assert_eq!(sent, "unavailable session_read_only");
}
