//! IT-git-lfs-stage-objects (T235): the LFS objects of the pointers that
//! `git/stage` and `git/revert` write reach the user's store from cupper's,
//! where the snapshot tier's git-lfs put them. Runs the pinned git-lfs
//! (`cargo xtask git-lfs`) and is skipped without it.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use cupper_protocol::session::StopReason;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::hunk_tests::Case;
use super::tests::{done, harness, send};
use crate::driver::api::PromptTag;

/// The user's object of `content`, its mode and its directory's.
fn object(c: &Case, content: &str) -> Option<(String, String, String)> {
    let oid = format!("{:x}", Sha256::digest(content));
    let path = format!(".git/lfs/objects/{}/{}/{oid}", &oid[..2], &oid[2..4]);
    let mode = |path: &Path| fs::metadata(path).map(|m| format!("{:o}", m.permissions().mode()));
    let path = c.path(&path);
    let (file, dir) = (mode(&path).ok()?, mode(path.parent().unwrap()).unwrap());
    Some((fs::read_to_string(path).unwrap(), file, dir))
}

/// `op` of the whole diff of `scope`, made now.
async fn all(c: &Case, op: &str, scope: Value, n: u32) -> Result<Value, Value> {
    let diff = json!({"session_id": c.session(), "scope": scope});
    let token = c.f.ok("git/diff", diff).await["diff_token"].take();
    let params = json!({"command_id": format!("l-{n}"), "session_id": c.session(),
        "diff_token": token, "target": {"all": true}});
    c.f.call(harness(), &format!("git/{op}"), params).await
}

#[tokio::test]
async fn it_git_lfs_stage_objects_reach_the_users_store() {
    let lfs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/git-lfs/git-lfs");
    let Ok(lfs) = lfs.canonicalize() else {
        eprintln!("skipped: no git-lfs; `cargo xtask git-lfs` downloads the pinned one");
        return;
    };
    let mut c = Case::with_lfs("lfs-objects", Some(lfs)).await;
    c.git(&["lfs", "install", "--local"], None).await;
    c.git(&["config", "core.sharedRepository", "group"], None)
        .await;
    c.write(
        ".gitattributes",
        "*.bin filter=lfs diff=lfs merge=lfs -text\n",
    );
    c.write("a.bin", "one\n");
    c.start().await;
    let shared = |content: &str| Some((content.into(), "100660".into(), "40770".into()));

    // Staged, then changed before the user commits: the commit's object is
    // in the user's store, as the shared repository has it.
    c.write("a.bin", "two\n");
    all(&c, "stage", json!("unstaged"), 1).await.unwrap();
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
    all(&c, "revert", json!("last_turn"), 2).await.unwrap();
    assert_eq!(fs::read_to_string(c.path("a.bin")).unwrap(), "start\n");
    assert_eq!(object(&c, "start\n"), shared("start\n"));
}
