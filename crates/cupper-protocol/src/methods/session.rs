//! Sessions (design 5.3.4, 5.2.6): creating one with its first message,
//! sending and the queue, configuration, and the session's lifecycle.

use std::collections::BTreeMap;

use schemars::{JsonSchema, SchemaGenerator};
use serde::{Deserialize, Serialize};

use super::Empty;
use super::catalog::Catalog;
use crate::annotations::TimestampMs;
use crate::capabilities::{self, ConfigOptionId};
use crate::domain::{AgentKind, PromptContent};
use crate::ids::{CommandId, Digest, ProjectId, QueueId, RequestId, SessionId, TurnId};
use crate::index::SessionSummary;
use crate::session::SessionWorkspace;
use crate::{open_enum, open_union};

method! {
    /// `session/create`: accepted once the session and its first message are
    /// stored together; the workspace, the agent and the first turn follow on
    /// the session's stream (design 5.2.6).
    request SessionCreate = "session/create" (SessionCreateParams) -> SessionCreateResult {
        scope: Write, idempotency: Command, milestone: M1a,
        errors: [NotFound, Unavailable, Unsupported, Forbidden / Scope],
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionCreateParams {
    pub command_id: CommandId,
    pub agent: AgentKind,
    pub project_id: ProjectId,
    pub workspace: WorkspaceSpec,
    /// Over the agent's defaults; a looser tier follows the tier rules
    /// (design 5.8.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<InitialConfig>,
    pub message: PromptContent,
}

open_union! {
    /// Where a new session works.
    pub enum WorkspaceSpec in tag "kind" {
        /// In the project directory.
        Local => "local",
        /// In a new worktree outside the repository (M1b); `unsupported`
        /// for a project that is not a git repository.
        Worktree(NewWorktree) => "worktree",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NewWorktree {
    /// The ref the new branch starts from.
    pub base_ref: String,
    /// A short name for the branch; the host makes it unique under `cupper/`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_hint: Option<String>,
}

/// Values as the capabilities list them for these option IDs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InitialConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionCreateResult {
    pub session_id: SessionId,
}

method! {
    /// `session/send`: starts a turn when none runs; otherwise queues, or
    /// steers when asked to (design 5.2.6). A paused queue still takes it.
    request SessionSend = "session/send" (SessionSendParams) -> SessionSendResult {
        scope: Write, idempotency: Command, milestone: M1a,
        errors: [NotFound, Unavailable, TooLarge],
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionSendParams {
    pub command_id: CommandId,
    pub session_id: SessionId,
    pub content: PromptContent,
    /// What to do while a turn runs.
    pub mode: SendMode,
    /// `steer`: the turn the client saw running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_turn_id: Option<TurnId>,
    /// "Reject with a message": the rejected request, resolved and in this
    /// session; only shown with the message (RP-12, v1.5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<RequestId>,
}

open_enum! {
    pub enum SendMode {
        Queue => "queue",
        /// Into the running turn (M1b), falling back to the queue.
        Steer => "steer",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionSendResult {
    pub disposition: Disposition,
    /// `started` or `steered`: the turn the message went into.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<TurnId>,
    /// `queued`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_id: Option<QueueId>,
    /// Why a steer went to the queue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<FallbackReason>,
}

open_enum! {
    /// What became of a message.
    pub enum Disposition {
        /// It started a turn.
        Started => "started",
        Queued => "queued",
        /// Into the running turn, or a new turn the agent started for it.
        Steered => "steered",
    }
}

open_enum! {
    /// Why a steer went to the queue (design 5.2.6).
    pub enum FallbackReason {
        /// The agent cannot steer.
        Unsupported => "unsupported",
        /// `expected_turn_id` is not the running turn.
        TurnMismatch => "turn_mismatch",
        /// The turn is not waiting on its prompt, e.g. still preparing.
        NotPrompting => "not_prompting",
        /// The agent wants a new prompt instead.
        PromptRequired => "prompt_required",
        /// The agent did not take it within 5 s.
        Timeout => "timeout",
        AgentError => "agent_error",
    }
}

method! {
    /// `session/interrupt`: cancels the running turn.
    request SessionInterrupt = "session/interrupt" (SessionInterruptParams) -> SessionInterruptResult {
        scope: Write, idempotency: Command, milestone: M1a,
        errors: [Conflict / TurnMismatch],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionInterruptParams {
    pub command_id: CommandId,
    pub session_id: SessionId,
    /// The turn the client saw running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_turn_id: Option<TurnId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionInterruptResult {
    /// Absent when no turn ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<TurnId>,
}

method! {
    /// `session/queue_resume`: resumes a paused queue.
    request SessionQueueResume = "session/queue_resume" (SessionQueueResumeParams) -> Empty {
        scope: Write, idempotency: Command, milestone: M1a,
        errors: [Conflict / ConfigUnconfirmed, Unavailable / ProjectUntrusted],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionQueueResumeParams {
    pub command_id: CommandId,
    pub session_id: SessionId,
}

method! {
    /// `session/queue_edit`: edits queued messages (v1.4, decision F3).
    request SessionQueueEdit = "session/queue_edit" (SessionQueueEditParams) -> SessionQueueEditResult {
        scope: Write, idempotency: Command, milestone: M1b,
        errors: [Conflict / VersionMismatch, NotFound, Unsupported],
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionQueueEditParams {
    pub command_id: CommandId,
    pub session_id: SessionId,
    /// The `version` of the `queue.updated` the client edited.
    pub expected_version: u64,
    /// A `steer` stands alone.
    pub ops: Vec<QueueOp>,
}

open_union! {
    /// One edit of the queue.
    pub enum QueueOp in tag "op" {
        /// Rewrites a queued message.
        Replace(QueueReplace) => "replace",
        /// Takes the message out and steers it like `session/send` with
        /// `mode: steer`; on a fallback it stays in place.
        Steer(QueueTarget) => "steer",
        /// CP-8 (M2); `unsupported` in M1.
        Remove(QueueTarget) => "remove",
        /// CP-8 (M2); `unsupported` in M1.
        Move(QueueMove) => "move",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QueueReplace {
    pub queue_id: QueueId,
    pub content: PromptContent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct QueueTarget {
    pub queue_id: QueueId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct QueueMove {
    pub queue_id: QueueId,
    /// Absent: to the end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_queue_id: Option<QueueId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionQueueEditResult {
    /// The queue's version after the edit.
    pub version: u64,
    /// What became of a `steer`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steer: Option<SteerOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SteerOutcome {
    /// `steered`, or `queued` on a fallback.
    pub disposition: Disposition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<TurnId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<FallbackReason>,
}

method! {
    /// `session/set_config`: sets one option, also while a turn runs. A
    /// looser tier follows the tier rules; a tighter one needs `write` only
    /// (design 5.8.2).
    request SessionSetConfig = "session/set_config" (SessionSetConfigParams) -> SessionSetConfigResult {
        scope: Write, idempotency: Command, milestone: M1b,
        errors: [Unsupported, AgentError, Forbidden / Scope],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionSetConfigParams {
    pub command_id: CommandId,
    pub session_id: SessionId,
    /// The option's `id` in the capabilities.
    pub option: ConfigOptionId,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionSetConfigResult {
    /// Every option after the change, as `session.configured` has them.
    pub config: BTreeMap<String, String>,
}

method! {
    /// `session/confirm_config`: sent by the main process after the native
    /// confirmation; lifts `config_unconfirmed` (design 5.2.6).
    request SessionConfirmConfig = "session/confirm_config" (SessionConfirmConfigParams) -> Empty {
        scope: Privileged, idempotency: Command, milestone: M1a,
        errors: [Conflict / DigestMismatch],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionConfirmConfigParams {
    pub command_id: CommandId,
    pub session_id: SessionId,
    /// From `session.configured.confirmation_required`.
    pub config_digest: Digest,
}

method! {
    /// `session/rename`: a title the agent no longer overrides.
    request SessionRename = "session/rename" (SessionRenameParams) -> Empty {
        scope: Write, idempotency: Command, milestone: M1b,
        errors: [],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionRenameParams {
    pub command_id: CommandId,
    pub session_id: SessionId,
    pub title: String,
}

method! {
    /// `session/restore`: brings an archived session back (PS-3).
    request SessionRestore = "session/restore" (SessionRestoreParams) -> SessionRestoreResult {
        scope: Write, idempotency: Command, milestone: M1b,
        errors: [],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionRestoreParams {
    pub command_id: CommandId,
    pub session_id: SessionId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionRestoreResult {
    pub mode: RestoreMode,
}

open_enum! {
    pub enum RestoreMode {
        Full => "full",
        /// The worktree's branch no longer exists.
        ReadOnly => "read_only",
    }
}

method! {
    /// `session/mark_read`: moves the read position forward only, for every client (U4).
    request SessionMarkRead = "session/mark_read" (SessionMarkReadParams) -> Empty {
        scope: Read, idempotency: Natural, milestone: M1a,
        errors: [],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionMarkReadParams {
    pub session_id: SessionId,
    /// The latest seq of the session's stream the user has seen.
    pub up_to_seq: u64,
}

method! {
    /// `session/get`: one session, archived or not.
    request SessionGet = "session/get" (SessionGetParams) -> SessionGetResult {
        scope: Read, idempotency: Natural, milestone: M1a,
        errors: [NotFound],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionGetParams {
    pub session_id: SessionId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionGetResult {
    pub summary: SessionSummary,
    pub workspace: SessionWorkspace,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<TimestampMs>,
}

method! {
    /// `session/capabilities`.
    request SessionCapabilities = "session/capabilities" (SessionCapabilitiesParams) -> capabilities::SessionCapabilities {
        scope: Read, idempotency: Natural, milestone: M1a,
        errors: [NotFound],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionCapabilitiesParams {
    pub session_id: SessionId,
}

method! {
    /// `session/list_archived`: archived sessions, which the index stream
    /// leaves out, the latest archived first.
    request SessionListArchived = "session/list_archived" (SessionListArchivedParams) -> SessionListArchivedResult {
        scope: Read, idempotency: Natural, milestone: M1b,
        errors: [],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionListArchivedParams {
    /// Absent: from the latest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<ArchivedCursor>,
}

/// Continues after this session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ArchivedCursor {
    pub archived_at: TimestampMs,
    pub session_id: SessionId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionListArchivedResult {
    pub sessions: Vec<ArchivedSession>,
    /// Absent on the last page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<ArchivedCursor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ArchivedSession {
    pub summary: SessionSummary,
    pub archived_at: TimestampMs,
}

pub fn register(c: &mut Catalog, g: &mut SchemaGenerator) {
    c.request::<SessionCreate>(g);
    c.request::<SessionSend>(g);
    c.request::<SessionInterrupt>(g);
    c.request::<SessionQueueResume>(g);
    c.request::<SessionQueueEdit>(g);
    c.request::<SessionSetConfig>(g);
    c.request::<SessionConfirmConfig>(g);
    c.request::<SessionRename>(g);
    c.request::<SessionRestore>(g);
    c.request::<SessionMarkRead>(g);
    c.request::<SessionGet>(g);
    c.request::<SessionCapabilities>(g);
    c.request::<SessionListArchived>(g);
}

#[cfg(test)]
pub(super) mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::methods::testkit::{capabilities, digest, request};
    use crate::testutil::{open_values, roundtrip};

    fn summary() -> Value {
        json!({"session_id": "s_1", "project_id": "p_1", "agent": "claude",
            "title": "Fix the build", "title_source": "user", "workspace_kind": "worktree",
            "branch": "cupper/fix-build", "status": "idle", "unread": false,
            "pending_interactions": 0, "last_activity_at": 1700000000000_u64, "read_only": false})
    }

    /// Typical values of every method in this group (design 5.3.4, 5.2.6).
    pub fn samples() -> Vec<&'static str> {
        let text = |t: &str| json!([{"type": "text", "text": t}]);
        let at = json!(1700000000000_u64);
        let cursor = json!({"archived_at": at, "session_id": "s_1"});
        vec![
            request::<SessionCreate>(
                json!({"command_id": "c-1", "agent": "claude", "project_id": "p_1",
                    "workspace": {"kind": "local"}, "config": {"permission_mode": "default",
                    "model": "opus"}, "message": [{"type": "text", "text": "fix the build"},
                    {"type": "file_ref", "path": "src/main.rs"}]}),
                json!({"session_id": "s_1"}),
            ),
            request::<SessionCreate>(
                json!({"command_id": "c-2", "agent": "codex", "project_id": "p_1",
                    "workspace": {"kind": "worktree", "base_ref": "main", "branch_hint": "fix-build"},
                    "message": text("go")}),
                json!({"session_id": "s_2"}),
            ),
            request::<SessionSend>(
                json!({"command_id": "c-3", "session_id": "s_1", "content": text("try again"),
                    "mode": "queue", "reply_to": "r_1"}),
                json!({"disposition": "queued", "queue_id": "q_1"}),
            ),
            request::<SessionSend>(
                json!({"command_id": "c-4", "session_id": "s_1", "content": text("also docs"),
                    "mode": "steer", "expected_turn_id": "t_1"}),
                json!({"disposition": "queued", "queue_id": "q_2", "fallback_reason": "prompt_required"}),
            ),
            request::<SessionSend>(
                json!({"command_id": "c-5", "session_id": "s_1", "content": text("next"), "mode": "steer"}),
                json!({"disposition": "steered", "turn_id": "t_1"}),
            ),
            request::<SessionInterrupt>(
                json!({"command_id": "c-6", "session_id": "s_1", "expected_turn_id": "t_1"}),
                json!({"turn_id": "t_1"}),
            ),
            request::<SessionInterrupt>(
                json!({"command_id": "c-7", "session_id": "s_1"}),
                json!({}),
            ),
            request::<SessionQueueResume>(
                json!({"command_id": "c-8", "session_id": "s_1"}),
                json!({}),
            ),
            request::<SessionQueueEdit>(
                json!({"command_id": "c-9", "session_id": "s_1", "expected_version": 4,
                    "ops": [{"op": "replace", "queue_id": "q_1", "content": text("v2")}]}),
                json!({"version": 5}),
            ),
            request::<SessionQueueEdit>(
                json!({"command_id": "c-10", "session_id": "s_1", "expected_version": 5,
                    "ops": [{"op": "steer", "queue_id": "q_2"}]}),
                json!({"version": 6, "steer": {"disposition": "queued", "fallback_reason": "timeout"}}),
            ),
            request::<SessionQueueEdit>(
                json!({"command_id": "c-11", "session_id": "s_1", "expected_version": 6,
                    "ops": [{"op": "remove", "queue_id": "q_1"},
                        {"op": "move", "queue_id": "q_2", "before_queue_id": "q_3"}]}),
                json!({"version": 7}),
            ),
            request::<SessionSetConfig>(
                json!({"command_id": "c-12", "session_id": "s_1", "option": "model", "value": "sonnet"}),
                json!({"config": {"model": "sonnet", "permission_mode": "default"}}),
            ),
            request::<SessionConfirmConfig>(
                json!({"command_id": "c-13", "session_id": "s_1", "config_digest": digest('e')}),
                json!({}),
            ),
            request::<SessionRename>(
                json!({"command_id": "c-14", "session_id": "s_1", "title": "Fix the build"}),
                json!({}),
            ),
            request::<SessionRestore>(
                json!({"command_id": "c-15", "session_id": "s_1"}),
                json!({"mode": "read_only"}),
            ),
            request::<SessionMarkRead>(json!({"session_id": "s_1", "up_to_seq": 120}), json!({})),
            request::<SessionGet>(
                json!({"session_id": "s_1"}),
                json!({"summary": summary(), "archived_at": at, "workspace": {"kind": "worktree",
                    "path": "/work/worktrees/fix-build", "branch": "cupper/fix-build", "base_ref": "main"}}),
            ),
            request::<SessionCapabilities>(json!({"session_id": "s_1"}), capabilities()),
            request::<SessionListArchived>(
                json!({"cursor": cursor}),
                json!({"sessions": [{"summary": summary(), "archived_at": at}], "next": cursor}),
            ),
            request::<SessionListArchived>(json!({}), json!({"sessions": []})),
        ]
    }

    #[test]
    fn unit_proto_methods_core_session_queue_ops_and_workspaces_are_open() {
        let ops: Vec<QueueOp> = roundtrip(&json!([{"op": "steer", "queue_id": "q_1"},
            {"op": "pin", "queue_id": "q_2"}]));
        let tags: Vec<&str> = ops.iter().map(QueueOp::tag).collect();
        assert_eq!(tags, ["steer", "pin"]);
        let spaces: Vec<WorkspaceSpec> =
            roundtrip(&json!([{"kind": "local"}, {"kind": "container", "image": "x"}]));
        assert!(matches!(spaces[1], WorkspaceSpec::Unknown(_)));

        // A known member with missing fields is an error, not an unknown one.
        let op = serde_json::from_value::<QueueOp>(json!({"op": "replace", "queue_id": "q_1"}));
        assert!(
            op.unwrap_err()
                .to_string()
                .contains("missing field `content`")
        );
        let space = serde_json::from_value::<WorkspaceSpec>(json!({"kind": "worktree"}));
        assert!(
            space
                .unwrap_err()
                .to_string()
                .contains("missing field `base_ref`")
        );

        open_values::<SendMode>(SendMode::KNOWN);
        open_values::<Disposition>(Disposition::KNOWN);
        open_values::<FallbackReason>(FallbackReason::KNOWN);
        open_values::<RestoreMode>(RestoreMode::KNOWN);
    }
}
