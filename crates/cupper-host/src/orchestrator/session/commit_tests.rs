//! IT-git-commit-push: `git/commit` and `git/push` in the user tier (design
//! 5.2.10) on a session's repository and a local bare remote: what a commit
//! leaves, a hook's refusal with its stderr redacted, nothing to commit, a
//! signing program that fails, a push with `--set-upstream`, also to a
//! remote whose name has a `/`, a push the remote rejects, an ssh remote
//! that refuses the key; and `git/status` naming the configuration the
//! session saw change, after a restart too. Nothing leaves the machine: the
//! ssh "remote" is a script.

use cupper_protocol::session::StopReason;
use serde_json::{Value, json};

use super::hunk_tests::Case;
use super::tests::{code, done, harness};
use crate::driver::api::PromptTag;

/// `method` with `params`, a command of its own and the session's.
async fn call(c: &Case, method: &str, n: u32, mut params: Value) -> Result<Value, Value> {
    params["command_id"] = json!(format!("k-{n}"));
    params["session_id"] = json!(c.session());
    c.f.call(harness(), method, params).await
}

/// The user's git, a command per line of `lines`, its words split at blanks.
pub(super) async fn gits(c: &Case, lines: &str) {
    for line in lines.lines().filter(|line| !line.trim().is_empty()) {
        let words: Vec<&str> = line.split_whitespace().collect();
        c.git(&words, None).await;
    }
}

/// A script in git's home that prints `said` to stderr and exits `status`.
fn script(c: &Case, name: &str, said: &str, status: u32) -> String {
    let path = c.home.path().join(name).display().to_string();
    let text = format!("#!/bin/sh\necho '{said}' >&2\nexit {status}\n");
    std::fs::write(&path, text).unwrap();
    c.chmod(&path, 0o755);
    path
}

const MESSAGE: &str = "Add b\n\n加上 b，写在 LC_ALL=C 下。";

#[tokio::test]
async fn it_git_commit_push_runs_as_the_user_does() {
    let mut c = Case::new("commit-push").await;
    c.git(&["checkout", "-q", "-b", "main"], None).await;
    c.write("a.txt", "a\n");
    c.start().await;
    let remote = c.path("../remote.git").display().to_string();
    let other = c.path("../other").display().to_string();
    let setup = format!("init -q --bare {remote}\nremote add origin {remote}");
    gits(&c, &setup).await;
    let commit = |message: &str| json!({"message": message});
    let refused = |done: Result<Value, Value>| code(done.unwrap_err());

    let nothing = call(&c, "git/commit", 1, commit("empty")).await;
    assert_eq!(refused(nothing), "git_error nothing_to_commit");
    c.write("b.txt", "b\n");
    gits(&c, "add b.txt").await;
    let made = call(&c, "git/commit", 2, commit(MESSAGE)).await.unwrap();
    let head = c.git(&["log", "-1", "--format=%H%n%B"], None).await;
    let want = format!("{}\n{MESSAGE}\n\n", made["oid"].as_str().unwrap());
    assert_eq!(String::from_utf8(head).unwrap(), want, "the commit");

    // The repository's hook runs, and refuses; its words come back redacted.
    let said = "see https://user:hunter2@example.invalid/x";
    let hook = script(&c, "pre-commit", said, 1);
    let home = c.home.path().display().to_string();
    c.write("c.txt", "c\n");
    gits(&c, &format!("config core.hooksPath {home}\nadd c.txt")).await;
    let error = call(&c, "git/commit", 3, commit("Add c")).await;
    let error = error.unwrap_err();
    let stderr = error["data"]["stderr"].as_str().unwrap();
    assert!(stderr.contains("example.invalid") && !stderr.contains("hunter2"));
    assert_eq!(code(error), "git_error hook_failed");
    std::fs::remove_file(hook).unwrap();
    // Signing, by a program that fails as gpg does when pinentry has no
    // terminal (Q23).
    let said = "gpg: signing failed: Inappropriate ioctl for device";
    let gpg = script(&c, "gpg", said, 2);
    let signs = format!("config gpg.program {gpg}\nconfig commit.gpgsign true");
    gits(&c, &signs).await;
    let unsigned = call(&c, "git/commit", 4, commit("Add c")).await;
    assert_eq!(refused(unsigned), "git_error signing_failed");
    gits(&c, "config commit.gpgsign false").await;
    call(&c, "git/commit", 5, commit("Add c")).await.unwrap();

    // A new upstream; then a remote whose name has a `/`, as its
    // upstream's does; then one the remote is ahead of.
    let pushed = call(&c, "git/push", 6, json!({"set_upstream": true})).await;
    assert_eq!(pushed, Ok(json!({"remote": "origin", "branch": "main"})));
    let session = json!({"session_id": c.session()});
    let status = c.f.ok("git/status", session.clone()).await;
    let tracked = (&status["upstream"], &status["ahead"]);
    assert_eq!(tracked, (&json!("origin/main"), &json!(0)), "upstream");
    c.write("d.txt", "d\n");
    gits(&c, "remote rename origin my/fork\nadd d.txt").await;
    call(&c, "git/commit", 7, commit("Add d")).await.unwrap();
    let pushed = call(&c, "git/push", 8, json!({"set_upstream": true})).await;
    assert_eq!(pushed, Ok(json!({"remote": "my/fork", "branch": "main"})));
    let theirs = format!(
        "clone -q -b main {remote} {other}
        -C {other} commit -q --allow-empty -m theirs
        -C {other} push -q"
    );
    gits(&c, &theirs).await;
    c.write("e.txt", "e\n");
    gits(&c, "add e.txt").await;
    call(&c, "git/commit", 9, commit("Add e")).await.unwrap();
    let rejected = call(&c, "git/push", 10, json!({})).await;
    assert_eq!(refused(rejected), "git_error rejected");
    // An ssh remote that refuses the key.
    let said = "git@example.invalid: Permission denied (publickey).";
    let ssh = script(&c, "ssh", said, 255);
    let url = "ssh://example.invalid/x.git";
    let via = format!("config core.sshCommand {ssh}\nremote set-url my/fork {url}");
    gits(&c, &via).await;
    let denied = call(&c, "git/push", 11, json!({})).await;
    assert_eq!(refused(denied), "git_error auth_failed");

    // The turn changed the repository's configuration: its notice, and
    // `git/status` naming the file, from the log after a restart too.
    c.f.emit(done(PromptTag(1), StopReason::EndTurn));
    c.f.events(c.session(), "turn.completed", 1).await;
    let config = json!(c.f.project().join(".git/config"));
    for restarted in [false, true] {
        if restarted {
            c.f.restart().await;
        }
        let status = c.f.ok("git/status", session.clone()).await;
        let changed = status["repo_config_changed"].as_array().cloned();
        let changed = changed.unwrap_or_default();
        assert!(changed.contains(&config), "{restarted}: {changed:?}");
    }
}
