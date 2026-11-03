//! The git service's port (design 5.2.10, 5.3.4): `git/diff`, each call in
//! its reply's task. A session's workspace and turn trees come from the
//! store, a turn's notes from the session's view.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cupper_protocol::error::{ErrorCode, ErrorReason, INVALID_PARAMS, RpcError};
use cupper_protocol::ids::{DiffToken, SessionId, TurnId};
use cupper_protocol::methods::git::{
    DiffCaveat, DiffScope, DiffSnapshot, GitDiffParams, GitDiffResult,
};
use cupper_protocol::sync::StreamId;
use cupper_protocol::view::{SessionView, View};
use futures::FutureExt;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::mpsc;

use super::diff::{self, Range, turn_notes};
use super::run::RunError;
use super::snapshot::ShotError;
use super::{GitError, GitTool, head};
use crate::clock::{Clock, IdGen};
use crate::durations::Durations;
use crate::gateway::{AuditRecord, Call, Port, Reply, Routes};
use crate::store::streams::{StreamView, Streams};
use crate::store::{Readers, log};

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
    /// For `git.unsafe_repo_config`.
    pub audit: mpsc::UnboundedSender<AuditRecord>,
}

/// The git service.
#[derive(Debug, Clone)]
pub struct GitService {
    deps: GitDeps,
}

impl GitService {
    pub fn new(deps: GitDeps) -> Self {
        Self { deps }
    }

    pub fn route(self, routes: &mut Routes) {
        routes.git(Arc::new(self));
    }

    async fn diff(&self, params: GitDiffParams) -> Result<GitDiffResult, RpcError> {
        let (tool, work_tree) = (self.tool()?, self.workspace(&params.session_id).await?);
        let (range, notes) = match &params.scope {
            DiffScope::Unstaged => (Range::Unstaged, None),
            DiffScope::Staged => (Range::Staged(self.head(tool, &work_tree).await?), None),
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
        let stat = diff::stat(&diffed.files);
        Ok(GitDiffResult {
            diff_token,
            files: diffed.files,
            snapshot,
            stat,
            caveats,
        })
    }

    /// The commit HEAD of `work_tree` is at, through the repo tier.
    async fn head(&self, tool: &GitTool, work_tree: &Path) -> Result<Option<String>, RpcError> {
        let writable = self.writable().await?;
        let (clock, durations) = (&*self.deps.clock, &self.deps.durations);
        let tier = tool.repo_tier(work_tree);
        let commit = head::commit(&tier, clock, durations, &writable).await;
        commit.map_err(|err| self.reported("head", err))
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
}

/// A `turns` row: ID, number, status, start and end trees.
type TurnRow = (String, u32, String, Option<String>, Option<String>);

/// A turn range and its notes, if the turn has any.
type Turn = (Range, Option<(DiffSnapshot, bool)>);

impl Port for GitService {
    fn deliver(&self, call: Call) -> Reply {
        let service = self.clone();
        async move {
            let params = GitDiffParams::deserialize(&call.params);
            let params = params.map_err(|err| RpcError::new(INVALID_PARAMS, err.to_string()))?;
            Ok(json!(service.diff(params).await?))
        }
        .boxed()
    }
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
