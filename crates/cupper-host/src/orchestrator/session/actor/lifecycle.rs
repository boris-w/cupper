//! A session's archive and rename (design 5.2.6, 5.5.9), which the actor
//! does as the one writer of the session's stream and row.

use std::path::PathBuf;

use cupper_protocol::envelope::Cause;
use cupper_protocol::error::{ErrorCode, ErrorReason, RpcError};
use cupper_protocol::ids::CommandId;
use cupper_protocol::index::WorkspaceKind;
use cupper_protocol::methods::interaction::InteractionRespond;
use cupper_protocol::methods::session::{
    ArchiveWorktree, SessionArchive, SessionArchiveParams, SessionArchiveResult,
    SessionConfirmConfig, SessionInterrupt, SessionQueueResume, SessionRenameParams, SessionSend,
    SessionSetConfig, WorktreeOutcome,
};
use cupper_protocol::rpc::Method;
use serde_json::json;
use tokio::sync::oneshot;

use super::{Command, Reply, SessionActor, Step};
use crate::gateway::Call;
use crate::ledger::{Outcome, Ticket};
use crate::orchestrator::session::{internal, parse};
use crate::orchestrator::turn;

/// The calls that change a session, which an archived one refuses.
const WRITES: [&str; 6] = [
    SessionSend::NAME,
    SessionInterrupt::NAME,
    SessionQueueResume::NAME,
    SessionConfirmConfig::NAME,
    SessionSetConfig::NAME,
    InteractionRespond::NAME,
];

impl SessionActor {
    /// `unavailable` for a write to an archived session, archiving it again
    /// too: `session/restore` brings it back first.
    pub(super) fn refused(&self, method: &str) -> Option<RpcError> {
        let writes = WRITES.contains(&method) || method == SessionArchive::NAME;
        let archived = RpcError::app(ErrorCode::Unavailable, "the session is archived");
        (self.archived && writes).then(|| archived.with_reason(ErrorReason::SessionArchived))
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
        let busy = self.parts.core.turn_id().is_some() || self.preparing.is_some();
        let running = RpcError::app(ErrorCode::Conflict, "a turn runs");
        let archived = match worktree {
            Ok(_) if busy => Err(running.with_reason(ErrorReason::TurnRunning)),
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
            archived: true,
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
