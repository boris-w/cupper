//! New worktrees for sessions (design 5.2.10 "worktree", 5.5.2; ADR-10).
//! `session/create` plans one: the branch `cupper/<name>` and the directory
//! `<root>/<repo>-<hash>/<name>`, of one name that a suffix makes free. The
//! session's actor then adds it in the repo tier under the repository's
//! lock, hearing the checkout's progress as git writes it, locks it for
//! the session, and refreshes its index a second later.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs::DirBuilder;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cupper_protocol::error::{ErrorCode, ErrorReason, INVALID_PARAMS, RpcError};
use cupper_protocol::ids::SessionId;
use cupper_protocol::methods::session::NewWorktree;
use cupper_protocol::session::{WorkspacePhase, WorkspacePreparing};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

use super::service::{RepoLock, internal, writable};
use super::stamp::Layout;
use super::{Done, GitCmd, GitError, GitTool, Repo, RepoNotice, RepoOp, lfs};
use crate::clock::Clock;
use crate::durations::{Durations, WORKTREE_REFRESH};
use crate::gateway::AuditRecord;
use crate::settings::SettingsHandle;
use crate::store::Readers;

/// The suffixes a name may take: `-2` to `-99`.
const SUFFIXES: u32 = 99;

/// `workspace.preparing` as the session is created.
pub const ADDING: WorkspacePreparing = WorkspacePreparing {
    phase: WorkspacePhase::WorktreeAdd,
    progress: None,
};

/// What the worktree steps need of the host.
#[derive(Debug, Clone)]
pub struct Worktrees {
    /// `None` when no trusted `PATH` entry has a git.
    pub git: Option<GitTool>,
    pub readers: Readers,
    pub clock: Arc<dyn Clock>,
    pub durations: Durations,
    /// For `git.unsafe_repo_config`.
    pub audit: mpsc::UnboundedSender<AuditRecord>,
    /// `worktrees.root`.
    pub settings: SettingsHandle,
    /// The git service's.
    pub locks: Arc<RepoLock>,
}

/// A session's worktree, as `session/create` decided it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The project's work tree, whose repository gets the worktree.
    pub repo: PathBuf,
    /// A real path, but for its last component, which the add makes.
    pub path: PathBuf,
    pub branch: String,
    pub base_ref: String,
}

/// What the session writes while its worktree is added.
#[derive(Debug, Clone, PartialEq)]
pub enum Heard {
    Progress(WorkspacePreparing),
    Notice(RepoNotice),
}

impl Worktrees {
    /// `repo`'s worktree `new`, named by its hint, else `fallback`:
    /// `invalid_params` for a hint with a `/` or one git takes for no
    /// branch's name, `conflict` once every suffix is taken (design 5.2.10).
    pub async fn plan(
        &self,
        repo: &Path,
        new: &NewWorktree,
        fallback: &str,
    ) -> Result<Plan, RpcError> {
        let writable = writable(&self.readers).await?;
        let name = new.branch_hint.as_deref().unwrap_or(fallback).to_owned();
        let invalid = RpcError::new(INVALID_PARAMS, "branch_hint names no branch");
        let check = GitCmd::repo(RepoOp::CheckRefFormat).args(["--branch", &branch(&name)]);
        match self.run(repo, &writable, check).await {
            _ if name.contains('/') => return Err(invalid),
            Err(GitError::Failed { .. }) => return Err(invalid),
            checked => drop(checked.map_err(|err| self.failed(err))?),
        }
        let refs = ["--format=%(refname:lstrip=2)", "refs/heads/cupper/"];
        let refs = GitCmd::repo(RepoOp::ForEachRef).args(refs);
        let refs = self.run(repo, &writable, refs).await;
        let refs = refs.map_err(|err| self.failed(err))?;
        let taken = String::from_utf8_lossy(&refs.output.stdout).into_owned();
        let taken: BTreeSet<&str> = taken.lines().collect();
        let dir = self.dir(repo)?;
        let suffixed = (2..=SUFFIXES).map(|n| format!("{name}-{n}"));
        let mut names = std::iter::once(name.clone()).chain(suffixed);
        let free =
            names.find(|name| !taken.contains(branch(name).as_str()) && !dir.join(name).exists());
        let name =
            free.ok_or_else(|| RpcError::app(ErrorCode::Conflict, "every suffix is taken"))?;
        let (repo, path, branch) = (repo.to_owned(), dir.join(&name), branch(&name));
        let base_ref = new.base_ref.clone();
        Ok(Plan {
            repo,
            path,
            branch,
            base_ref,
        })
    }

    /// Adds `plan`'s worktree for `session` (design 5.2.10): `worktree add
    /// -b` under the repository's lock, then `worktree lock` with the
    /// session's ID; `update-index --refresh` in it a second later. `heard`
    /// gets the checkout's progress and the repo tier's notices. A
    /// `required` filter, which the repo tier nulls, fails it
    /// `filter_unsupported`. What a crash left (design 5.2.13) is taken
    /// over: a worktree there is not added again, nor locked again once
    /// locked, and a branch there is checked out without `-b`; a worktree
    /// whose directory is gone fails it `not_found`.
    pub async fn add(
        &self,
        plan: &Plan,
        session: &SessionId,
        heard: impl Fn(Heard),
    ) -> Result<(), RpcError> {
        let writable = writable(&self.readers).await?;
        let (repo, path) = (&plan.repo, plan.path.as_os_str());
        let lock = self.locks.hold(repo).await?;
        let failed = |err| {
            let required = matches!(err, GitError::RequiredFilter(_));
            heard_notice(&heard, err.notice());
            let err = self.failed(err);
            match required {
                true => err.with_reason(ErrorReason::FilterUnsupported),
                false => err,
            }
        };
        let list = GitCmd::repo(RepoOp::WorktreeList);
        let list = self.run(repo, &writable, list).await.map_err(failed)?;
        let locked = listed(&list.output.stdout, &plan.path);
        // Registered, its directory gone (`prunable`): only the user prunes,
        // which takes their other stale worktrees too.
        if locked.is_some() && !plan.path.is_dir() {
            return Err(RpcError::app(
                ErrorCode::NotFound,
                "the worktree's directory is gone",
            ));
        }
        if locked.is_none() {
            let heads = format!("refs/heads/{}", plan.branch);
            let heads = GitCmd::repo(RepoOp::ForEachRef).args(["--format=%(refname)", &heads]);
            let heads = self.run(repo, &writable, heads).await.map_err(failed)?;
            self.checkout(
                plan,
                &writable,
                !heads.output.stdout.is_empty(),
                &failed,
                &heard,
            )
            .await?;
        }
        if !matches!(locked, Some(Some(_))) {
            let reason = lock_reason(session);
            let args = [OsStr::new("--reason"), reason.as_ref(), path];
            let lock_it = GitCmd::repo(RepoOp::WorktreeLock).args(args);
            let locked = self.run(repo, &writable, lock_it).await;
            heard_notice(&heard, locked.map_err(failed)?.notice);
        }
        drop(lock);
        heard(progress(WorkspacePhase::Refresh, None));
        let delay = self.durations.get(WORKTREE_REFRESH);
        self.clock.timer_at(self.clock.now_mono() + delay).await;
        let _lock = self.locks.hold(repo).await?;
        // The refresh only spares later reads hashing the files again: its
        // failure fails nothing.
        let refresh = GitCmd::repo(RepoOp::RefreshIndex);
        let _ = self.run(&plan.path, &writable, refresh).await;
        Ok(())
    }

    /// `worktree add` of `plan`, of its new branch unless `branched`, the
    /// checkout's progress to `heard`.
    async fn checkout(
        &self,
        plan: &Plan,
        writable: &[PathBuf],
        branched: bool,
        failed: &impl Fn(GitError) -> RpcError,
        heard: &impl Fn(Heard),
    ) -> Result<(), RpcError> {
        let (branch, base) = (plan.branch.as_ref(), plan.base_ref.as_ref());
        let tree = if branched { branch } else { base };
        self.lfs_objects(&plan.repo, writable, tree, failed).await?;
        let (said, mut chunks) = mpsc::unbounded_channel();
        let args = match branched {
            true => vec![plan.path.as_os_str(), branch],
            false => vec![OsStr::new("-b"), branch, plan.path.as_os_str(), base],
        };
        let add = GitCmd::repo(RepoOp::WorktreeAdd).args(args).heard(said);
        let follow = async {
            let (mut said, mut last) = (Vec::new(), None);
            while let Some(chunk) = chunks.recv().await {
                said.extend(chunk);
                let changed = percent(&mut said).filter(|n| last != Some(*n));
                if let Some(n) = changed {
                    last = Some(n);
                    heard(progress(WorkspacePhase::Checkout, Some(n)));
                }
            }
        };
        crate::failpoint!(WORKTREE_ADDING);
        let (added, ()) = tokio::join!(self.run(&plan.repo, writable, add), follow);
        heard_notice(heard, added.map_err(failed)?.notice);
        crate::failpoint!(WORKTREE_ADDED);
        Ok(())
    }

    pub(super) async fn run(
        &self,
        at: &Path,
        writable: &[PathBuf],
        cmd: GitCmd<Repo>,
    ) -> Result<Done, GitError> {
        let Some(git) = &self.git else {
            let missing = std::io::Error::other("no git to run");
            return Err(GitError::NotARepo(at.to_owned(), missing));
        };
        let tier = git.repo_tier(at);
        tier.run(&*self.clock, &self.durations, writable, cmd).await
    }

    /// Refused, `lfs_object_missing`, while the repository lacks an LFS
    /// object of `tree`'s, which the checkout would download (T237).
    async fn lfs_objects(
        &self,
        repo: &Path,
        writable: &[PathBuf],
        tree: &OsStr,
        failed: &impl Fn(GitError) -> RpcError,
    ) -> Result<(), RpcError> {
        let Some(git) = &self.git else {
            return Ok(());
        };
        let (clock, durations) = (&*self.clock, &self.durations);
        let stores = lfs::Stores::of(git, repo, clock, durations, writable).await;
        let stores = stores.map_err(failed)?;
        if !stores.runs {
            return Ok(());
        }
        let oids = lfs::pointers(&git.repo_tier(repo), tree, clock, durations, writable).await;
        let oids = oids.map_err(failed)?;
        let copied = tokio::task::spawn_blocking(move || lfs::supply(&stores, &oids)).await;
        lfs::none_missing(&copied.map_err(internal)?.map_err(internal)?)
    }

    pub(super) fn failed(&self, err: GitError) -> RpcError {
        err.reported("worktree", &self.audit, &*self.clock)
    }

    /// `<root>/<repo>-<hash>`, made 0700 as far as it is new, as a real
    /// path: the name of the repository's main work tree (or of a bare
    /// repository, without `.git`), and the start of its common dir's
    /// SHA-256.
    fn dir(&self, repo: &Path) -> Result<PathBuf, RpcError> {
        let common = Layout::read(repo).map_err(internal)?.common_dir;
        let main = if common.ends_with(".git") {
            common.parent().unwrap_or(&common)
        } else {
            &common
        };
        let name = main.file_name().unwrap_or_default().to_string_lossy();
        let hash = Sha256::digest(common.as_os_str().as_bytes());
        let hash: String = hash[..4].iter().map(|b| format!("{b:02x}")).collect();
        let root = PathBuf::from(self.settings.current().worktrees_root.0);
        let dir = root.join(format!("{}-{hash}", name.trim_end_matches(".git")));
        let mut builder = DirBuilder::new();
        builder.recursive(true).mode(0o700);
        builder.create(&dir).map_err(internal)?;
        dir.canonicalize().map_err(internal)
    }
}

/// What a session's worktree is locked with (design 5.2.10).
pub(super) fn lock_reason(session: &SessionId) -> String {
    format!("cupper session {session}")
}

fn branch(name: &str) -> String {
    format!("cupper/{name}")
}

fn progress(phase: WorkspacePhase, progress: Option<u8>) -> Heard {
    Heard::Progress(WorkspacePreparing { phase, progress })
}

fn heard_notice(heard: &impl Fn(Heard), notice: Option<RepoNotice>) {
    if let Some(notice) = notice {
        heard(Heard::Notice(notice));
    }
}

/// Whether `listed` (`worktree list --porcelain -z`) has the worktree at
/// `path`, and then its lock's reason, empty for a lock without one.
pub(super) fn listed(listed: &[u8], path: &Path) -> Option<Option<String>> {
    let want = [b"worktree ", path.as_os_str().as_bytes()].concat();
    let fields: Vec<&[u8]> = listed.split(|b| *b == 0).collect();
    let mut records = fields.split(|field| field.is_empty());
    let record = records.find(|record| record.first() == Some(&want.as_slice()))?;
    let lock = record
        .iter()
        .find_map(|field| field.strip_prefix(b"locked"));
    Some(lock.map(|reason| String::from_utf8_lossy(reason.trim_ascii_start()).into_owned()))
}

/// The percentage of the last `Updating files: n% (..)` among the whole
/// lines of what git `said`, which it takes out of `said`.
fn percent(said: &mut Vec<u8>) -> Option<u8> {
    let end = said.iter().rposition(|b| matches!(b, b'\r' | b'\n'))?;
    let whole: Vec<u8> = said.drain(..=end).collect();
    let whole = String::from_utf8_lossy(&whole);
    whole.split(['\r', '\n']).rev().find_map(|line| {
        let rest = line.strip_prefix("Updating files:")?;
        rest.trim_start().split('%').next()?.parse().ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_git_worktree_progress_is_the_last_whole_line_of_the_checkout() {
        let mut said = b"Preparing worktree (new branch 'cupper/x')\nUpdating files:".to_vec();
        assert_eq!(percent(&mut said), None);
        assert_eq!(said, b"Updating files:");
        said.extend(b"  1% (20/2000)\rUpdating files:  2% (40/2000)\rUpdating files:  3");
        assert_eq!(percent(&mut said), Some(2));
        said.extend(b"% (60/2000)\rUpdating files: 100% (2000/2000), done.\n");
        assert_eq!((percent(&mut said), said.len()), (Some(100), 0));
        said.extend(b"HEAD is now at c31b134 init\n");
        assert_eq!(percent(&mut said), None);
    }
}
