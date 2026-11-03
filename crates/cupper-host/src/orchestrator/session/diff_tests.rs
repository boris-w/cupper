//! IT-git-diff-scopes: `git/diff` of every range (design 5.2.10) on
//! IT-orch-lifecycle's fixture, its project a git repository and its
//! sessions running the git on PATH: a turn while it runs and once it
//! ended, `unstaged`, `staged` before and after the first commit, and the
//! errors of a turn without its snapshots and of an unmerged index.

use std::time::Duration;

use cupper_protocol::ids::SessionId;
use cupper_protocol::session::StopReason;
use cupper_sys::{Command, Env};
use serde_json::{Value, json};

use super::tests::{Fixture, code, done, harness};
use super::trust_tests::git_project;
use crate::clock::ManualClock;
use crate::driver::api::PromptTag;
use crate::git::run::run;
use crate::store::testutil::TempDir;

/// Runs `script` in the project, with git's home in `home`.
async fn sh(f: &Fixture, home: &TempDir, script: &str) {
    let mut env = Env::new();
    env.set("HOME", home.path()).unwrap();
    env.set("GIT_CONFIG_NOSYSTEM", "1").unwrap();
    env.set("PATH", std::env::var_os("PATH").unwrap()).unwrap();
    let mut sh = Command::new("/bin/sh");
    let script = format!("set -e\n{script}");
    sh.args(["-c", &script]).current_dir(f.project()).env(env);
    let out = run(&ManualClock::new(), sh, None, Duration::from_secs(30)).await;
    let out = out.unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{script}: {err}");
}

/// The project before the session: files in the index, one of them under a
/// filter the snapshot tier does not run; the first commit; the agent's changes.
const ADDED: &str = "printf 'a\\n' > a.txt; printf 'b\\n' > b.txt; seq 1 20 > m.txt; \
    printf 's\\n' > s.txt; printf 's.txt filter=crypt\\n' > .gitattributes; \
    git config filter.crypt.clean cat; git add -A";
const COMMIT: &str = "git -c user.name=t -c user.email=t@example.invalid commit -qm init";
const CHANGES: &str = "printf 'a2\\n' >> a.txt; rm b.txt; mv m.txt n.txt; printf '\\0\\1' > c.bin; \
    printf 's2\\n' > s.txt";
/// Two stages of `u.txt` in the index, as a merge conflict leaves it.
const CONFLICT: &str = "o=$(git hash-object -w a.txt); \
    printf '100644 %s 1\\tu.txt\\n100644 %s 2\\tu.txt\\n' $o $o | git update-index --index-info";

/// `git/diff` of `scope`, its files shown as `<change> <path>[ <- <old>]
/// +<added> -<deleted>[ binary][ filter]`.
async fn diff(f: &Fixture, session: &SessionId, scope: Value) -> (Vec<String>, Value) {
    let params = json!({"session_id": session, "scope": scope});
    let mut result = f.ok("git/diff", params).await;
    let files = result["files"].take();
    let shown = files.as_array().unwrap().iter().map(|file| {
        let text = |key: &str| file[key].as_str().unwrap_or_default();
        let (change, path, old) = (text("change"), text("path"), text("old_path"));
        let to = if old.is_empty() { "" } else { " <- " };
        let flag = |key: &str, name| if file[key] == true { name } else { "" };
        let binary = flag("binary", " binary");
        let filter = flag("filter_unsupported", " filter");
        let counts = format!("+{} -{}", file["additions"], file["deletions"]);
        format!("{change} {path}{to}{old} {counts}{binary}{filter}")
    });
    (shown.collect(), result)
}

/// The error `git/diff` of `scope` fails with.
async fn refused(f: &Fixture, session: &SessionId, scope: Value) -> String {
    let params = json!({"session_id": session, "scope": scope});
    code(f.call(harness(), "git/diff", params).await.unwrap_err())
}

#[tokio::test]
async fn it_git_diff_scopes_turns_the_work_tree_the_index_and_head() {
    let (mut f, home) = git_project("diff-scopes").await;
    sh(&f, &home, ADDED).await;
    let session = f.started().await;
    // `staged` before the first commit: the index against the empty tree.
    let (staged, _) = diff(&f, &session, json!("staged")).await;
    assert_eq!(staged.len(), 5);
    assert_eq!(staged[3..], ["add m.txt +20 -0", "add s.txt +1 -0 filter"]);
    sh(&f, &home, COMMIT).await;

    // The agent's changes, while its turn runs: against a snapshot taken now.
    sh(&f, &home, CHANGES).await;
    let changes = [
        "update a.txt +1 -0",
        "delete b.txt +0 -1",
        "add c.bin +0 -0 binary",
        "update n.txt <- m.txt +0 -0",
        // A placeholder on either side: changed, without its content.
        "update s.txt +1 -1 filter",
    ];
    let (files, running) = diff(&f, &session, json!("last_turn")).await;
    assert_eq!(files, changes);
    let excluded = json!([{"path": "s.txt", "reason": "filter_unsupported"}]);
    let snapshot = json!({"status": "partial", "excluded": excluded});
    assert_eq!(running["snapshot"], snapshot);
    let stat = json!({"files": 5, "added": 2, "removed": 2});
    assert_eq!(running["stat"], stat);
    assert_eq!(running["caveats"], json!(["mixed_changes"]));

    // Once it ended: its two snapshots, by `last_turn` or by its ID.
    f.emit(done(PromptTag(1), StopReason::EndTurn));
    f.events(&session, "turn.completed", 1).await;
    let turn = f.rows("SELECT turn_id FROM turns").remove(0);
    for scope in [json!("last_turn"), json!({"turn": turn})] {
        let (files, ended) = diff(&f, &session, scope).await;
        assert_eq!(files, changes);
        assert_eq!(ended["snapshot"], snapshot);
        assert_ne!(ended["diff_token"], running["diff_token"]);
    }

    // The index against the work tree, untracked files in; HEAD against the index.
    let (unstaged, result) = diff(&f, &session, json!("unstaged")).await;
    assert_eq!(unstaged, changes);
    assert!(result["caveats"] == json!([]) && result.get("snapshot").is_none());
    assert!(diff(&f, &session, json!("staged")).await.0.is_empty());
    sh(&f, &home, "git add a.txt c.bin").await;
    let (staged, _) = diff(&f, &session, json!("staged")).await;
    assert_eq!(staged, [changes[0], changes[2]]);
    let (unstaged, _) = diff(&f, &session, json!("unstaged")).await;
    assert_eq!(unstaged, [changes[1], changes[3], changes[4]]);

    // A turn without its end snapshot, one that is not there, and an index
    // in the middle of a merge conflict.
    let ended = "UPDATE turns SET snapshot_end = NULL WHERE turn_id = ?1";
    f.exec(ended, turn.clone());
    let missing = refused(&f, &session, json!({"turn": turn})).await;
    assert_eq!(missing, "unavailable snapshot_missing");
    let unknown = refused(&f, &session, json!({"turn": "t_0"})).await;
    assert_eq!(unknown, "not_found ");
    sh(&f, &home, CONFLICT).await;
    for scope in ["unstaged", "staged"] {
        let unmerged = refused(&f, &session, json!(scope)).await;
        assert_eq!(unmerged, "unavailable index_unmerged", "{scope}");
    }
}
