//! Items of the session stream (design 5.3.5): the `item.*` events and the
//! item union. An item starts with its kind and fields, its text streams in
//! `item.delta`, and `item.updated` and `item.completed` change fields
//! through one shape shared by every kind, [`ItemPatch`] (design 5.3.11).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::annotations::{DurationMs, TimestampMs, WirePath};
use crate::domain::{Content, FailureCode, PromptContent};
use crate::ids::{GroupId, ItemId, QueueId, RequestId, TurnId};
use crate::{open_enum, open_union};

/// `item.started`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ItemStarted {
    pub item_id: ItemId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<TurnId>,
    pub started_at: TimestampMs,
    /// The activity group: consecutive tool calls in a turn and the
    /// thoughts among them (design 5.3.5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<GroupId>,
    #[serde(flatten)]
    pub item: Item,
}

open_union! {
    /// What an item is, tagged by `kind`; its fields sit next to the tag.
    pub enum Item in tag "kind" {
        UserMessage(UserMessage) => "user_message",
        /// Markdown; the text streams in `item.delta`.
        AgentMessage => "agent_message",
        /// Thinking or a reasoning summary, shown collapsed (MS-2).
        Thought => "thought",
        ToolCall(ToolCall) => "tool_call",
        LateToolOutput(LateToolOutput) => "late_tool_output",
        Plan(Plan) => "plan",
        /// A hint in the message stream (MS-5).
        Notice(Notice) => "notice",
        Error(ErrorItem) => "error",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UserMessage {
    /// Text may also arrive in `item.delta`.
    pub content: PromptContent,
    /// Steered into the running turn.
    pub steered: bool,
    /// The queued message this was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queue_id: Option<QueueId>,
    /// The rejected request this message is attached to (RP-12).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<RequestId>,
}

/// One tool call, upserted by the adapter's `toolCallId`. The host gives the
/// semantics and the client lays them out; see [`ToolCall::missing_required`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToolCall {
    pub tool_kind: ToolKind,
    /// The agent's title, truncated: the fallback when structured fields are missing.
    pub title: String,
    /// Relative to the workspace when inside it.
    pub paths: Vec<WirePath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<WirePath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<String>,
    /// Inline up to the inline limit, a blob beyond it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Content>,
    pub output_state: OutputState,
    /// `truncated`: the last 4,000 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tail: Option<String>,
    /// `truncated`: bytes received so far.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_bytes: Option<u64>,
    /// The agent's snippets, for showing progress; git is the record of a turn's changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diffs: Option<Vec<ToolDiff>>,
    /// A subagent's tool call points at its parent item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_item_id: Option<ItemId>,
    pub extraction: Extraction,
}

impl ToolCall {
    /// The fields `tool_kind` requires (design 5.3.5) that are missing or
    /// empty; `exit_code` counts once the call has `ended`. The extraction
    /// layer lowers `extraction` to `partial` when this is not empty.
    pub fn missing_required(&self, ended: bool) -> Vec<&'static str> {
        let some = |field: &Option<String>| field.as_deref().is_some_and(|s| !s.is_empty());
        let required = match &self.tool_kind {
            ToolKind::Read | ToolKind::Edit | ToolKind::Delete | ToolKind::Move => {
                vec![("paths", !self.paths.is_empty())]
            }
            ToolKind::Search => vec![("query", some(&self.query))],
            ToolKind::Execute => vec![
                ("command", some(&self.command)),
                ("cwd", self.cwd.is_some()),
                ("exit_code", !ended || self.exit_code.is_some()),
            ],
            ToolKind::Fetch => vec![("url", some(&self.url))],
            ToolKind::Think | ToolKind::SwitchMode | ToolKind::Other | ToolKind::Unknown(_) => {
                vec![("title", !self.title.is_empty())]
            }
        };
        required
            .into_iter()
            .filter_map(|(field, present)| (!present).then_some(field))
            .collect()
    }
}

open_enum! {
    pub enum ToolKind {
        Read => "read",
        Edit => "edit",
        Delete => "delete",
        Move => "move",
        Search => "search",
        Execute => "execute",
        Think => "think",
        Fetch => "fetch",
        SwitchMode => "switch_mode",
        Other => "other",
    }
}

open_enum! {
    pub enum OutputState {
        /// No output yet.
        None => "none",
        /// Appended by `item.delta` with `field: output`.
        Streaming => "streaming",
        /// Over the inline limit: deltas stop, and `item.updated` refreshes
        /// `output_tail` and `output_bytes` at most once a second.
        Truncated => "truncated",
        /// The full output is in `output`.
        Complete => "complete",
    }
}

open_enum! {
    /// How much the host could extract; the client degrades its display below `full` (MS-3).
    pub enum Extraction {
        Full => "full",
        Partial => "partial",
        None => "none",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ToolDiff {
    pub path: WirePath,
    pub change: DiffChange,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hunks: Option<Content>,
    pub stat: LineStat,
}

open_enum! {
    pub enum DiffChange {
        Add => "add",
        Update => "update",
        Delete => "delete",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LineStat {
    pub added: u32,
    pub removed: u32,
}

/// Output and end of a command that kept running after its turn was
/// cancelled (K3, design 5.2.6): one per original tool call, in the current
/// turn, never in an activity group. Its output streams like a tool call's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LateToolOutput {
    #[serde(rename = "ref")]
    pub of: ToolRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Content>,
    pub output_state: OutputState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<String>,
}

/// The original tool call, self-contained for when it is not loaded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ToolRef {
    pub item_id: ItemId,
    pub turn_id: TurnId,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

/// The agent's plan (MS-4): steps with their status, or Markdown only (Claude).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Plan {
    pub source: PlanSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entries: Option<Vec<PlanEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub markdown: Option<Content>,
}

open_enum! {
    pub enum PlanSource {
        Entries => "entries",
        Markdown => "markdown",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanEntry {
    pub content: String,
    pub status: PlanEntryStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<PlanPriority>,
}

open_enum! {
    pub enum PlanEntryStatus {
        Pending => "pending",
        InProgress => "in_progress",
        Completed => "completed",
    }
}

open_enum! {
    pub enum PlanPriority {
        High => "high",
        Medium => "medium",
        Low => "low",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Notice {
    pub code: NoticeCode,
    pub params: NoticeParams,
}

open_enum! {
    pub enum NoticeCode {
        AgentRestarted => "agent_restarted",
        SessionResumed => "session_resumed",
        Compaction => "compaction",
        /// The agent retries upstream; upserted within the turn. Params: `attempt`, `max`.
        UpstreamRetry => "upstream_retry",
        /// The start snapshot is missing: the turn has no diff.
        SnapshotUnavailable => "snapshot_unavailable",
        /// Params: `paths`, when it changed between the turn's start and end.
        RepoConfigChanged => "repo_config_changed",
        UnsafeRepoConfig => "unsafe_repo_config",
        ProjectConfigChanged => "project_config_changed",
        ConfigUnconfirmed => "config_unconfirmed",
    }
}

/// Parameters of a [`Notice`]; which ones a code uses is documented on [`NoticeCode`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NoticeParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attempt: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paths: Option<Vec<WirePath>>,
    /// `project_config_changed`: the scan's failure (`too_many_files`, ...),
    /// when the rescan failed rather than found a change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// The uniform error item (ADR-06).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ErrorItem {
    pub code: FailureCode,
    /// The agent's own error code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_code: Option<String>,
    pub retryable: bool,
    pub guidance: Guidance,
    /// The agent's error text, truncated and redacted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
}

open_enum! {
    /// What the user can do about an error; the client shows it as a button (MS-5).
    pub enum Guidance {
        Retry => "retry",
        RunLoginInTerminal => "run_login_in_terminal",
        UpgradeCli => "upgrade_cli",
        InstallCli => "install_cli",
        /// Wait for the quota to reset.
        QuotaExhausted => "quota_exhausted",
        ChangeModel => "change_model",
        /// Restart the agent in settings.
        RestartAgent => "restart_agent",
        /// Nothing beyond reading the error.
        None => "none",
    }
}

/// `item.delta`: text appended to an item. Mergeable by `(item_id, field)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ItemDelta {
    pub item_id: ItemId,
    pub field: DeltaField,
    pub text: String,
}

open_enum! {
    pub enum DeltaField {
        /// The body of a message or thought.
        Text => "text",
        /// A tool's command output.
        Output => "output",
    }
}

/// `item.updated`: tool arguments filled in, output state, diffs and the like.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ItemUpdated {
    pub item_id: ItemId,
    #[serde(flatten)]
    pub patch: ItemPatch,
}

/// `item.completed`, with the item's final fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ItemCompleted {
    pub item_id: ItemId,
    pub status: ItemStatus,
    pub ended_at: TimestampMs,
    /// Computed by the host from the start and end times.
    pub duration_ms: DurationMs,
    /// An `abandoned` command of an interrupted turn may still run
    /// (`interrupt_stops_commands` is false); present only when true.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub may_still_run: bool,
    #[serde(flatten)]
    pub patch: ItemPatch,
}

open_enum! {
    pub enum ItemStatus {
        Completed => "completed",
        Failed => "failed",
        Cancelled => "cancelled",
        /// Still running when its turn ended.
        Abandoned => "abandoned",
    }
}

/// The fields `item.updated` and `item.completed` may set, for every kind
/// at once and without `kind`: a field has the same type in every kind that
/// has it. A present field replaces the item's value; an absent one keeps it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ItemPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_kind: Option<ToolKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paths: Option<Vec<WirePath>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<WirePath>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Content>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_state: Option<OutputState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diffs: Option<Vec<ToolDiff>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_item_id: Option<ItemId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extraction: Option<Extraction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<PlanSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entries: Option<Vec<PlanEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub markdown: Option<Content>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<NoticeParams>,
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::testutil::{open_values, roundtrip};

    fn tool(fields: Value) -> ToolCall {
        let mut raw = json!({"tool_kind": "other", "title": "", "paths": [],
            "output_state": "none", "extraction": "full"});
        raw.as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        roundtrip(&raw)
    }

    #[test]
    fn unit_proto_events_session_tool_call_missing_required() {
        let check = |fields: Value, ended: bool, missing: &[&str]| {
            assert_eq!(
                tool(fields.clone()).missing_required(ended),
                missing,
                "{fields}"
            );
        };
        check(json!({"tool_kind": "read", "paths": ["a.rs"]}), false, &[]);
        check(json!({"tool_kind": "move"}), false, &["paths"]);
        check(
            json!({"tool_kind": "search", "paths": ["src"]}),
            false,
            &["query"],
        );
        check(
            json!({"tool_kind": "search", "query": ""}),
            false,
            &["query"],
        );
        check(json!({"tool_kind": "execute"}), false, &["command", "cwd"]);
        let ls = json!({"tool_kind": "execute", "command": "ls", "cwd": "/w"});
        check(ls.clone(), false, &[]);
        check(ls, true, &["exit_code"]);
        let ran = json!({"tool_kind": "execute", "command": "ls", "cwd": "/w", "exit_code": 0});
        check(ran, true, &[]);
        check(
            json!({"tool_kind": "fetch", "title": "Fetch"}),
            false,
            &["url"],
        );
        check(json!({"tool_kind": "switch_mode"}), true, &["title"]);
        check(
            json!({"tool_kind": "browse", "title": "Open page"}),
            true,
            &[],
        );
    }

    #[test]
    fn unit_proto_events_session_item_kind_sits_next_to_the_fields() {
        let started = json!({"item_id": "i_1", "started_at": 3, "kind": "canvas", "frames": [1]});
        let parsed: ItemStarted = roundtrip(&started);
        assert_eq!(parsed.item.tag(), "canvas");
        assert!(matches!(parsed.item, Item::Unknown(_)));
        let thought: ItemStarted = roundtrip(&json!({"item_id": "i_2", "turn_id": "t_1",
            "started_at": 4, "group_id": "g_1", "kind": "thought"}));
        assert_eq!(thought.item, Item::Thought);

        let rejects = |bad: Value, why: &str| {
            let err = serde_json::from_value::<ItemStarted>(bad.clone()).unwrap_err();
            assert!(err.to_string().contains(why), "{bad}: {err}");
        };
        rejects(
            json!({"item_id": "i_1", "started_at": 3}),
            "missing or non-string `kind`",
        );
        let notice = json!({"item_id": "i_1", "started_at": 3, "kind": "notice", "code": "x"});
        rejects(notice, "`notice`: missing field `params`");
        rejects(
            json!({"item_id": "i_1", "kind": "thought"}),
            "missing field `started_at`",
        );
    }

    /// Every updatable field of a kind parses as the same field of
    /// `ItemPatch`; only the fixed fields are left over.
    #[test]
    fn unit_proto_events_session_item_patch_shares_field_types() {
        let blob = json!({"type": "blob", "preview": "a",
            "blob": {"hash": "ab".repeat(32), "size": 20000, "media_type": "text/plain"}});
        let tool_call = json!({"tool_kind": "execute", "title": "t", "paths": ["a"], "query": "q",
            "url": "u", "command": "c", "cwd": "/w", "exit_code": -1, "signal": "SIGTERM",
            "output": blob, "output_state": "truncated", "output_tail": "end", "output_bytes": 9,
            "diffs": [{"path": "a", "change": "add", "stat": {"added": 1, "removed": 0}}],
            "parent_item_id": "i_0", "extraction": "partial"});
        let late = json!({"ref": {"item_id": "i_0", "turn_id": "t_0", "title": "t"}, "output": blob,
            "output_state": "complete", "output_tail": "x", "output_bytes": 1, "exit_code": 2,
            "signal": "SIGKILL"});
        let plan = json!({"source": "entries", "markdown": {"type": "text", "text": "# P"},
            "entries": [{"content": "a", "status": "in_progress", "priority": "high"}]});
        let notice = json!({"code": "upstream_retry", "params": {"attempt": 2, "max": 5}});
        let cases = [(tool_call, ""), (late, "ref"), (plan, ""), (notice, "code")];
        for (fields, fixed) in cases {
            let patch: ItemPatch = serde_json::from_value(fields.clone()).unwrap();
            let mut rest = fields.as_object().unwrap().clone();
            rest.remove(fixed);
            assert_eq!(serde_json::to_value(&patch).unwrap(), Value::Object(rest));
        }
    }

    #[test]
    fn unit_proto_events_session_item_enums_are_open() {
        open_values::<ToolKind>(ToolKind::KNOWN);
        open_values::<OutputState>(OutputState::KNOWN);
        open_values::<Extraction>(Extraction::KNOWN);
        open_values::<DiffChange>(DiffChange::KNOWN);
        open_values::<PlanSource>(PlanSource::KNOWN);
        open_values::<PlanEntryStatus>(PlanEntryStatus::KNOWN);
        open_values::<PlanPriority>(PlanPriority::KNOWN);
        open_values::<NoticeCode>(NoticeCode::KNOWN);
        open_values::<Guidance>(Guidance::KNOWN);
        open_values::<DeltaField>(DeltaField::KNOWN);
        open_values::<ItemStatus>(ItemStatus::KNOWN);
    }
}
