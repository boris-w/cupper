//! A session's archive, restore and rename (design 5.2.6, 5.5.9, PS-3),
//! which the actor does as the one writer of the session's stream and row.

use std::path::PathBuf;

use cupper_protocol::envelope::Cause;
use cupper_protocol::error::{ErrorCode, ErrorReason, RpcError};
use cupper_protocol::event::EventBody;
use cupper_protocol::ids::CommandId;
use cupper_protocol::index::WorkspaceKind;
use cupper_protocol::methods::interaction::InteractionRespond;
use cupper_protocol::methods::session::{
    ArchiveWorktree, RestoreMode, SessionArchive, SessionArchiveParams, SessionArchiveResult,
    SessionConfirmConfig, SessionInterrupt, SessionQueueEdit, SessionQueueResume,
    SessionRenameParams, SessionRestoreResult, SessionSend, SessionSetConfig, WorktreeOutcome,
};
use cupper_protocol::rpc::Method;
use serde_json::json;
use tokio::sync::oneshot;

use super::{Command, Reply, SessionActor, Step};
use crate::gateway::Call;
use crate::git::worktree::Plan;
use crate::ledger::{Outcome, Ticket};
use crate::orchestrator::session::{internal, parse};
use crate::orchestrator::turn;

/// The calls that change a session, which an archived or read-only one
/// refuses.
const WRITES: [&str; 7] = [
    SessionSend::NAME,
    SessionInterrupt::NAME,
    SessionQueueResume::NAME,
    SessionQueueEdit::NAME,
    SessionConfirmConfig::NAME,
    SessionSetConfig::NAME,
    InteractionRespond::NAME,
];

impl SessionActor {
    /// `unavailable` for a write to an archived session, archiving it again
    /// too, which `session/restore` brings back first; and for a write to a
    /// read-only one (PS-3).
    pub(super) fn refused(&self, method: &str) -> Option<RpcError> {
        let writes = WRITES.contains(&method);
        let reason = match () {
            _ if self.archived && (writes || method == SessionArchive::NAME) => {
                ErrorReason::SessionArchived
            }
            _ if self.parts.configured.read_only && writes => ErrorReason::SessionReadOnly,
            _ => return None,
        };
        Some(RpcError::app(ErrorCode::Unavailable, reason.to_string()).with_reason(reason))
    }

    /// A turn runs, or the worktree is still added: no archive now.
    pub(super) fn busy(&self) -> bool {
        self.parts.core.turn_id().is_some() || self.preparing.is_some()
    }

    /// `session/archive` (design 5.5.9).
    pub(super) async fn archive(&mut self, call: Call, reply: oneshot::Sender<Outcome>) {
        let worktree = parse::<SessionArchiveParams>(&call.params).map(|p| p.worktree);
        let cause = cause(&call);
        self.archive_with(worktree, call.ticket, reply, cause).await;
    }

    /// `project/remove`'s archive of the session, its worktree kept.
    pub(super) async fn archive_kept(&mut self, done: oneshot::Sender<Outcome>) {
        let keep = Ok(ArchiveWorktree::Keep);
        self.archive_with(keep, None, done, None).await;
    }

    /// Refused while a turn runs or the worktree is still added; else the
    /// agent's session closed, the worktree kept or removed as asked and the
    /// cleanup rules allow, the session's `archived_at` with the command,
    /// and the session off the index.
    async fn archive_with(
        &mut self,
        worktree: Result<ArchiveWorktree, RpcError>,
        mut ticket: Option<Ticket>,
        reply: oneshot::Sender<Outcome>,
        cause: Option<Cause>,
    ) {
        let running = RpcError::app(ErrorCode::Conflict, "a turn runs");
        let archived = match worktree {
            Ok(_) if self.busy() => Err(running.with_reason(ErrorReason::TurnRunning)),
            Ok(worktree) => {
                let detach = turn::Input::Detach;
                self.input(super::Input::Machine(detach), None, Step::default())
                    .await;
                self.removed(worktree, &mut ticket).await
            }
            Err(err) => Err(err),
        };
        let archived = match archived {
            Ok(archived) => archived,
            Err(err) => return refuse(ticket, reply, err),
        };
        self.archived = true;
        let step = Step {
            reply: Some(Reply::Result(Ok(json!(archived)))),
            archived: Some(true),
            ..Step::default()
        };
        let command = command(ticket, cause, reply);
        self.commit(step, Some(command)).await;
    }

    async fn removed(
        &mut self,
        worktree: ArchiveWorktree,
        ticket: &mut Option<Ticket>,
    ) -> Result<SessionArchiveResult, RpcError> {
        let local = self.parts.configured.workspace.kind != WorkspaceKind::Worktree;
        let (outcome, blockers) = match worktree {
            _ if local => (WorktreeOutcome::Kept, Vec::new()),
            ArchiveWorktree::Keep => (WorktreeOutcome::Kept, Vec::new()),
            worktree => {
                let case_only = worktree == ArchiveWorktree::RemoveCaseOnlyDirty;
                let (repo, path) = (self.repo().await?, self.parts.cwd.clone());
                let worktrees = &self.parts.worktrees;
                let removed = worktrees.remove((&repo, &path), &self.parts.id, case_only, ticket);
                removed.await?
            }
        };
        Ok(SessionArchiveResult {
            worktree: outcome,
            blockers,
        })
    }

    /// `session/restore` (design 5.2.6, PS-3): an archived session back on
    /// the index, a worktree session's worktree brought back with it, or
    /// the session read-only once its branch is gone. One not archived
    /// answers with what it is.
    pub(super) async fn restore(&mut self, call: Call, reply: oneshot::Sender<Outcome>) {
        let (was, cause, mut ticket) = (self.archived, cause(&call), call.ticket);
        let read_only = self.parts.configured.read_only;
        let worktree = self.parts.configured.workspace.kind == WorkspaceKind::Worktree;
        let full = match was && worktree && !read_only {
            true => self.rebuilt(&mut ticket).await,
            false => Ok(!read_only),
        };
        let full = match full {
            Ok(full) => full,
            Err(err) => return refuse(ticket, reply, err),
        };
        let mut step = Step {
            archived: Some(false),
            ..Step::default()
        };
        if !full && !read_only {
            self.parts.configured.read_only = true;
            let configured = self.parts.configured.clone();
            step.events.push(EventBody::SessionConfigured(configured));
        }
        let mode = match full {
            true => RestoreMode::Full,
            false => RestoreMode::ReadOnly,
        };
        step.reply = Some(Reply::Result(Ok(json!(SessionRestoreResult { mode }))));
        self.archived = false;
        self.commit(step, Some(command(ticket, cause, reply))).await;
        if let (true, Ok((summary, _))) = (was, self.summary().await) {
            self.parts.index.created(summary);
        }
    }

    /// The session's worktree back from its branch; `false` without one.
    async fn rebuilt(&mut self, ticket: &mut Option<Ticket>) -> Result<bool, RpcError> {
        let workspace = &self.parts.configured.workspace;
        let (branch, base_ref) = (workspace.branch.clone(), workspace.base_ref.clone());
        let (Some(branch), Some(base_ref)) = (branch, base_ref) else {
            return Ok(false);
        };
        let (repo, path) = (self.repo().await?, self.parts.cwd.clone());
        let plan = Plan {
            repo,
            path,
            branch,
            base_ref,
        };
        if let Some(ticket) = ticket.as_mut() {
            ticket.mark_external().await.map_err(internal)?;
        }
        self.parts.worktrees.restore(&plan, &self.parts.id).await
    }

    /// `session/rename`: the user's title, which the agent no longer sets.
    pub(super) async fn rename(&mut self, call: Call, reply: oneshot::Sender<Outcome>) {
        let cause = cause(&call);
        let title = match parse::<SessionRenameParams>(&call.params) {
            Ok(params) => params.title,
            Err(err) => return refuse(call.ticket, reply, err),
        };
        let step = Step {
            title: Some(title),
            reply: Some(Reply::Done),
            ..Step::default()
        };
        self.commit(step, Some(command(call.ticket, cause, reply)))
            .await;
    }

    /// The project's path, the repository of the session's worktree.
    async fn repo(&self) -> Result<PathBuf, RpcError> {
        let project = self.parts.project.to_string();
        let sql = "SELECT path FROM projects WHERE project_id = ?1";
        let path = (self.parts.readers).read(move |c| c.query_row(sql, [project], |r| r.get(0)));
        let path: String = path.await.map_err(internal)?;
        Ok(path.into())
    }
}

fn cause(call: &Call) -> Option<Cause> {
    let command_id = parse::<CommandId>(&call.params["command_id"]).ok()?;
    let principal = call.principal.id.clone();
    Some(Cause {
        command_id,
        principal,
    })
}

fn command(
    ticket: Option<Ticket>,
    cause: Option<Cause>,
    reply: oneshot::Sender<Outcome>,
) -> Command {
    let (audit, deferrable) = (Vec::new(), false);
    Command {
        ticket,
        cause,
        reply,
        audit,
        deferrable,
    }
}

fn refuse(ticket: Option<Ticket>, reply: oneshot::Sender<Outcome>, err: RpcError) {
    if let Some(ticket) = ticket {
        ticket.reject(err.clone());
    }
    let _ = reply.send(Err(err));
}
