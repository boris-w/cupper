//! Data of the session stream's events other than items and interaction
//! requests (design 5.3.5): configuration, workspace, turns, the queue,
//! snapshots and usage. The session's actor is the stream's only writer
//! (design 5.2.6).

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::annotations::{DurationMs, TimestampMs, WirePath};
use crate::domain::{AgentKind, Cost, FailureCode, PromptContent};
use crate::error::{ErrorCode, ErrorReason};
use crate::ids::{Digest, ItemId, PrincipalId, QueueId, RequestId, TurnId};
use crate::index::WorkspaceKind;
use crate::open_enum;

/// `session.configured`: on creation, a configuration change, a restore, or
/// a change of the project's trust state. A later one replaces an earlier one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionConfigured {
    pub agent: AgentKind,
    pub workspace: SessionWorkspace,
    /// Current value of each option, keyed by the option's `id` in the capabilities.
    pub config: BTreeMap<String, String>,
    /// A restored session whose branch no longer exists (PS-3).
    pub read_only: bool,
    pub project_config: ProjectConfig,
    /// Present while the queue waits for `session/confirm_config` (design 5.2.6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmation_required: Option<ConfirmationRequired>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionWorkspace {
    pub kind: WorkspaceKind,
    pub path: WirePath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// The ref a worktree was created from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_ref: Option<String>,
}

open_enum! {
    /// Whether the agent loaded the project's own configuration (design 5.2.6).
    pub enum ProjectConfig {
        Loaded => "loaded",
        /// The project is not trusted: user-level configuration only.
        Ignored => "ignored",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConfirmationRequired {
    pub reason: ConfirmationReason,
    /// What `session/confirm_config` echoes back.
    pub config_digest: Digest,
}

open_enum! {
    pub enum ConfirmationReason {
        /// `full_access` without a native confirmation on record.
        FullAccessUnconfirmed => "full_access_unconfirmed",
        /// A tier above what its setter may set (design 5.8.2).
        TierAboveSetter => "tier_above_setter",
    }
}

/// `workspace.preparing`: while a worktree is created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkspacePreparing {
    pub phase: WorkspacePhase,
    /// Percent done, 0 to 100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
}

open_enum! {
    pub enum WorkspacePhase {
        WorktreeAdd => "worktree_add",
        Checkout => "checkout",
        Refresh => "refresh",
    }
}

/// `workspace.ready`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceReady {
    pub path: WirePath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

/// `workspace.failed`, written like an error object (design 5.3.11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceFailed {
    pub code: ErrorCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<ErrorReason>,
}

/// `turn.started`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TurnStarted {
    pub turn_id: TurnId,
    /// From 1 within the session; views order turns by it.
    pub turn_no: u32,
    pub origin: TurnOrigin,
    pub started_at: TimestampMs,
}

open_enum! {
    pub enum TurnOrigin {
        User => "user",
        Queued => "queued",
        /// A steer the agent turned into a new turn.
        SteerNewTurn => "steer_new_turn",
        /// Updates outside any prompt (K9).
        Autonomous => "autonomous",
    }
}

/// `turn.completed`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TurnCompleted {
    pub turn_id: TurnId,
    pub status: TurnStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<StopReason>,
    pub ended_at: TimestampMs,
    pub duration_ms: DurationMs,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<TurnUsage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<FailureCode>,
    /// The final answer: the `agent_message` items after the last tool call
    /// or grouped thought, or all of them when there is none (design 5.3.5).
    pub final_item_ids: Vec<ItemId>,
}

open_enum! {
    pub enum TurnStatus {
        Completed => "completed",
        /// Interrupted by the user.
        Cancelled => "cancelled",
        Failed => "failed",
        /// The host stopped while the turn ran.
        Interrupted => "interrupted",
    }
}

open_enum! {
    /// ACP's stop reasons, plus the host's own.
    pub enum StopReason {
        EndTurn => "end_turn",
        MaxTokens => "max_tokens",
        MaxTurnRequests => "max_turn_requests",
        Refusal => "refusal",
        Cancelled => "cancelled",
        /// The agent did not confirm a cancel in time (design 5.2.6).
        HostTimeout => "host_timeout",
    }
}

/// Tokens of one turn, as the agent reports them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TurnUsage {
    pub input: u64,
    pub output: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<Cost>,
}

/// `queue.updated`: the whole queue after a change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QueueUpdated {
    /// What `session/queue_edit` checks as `expected_version`.
    pub version: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused: Option<QueuePause>,
    pub entries: Vec<QueueEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct QueuePause {
    pub reason: PauseReason,
}

open_enum! {
    pub enum PauseReason {
        TurnCancelled => "turn_cancelled",
        TurnFailed => "turn_failed",
        TurnInterrupted => "turn_interrupted",
        ConfigUnconfirmed => "config_unconfirmed",
        ProjectConfigChanged => "project_config_changed",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QueueEntry {
    pub queue_id: QueueId,
    pub preview: String,
    /// The full message, for editing it in the queue.
    pub content: PromptContent,
    /// The rejected request this message is attached to (RP-12).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<RequestId>,
    pub created_at: TimestampMs,
    pub created_by: PrincipalId,
}

/// `snapshot.recorded`: a turn's start or end snapshot was taken. The tree
/// ID stays in the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SnapshotRecorded {
    pub turn_id: TurnId,
    pub phase: SnapshotPhase,
    pub status: SnapshotStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<SnapshotReason>,
    pub excluded: Vec<ExcludedPath>,
    /// `phase: end` only: the change against the start snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stat: Option<DiffStat>,
}

open_enum! {
    pub enum SnapshotPhase {
        Start => "start",
        End => "end",
    }
}

open_enum! {
    pub enum SnapshotStatus {
        Ok => "ok",
        /// Some paths are excluded.
        Partial => "partial",
        Failed => "failed",
        Skipped => "skipped",
    }
}

open_enum! {
    /// Why a snapshot is not `ok`.
    pub enum SnapshotReason {
        /// The start snapshot took longer than 3 s; the turn did not wait.
        Timeout => "timeout",
        UnsafeRepoConfig => "unsafe_repo_config",
        NotARepo => "not_a_repo",
        GitFailed => "git_failed",
    }
}

/// A path the snapshot left out (design 5.2.10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExcludedPath {
    /// Relative to the workspace.
    pub path: WirePath,
    pub reason: ExclusionReason,
}

open_enum! {
    pub enum ExclusionReason {
        /// An untracked file over 50 MB.
        TooLarge => "too_large",
        NestedRepo => "nested_repo",
        LargeUntrackedDir => "large_untracked_dir",
        /// Stored as a placeholder: a custom filter the host does not run.
        FilterUnsupported => "filter_unsupported",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DiffStat {
    pub files: u32,
    pub added: u32,
    pub removed: u32,
}

/// `usage.updated`: the agent's context usage. A later one replaces an earlier one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UsageUpdated {
    pub context_used: u64,
    pub context_size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<Cost>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::open_values;

    #[test]
    fn unit_proto_events_session_session_enums_are_open() {
        open_values::<ProjectConfig>(ProjectConfig::KNOWN);
        open_values::<ConfirmationReason>(ConfirmationReason::KNOWN);
        open_values::<WorkspacePhase>(WorkspacePhase::KNOWN);
        open_values::<TurnOrigin>(TurnOrigin::KNOWN);
        open_values::<TurnStatus>(TurnStatus::KNOWN);
        open_values::<StopReason>(StopReason::KNOWN);
        open_values::<PauseReason>(PauseReason::KNOWN);
        open_values::<SnapshotPhase>(SnapshotPhase::KNOWN);
        open_values::<SnapshotStatus>(SnapshotStatus::KNOWN);
        open_values::<SnapshotReason>(SnapshotReason::KNOWN);
        open_values::<ExclusionReason>(ExclusionReason::KNOWN);
    }
}
