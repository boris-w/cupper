//! HEAD of the user's repository, through the repo tier's read-only ref
//! commands (design 5.2.10): its branch, the commit `staged` compares the
//! index with, and the branch's upstream for `git/status`. Both calls exit
//! 0 whatever HEAD is (on a branch, detached, before the first commit): in
//! a repository with required filters the repo tier takes any failure for
//! theirs (T098, rule 5).

use std::path::PathBuf;

use super::{GitCmd, GitError, RepoOp, RepoTier};
use crate::clock::Clock;
use crate::durations::Durations;

/// What HEAD is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Head {
    /// Absent on a detached HEAD.
    pub branch: Option<String>,
    /// Absent before the first commit.
    pub commit: Option<String>,
    pub upstream: Option<String>,
    /// Commits ahead of and behind the upstream.
    pub ahead: u32,
    pub behind: u32,
}

/// HEAD of the repository of `tier`. `writable` as [`RepoTier::run`] takes it.
pub async fn head(
    tier: &RepoTier<'_>,
    clock: &dyn Clock,
    durations: &Durations,
    writable: &[PathBuf],
) -> Result<Head, GitError> {
    let read = async |cmd| -> Result<String, GitError> {
        let done = tier.run(clock, durations, writable, cmd).await?;
        Ok(String::from_utf8_lossy(&done.output.stdout).into_owned())
    };
    // Also before the first commit; nothing on a detached HEAD.
    let branch = read(GitCmd::repo(RepoOp::BranchCurrent)).await?;
    let branch = branch.trim_end();
    if branch.is_empty() {
        let commit = read(GitCmd::repo(RepoOp::RevParse).args(["--verify", "-q", "HEAD"])).await?;
        let commit = Some(commit.trim_end().to_owned()).filter(|c| !c.is_empty());
        return Ok(Head {
            commit,
            ..Head::default()
        });
    }
    let refname = format!("refs/heads/{branch}");
    let format =
        "--format=%(refname)%00%(objectname)%00%(upstream:short)%00%(upstream:track,nobracket)";
    let found = read(GitCmd::repo(RepoOp::ForEachRef).args([format, &refname])).await?;
    // The pattern matches `refs/heads/<branch>/...` too; the branch itself
    // is missing before the first commit.
    let found = found
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{refname}\0")));
    Ok(Head {
        branch: Some(branch.to_owned()),
        ..found.map(tracked).unwrap_or_default()
    })
}

/// `<commit>\0<upstream>\0<track>` of a branch: `track` is `ahead 2,
/// behind 1`, either part alone, `gone`, or nothing.
fn tracked(fields: &str) -> Head {
    let mut fields = fields.split('\0');
    let mut field = || Some(fields.next()?.to_owned()).filter(|f| !f.is_empty());
    let (commit, upstream, track) = (field(), field(), field().unwrap_or_default());
    let mut head = Head {
        commit,
        upstream,
        ..Head::default()
    };
    for part in track.split(", ") {
        match part.split_once(' ') {
            Some(("ahead", n)) => head.ahead = n.parse().unwrap_or(0),
            Some(("behind", n)) => head.behind = n.parse().unwrap_or(0),
            _ => {}
        }
    }
    head
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_git_status_head_reads_the_upstream_and_how_far_apart() {
        let head = |commit: &str, upstream: Option<&str>, ahead, behind| Head {
            branch: None,
            commit: Some(commit.into()),
            upstream: upstream.map(Into::into),
            ahead,
            behind,
        };
        let cases = [
            (
                "c1\0origin/main\0ahead 2, behind 1",
                head("c1", Some("origin/main"), 2, 1),
            ),
            (
                "c2\0origin/main\0behind 3",
                head("c2", Some("origin/main"), 0, 3),
            ),
            (
                "c3\0origin/gone\0gone",
                head("c3", Some("origin/gone"), 0, 0),
            ),
            ("c4\0\0", head("c4", None, 0, 0)),
        ];
        for (fields, want) in cases {
            assert_eq!(tracked(fields), want, "{fields:?}");
        }
    }
}
