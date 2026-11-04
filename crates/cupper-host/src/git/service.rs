//! The git service's port (design 5.2.10, 5.3.4): `git/status`, `git/diff`,
//! `git/file_diff`, `git/branches`, and the writes `git/stage`,
//! `git/unstage` and `git/revert`, each call in its reply's task. A
//! session's workspace and turn trees come from the store, a turn's notes
//! from the session's view, and the diffs made stay in a [`DiffCache`] for
//! their `diff_token` to name. The repo tier's notices go to the session's
//! stream through its actor, the stream's only writer.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cupper_protocol::annotations::WirePath;
use cupper_protocol::error::{ErrorCode, ErrorReason, INVALID_PARAMS, RpcError};
use cupper_protocol::ids::{DiffToken, SessionId, TurnId};
use cupper_protocol::limits::V1;
use cupper_protocol::methods::git::{
    DiffCaveat, DiffHunk, DiffScope, DiffSnapshot, GitApplyParams, GitBranches, GitBranchesParams,
    GitBranchesResult, GitCommit, GitCommitParams, GitCommitResult, GitDiffParams, GitDiffResult,
    GitFileDiff, GitFileDiffParams, GitFileDiffResult, GitPush, GitPushParams, GitPushResult,
    GitRevert, GitStage, GitStatus, GitStatusParams, GitStatusResult, GitTarget, GitUnstage,
};
use cupper_protocol::rpc::Method;
use cupper_protocol::sync::StreamId;
use cupper_protocol::view::{SessionView, View};
use futures::FutureExt;
use futures::future::BoxFuture;
use rusqlite::OptionalExtension;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::diff::{self, DiffCache, Diffed, Range, turn_notes};
use super::head::{self, Head};
use super::run::RunError;
use super::snapshot::ShotError;
use super::stamp::Layout;
use super::{
    GitCmd, GitError, GitTool, RepoNotice, RepoOp, User, UserOp, lfs, rules, snapshot, status, user,
};
use crate::clock::{Clock, IdGen};
use crate::durations::{DIFF_CACHE_TTL, Durations};
use crate::gateway::{AuditRecord, Call, Port, Reply, Routes};
use crate::ledger::{Outcome, Ticket};
use crate::store::streams::{StreamView, Streams};
use crate::store::{BlobStore, Readers, log};

/// What the git service needs of the host.
#[derive(Debug, Clone)]
pub struct GitDeps {
    /// `None` when no trusted `PATH` entry has a git.
    pub git: Option<GitTool>,
    pub readers: Readers,
    pub streams: Streams,
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<IdGen>,
    pub durations: Durations,
    /// Where hunks too long to send inline go.
    pub blobs: BlobStore,
    /// For `git.unsafe_repo_config`.
    pub audit: mpsc::UnboundedSender<AuditRecord>,
    /// Where the repo tier's notices go.
    pub notes: Arc<dyn SessionNotes>,
}

/// The session streams, for notices of the repo tier (design 5.2.10, rules
/// 2, 5 and 6), which the session's actor writes.
pub trait SessionNotes: Send + Sync + std::fmt::Debug {
    fn notice(&self, session: &SessionId, notice: RepoNotice);
    /// The paths of the session's `repo_config_changed` notices so far.
    fn repo_config_changed(&self, session: &SessionId) -> BoxFuture<'static, Vec<WirePath>>;
}

/// The git service. Clones share its cache and locks.
#[derive(Debug, Clone)]
pub struct GitService {
    deps: GitDeps,
    cache: Arc<Mutex<DiffCache<Diffed>>>,
    locks: Arc<RepoLock>,
}

/// One lock per repository, for what changes a user's index or work tree
/// (design 5.2.10, "并发"); the snapshot tier and reads take none.
#[derive(Debug, Default)]
pub(super) struct RepoLock(Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>);

impl RepoLock {
    /// The lock of the repository `work_tree` belongs to, by its common dir.
    pub(super) async fn hold(
        &self,
        work_tree: &Path,
    ) -> Result<tokio::sync::OwnedMutexGuard<()>, RpcError> {
        let tree = work_tree.to_owned();
        let layout = tokio::task::spawn_blocking(move || Layout::read(&tree.canonicalize()?));
        let layout = layout.await.map_err(internal)?.map_err(internal)?;
        let lock = {
            let mut locks = self.0.lock().unwrap_or_else(PoisonError::into_inner);
            Arc::clone(locks.entry(layout.common_dir).or_default())
        };
        Ok(lock.lock_owned().await)
    }
}

impl GitService {
    pub fn new(deps: GitDeps) -> Self {
        let cache = DiffCache::new(deps.durations.get(DIFF_CACHE_TTL));
        let cache = Arc::new(Mutex::new(cache));
        let locks = Arc::default();
        Self { deps, cache, locks }
    }

    pub fn route(self, routes: &mut Routes) {
        routes.git(Arc::new(self));
    }

    async fn diff(&self, params: GitDiffParams) -> Result<GitDiffResult, RpcError> {
        let (tool, work_tree) = (self.tool()?, self.workspace(&params.session_id).await?);
        let (range, notes) = match &params.scope {
            DiffScope::Unstaged => (Range::Unstaged, None),
            DiffScope::Staged => (
                Range::Staged(
                    self.head(&params.session_id, tool, &work_tree)
                        .await?
                        .commit,
                ),
                None,
            ),
            DiffScope::LastTurn => self.turn(&params.session_id, None).await?,
            DiffScope::Turn(turn) => self.turn(&params.session_id, Some(turn)).await?,
        };
        let (clock, durations) = (&*self.deps.clock, &self.deps.durations);
        let diffed = diff::diff(tool, &work_tree, range, clock, durations).await;
        let diffed = diffed.map_err(shot)?;
        let mut caveats = Vec::new();
        let snapshot = notes.map(|(mut snapshot, late)| {
            caveats.push(DiffCaveat::MixedChanges);
            if late {
                caveats.push(DiffCaveat::LateToolEffects);
            }
            // A running turn's end is the snapshot taken now.
            if let Some(taken) = &diffed.taken {
                diff::add(&mut snapshot, taken.partial, &taken.excluded);
            }
            snapshot
        });
        let token = format!("dt_{:032x}", self.deps.ids.new_id().as_u128());
        let diff_token: DiffToken = token.parse().expect("dt_ and hex digits");
        let (files, stat) = (diffed.files.clone(), diff::stat(&diffed.files));
        let now = clock.now_mono();
        self.cache()
            .insert(diff_token.clone(), &work_tree, diffed, now);
        Ok(GitDiffResult {
            diff_token,
            files,
            snapshot,
            stat,
            caveats,
        })
    }

    /// The units of one file of a diff in the cache; none for a file under
    /// a filter the host does not run.
    async fn file_diff(&self, params: GitFileDiffParams) -> Result<GitFileDiffResult, RpcError> {
        let (tool, work_tree) = (self.tool()?, self.workspace(&params.session_id).await?);
        let (clock, durations) = (&*self.deps.clock, &self.deps.durations);
        let diffed = self
            .cache()
            .get(&params.diff_token, &work_tree, clock.now_mono());
        let stale = || {
            error(
                ErrorCode::Conflict,
                Some(ErrorReason::StaleDiff),
                "no such diff",
            )
        };
        let diffed = diffed.ok_or_else(stale)?;
        let file = diffed.files.iter().find(|f| f.path == params.path);
        let file = file.ok_or_else(|| not_found("file in the diff"))?;
        let mut hunks = Vec::new();
        if !file.filter_unsupported {
            let patch = diff::patch(tool, &diffed, Some(&[file]), clock, durations).await;
            hunks = self.inline(diff::units(&patch.map_err(shot)?)).await?;
        }
        Ok(GitFileDiffResult {
            path: params.path,
            hunks,
        })
    }

    /// `hunks` with the lines past the inline limit in blobs, and those of
    /// every later hunk once the result would outgrow a frame (design 5.3.1).
    async fn inline(&self, mut hunks: Vec<DiffHunk>) -> Result<Vec<DiffHunk>, RpcError> {
        // Room for the envelope and the path.
        let mut room = V1.max_frame_bytes as usize - 8_000;
        for hunk in &mut hunks {
            if size(&hunk.lines) > V1.max_inline_bytes as usize || size(hunk) > room {
                let lines = hunk.lines.take().unwrap_or_default();
                let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
                let put = self.deps.blobs.put(text.into(), "text/x-diff".into(), None);
                hunk.lines_blob = Some(put.await.map_err(internal)?);
            }
            room = room.saturating_sub(size(hunk));
        }
        Ok(hunks)
    }

    async fn status(&self, params: GitStatusParams) -> Result<GitStatusResult, RpcError> {
        let (tool, work_tree) = (self.tool()?, self.workspace(&params.session_id).await?);
        let head = self.head(&params.session_id, tool, &work_tree).await?;
        let (clock, durations) = (&*self.deps.clock, &self.deps.durations);
        let commit = head.commit.as_deref();
        let found = status::files(tool, &work_tree, commit, clock, durations).await;
        let (files, in_progress) = found.map_err(shot)?;
        let notes = &self.deps.notes;
        let changed = notes.repo_config_changed(&params.session_id).await;
        Ok(GitStatusResult {
            repo_config_changed: (!changed.is_empty()).then_some(changed),
            branch: head.branch,
            upstream: head.upstream,
            ahead: head.ahead,
            behind: head.behind,
            in_progress,
            files,
        })
    }

    async fn branches(&self, params: GitBranchesParams) -> Result<GitBranchesResult, RpcError> {
        let tool = self.tool()?;
        let id = params.project_id.to_string();
        let sql = "SELECT path FROM projects WHERE project_id = ?1 AND removed_at IS NULL";
        let path = self.read(move |c| c.query_row(sql, [id], |r| r.get::<_, String>(0)).optional());
        let path = path.await?.ok_or_else(|| not_found("project"))?;
        let writable = self.writable().await?;
        let tier = tool.repo_tier(Path::new(&path));
        let (clock, durations) = (&*self.deps.clock, &self.deps.durations);
        let listed = status::branches(&tier, clock, durations, &writable).await;
        listed.map_err(|err| self.reported(None, "for_each_ref", err))
    }

    /// `git/stage`, `git/unstage` or `git/revert` (`op`): the units a cached
    /// diff's target names, applied in the repo tier under the repository's
    /// lock once the command is `pending_external` on disk (design 5.2.5).
    async fn write(&self, op: RepoOp, call: Call) -> Outcome {
        let (params, mut ticket) = (parse::<GitApplyParams>(&call.params), call.ticket);
        let prepared = match params {
            Ok(params) => self.patch(op, &params).await.map(|made| (params, made)),
            Err(err) => Err(err),
        };
        let (params, (work_tree, patch)) = match prepared {
            Ok(prepared) => prepared,
            Err(err) => {
                if let Some(ticket) = ticket {
                    ticket.reject(err.clone());
                }
                return Err(err);
            }
        };
        let (session, notes) = (&params.session_id, &self.deps.notes);
        let lock = self.locks.hold(&work_tree).await?;
        if let Err(err) = self.objects(op, session, &work_tree, &patch).await {
            if let Some(ticket) = ticket {
                ticket.reject(err.clone());
            }
            return Err(err);
        }
        if let Some(ticket) = ticket.as_mut() {
            ticket.mark_external().await.map_err(internal)?;
        }
        crate::failpoint!(GIT_BEFORE_APPLY);
        let cmd = GitCmd::repo(op).input(patch);
        let (tier, writable) = (self.tool()?.repo_tier(&work_tree), self.writable().await?);
        let done = tier.run(&*self.deps.clock, &self.deps.durations, &writable, cmd);
        let done = done.await;
        crate::failpoint!(GIT_AFTER_APPLY);
        drop(lock);
        let err = match done {
            Ok(done) => {
                if let Some(notice) = done.notice {
                    notes.notice(session, notice);
                }
                let outcome = Ok(json!({}));
                if let Some(ticket) = ticket {
                    let committed = ticket.complete(outcome.clone()).commit().await;
                    committed.map_err(internal)?;
                }
                return outcome;
            }
            Err(err) => err,
        };
        // Killed, or gone wrong after git ran: whether it applied is unknown,
        // which the ledger tells a retry. Otherwise nothing changed, and a
        // retry may run it again.
        let unknown = matches!(err, GitError::Run(_) | GitError::Fingerprint(_));
        let lock_held =
            matches!(&err, GitError::Failed { stderr, .. } if stderr.contains("index.lock"));
        let err = match &err {
            GitError::Failed { .. } if lock_held => {
                notes_of(notes, session, &err);
                error(
                    ErrorCode::GitError,
                    Some(ErrorReason::LockHeld),
                    "index.lock is held",
                )
            }
            GitError::Failed { .. } => {
                notes_of(notes, session, &err);
                stale("the patch does not apply")
            }
            _ => self.reported(Some(session), "apply", err),
        };
        if let Some(ticket) = ticket.filter(|_| !unknown) {
            ticket.reject(err.clone());
        }
        Err(err)
    }

    /// The LFS objects of the pointers `op` writes with `patch`, from
    /// cupper's store into the user's (T235).
    async fn objects(
        &self,
        op: RepoOp,
        session: &SessionId,
        work_tree: &Path,
        patch: &[u8],
    ) -> Result<(), RpcError> {
        let oids = lfs::wanted(patch, op);
        if oids.is_empty() {
            return Ok(());
        }
        let (tool, writable) = (self.tool()?, self.writable().await?);
        let (clock, durations) = (&*self.deps.clock, &self.deps.durations);
        let list = GitCmd::repo(RepoOp::ConfigList);
        let listed = tool
            .repo_tier(work_tree)
            .run(clock, durations, &writable, list)
            .await;
        let listed = listed.map_err(|err| self.reported(Some(session), "lfs", err))?;
        let entries = rules::parse(&listed.output.stdout, work_tree).map_err(internal)?;
        let (dir, tree) = (tool.dir.clone(), work_tree.to_owned());
        let copied = tokio::task::spawn_blocking(move || {
            let layout = Layout::read(&tree.canonicalize()?)?;
            let private = snapshot::private(&dir, &layout).1.join("lfs");
            let (store, mode) = (lfs::store(&layout, &entries), lfs::shared(&entries));
            lfs::supply(&private, &store, &oids, mode)
        });
        let failed = |err: std::io::Error| {
            let message = format!("cannot copy an LFS object into the repository: {err}");
            error(ErrorCode::GitError, None, &message)
        };
        copied.await.map_err(internal)?.map_err(failed)
    }

    /// `git/commit` (design 5.2.10, the user tier): `commit -F -`, the
    /// message on stdin, the repository's hooks and signing as in a
    /// terminal; then the new commit, read through the repo tier.
    async fn commit(&self, call: Call) -> Outcome {
        let mut ticket = call.ticket;
        let asked = async {
            let params: GitCommitParams = parse(&call.params)?;
            let work_tree = self.workspace(&params.session_id).await?;
            Ok((params, work_tree))
        };
        let (params, work_tree) = refuse(asked.await, &mut ticket)?;
        let (op, tool) = (UserOp::Commit, self.tool()?);
        let cmd = GitCmd::user(op).input(params.message.into_bytes());
        self.user(op, &work_tree, cmd, &mut ticket).await?;
        // The commit stands: should HEAD not be read, the ledger answers
        // retries `outcome_unknown`.
        let head = self.head(&params.session_id, tool, &work_tree).await?;
        let oid = head.commit.unwrap_or_default();
        settle(ticket, Ok(json!(GitCommitResult { oid }))).await
    }

    /// `git/push` (the user tier): the current branch, to its upstream, or
    /// with `--set-upstream` to the upstream's remote, else `origin`.
    async fn push(&self, call: Call) -> Outcome {
        let mut ticket = call.ticket;
        let asked = async {
            let params: GitPushParams = parse(&call.params)?;
            let work_tree = self.workspace(&params.session_id).await?;
            let tool = self.tool()?;
            let head = self.head(&params.session_id, tool, &work_tree).await?;
            let detached = || RpcError::app(ErrorCode::GitError, "HEAD is detached");
            let branch = head.branch.ok_or_else(detached)?;
            let remote = head.remote.unwrap_or_else(|| "origin".into());
            Ok((params, work_tree, branch, remote))
        };
        let (params, work_tree, branch, remote) = refuse(asked.await, &mut ticket)?;
        let args = match params.set_upstream {
            true => vec!["--set-upstream", &remote, "HEAD"],
            false => Vec::new(),
        };
        let (op, cmd) = (UserOp::Push, GitCmd::user(UserOp::Push).args(args));
        self.user(op, &work_tree, cmd, &mut ticket).await?;
        settle(ticket, Ok(json!(GitPushResult { remote, branch }))).await
    }

    /// Runs `cmd` in the user tier once the command is `pending_external`
    /// on disk; a commit under the repository's lock (design 5.2.10, "并发").
    /// A failure rejects the command. One that may have taken effect, git
    /// killed at its time limit or not started, is `outcome_unknown`, as
    /// the ledger then answers its retries.
    async fn user(
        &self,
        op: UserOp,
        work_tree: &Path,
        cmd: GitCmd<User>,
        ticket: &mut Option<Ticket>,
    ) -> Result<(), RpcError> {
        let tier = self.tool()?.user_tier(work_tree);
        let lock = match op {
            UserOp::Commit => Some(self.locks.hold(work_tree).await?),
            UserOp::Push => None,
        };
        if let Some(ticket) = ticket.as_mut() {
            ticket.mark_external().await.map_err(internal)?;
        }
        let prepared = tier.prepare(cmd);
        let done = prepared.run(&*self.deps.clock, &self.deps.durations).await;
        drop(lock);
        match done {
            Ok(output) if output.status.success() => Ok(()),
            Ok(output) => {
                let err = user::failed(op, &output);
                if let Some(ticket) = ticket.take() {
                    ticket.reject(err.clone());
                }
                Err(err)
            }
            Err(err) => {
                drop(ticket.take());
                let reason = Some(ErrorReason::OutcomeUnknown);
                Err(error(ErrorCode::Conflict, reason, &err.to_string()))
            }
        }
    }

    /// The work tree and the patch `op` applies for `params`: the target's
    /// units of the cached diff, which must be of the range `op` goes with.
    /// Staging checks the user's own attributes too.
    async fn patch(
        &self,
        op: RepoOp,
        params: &GitApplyParams,
    ) -> Result<(PathBuf, Vec<u8>), RpcError> {
        let (tool, work_tree) = (self.tool()?, self.workspace(&params.session_id).await?);
        let (clock, durations) = (&*self.deps.clock, &self.deps.durations);
        let diffed = self
            .cache()
            .get(&params.diff_token, &work_tree, clock.now_mono());
        let diffed = diffed.ok_or_else(|| stale("no such diff"))?;
        // Staging takes the unstaged changes, unstaging the staged ones;
        // reverting those of the work tree or of a turn.
        let fits = matches!(
            (op, &diffed.range),
            (RepoOp::Stage, Range::Unstaged)
                | (RepoOp::Unstage, Range::Staged(_))
                | (RepoOp::Revert, Range::Unstaged | Range::Turn(..))
        );
        if !fits {
            return Err(stale("the diff is of another range"));
        }
        let filtered = || {
            error(
                ErrorCode::GitError,
                Some(ErrorReason::FilterUnsupported),
                "a filter the host does not run",
            )
        };
        let made = |patch: Result<Vec<u8>, ShotError>| patch.map_err(shot);
        let (patch, files) = match &params.target {
            GitTarget::All(false) => return Err(RpcError::new(INVALID_PARAMS, "`all` is true")),
            GitTarget::All(true) => (
                made(diff::patch(tool, &diffed, None, clock, durations).await)?,
                None,
            ),
            GitTarget::Paths(paths) => {
                let found = paths
                    .iter()
                    .map(|p| diffed.files.iter().find(|f| f.path == *p));
                let files: Vec<_> = found
                    .collect::<Option<_>>()
                    .ok_or_else(|| stale("a path is not in the diff"))?;
                if files.iter().any(|f| f.filter_unsupported) {
                    return Err(filtered());
                }
                let patch = diff::patch(tool, &diffed, Some(&files), clock, durations).await;
                (made(patch)?, Some(files))
            }
            GitTarget::HunkIds(ids) => {
                let whole = made(diff::patch(tool, &diffed, None, clock, durations).await)?;
                let ids: BTreeSet<_> = ids.iter().cloned().collect();
                let (patch, found) = diff::pick(&whole, &ids);
                if found < ids.len() {
                    return Err(stale("a hunk is not in the diff"));
                }
                (patch, None)
            }
        };
        // The target's files are all under a filter, or the diff is empty.
        if patch.is_empty() {
            return Err(stale("nothing to apply"));
        }
        if op == RepoOp::Stage && !diffed.unrun.is_empty() {
            let all = diffed
                .files
                .iter()
                .filter(|f| !f.filter_unsupported)
                .collect();
            let targets = files.unwrap_or(all);
            let paths = targets
                .iter()
                .flat_map(|f| [f.path.0.as_bytes(), b"\0"].concat());
            let check = GitCmd::repo(RepoOp::CheckAttr).input(paths.collect());
            let writable = self.writable().await?;
            let tier = tool.repo_tier(&work_tree);
            let done = tier.run(clock, durations, &writable, check).await;
            let done =
                done.map_err(|err| self.reported(Some(&params.session_id), "check_attr", err))?;
            if !snapshot::filtered(&done.output.stdout, &diffed.unrun).is_empty() {
                return Err(filtered());
            }
        }
        Ok((work_tree, patch))
    }

    /// HEAD of `session`'s `work_tree`, through the repo tier.
    async fn head(
        &self,
        session: &SessionId,
        tool: &GitTool,
        work_tree: &Path,
    ) -> Result<Head, RpcError> {
        let writable = self.writable().await?;
        let (clock, durations) = (&*self.deps.clock, &self.deps.durations);
        let tier = tool.repo_tier(work_tree);
        let head = head::head(&tier, clock, durations, &writable).await;
        head.map_err(|err| self.reported(Some(session), "head", err))
    }

    /// A turn range: its trees from its `turns` row (the latest turn's
    /// without `turn`), its notes from the session's view.
    async fn turn(&self, session: &SessionId, turn: Option<&TurnId>) -> Result<Turn, RpcError> {
        let (id, asked) = (session.to_string(), turn.map(ToString::to_string));
        let row = self.read(move |c| {
            let sql = "SELECT turn_id, turn_no, status, snapshot_start, snapshot_end FROM turns
                WHERE session_id = ?1 AND (?2 IS NULL OR turn_id = ?2) ORDER BY turn_no DESC LIMIT 1";
            let row = |r: &rusqlite::Row<'_>| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?));
            c.query_row(sql, (id, asked), row).optional()
        });
        let row: Option<TurnRow> = row.await?;
        let (id, turn_no, status, start, end) = match row {
            Some(row) => row,
            None if turn.is_some() => return Err(not_found("turn")),
            None => return Err(missing()),
        };
        let range = match (start, end) {
            (Some(start), Some(end)) => Range::Turn(start, Some(end)),
            (Some(start), None) if status == "running" => Range::Turn(start, None),
            _ => return Err(missing()),
        };
        let id: TurnId = id.parse().map_err(internal)?;
        let stream = StreamId::Session(session.clone());
        let handle = self.deps.streams.open(&stream).await.map_err(internal)?;
        let notes = handle.read(|view, _| match view {
            StreamView::Session(view) => turn_notes(view, &id),
            StreamView::Index(_) => None,
        });
        if notes.is_some() {
            return Ok((range, notes));
        }
        // Before the view's window: the turn's snapshot in the store.
        let session = session.clone();
        let ops = self.read(move |c| Ok(log::turn_ops(c, &session, turn_no)));
        let mut view = SessionView::default();
        for op in ops.await?.map_err(internal)?.unwrap_or_default() {
            // A turn snapshot applies whole; nothing to skip.
            let _ = view.apply(&op);
        }
        Ok((range, turn_notes(&view, &id)))
    }

    fn tool(&self) -> Result<&GitTool, RpcError> {
        let reason = Some(ErrorReason::NotARepo);
        let unsupported = || error(ErrorCode::Unsupported, reason, "no git to run");
        self.deps.git.as_ref().ok_or_else(unsupported)
    }

    async fn workspace(&self, session: &SessionId) -> Result<PathBuf, RpcError> {
        let id = session.to_string();
        let sql = "SELECT workspace_path FROM sessions WHERE session_id = ?1";
        let path = self.read(move |c| c.query_row(sql, [id], |r| r.get::<_, String>(0)).optional());
        path.await?
            .map(PathBuf::from)
            .ok_or_else(|| not_found("session"))
    }

    /// Every project and session workspace, where the repo tier takes no
    /// configuration from (design 5.2.10).
    async fn writable(&self) -> Result<Vec<PathBuf>, RpcError> {
        self.read(|c| {
            let sql = "SELECT path FROM projects WHERE removed_at IS NULL
                UNION SELECT workspace_path FROM sessions";
            let mut select = c.prepare(sql)?;
            let paths = select.query_map([], |r| r.get::<_, String>(0).map(PathBuf::from))?;
            paths.collect()
        })
        .await
    }

    async fn read<T: Send + 'static>(
        &self,
        read: impl FnOnce(&rusqlite::Connection) -> rusqlite::Result<T> + Send + 'static,
    ) -> Result<T, RpcError> {
        self.deps.readers.read(read).await.map_err(internal)
    }

    /// The protocol's error for a repo tier call; its notice goes to
    /// `session`'s stream.
    fn reported(&self, session: Option<&SessionId>, command: &str, err: GitError) -> RpcError {
        if let Some(session) = session {
            notes_of(&self.deps.notes, session, &err);
        }
        err.reported(command, &self.deps.audit, &*self.deps.clock)
    }

    fn cache(&self) -> MutexGuard<'_, DiffCache<Diffed>> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A `turns` row: ID, number, status, start and end trees.
type TurnRow = (String, u32, String, Option<String>, Option<String>);

/// A turn range and its notes, if the turn has any.
type Turn = (Range, Option<(DiffSnapshot, bool)>);

impl Port for GitService {
    fn deliver(&self, call: Call) -> Reply {
        let service = self.clone();
        async move {
            match call.method {
                GitStatus::NAME => Ok(json!(service.status(parse(&call.params)?).await?)),
                GitFileDiff::NAME => Ok(json!(service.file_diff(parse(&call.params)?).await?)),
                GitBranches::NAME => Ok(json!(service.branches(parse(&call.params)?).await?)),
                GitStage::NAME => service.write(RepoOp::Stage, call).await,
                GitUnstage::NAME => service.write(RepoOp::Unstage, call).await,
                GitRevert::NAME => service.write(RepoOp::Revert, call).await,
                GitCommit::NAME => service.commit(call).await,
                GitPush::NAME => service.push(call).await,
                _ => Ok(json!(service.diff(parse(&call.params)?).await?)),
            }
        }
        .boxed()
    }
}

fn parse<T: DeserializeOwned>(params: &Value) -> Result<T, RpcError> {
    T::deserialize(params).map_err(|err| RpcError::new(INVALID_PARAMS, err.to_string()))
}

/// The size of `value` in the wire's JSON.
fn size(value: &impl serde::Serialize) -> usize {
    serde_json::to_string(value).map_or(0, |json| json.len())
}

/// A snapshot tier failure as the protocol has it.
fn shot(err: ShotError) -> RpcError {
    let (code, reason) = match &err {
        ShotError::NotARepo(_) => (ErrorCode::Unsupported, Some(ErrorReason::NotARepo)),
        ShotError::Unmerged => (ErrorCode::Unavailable, Some(ErrorReason::IndexUnmerged)),
        ShotError::Run(RunError::Timeout) => (ErrorCode::Timeout, None),
        _ => (ErrorCode::GitError, None),
    };
    error(code, reason, &err.to_string())
}

fn error(code: ErrorCode, reason: Option<ErrorReason>, message: &str) -> RpcError {
    let error = RpcError::app(code, message);
    reason.map_or(error.clone(), |reason| error.with_reason(reason))
}

fn missing() -> RpcError {
    let reason = Some(ErrorReason::SnapshotMissing);
    error(ErrorCode::Unavailable, reason, "the turn has no snapshots")
}

/// `outcome`, the command's, with its ticket completed.
async fn settle(ticket: Option<Ticket>, outcome: Outcome) -> Outcome {
    if let Some(ticket) = ticket {
        let committed = ticket.complete(outcome.clone()).commit().await;
        committed.map_err(internal)?;
    }
    outcome
}

/// `asked`, or its error with the command rejected: it never ran.
fn refuse<T>(asked: Result<T, RpcError>, ticket: &mut Option<Ticket>) -> Result<T, RpcError> {
    asked.inspect_err(|err| {
        if let Some(ticket) = ticket.take() {
            ticket.reject(err.clone());
        }
    })
}

/// The notice of a repo tier call that failed, in `session`'s stream.
fn notes_of(notes: &Arc<dyn SessionNotes>, session: &SessionId, err: &GitError) {
    if let Some(notice) = err.notice() {
        notes.notice(session, notice);
    }
}

fn stale(why: &str) -> RpcError {
    error(ErrorCode::Conflict, Some(ErrorReason::StaleDiff), why)
}

fn not_found(what: &str) -> RpcError {
    RpcError::app(ErrorCode::NotFound, format!("no such {what}"))
}

fn internal(err: impl std::fmt::Display) -> RpcError {
    RpcError::app(ErrorCode::Internal, err.to_string())
}
