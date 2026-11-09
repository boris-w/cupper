//! IT-git-lfs-stage-objects (T235) and IT-git-lfs-repo-config (T237), with
//! the pinned git-lfs (`cargo xtask git-lfs`); skipped without it.

use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use cupper_protocol::ids::{ProjectId, SessionId};
use cupper_protocol::session::StopReason;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

use super::commit_tests::gits;
use super::hunk_tests::Case;
use super::tests::{code, done, harness, send};
use crate::driver::api::PromptTag;
use crate::orchestrator::testkit::text;

/// The user's object of `content`, its mode and its directory's.
fn object(c: &Case, content: &str) -> Option<(String, String, String)> {
    let oid = format!("{:x}", Sha256::digest(content));
    let path = format!(".git/lfs/objects/{}/{}/{oid}", &oid[..2], &oid[2..4]);
    let mode = |path: &Path| fs::metadata(path).map(|m| format!("{:o}", m.permissions().mode()));
    let path = c.path(&path);
    let (file, dir) = (mode(&path).ok()?, mode(path.parent().unwrap()).unwrap());
    Some((fs::read_to_string(path).unwrap(), file, dir))
}

/// `op` of `target` of the diff of `scope`, made now.
async fn apply(c: &Case, op: &str, scope: Value, target: Value, n: u32) -> Result<Value, Value> {
    let diff = json!({"session_id": c.session(), "scope": scope});
    let token = c.f.ok("git/diff", diff).await["diff_token"].take();
    let params = json!({"command_id": format!("l-{n}"), "session_id": c.session(),
        "diff_token": token, "target": target});
    c.f.call(harness(), &format!("git/{op}"), params).await
}

/// A case whose user and host run the pinned git-lfs, with the repository
/// set up for it; `None` without git-lfs.
async fn lfs_case(name: &str) -> Option<Case> {
    let lfs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/git-lfs/git-lfs");
    let Ok(lfs) = lfs.canonicalize() else {
        eprintln!("skipped: no git-lfs; `cargo xtask git-lfs` downloads the pinned one");
        return None;
    };
    let c = Case::with_lfs(name, Some(lfs)).await;
    gits(&c, "checkout -q -b main\nlfs install --local").await;
    c.write(
        ".gitattributes",
        "*.bin filter=lfs diff=lfs merge=lfs -text\n",
    );
    Some(c)
}

#[tokio::test]
async fn it_git_lfs_stage_objects_reach_the_users_store() {
    let Some(mut c) = lfs_case("lfs-objects").await else {
        return;
    };
    c.git(&["config", "core.sharedRepository", "group"], None)
        .await;
    c.write("a.bin", "one\n");
    c.start().await;
    let shared = |content: &str| Some((content.into(), "100660".into(), "40770".into()));

    // Staged, then changed before the user commits: the commit's object is
    // in the user's store, as the shared repository has it.
    c.write("a.bin", "two\n");
    apply(&c, "stage", json!("unstaged"), json!({"all": true}), 1)
        .await
        .unwrap();
    assert_eq!(object(&c, "two\n"), shared("two\n"));
    c.write("a.bin", "three\n");
    c.git(&["commit", "-qm", "staged"], None).await;
    c.git(&["lfs", "fsck", "--objects"], None).await;

    // A turn reverted: the work tree goes back to what the turn started
    // from, whose object only cupper had. There is no LFS server to ask.
    c.write("a.bin", "start\n");
    c.f.emit(done(PromptTag(1), StopReason::EndTurn));
    c.f.events(c.session(), "turn.completed", 1).await;
    c.f.ok("session/send", send(c.session(), "s-1", "next"))
        .await;
    let (tag, _) = c.f.prompt().await;
    c.write("a.bin", "agent\n");
    c.f.emit(done(tag, StopReason::EndTurn));
    c.f.events(c.session(), "turn.completed", 2).await;
    assert_eq!(object(&c, "start\n"), None);
    apply(&c, "revert", json!("last_turn"), json!({"all": true}), 2)
        .await
        .unwrap();
    assert_eq!(fs::read_to_string(c.path("a.bin")).unwrap(), "start\n");
    assert_eq!(object(&c, "start\n"), shared("start\n"));
}

/// Removes the object of `content` from the user's store, and cupper's
/// stores of LFS objects.
fn forget(c: &Case, content: &str) {
    let oid = format!("{:x}", Sha256::digest(content));
    let object = format!(".git/lfs/objects/{}/{}/{oid}", &oid[..2], &oid[2..4]);
    fs::remove_file(c.path(&object)).unwrap();
    for repo in fs::read_dir(c.home.path().join("git")).unwrap().flatten() {
        for tree in fs::read_dir(repo.path().join("ws")).into_iter().flatten() {
            let _ = fs::remove_dir_all(tree.unwrap().path().join("lfs"));
        }
    }
}

/// `session/create` in a new worktree from `main`: its `workspace.failed`.
async fn worktree_failed(c: &Case, n: u32) -> Value {
    let worktree = json!({"kind": "worktree", "base_ref": "main", "branch_hint": "w"});
    let create = json!({"command_id": format!("w-{n}"), "agent": "claude", "workspace": worktree,
        "project_id": ProjectId::from_uuid(1), "message": text("hi")});
    let created = c.f.ok("session/create", create).await;
    let session: SessionId = serde_json::from_value(created["session_id"].clone()).unwrap();
    let events = c.f.events(&session, "workspace.failed", 1).await;
    let failed = events.iter().find(|b| b.tag() == "workspace.failed");
    json!(failed.unwrap())["data"].clone()
}

/// A local endpoint on 127.0.0.1 that answers each connection 404: how
/// many it had.
async fn endpoint() -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/lfs", listener.local_addr().unwrap());
    let hits = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&hits);
    let serving = tokio::spawn(async move {
        while let Ok((mut conn, _)) = listener.accept().await {
            count.fetch_add(1, Ordering::SeqCst);
            let answer = "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";
            let _ = conn.write_all(answer.as_bytes()).await;
        }
    });
    (url, hits, serving)
}

/// IT-git-lfs-repo-config. The network: a committed `.lfsconfig` names a
/// local endpoint and leaves `b.bin` out of smudges. The user's git writes
/// `b.bin`'s pointer and asks the endpoint for an object its store lacks;
/// the host restores `b.bin`'s content and refuses the revert and the
/// worktree that would download, `lfs_object_missing`. Programs: with an
/// extension and a standalone transfer agent configured, the user's git
/// runs both; the host runs neither, and LFS is a filter it does not
/// support. git-lfs never installs its hooks for the host.
#[tokio::test]
async fn it_git_lfs_repo_config_downloads_nothing_and_runs_no_program() {
    let Some(mut c) = lfs_case("lfs-repo-config").await else {
        return;
    };
    let (url, hits, serving) = endpoint().await;
    let lfsconfig = format!("[lfs]\n\turl = {url}\n\tfetchexclude = b.bin\n");
    c.write(".lfsconfig", lfsconfig);
    c.write("a.bin", "one\n");
    c.write("b.bin", "two\n");
    c.start().await;
    fs::remove_file(c.path("b.bin")).unwrap();
    c.git(&["checkout", "--", "b.bin"], None).await;
    let b = fs::read_to_string(c.path("b.bin")).unwrap();
    assert!(b.starts_with("version https://git-lfs"), "control: {b}");
    forget(&c, "one\n");
    fs::remove_file(c.path("a.bin")).unwrap();
    c.try_git(&["checkout", "--", "a.bin"], None).await;
    let asked = hits.load(Ordering::SeqCst);
    assert!(asked > 0, "control: no request from the user's git-lfs");

    c.write("b.bin", "changed\n");
    let b = json!({"paths": ["b.bin"]});
    apply(&c, "revert", json!("unstaged"), b, 1).await.unwrap();
    let b = fs::read_to_string(c.path("b.bin")).unwrap();
    assert_eq!(b, "two\n", "network: the revert wrote the pointer");
    c.write("a.bin", "changed\n");
    let a = json!({"paths": ["a.bin"]});
    let refused = apply(&c, "revert", json!("unstaged"), a, 2).await;
    let missing = "git_error lfs_object_missing";
    assert_eq!(code(refused.unwrap_err()), missing, "network: revert");
    let missing = json!({"code": "git_error", "reason": "lfs_object_missing"});
    assert_eq!(worktree_failed(&c, 3).await, missing, "network: worktree");
    let listed = String::from_utf8(c.git(&["worktree", "list"], None).await).unwrap();
    let worktrees = listed.lines().count();
    assert_eq!(worktrees, 1, "network: a worktree is left: {listed}");
    let host = hits.load(Ordering::SeqCst) - asked;
    assert_eq!(host, 0, "network: the host asked the endpoint");
    serving.abort();

    let marks = c.home.path().join("marks");
    let to = marks.display();
    for name in ["extension", "agent"] {
        let body = format!("#!/bin/sh\necho {name} >> '{to}'\n/bin/cat\n");
        fs::write(c.home.path().join(name), body).unwrap();
        c.chmod(&c.home.path().join(name).display().to_string(), 0o755);
    }
    let home = c.home.path().display();
    let config = format!(
        "config lfs.extension.x.clean {home}/extension\nconfig lfs.extension.x.priority 0
        config lfs.extension.x.smudge {home}/extension\nconfig lfs.customtransfer.x.path {home}/agent
        config lfs.standalonetransferagent x"
    );
    gits(&c, &config).await;
    let ran = || fs::read_to_string(&marks).unwrap_or_default();
    c.write("a.bin", "three\n");
    gits(&c, "add a.bin\nreset -q").await;
    fs::remove_file(c.path("a.bin")).unwrap();
    c.try_git(&["checkout", "--", "a.bin"], None).await;
    let seen: BTreeSet<String> = ran().lines().map(str::to_owned).collect();
    let both = BTreeSet::from(["agent".into(), "extension".into()]);
    assert_eq!(seen, both, "control: the user's git-lfs ran");
    fs::remove_file(&marks).unwrap();
    c.write("a.bin", "four\n");
    let diff = json!({"session_id": c.session(), "scope": "unstaged"});
    let diff = c.f.ok("git/diff", diff).await;
    let lfs = &diff["files"][0]["filter_unsupported"];
    assert_eq!(lfs, true, "programs: the snapshot ran LFS: {diff}");
    c.f.ok("git/status", json!({"session_id": c.session()}))
        .await;
    let unsupported = json!({"code": "git_error", "reason": "filter_unsupported"});
    let failed = worktree_failed(&c, 4).await;
    assert_eq!(failed, unsupported, "programs: worktree");
    assert_eq!(ran(), "", "programs: the host ran the repository's");
    let hooks = fs::read_dir(c.home.path().join("git/empty-hooks")).unwrap();
    assert_eq!(hooks.count(), 0, "git-lfs installed its hooks for the host");
}
