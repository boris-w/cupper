//! HEAD of the user's repository, through the repo tier's read-only ref
//! commands (design 5.2.10): the commit `staged` compares the index with.
//! Both calls exit 0 whatever HEAD is (on a branch, detached, before the
//! first commit): in a repository with required filters the repo tier takes
//! any failure for theirs (T098, rule 5).

use std::path::PathBuf;

use super::{GitCmd, GitError, RepoOp, RepoTier};
use crate::clock::Clock;
use crate::durations::Durations;

/// The commit HEAD is at in the repository of `tier`; `None` before the
/// first one. `writable` as [`RepoTier::run`] takes it.
pub async fn commit(
    tier: &RepoTier<'_>,
    clock: &dyn Clock,
    durations: &Durations,
    writable: &[PathBuf],
) -> Result<Option<String>, GitError> {
    let read = async |cmd| -> Result<String, GitError> {
        let done = tier.run(clock, durations, writable, cmd).await?;
        Ok(String::from_utf8_lossy(&done.output.stdout).into_owned())
    };
    // Also before the first commit; nothing on a detached HEAD.
    let branch = read(GitCmd::repo(RepoOp::BranchCurrent)).await?;
    let commit = match branch.trim_end() {
        "" => read(GitCmd::repo(RepoOp::RevParse).args(["--verify", "-q", "HEAD"])).await?,
        branch => {
            let refname = format!("refs/heads/{branch}");
            let args = ["--format=%(refname)%00%(objectname)", &refname];
            let found = read(GitCmd::repo(RepoOp::ForEachRef).args(args)).await?;
            // The pattern matches `refs/heads/<branch>/...` too; the branch
            // itself is missing before the first commit.
            let found = found
                .lines()
                .find_map(|line| line.strip_prefix(&format!("{refname}\0")));
            found.unwrap_or_default().to_owned()
        }
    };
    Ok(Some(commit.trim_end().to_owned()).filter(|commit| !commit.is_empty()))
}
