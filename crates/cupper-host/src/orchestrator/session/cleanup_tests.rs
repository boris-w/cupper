//! IT-worktree-check-rules: `worktree/check` (design 5.2.10, ADR-10's
//! cleanup rules) on a session's worktree, each rule kept by one change to
//! it and let go once the change is undone; the ignored files and the files
//! dirty only by case are worth a warning, no more. Paths that differ in
//! case alone collide only on a case-insensitive file system: on another,
//! that change leaves the worktree clean.

use std::path::Path;
use std::time::Duration;

use serde_json::json;

use super::tests::{code, create, harness};
use super::worktree_tests::waiting;

/// `<label> | <actions> | <blockers, or -> <ignored files> <case only> |
/// <undo>`. An action is `git` with its words, run in the worktree; `write`
/// a file of the worktree or `drop` a directory. `@git` is the worktree's
/// git dir, `@a` and `@b` two blobs, `@ours` the session's lock.
const CASES: &str = "
    clean | | - 0 false |
    modified | write a.txt changed | modified_files 0 false | git checkout -- a.txt
    staged | write b.txt b; git add b.txt | modified_files 0 false | git rm -q --cached b.txt; git clean -fq
    untracked | write c.txt c | untracked_files 0 false | git clean -fq
    ignored | write x.log x | - 1 false | git clean -fqX
    in progress | git bisect start | operation_in_progress 0 false | git bisect reset
    foreign lock | git worktree unlock .; git worktree lock --reason mine . | foreign_lock 0 false | git worktree unlock .; @ours
    head only | git commit -q --allow-empty -m mine | head_only_commits 0 false | git reset -q --hard HEAD~1
    submodule | write @git/modules/sub/HEAD x | initialized_submodules 0 false | drop @git/modules
    case only | git update-index --add --cacheinfo 100644,@a,C.txt; git update-index --add --cacheinfo 100644,@b,c.txt; git commit -qm cased; git branch cased; git checkout -qf -- . | - 0 true | git branch -qD cased; git reset -q --hard HEAD~1
    unpushed | git init -q --bare ../remote.git; git remote add origin ../remote.git; git push -q origin main; git commit -q --allow-empty -m mine; git branch keep | unpushed_commits 0 false | git branch -qD keep; git reset -q --hard HEAD~1";

#[tokio::test]
async fn it_worktree_check_rules_keep_a_worktree_with_work_in_it() {
    let (mut c, session, wt) = waiting("cleanup").await;
    c.f.clock.advance(Duration::from_secs(1));
    c.f.attach().await;
    let text = |out: Vec<u8>| String::from_utf8(out).unwrap().trim().to_owned();
    let git_dir = text(
        c.git(&["-C", &wt, "rev-parse", "--absolute-git-dir"], None)
            .await,
    );
    let a = text(c.git(&["rev-parse", "main:a.txt"], None).await);
    let b = text(c.git(&["hash-object", "-w", "--stdin"], Some(b"b\n")).await);
    c.write(".git/info/exclude", "*.log\n");
    let ours = format!("cupper session {session}");
    let run = async |actions: &str| {
        let actions = actions
            .replace("@git", &git_dir)
            .replace("@a", &a)
            .replace("@b", &b);
        for action in actions.split(';').map(str::trim).filter(|a| !a.is_empty()) {
            let words: Vec<&str> = action.split_whitespace().collect();
            let lock = ["-C", &wt, "worktree", "lock", "--reason", &ours, "."];
            match words[..] {
                ["write", path, text] => {
                    c.write(&Path::new(&wt).join(path).display().to_string(), text)
                }
                ["drop", path] => std::fs::remove_dir_all(path).unwrap(),
                ["@ours"] => drop(c.git(&lock, None).await),
                _ => drop(c.git(&[&["-C", &wt][..], &words[1..]].concat(), None).await),
            }
        }
    };
    let probe = Path::new(&wt).join("Case-Probe");
    std::fs::write(&probe, "").unwrap();
    let insensitive = Path::new(&wt).join("case-probe").exists();
    std::fs::remove_file(probe).unwrap();
    if !insensitive {
        eprintln!("a case-sensitive file system: the case only row is clean");
    }
    let check = json!({"session_id": session});
    for case in CASES.trim().lines() {
        let mut fields: Vec<&str> = case.split('|').map(str::trim).collect();
        if fields[0] == "case only" && !insensitive {
            fields[2] = "- 0 false";
        }
        run(fields[1]).await;
        let got = c.f.ok("worktree/check", check.clone()).await;
        let blockers = got["blockers"].as_array().unwrap().iter();
        let blockers: Vec<&str> = blockers.map(|b| b.as_str().unwrap()).collect();
        let blockers = Some(blockers.join(",")).filter(|b| !b.is_empty());
        let (warnings, removable) = (&got["warnings"], &got["removable"]);
        let (ignored, cased) = (&warnings["ignored_files"], &warnings["case_only_dirty"]);
        let blockers = blockers.unwrap_or("-".into());
        let found = format!("{blockers} {ignored} {cased}");
        assert_eq!(
            (found.as_str(), removable),
            (fields[2], &json!(blockers == "-")),
            "{case}"
        );
        run(fields[3]).await;
    }
    let local = c.f.ok("session/create", create("c-1")).await;
    let local = json!({"session_id": local["session_id"]});
    let local = c.f.call(harness(), "worktree/check", local).await;
    assert_eq!(code(local.unwrap_err()), "unsupported ");
}
