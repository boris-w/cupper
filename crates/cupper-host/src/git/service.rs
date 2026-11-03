//! The git service's port (design 5.2.10, 5.3.4): `git/status`, `git/diff`,
//! `git/file_diff` and `git/branches`, each call in its reply's task. A
//! session's workspace and turn trees come from the store, a turn's notes
//! from the session's view, and the diffs made stay in a [`DiffCache`] for
//! their `diff_token` to name.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cupper_protocol::error::{ErrorCode, ErrorReason, INVALID_PARAMS, RpcError};
use cupper_protocol::ids::{DiffToken, SessionId, TurnId};
use cupper_protocol::limits::V1;
use cupper_protocol::methods::git::{
    DiffCaveat, DiffHunk, DiffScope, DiffSnapshot, GitBranches, GitBranchesParams,
    GitBranchesResult, GitDiffParams, GitDiffResult, GitFileDiff, GitFileDiffParams,
    GitFileDiffResult, GitStatus, GitStatusParams, GitStatusResult,
};
use cupper_protocol::rpc::Method;
use cupper_protocol::sync::StreamId;
use cupper_protocol::view::{SessionView, View};
use futures::FutureExt;
use rusqlite::OptionalExtension;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::diff::{self, DiffCache, Diffed, Range, turn_notes};
use super::head::{self, Head};
use super::run::RunError;
use super::snapshot::ShotError;
use super::{GitError, GitTool, status};
use crate::clock::{Clock, IdGen};
use crate::durations::{DIFF_CACHE_TTL, Durations};
use crate::gateway::{AuditRecord, Call, Port, Reply, Routes};
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
}

/// The git service. Clones share its cache.
#[derive(Debug, Clone)]
pub struct GitService {
    deps: GitDeps,
    cache: Arc<Mutex<DiffCache<Diffed>>>,
}

impl GitService {
    pub fn new(deps: GitDeps) -> Self {
        let cache = DiffCache::new(deps.durations.get(DIFF_CACHE_TTL));
        let cache = Arc::new(Mutex::new(cache));
        Self { deps, cache }
    }

    pub fn route(self, routes: &mut Routes) {
        routes.git(Arc::new(self));
    }

    async fn diff(&self, params: GitDiffParams) -> Result<GitDiffResult, RpcError> {
        let (tool, work_tree) = (self.tool()?, self.workspace(&params.session_id).await?);
        let (range, notes) = match &params.scope {
            DiffScope::Unstaged => (Range::Unstaged, None),
            DiffScope::Staged => (
                Range::Staged(self.head(tool, &work_tree).await?.commit),
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
            let patch = diff::patch(tool, &diffed, file, clock, durations).await;
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
        let head = self.head(tool, &work_tree).await?;
        let (clock, durations) = (&*self.deps.clock, &self.deps.durations);
        let commit = head.commit.as_deref();
        let found = status::files(tool, &work_tree, commit, clock, durations).await;
        let (files, in_progress) = found.map_err(shot)?;
        Ok(GitStatusResult {
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
        listed.map_err(|err| self.reported("for_each_ref", err))
    }

    /// HEAD of `work_tree`, through the repo tier.
    async fn head(&self, tool: &GitTool, work_tree: &Path) -> Result<Head, RpcError> {
        let writable = self.writable().await?;
        let (clock, durations) = (&*self.deps.clock, &self.deps.durations);
        let tier = tool.repo_tier(work_tree);
        let head = head::head(&tier, clock, durations, &writable).await;
        head.map_err(|err| self.reported("head", err))
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

    fn reported(&self, command: &str, err: GitError) -> RpcError {
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

fn not_found(what: &str) -> RpcError {
    RpcError::app(ErrorCode::NotFound, format!("no such {what}"))
}

fn internal(err: impl std::fmt::Display) -> RpcError {
    RpcError::app(ErrorCode::Internal, err.to_string())
}
