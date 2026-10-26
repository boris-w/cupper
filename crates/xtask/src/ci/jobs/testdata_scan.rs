//! `testdata-scan` (design 5.1.6): `tools/trace-ids` checks the test ID
//! registry `tests/ids.toml` against the plan and the design (5.9.10), and
//! `tools/sanitize` scans `testdata/` and the committed CI summaries for
//! personal data (5.9.3, plan 1.8): the generic patterns, then the
//! maintainer's local sensitive word list when there is one. `xtask
//! compat-reports` checks the committed compat reports and the verified CLI
//! ranges they give (5.9.5).

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::ci::ctx::JobCtx;
use crate::ci::git;

/// What the personal data scans cover.
const SCOPE: &[&str] = &["testdata", "docs/tasks/*/ci/*.json"];
/// Kept by the maintainer in the main work tree and never committed (plan section 6).
pub const WORD_LIST: &str = ".private/sensitive-words.txt";

pub fn run(ctx: &mut JobCtx) -> Result<()> {
    let mut install = ctx.command("pnpm");
    install.args(["install", "--frozen-lockfile"]);
    ctx.check("install", &mut install)?;

    let mut trace_ids = ctx.command("node");
    trace_ids.args(["tools/trace-ids/src/main.ts", "check"]);
    ctx.check("trace-ids", &mut trace_ids)?;

    let mut reports = ctx.command(std::env::current_exe()?);
    reports.arg("compat-reports");
    ctx.check("compat-reports", &mut reports)?;

    let mut scan = ctx.command("node");
    scan.args(["tools/sanitize/src/main.ts", "scan"])
        .args(SCOPE);
    ctx.check("pii-scan", &mut scan)?;
    sensitive_words(ctx)
}

/// Scans for the local word list's entries, or skips with a notice when the
/// list does not exist yet. A finding names the entry, never the word.
fn sensitive_words(ctx: &mut JobCtx) -> Result<()> {
    let Some(list) = word_list(ctx.root)? else {
        let reason = format!("no local sensitive word list ({WORD_LIST} in the main work tree)");
        ctx.skip("sensitive-words", &reason);
        return Ok(());
    };
    words(ctx, &list)
}

fn words(ctx: &mut JobCtx, list: &Path) -> Result<()> {
    let mut words = ctx.command("node");
    words
        .args(["tools/sanitize/src/main.ts", "words", "--list"])
        .arg(list)
        .args(SCOPE);
    ctx.check("sensitive-words", &mut words)
}

/// The word list of the main work tree, which a linked work tree such as an
/// agent's finds through the repository's common git directory.
pub fn word_list(root: &Path) -> Result<Option<PathBuf>> {
    let common = git::common_dir(root)?;
    let main = common.parent().unwrap_or(&common);
    let list = main.join(WORD_LIST);
    Ok(list.is_file().then_some(list))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::testutil::{TempDir, clean_repo, git as run_git};

    #[test]
    fn unit_sensitive_list_is_found_from_a_linked_work_tree() {
        let repo = clean_repo("words-main");
        let linked = TempDir::new("words-linked");
        let linked_root = linked.path().join("wt");
        let linked_arg = linked_root.to_str().expect("a UTF-8 temp path");
        run_git(repo.path(), &["worktree", "add", "--quiet", linked_arg]);
        assert_eq!(word_list(&linked_root).expect("git"), None);

        let list = repo.path().join(WORD_LIST);
        fs::create_dir_all(list.parent().expect("a parent")).expect("mkdir");
        fs::write(&list, "# local words\nexample-word\n").expect("write the list");
        let found = word_list(&linked_root).expect("git").expect("found");
        assert_eq!(found.canonicalize().ok(), list.canonicalize().ok());
        let from_main = word_list(repo.path()).expect("git").expect("found");
        assert_eq!(from_main.canonicalize().ok(), list.canonicalize().ok());
    }

    #[test]
    fn unit_sensitive_list_missing_is_skipped_and_recorded() {
        let repo = clean_repo("words-missing");
        let logs = TempDir::new("words-missing-logs");
        let mut ctx = JobCtx::new(repo.path(), logs.path(), "testdata-scan");
        sensitive_words(&mut ctx).expect("a missing list does not fail the job");
        assert_eq!(ctx.skipped, ["sensitive-words"]);
        assert_eq!((ctx.passed, ctx.failed), (0, 0));
    }

    /// Runs the step on this repository's own test data with a list made up
    /// for the test: a word that occurs there fails it.
    #[test]
    fn unit_sensitive_list_hit_fails_the_step_without_the_word() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let dir = TempDir::new("words-hit");
        let (logs, list) = (dir.path().join("logs"), dir.path().join("list.txt"));
        fs::create_dir_all(&logs).expect("mkdir");

        fs::write(&list, "# none of these occur\nno-such-word-in-testdata-0\n").expect("write");
        let mut ctx = JobCtx::new(&root, &logs, "testdata-scan");
        words(&mut ctx, &list).expect("no entry occurs");

        // A note event of every committed recording.
        fs::write(&list, "no-such-word-in-testdata-0\nInstance_Started\n").expect("write");
        let mut ctx = JobCtx::new(&root, &logs, "testdata-scan");
        words(&mut ctx, &list).expect_err("entry 2 occurs");
        assert_eq!(ctx.failed, 1);
        let log = fs::read_to_string(ctx.step_file("sensitive-words", "log")).expect("the log");
        assert!(
            log.contains(": entry 2 of the sensitive word list"),
            "{log}"
        );
        assert!(!log.to_lowercase().contains("instance_started"), "{log}");
    }
}
