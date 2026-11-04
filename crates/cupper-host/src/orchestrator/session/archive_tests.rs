//! IT-session-archive (PS-3, GT-5): `session/archive` (design 5.2.6,
//! 5.5.9) of sessions in worktrees: kept; removed, its branch left;
//! blocked by a cleanup rule; removed though dirty by case alone; gone
//! already, unregistered or registered. Refused while a turn runs, as an
//! archived session's writes are; and `session/rename`,
//! `session/list_archived` and `project/remove`.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use cupper_protocol::ids::{ProjectId, SessionId};
use cupper_protocol::session::StopReason;
use serde_json::{Value, json};

use super::commit_tests::gits;
use super::hunk_tests::Case;
use super::tests::{create, done, send};
use super::worktree_tests::{created, repo};
use crate::clock::Clock;

/// Session `n` in a new worktree named `hint`, its first turn ended; the
/// worktree's path.
pub(super) async fn ended(c: &mut Case, n: u32, hint: &str) -> (SessionId, String) {
    let session = created(c, n, hint).await;
    c.f.events(&session, "workspace.preparing", 2).await;
    let refresh = |c: &Case| Some(c.f.clock.now_mono() + Duration::from_secs(1));
    while c.f.clock.next_deadline() != refresh(c) {
        tokio::task::yield_now().await;
    }
    c.f.clock.advance(Duration::from_secs(1));
    let cwd = c.f.attach().await.cwd;
    let (tag, _) = c.f.prompt().await;
    c.f.emit(done(tag, StopReason::EndTurn));
    c.f.events(&session, "turn.completed", 1).await;
    (session, cwd.display().to_string())
}

/// `more` as the command `a-<n>` of `session`.
pub(super) fn on(session: &SessionId, n: &str, mut more: Value) -> Value {
    (more["command_id"], more["session_id"]) = (json!(format!("a-{n}")), json!(session));
    more
}

#[tokio::test]
async fn it_session_archive_keeps_removes_and_is_kept_from_work() {
    let mut c = repo("archive").await;
    let hints = [
        "kept", "removed", "blocked", "cased", "unlisted", "gone", "unsure",
    ];
    let mut made = HashMap::new();
    for (n, hint) in (1..).zip(hints) {
        made.insert(hint, ended(&mut c, n, hint).await);
    }
    let path = |hint: &str| made[hint].1.clone();
    let text = |out: Vec<u8>| String::from_utf8(out).unwrap().trim().to_owned();
    let a = text(c.git(&["rev-parse", "main:a.txt"], None).await);
    let b = text(c.git(&["hash-object", "-w", "--stdin"], Some(b"b\n")).await);
    c.write(&format!("{}/new.txt", path("blocked")), "new\n");
    // Dirty by case alone on a case-insensitive file system, as macOS's
    // is: removed once confirmed, and not before (T125's probe).
    for (hint, p) in [("cased", path("cased")), ("unsure", path("unsure"))] {
        let cased = format!(
            "-C {p} update-index --add --cacheinfo 100644,{a},C.txt
            -C {p} update-index --add --cacheinfo 100644,{b},c.txt
            -C {p} commit -qm cased
            -C {p} branch {hint}-keep
            -C {p} checkout -qf -- ."
        );
        gits(&c, &cased).await;
    }
    let insensitive = Path::new(&path("unsure")).join("c.TXT").exists();
    let u = path("unlisted");
    gits(
        &c,
        &format!("worktree unlock {u}\nworktree remove --force {u}"),
    )
    .await;
    std::fs::remove_dir_all(path("gone")).unwrap();
    let kept = made["kept"].0.clone();
    c.f.ok("session/rename", on(&kept, "r", json!({"title": "mine"})))
        .await;

    // A turn runs in a session of the project's own.
    let local = c.f.ok("session/create", create("c-local")).await;
    let local: SessionId = serde_json::from_value(local["session_id"].clone()).unwrap();
    c.f.attach().await;
    let (tag, _) = c.f.prompt().await;
    let busy = c.f.err(
        "session/archive",
        on(&local, "busy", json!({"worktree": "keep"})),
    );
    assert_eq!(busy.await, "conflict turn_running");
    let project = json!({"command_id": "p-1", "project_id": ProjectId::from_uuid(1)});
    let refused = c.f.err("project/remove", project.clone()).await;
    assert_eq!(refused, "conflict turn_running");

    let asked = "keep remove remove remove_case_only_dirty remove remove remove".split(' ');
    let unsure = if insensitive { "blocked" } else { "removed" };
    let outcomes = [
        "kept", "removed", "blocked", "removed", "removed", "kept", unsure,
    ];
    for ((hint, asked), outcome) in hints.iter().zip(asked).zip(outcomes) {
        let archive = on(&made[hint].0, hint, json!({"worktree": asked}));
        let archived = c.f.ok("session/archive", archive).await;
        assert_eq!(archived["worktree"], outcome, "{hint}");
        let rule = if *hint == "unsure" {
            "modified_files"
        } else {
            "untracked_files"
        };
        let blocked = (outcome == "blocked").then(|| json!([rule]));
        assert_eq!(archived.get("blockers").cloned(), blocked, "{hint}");
        let there = Path::new(&path(hint)).exists();
        let stays = ["kept", "blocked"].contains(hint) || *hint == "unsure" && insensitive;
        assert_eq!(there, stays, "{hint}");
    }
    let heads = ["for-each-ref", "--format=%(refname)", "refs/heads/cupper/"];
    let heads = text(c.git(&heads, None).await);
    assert_eq!(heads.lines().count(), 7, "every branch stays");
    let again = c.f.err(
        "session/archive",
        on(&kept, "k", json!({"worktree": "keep"})),
    );
    assert_eq!(again.await, "unavailable session_archived");
    let sent = c.f.err("session/send", send(&kept, "s-1", "more")).await;
    assert_eq!(sent, "unavailable session_archived");
    let got = c.f.ok("session/get", json!({"session_id": kept})).await;
    let title = (&got["summary"]["title"], &got["summary"]["title_source"]);
    assert_eq!(title, (&json!("mine"), &json!("user")));
    assert!(got["archived_at"].is_u64());

    // The turn ends: the project goes, its last session archived with it.
    c.f.emit(done(tag, StopReason::EndTurn));
    c.f.events(&local, "turn.completed", 1).await;
    c.f.ok("project/remove", project).await;
    let listed = c.f.ok("session/list_archived", json!({})).await;
    let listed = listed["sessions"].as_array().unwrap().clone();
    assert_eq!(listed.len(), 8);
    assert!(
        listed
            .iter()
            .any(|s| s["summary"]["session_id"] == json!(local))
    );
    let after = &listed[2];
    let cursor =
        json!({"archived_at": after["archived_at"], "session_id": after["summary"]["session_id"]});
    let rest =
        c.f.ok("session/list_archived", json!({"cursor": cursor}))
            .await;
    assert_eq!(rest["sessions"].as_array().unwrap()[..], listed[3..]);
    assert!(rest.get("next").is_none());
}
