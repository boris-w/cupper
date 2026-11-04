//! `worktree/check` (design 5.2.10 "worktree", ADR-10's cleanup rules):
//! whether removing a session's worktree could lose work. Its files come
//! from the snapshot tier on a copy of its index (rules 1 and 2), the
//! operation in progress and the submodules from its git dir (3 and 7), its
//! lock from `worktree list` (4), and its commits from `rev-list` in the
//! repo tier (5 and 6). All of it is read only.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cupper_protocol::error::RpcError;
use cupper_protocol::ids::SessionId;
use cupper_protocol::methods::git::{
    GitFileStatus, GitStatusFile, WorktreeBlocker, WorktreeCheckResult, WorktreeWarnings,
};
use cupper_protocol::methods::session::WorktreeOutcome;

use super::head::{self, Head};
use super::service::{internal, shot, writable};
use super::snapshot::{self, ShotError, blocking, output, split};
use super::worktree::{Worktrees, listed, lock_reason};
use super::{GitCmd, GitError, GitTool, RepoOp, SnapshotOp, status};
use crate::clock::Clock;
use crate::durations::Durations;
use crate::ledger::Ticket;

#[derive(Debug, thiserror::Error)]
pub enum CheckError {
    #[error(transparent)]
    Shot(#[from] ShotError),
    #[error(transparent)]
    Git(#[from] GitError),
}

/// The cleanup rules for `session`'s worktree at `work_tree`, whose HEAD is
/// `head`. `writable` as [`RepoTier::run`](super::RepoTier::run) takes it.
pub async fn check(
    tool: &GitTool,
    (work_tree, head): (&Path, &Head),
    session: &SessionId,
    writable: &[PathBuf],
    (clock, durations): (&dyn Clock, &Durations),
    count_ignored: bool,
) -> Result<WorktreeCheckResult, CheckError> {
    let opened = snapshot::open(tool, work_tree, clock, durations).await?;
    let tier = tool.snapshot_tier(&opened.place);
    let commit = head.commit.as_deref();
    let files = status::states(tool, &opened, commit, clock, durations).await;
    let ls = |op| output(&tier, GitCmd::snapshot(op), "ls-files", clock, durations);
    let tracked = ls(SnapshotOp::LsFiles).await;
    // Every ignored file, which a large one such as `node_modules` holds
    // many of: archiving does without.
    let ignored = match count_ignored {
        true => ls(SnapshotOp::Ignored).await,
        false => Ok(Vec::new()),
    };
    let git_dir = opened.layout.git_dir.clone();
    opened.close().await;
    let (files, tracked, ignored) = (files?, tracked?, ignored?);
    let (operation, modules) = blocking(move || {
        let modules = std::fs::read_dir(git_dir.join("modules"));
        let modules = modules.is_ok_and(|mut entries| entries.next().is_some());
        Ok((status::in_progress(&git_dir), modules))
    })
    .await?;

    // A file shows as changed only because another one's path differs in
    // case alone, which a case-insensitive file system cannot hold.
    let mut cased = BTreeMap::<String, u32>::new();
    for path in split(&tracked) {
        let folded = String::from_utf8_lossy(path).to_lowercase();
        *cased.entry(folded).or_default() += 1;
    }
    let collides = |path: &str| cased.get(&path.to_lowercase()).is_some_and(|n| *n > 1);
    let untracked = |s: &GitFileStatus| *s == GitFileStatus::Untracked;
    let unchanged = |s: &GitFileStatus| *s == GitFileStatus::Unmodified;
    // Each file the states name changed somehow.
    let dirty: Vec<_> = files.iter().filter(|f| !untracked(&f.worktree)).collect();
    let cased_only = |f: &GitStatusFile| unchanged(&f.index) && collides(&f.path.0);
    let case_only_dirty = !dirty.is_empty() && dirty.iter().all(|f| cased_only(f));

    let tier = tool.repo_tier(work_tree);
    let read = async |args: &[&str]| -> Result<String, GitError> {
        let cmd = GitCmd::repo(RepoOp::RevList).args(args);
        let done = tier.run(clock, durations, writable, cmd).await?;
        let counted = String::from_utf8_lossy(&done.output.stdout);
        Ok(counted.trim().to_owned())
    };
    let own = head.branch.as_ref().map(|b| format!("--exclude={b}"));
    let mut others = vec!["--count", "HEAD", "--not"];
    others.extend(own.as_deref());
    others.extend(["--branches", "--remotes", "--tags"]);
    let head_only = head.commit.is_some() && read(&others).await? != "0";
    // A remote nobody fetched from or pushed to has no branches here: what
    // only this worktree has, rule 5 keeps.
    let remotes = read(&["--count", "--max-count=1", "--remotes"]).await? != "0";
    let unpushed = ["--count", "HEAD", "--not", "--remotes"];
    let unpushed = remotes && head.commit.is_some() && read(&unpushed).await? != "0";
    let list = GitCmd::repo(RepoOp::WorktreeList);
    let list = tier.run(clock, durations, writable, list).await?;
    let lock = listed(&list.output.stdout, work_tree).flatten();
    let ours = lock_reason(session);

    let modified = !case_only_dirty && !dirty.is_empty();
    let added = files.iter().any(|f| untracked(&f.worktree));
    let foreign = lock.is_some_and(|reason| reason != ours);
    let rules = [
        (modified, WorktreeBlocker::ModifiedFiles),
        (added, WorktreeBlocker::UntrackedFiles),
        (operation.is_some(), WorktreeBlocker::OperationInProgress),
        (foreign, WorktreeBlocker::ForeignLock),
        (head_only, WorktreeBlocker::HeadOnlyCommits),
        (unpushed, WorktreeBlocker::UnpushedCommits),
        (modules, WorktreeBlocker::InitializedSubmodules),
    ];
    let blockers: Vec<_> = rules
        .into_iter()
        .filter_map(|(kept, r)| kept.then_some(r))
        .collect();
    let ignored_files = u32::try_from(split(&ignored).count()).unwrap_or(u32::MAX);
    Ok(WorktreeCheckResult {
        removable: blockers.is_empty(),
        blockers,
        warnings: WorktreeWarnings {
            ignored_files,
            case_only_dirty,
        },
    })
}

impl Worktrees {
    /// Removes `session`'s worktree at `path` of the repository at `repo`
    /// (design 5.5.9), unless a cleanup rule keeps it: unlocked, then
    /// `worktree remove`, with `--force` only when `case_only` was asked and
    /// case alone dirties it. One git does not list is `removed`; one whose
    /// directory is gone `kept`, for the user to prune. `ticket` goes
    /// `pending_external` before git changes anything.
    pub async fn remove(
        &self,
        (repo, path): (&Path, &Path),
        session: &SessionId,
        case_only: bool,
        ticket: &mut Option<Ticket>,
    ) -> Result<(WorktreeOutcome, Vec<WorktreeBlocker>), RpcError> {
        let writable = writable(&self.readers).await?;
        let failed = |err| self.failed(err);
        let _lock = self.locks.hold(repo).await?;
        let list = GitCmd::repo(RepoOp::WorktreeList);
        let list = self.run(repo, &writable, list).await.map_err(failed)?;
        match listed(&list.output.stdout, path) {
            None => return Ok((WorktreeOutcome::Removed, Vec::new())),
            Some(_) if !path.is_dir() => return Ok((WorktreeOutcome::Kept, Vec::new())),
            Some(_) => {}
        }
        let git = self.git.as_ref().ok_or_else(|| internal("no git to run"))?;
        let (clock, durations) = (&*self.clock, &self.durations);
        let head = head::head(&git.repo_tier(path), clock, durations, &writable).await;
        let head = head.map_err(failed)?;
        let checked = check(
            git,
            (path, &head),
            session,
            &writable,
            (clock, durations),
            false,
        );
        let checked = checked.await.map_err(|err| match err {
            CheckError::Shot(err) => shot(err),
            CheckError::Git(err) => failed(err),
        })?;
        let mut blockers = checked.blockers;
        let force = checked.warnings.case_only_dirty;
        if force && !case_only {
            blockers.insert(0, WorktreeBlocker::ModifiedFiles);
        }
        if !blockers.is_empty() {
            return Ok((WorktreeOutcome::Blocked, blockers));
        }
        if let Some(ticket) = ticket.as_mut() {
            ticket.mark_external().await.map_err(internal)?;
        }
        let unlock = GitCmd::repo(RepoOp::WorktreeUnlock).args([path]);
        let _ = self.run(repo, &writable, unlock).await;
        let mut remove = GitCmd::repo(RepoOp::WorktreeRemove);
        if force {
            remove = remove.args(["--force"]);
        }
        let remove = remove.args([path]);
        self.run(repo, &writable, remove).await.map_err(failed)?;
        Ok((WorktreeOutcome::Removed, Vec::new()))
    }
}
