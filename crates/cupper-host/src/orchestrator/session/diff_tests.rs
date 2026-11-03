//! IT-git-diff-scopes and IT-git-file-diff: `git/diff` of every range and
//! `git/file_diff` (design 5.2.10) on IT-orch-lifecycle's fixture, its
//! project a git repository and its sessions running the git on PATH. Scopes:
//! a turn while it runs and once it ended, `unstaged`, `staged` before and
//! after the first commit, and the errors of a turn without its snapshots
//! and of an unmerged index. File diffs: the whole path from a diff's token
//! to a file's units, made again from the cached trees; the kinds of units
//! themselves are `git::diff::hunks`' unit tests.

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

/// A mode change with two hunks, a rename, a long new file and a filtered one.
const EDITED: &str = "seq 1 20 | sed 's/^2$/two/; s/^18$/eighteen/' > a.txt; chmod +x a.txt; \
    mv m.txt n.txt; seq 1 3000 > long.txt; printf 's2\\n' > s.txt";

#[tokio::test]
async fn it_git_file_diff_units_of_a_diff_by_its_token() {
    let (mut f, home) = git_project("file-diff").await;
    sh(
        &f,
        &home,
        &format!("{ADDED}; seq 1 20 > a.txt; git add -A; {COMMIT}"),
    )
    .await;
    let session = f.started().await;
    sh(&f, &home, EDITED).await;
    let token = diff(&f, &session, json!("unstaged")).await.1["diff_token"].take();
    let file_diff = |token: &Value, path: &str| {
        let params = json!({"session_id": session, "diff_token": token, "path": path});
        f.call(harness(), "git/file_diff", params)
    };
    let units = |result: &Value| {
        let hunks = result["hunks"].as_array().unwrap().iter();
        let unit = |h: &Value| {
            format!(
                "{} {}",
                h["unit"].as_str().unwrap(),
                h["header"].as_str().unwrap()
            )
        };
        hunks.map(unit).collect::<Vec<_>>()
    };
    let a = file_diff(&token, "a.txt").await.unwrap();
    let mode = "mode old mode 100644\nnew mode 100755";
    assert_eq!(
        units(&a),
        [mode, "hunk @@ -1,5 +1,5 @@", "hunk @@ -15,6 +15,6 @@"]
    );
    assert_eq!(
        a["hunks"][1]["lines"],
        json!([" 1", "-2", "+two", " 3", " 4", " 5"])
    );
    assert_eq!(
        file_diff(&token, "a.txt").await.unwrap(),
        a,
        "the same from the same trees"
    );
    // A rename: its source's deletion and its addition, whole files.
    let renamed = units(&file_diff(&token, "n.txt").await.unwrap());
    let gone = "whole_file deleted file mode 100644\n@@ -1,20 +0,0 @@";
    assert_eq!(
        renamed,
        [gone, "whole_file new file mode 100644\n@@ -0,0 +1,20 @@"]
    );
    // Lines past the inline limit come as a blob.
    let long = &file_diff(&token, "long.txt").await.unwrap()["hunks"][0];
    assert!(long.get("lines").is_none());
    let blob = String::from_utf8(f.blob(&long["lines_blob"]["hash"]).await).unwrap();
    assert_eq!((blob.lines().count(), &blob[..6]), (3000, "+1\n+2\n"));
    // A file under a filter the host does not run has nothing to act on.
    assert_eq!(
        file_diff(&token, "s.txt").await.unwrap()["hunks"],
        json!([])
    );

    let refused = async |token: &Value, path| code(file_diff(token, path).await.unwrap_err());
    assert_eq!(refused(&token, "b.txt").await, "not_found ");
    assert_eq!(
        refused(&json!("dt_0"), "a.txt").await,
        "conflict stale_diff"
    );
    f.clock.advance(Duration::from_secs(600));
    assert_eq!(refused(&token, "a.txt").await, "conflict stale_diff");
}
