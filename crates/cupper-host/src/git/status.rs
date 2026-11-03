//! `git/status` and `git/branches` (design 5.2.10, "git/status"): the
//! branch, its upstream and how far apart they are come from the repo
//! tier's read-only ref commands ([`super::head`]); an operation in progress
//! from the marker files in the git dir; the files' states from the
//! snapshot tier, on a fresh copy of the index.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cupper_protocol::annotations::WirePath;
use cupper_protocol::methods::git::{
    GitBranchesResult, GitFileStatus, GitOperation, GitStatusFile,
};

use super::snapshot::{self, Opened, ShotError, blocking, output, split};
use super::{GitCmd, GitError, GitTool, RepoOp, RepoTier, SnapshotOp};
use crate::clock::Clock;
use crate::durations::Durations;

/// The changed, staged, untracked and unmerged files of `work_tree`, whose
/// HEAD is `commit`, and the operation in progress.
pub async fn files(
    tool: &GitTool,
    work_tree: &Path,
    commit: Option<&str>,
    clock: &dyn Clock,
    durations: &Durations,
) -> Result<(Vec<GitStatusFile>, Option<GitOperation>), ShotError> {
    let opened = snapshot::open(tool, work_tree, clock, durations).await?;
    let found = states(tool, &opened, commit, clock, durations).await;
    let git_dir = opened.layout.git_dir.clone();
    opened.close().await;
    let operation = blocking(move || Ok(in_progress(&git_dir))).await?;
    Ok((found?, operation))
}

async fn states(
    tool: &GitTool,
    opened: &Opened,
    commit: Option<&str>,
    clock: &dyn Clock,
    durations: &Durations,
) -> Result<Vec<GitStatusFile>, ShotError> {
    let tier = tool.snapshot_tier(&opened.place);
    let out = |cmd, what| output(&tier, cmd, what, clock, durations);
    // Exits 1 when files differ or are unmerged; `diff-files` says which.
    tier.prepare(GitCmd::snapshot(SnapshotOp::Refresh))
        .run(clock, durations)
        .await?;
    let head = commit.unwrap_or(opened.empty_tree());
    let staged = out(
        GitCmd::snapshot(SnapshotOp::DiffIndex).args([head]),
        "diff-index",
    )
    .await?;
    let changed = out(GitCmd::snapshot(SnapshotOp::DiffFiles), "diff-files").await?;
    let untracked = out(GitCmd::snapshot(SnapshotOp::Untracked), "ls-files").await?;
    let mut files = BTreeMap::new();
    for (status, path, old_path) in name_status(&staged) {
        let file = at(&mut files, path);
        (file.index, file.old_path) = (status, old_path.map(WirePath));
    }
    for (status, path, _) in name_status(&changed) {
        at(&mut files, path).worktree = status;
    }
    for path in split(&untracked) {
        let file = at(&mut files, String::from_utf8_lossy(path).into_owned());
        (file.index, file.worktree) = (GitFileStatus::Untracked, GitFileStatus::Untracked);
    }
    Ok(files.into_values().collect())
}

/// The state of `path` so far, unmodified at first.
fn at(files: &mut BTreeMap<String, GitStatusFile>, path: String) -> &mut GitStatusFile {
    files.entry(path).or_insert_with_key(|path| GitStatusFile {
        path: WirePath(path.clone()),
        old_path: None,
        index: GitFileStatus::Unmodified,
        worktree: GitFileStatus::Unmodified,
    })
}

/// `--name-status -z` output: each file's status letter, path, and a rename's
/// or copy's source.
fn name_status(output: &[u8]) -> Vec<(GitFileStatus, String, Option<String>)> {
    let mut fields = split(output).map(|f| String::from_utf8_lossy(f).into_owned());
    let mut found = Vec::new();
    while let (Some(status), Some(first)) = (fields.next(), fields.next()) {
        let (status, path, old_path) = match status.as_bytes()[0] {
            b'R' => (GitFileStatus::Renamed, fields.next(), Some(first)),
            b'C' => (GitFileStatus::Copied, fields.next(), Some(first)),
            letter => {
                let status = match letter {
                    b'A' => GitFileStatus::Added,
                    b'D' => GitFileStatus::Deleted,
                    b'T' => GitFileStatus::TypeChanged,
                    b'U' => GitFileStatus::Unmerged,
                    _ => GitFileStatus::Modified,
                };
                (status, Some(first), None)
            }
        };
        found.extend(path.map(|path| (status, path, old_path)));
    }
    found
}

/// The operation git started and has not finished, by its marker in the
/// work tree's git dir (ADR-10, rule 3); a rebase first, which runs the others.
pub fn in_progress(git_dir: &Path) -> Option<GitOperation> {
    let markers = [
        ("rebase-merge", GitOperation::Rebase),
        ("rebase-apply", GitOperation::Rebase),
        ("MERGE_HEAD", GitOperation::Merge),
        ("CHERRY_PICK_HEAD", GitOperation::CherryPick),
        ("REVERT_HEAD", GitOperation::Revert),
        ("BISECT_LOG", GitOperation::Bisect),
    ];
    let mut markers = markers.into_iter();
    markers
        .find(|(marker, _)| git_dir.join(marker).exists())
        .map(|(_, operation)| operation)
}

/// The local and remote-tracking branches of the repository at `tier`, and
/// the one HEAD is on.
pub async fn branches(
    tier: &RepoTier<'_>,
    clock: &dyn Clock,
    durations: &Durations,
    writable: &[PathBuf],
) -> Result<GitBranchesResult, GitError> {
    let args = [
        "--format=%(HEAD)%00%(refname)",
        "refs/heads",
        "refs/remotes",
    ];
    let cmd = GitCmd::repo(RepoOp::ForEachRef).args(args);
    let done = tier.run(clock, durations, writable, cmd).await?;
    let listed = String::from_utf8_lossy(&done.output.stdout);
    let mut result = GitBranchesResult {
        current: None,
        local: Vec::new(),
        remote: Vec::new(),
    };
    for (head, name) in listed.lines().filter_map(|line| line.split_once('\0')) {
        if let Some(local) = name.strip_prefix("refs/heads/") {
            if head == "*" {
                result.current = Some(local.to_owned());
            }
            result.local.push(local.to_owned());
        } else if let Some(remote) = name.strip_prefix("refs/remotes/") {
            result.remote.push(remote.to_owned());
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_git_status_name_status_reads_each_letter_and_renames() {
        let output = b"M\0a.txt\0R100\0old\0new\0C075\0src\0copy\0A\0b\0D\0c\0T\0d\0U\0e\0";
        let found = name_status(output);
        let shown: Vec<_> = found
            .iter()
            .map(|(s, p, o)| (s.as_str(), p.as_str(), o.as_deref()))
            .collect();
        assert_eq!(
            shown,
            [
                ("modified", "a.txt", None),
                ("renamed", "new", Some("old")),
                ("copied", "copy", Some("src")),
                ("added", "b", None),
                ("deleted", "c", None),
                ("type_changed", "d", None),
                ("unmerged", "e", None),
            ]
        );
    }
}
