//! The git facts a CI run depends on: where the work tree is, which commit it
//! tests, and that nothing uncommitted can leak into the result (plan 1.8).

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

/// Root of the work tree that contains `dir`.
pub fn toplevel(dir: &Path) -> Result<PathBuf> {
    Ok(PathBuf::from(git(dir, &["rev-parse", "--show-toplevel"])?))
}

/// Full hash of the commit checked out in `root`.
pub fn head(root: &Path) -> Result<String> {
    git(root, &["rev-parse", "--verify", "HEAD"])
}

/// The git directory that every work tree of the repository shares.
pub fn common_dir(root: &Path) -> Result<PathBuf> {
    let args = ["rev-parse", "--path-format=absolute", "--git-common-dir"];
    Ok(PathBuf::from(git(root, &args)?))
}

/// Fails unless `git status` is empty: no staged, unstaged or untracked changes.
pub fn ensure_clean(root: &Path) -> Result<()> {
    let status = git(root, &["status", "--porcelain=v1", "--untracked-files=all"])?;
    if status.is_empty() {
        return Ok(());
    }
    let shown: Vec<&str> = status.lines().take(20).collect();
    bail!(
        "the work tree is not clean; commit or stash these changes first:\n{}",
        shown.join("\n")
    )
}

/// The paths the commits since `main` changed, or `None` when there is
/// nothing to compare: no `main` branch, or no commit ahead of it.
pub fn changed_since_main(root: &Path) -> Result<Option<Vec<String>>> {
    let Ok(base) = git(root, &["merge-base", "main", "HEAD"]) else {
        return Ok(None);
    };
    if base == head(root)? {
        return Ok(None);
    }
    let names = git(root, &["diff", "--name-only", &base, "HEAD"])?;
    Ok(Some(names.lines().map(str::to_owned).collect()))
}

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .context("cannot run git")?;
    if !output.status.success() {
        bail!(
            "`git {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8(output.stdout)
        .context("git printed non-UTF-8 output")?
        .trim_end()
        .to_owned())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::testutil::{clean_repo, git as run_git};

    #[test]
    fn unit_ci_lock_clean_check_rejects_any_change() {
        let repo = clean_repo("git-clean");
        let dir = repo.path();
        ensure_clean(dir).expect("a fresh commit is clean");

        fs::write(dir.join("new.txt"), "untracked\n").expect("write");
        let err = ensure_clean(dir).expect_err("untracked file");
        assert!(err.to_string().contains("new.txt"), "{err}");
        fs::remove_file(dir.join("new.txt")).expect("remove");
        ensure_clean(dir).expect("clean again");

        fs::write(dir.join("README"), "edited\n").expect("write");
        ensure_clean(dir).expect_err("unstaged edit");
        run_git(dir, &["add", "README"]);
        ensure_clean(dir).expect_err("staged edit");
    }

    #[test]
    fn changed_paths_are_the_commits_since_main() {
        let repo = clean_repo("git-changed");
        let dir = repo.path();
        run_git(dir, &["branch", "-M", "main"]);
        assert_eq!(changed_since_main(dir).expect("on main"), None);

        run_git(dir, &["checkout", "--quiet", "-b", "task"]);
        fs::create_dir(dir.join("ui")).expect("mkdir");
        fs::write(dir.join("ui/page.ts"), "changed\n").expect("write");
        run_git(dir, &["add", "ui/page.ts"]);
        run_git(dir, &["commit", "--quiet", "-m", "task"]);
        let changed = changed_since_main(dir).expect("on a branch");
        assert_eq!(changed, Some(vec!["ui/page.ts".to_owned()]));

        run_git(dir, &["branch", "-D", "main"]);
        assert_eq!(changed_since_main(dir).expect("no main"), None);
    }
}
