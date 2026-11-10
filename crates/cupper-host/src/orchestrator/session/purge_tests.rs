//! IT-snapshot-purge (T127): `host/storage` and `host/purge_snapshots` on
//! IT-orch-lifecycle's fixture, its project a git repository; the gates
//! themselves are the snapshot tier's tests.

use std::fs;
use std::time::Duration;

use cupper_protocol::session::StopReason;
use serde_json::{Value, json};

use super::diff_tests::{diff, refused};
use super::tests::{code, done, send};
use super::trust_tests::git_project;
use crate::driver::api::PromptTag;
use crate::git::purge::Tally;
use crate::policy::testkit::column;

#[tokio::test]
async fn it_snapshot_purge_frees_the_snapshots_and_old_turns_go_missing() {
    let (mut f, home) = git_project("purge").await;
    let (a, data) = (
        f.project().join("a.txt"),
        f.project().parent().unwrap().to_owned(),
    );
    fs::write(&a, "a\n").unwrap();
    let session = f.started().await;
    fs::write(&a, "a2\n").unwrap();
    // Not while a turn runs, nor to the window or the CLI.
    let purge = |id: &str| json!({"command_id": id});
    let running = f.err("host/purge_snapshots", purge("p-1")).await;
    assert_eq!(running, "conflict turn_running");
    for by in ["UI window", "cli + approve"] {
        let refused = f
            .call(column(by), "host/purge_snapshots", purge("p-2"))
            .await;
        assert_eq!(code(refused.unwrap_err()), "forbidden scope", "{by}");
    }
    f.emit(done(PromptTag(1), StopReason::EndTurn));
    f.events(&session, "turn.completed", 1).await;
    let turn = json!({"turn": f.rows("SELECT turn_id FROM turns").remove(0)});
    let (old, made) = diff(&f, &session, turn.clone()).await;
    assert_eq!(old, ["update a.txt +1 -1"]);

    // The sizes; what the native confirmation shows; the purge.
    fs::create_dir_all(data.join("blobs/zz")).unwrap();
    fs::write(data.join("blobs/zz/extra"), [0; 1000]).unwrap();
    let before = f.ok("host/storage", json!({})).await;
    let db = ["", "-wal", "-shm"].map(|end| data.join(format!("host.db{end}")));
    let db: u64 = db
        .iter()
        .filter_map(|p| fs::metadata(p).ok())
        .map(|m| m.len())
        .sum();
    assert_eq!(
        (&before["host_db"], &before["raw_db"]),
        (&json!(db), &json!(0))
    );
    assert_eq!(
        (&before["blobs"], &before["partial"]),
        (&json!(1000), &json!(false))
    );
    let describe = json!({"method": "host/purge_snapshots", "params": purge("p-3")});
    let described = f.ok("policy/describe", describe).await;
    assert_eq!(described["action"], "purge_snapshots");
    assert_eq!(described["details"]["snapshot_bytes"], before["snapshots"]);
    let freed = f.ok("host/purge_snapshots", purge("p-3")).await["freed_bytes"].clone();
    let after = f.ok("host/storage", json!({})).await;
    let (was, now) = (before["snapshots"].as_u64(), after["snapshots"].as_u64());
    assert!(freed.as_u64() > Some(0) && freed.as_u64() == was.zip(now).map(|(b, a)| b - a));

    // The old turn's trees are gone; its diff token went with the cache.
    let missing = refused(&f, &session, turn).await;
    assert_eq!(missing, "unavailable snapshot_missing");
    let revert = |id: &str, token: &Value| {
        json!({"command_id": id, "session_id": session, "diff_token": token,
            "target": {"all": true}})
    };
    let stale = f
        .err("git/revert", revert("r-1", &made["diff_token"]))
        .await;
    assert_eq!(stale, "conflict stale_diff");

    // A new turn snapshots into the new store as before.
    f.ok("session/send", send(&session, "s-1", "next")).await;
    f.prompt().await;
    fs::write(&a, "a3\n").unwrap();
    f.emit(done(PromptTag(2), StopReason::EndTurn));
    f.events(&session, "turn.completed", 2).await;
    let (new, made) = diff(&f, &session, json!("last_turn")).await;
    assert_eq!(new, ["update a.txt +1 -1"]);
    // Its trees going while its diff is cached: the revert finds them gone.
    for repo in fs::read_dir(home.path().join("git")).unwrap() {
        let objects = repo.unwrap().path().join("objects");
        if objects.is_dir() {
            fs::remove_dir_all(&objects).unwrap();
            fs::create_dir(&objects).unwrap();
        }
    }
    let gone = f
        .err("git/revert", revert("r-2", &made["diff_token"]))
        .await;
    assert_eq!(gone, "unavailable snapshot_missing");

    // cupper's worktrees count, under the worktree root; the walk stops
    // once the clock passes its deadline.
    let worktree = data.join("worktrees/app-0/w");
    fs::create_dir_all(&worktree).unwrap();
    fs::write(worktree.join("f"), [0; 300]).unwrap();
    let moved = "UPDATE sessions SET workspace_kind = 'worktree', workspace_path = ?1";
    f.exec(moved, worktree.display().to_string());
    assert_eq!(f.ok("host/storage", json!({})).await["worktrees"], 300);
    let deadline = crate::clock::Clock::now_mono(&f.clock) + Duration::from_secs(5);
    let mut tally = Tally {
        clock: &f.clock,
        deadline,
        partial: false,
    };
    assert_eq!(
        (tally.bytes([worktree.clone()]), tally.partial),
        (300, false)
    );
    f.clock.advance(Duration::from_secs(5));
    assert_eq!((tally.bytes([worktree]), tally.partial), (0, true));
}
