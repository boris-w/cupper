//! Event envelope and event bodies.
//!
//! The envelope is open: `type` is a plain string and each type defines its own
//! `data` schema. Readers map unknown types to [`EventBody::Unknown`] and show
//! the optional `fallback` placeholder.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::open_enum;

/// Current schema version of every event type defined here.
pub const EVENT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Event {
    /// Sequence number in its stream. Contiguous in the host log.
    pub seq: u64,
    /// Present on a range event produced by send-time coalescing: this event
    /// covers `from..=seq`. Readers treat a missing `from` as `from = seq`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<u64>,
    /// Schema version of this event type.
    pub v: u32,
    /// UTC milliseconds.
    pub ts: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<Cause>,
    /// Placeholder a reader shows when it does not understand `type`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<Fallback>,
    #[serde(flatten)]
    pub body: EventBody,
}

impl Event {
    /// First seq covered by this event.
    pub fn first_seq(&self) -> u64 {
        self.from.unwrap_or(self.seq)
    }
}

/// Which client command caused an event, so a client can match its own commands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Cause {
    pub command_id: String,
    pub principal: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Fallback {
    pub kind: String,
    pub summary: String,
}

/// Event body, tagged by `type` with the payload in `data`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "data")]
pub enum EventBody {
    #[serde(rename = "session.upsert")]
    SessionUpsert(SessionSummary),
    #[serde(rename = "turn.started")]
    TurnStarted(TurnStarted),
    #[serde(rename = "turn.completed")]
    TurnCompleted(TurnCompleted),
    #[serde(rename = "item.started")]
    ItemStarted(ItemStarted),
    /// Mergeable: consecutive deltas of one item may be concatenated.
    #[serde(rename = "item.delta")]
    ItemDelta(ItemDelta),
    #[serde(rename = "item.updated")]
    ItemUpdated(ItemUpdated),
    #[serde(rename = "item.completed")]
    ItemCompleted(ItemCompleted),
    #[serde(rename = "interaction.requested")]
    InteractionRequested(InteractionRequested),
    #[serde(rename = "interaction.resolved")]
    InteractionResolved(InteractionResolved),
    #[serde(rename = "agent.notice")]
    AgentNotice(AgentNotice),
    /// Any type this version does not know.
    #[serde(untagged)]
    Unknown(UnknownEvent),
}

impl EventBody {
    pub fn type_name(&self) -> &str {
        match self {
            EventBody::SessionUpsert(_) => "session.upsert",
            EventBody::TurnStarted(_) => "turn.started",
            EventBody::TurnCompleted(_) => "turn.completed",
            EventBody::ItemStarted(_) => "item.started",
            EventBody::ItemDelta(_) => "item.delta",
            EventBody::ItemUpdated(_) => "item.updated",
            EventBody::ItemCompleted(_) => "item.completed",
            EventBody::InteractionRequested(_) => "interaction.requested",
            EventBody::InteractionResolved(_) => "interaction.resolved",
            EventBody::AgentNotice(_) => "agent.notice",
            EventBody::Unknown(u) => &u.kind,
        }
    }

    /// Event types that send-time coalescing may merge (see [`crate::coalesce`]).
    pub const MERGEABLE_TYPES: &'static [&'static str] = &["item.delta"];
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UnknownEvent {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub data: Value,
}

open_enum! {
    pub enum SessionStatus {
        Starting => "starting",
        Idle => "idle",
        Running => "running",
        AwaitingInput => "awaiting_input",
        Interrupted => "interrupted",
        Error => "error",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SessionSummary {
    pub session_id: String,
    pub agent: String,
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub status: SessionStatus,
    pub pending_interactions: u32,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TurnStarted {
    pub turn_id: String,
}

open_enum! {
    pub enum TurnStatus {
        Running => "running",
        Completed => "completed",
        Cancelled => "cancelled",
        Failed => "failed",
        /// The host restarted while the turn was running.
        Interrupted => "interrupted",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TurnCompleted {
    pub turn_id: String,
    pub status: TurnStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
}

open_enum! {
    pub enum ItemKind {
        UserMessage => "user_message",
        AgentMessage => "agent_message",
        Thought => "thought",
        ToolCall => "tool_call",
        Plan => "plan",
    }
}

open_enum! {
    pub enum ItemStatus {
        Running => "running",
        Completed => "completed",
        Failed => "failed",
        Cancelled => "cancelled",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ItemStarted {
    pub item_id: String,
    pub turn_id: String,
    pub kind: ItemKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Semantic tool kind for tool calls (read, edit, execute, ...), open set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ItemDelta {
    pub item_id: String,
    /// Appended text. Never an offset, so no UTF-8/UTF-16 conversions.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ItemUpdated {
    pub item_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ItemCompleted {
    pub item_id: String,
    pub status: ItemStatus,
}

/// Content that is either inline or, above the inline threshold, a blob reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum Content {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "blob")]
    Blob { blob: BlobRef, preview: String },
    #[serde(untagged)]
    Unknown(UnknownTagged),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BlobRef {
    /// Hex sha-256 of the blob bytes.
    pub hash: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
}

/// Fallback for an internally tagged union member this version does not know.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UnknownTagged {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(flatten)]
    pub rest: serde_json::Map<String, Value>,
}

open_enum! {
    pub enum PrincipalKind {
        DesktopUi => "desktop_ui",
        Cli => "cli",
        CliElevated => "cli_elevated",
        TestHarness => "test_harness",
        RemoteDevice => "remote_device",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InteractionRequested {
    pub request_id: String,
    pub turn_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    /// Computed by the host; clients echo it back verbatim in `interaction/respond`.
    pub digest: String,
    /// Principal kinds allowed to answer. The host re-checks on respond.
    pub actionable_by: Vec<PrincipalKind>,
    pub request: InteractionRequest,
}

/// What the agent is asking for, tagged by `type`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum InteractionRequest {
    #[serde(rename = "permission")]
    Permission {
        title: String,
        options: Vec<PermissionOption>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<Content>,
    },
    #[serde(rename = "question")]
    Question {
        prompt: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        choices: Vec<String>,
        free_text: bool,
    },
    #[serde(untagged)]
    Unknown(UnknownTagged),
}

open_enum! {
    pub enum PermissionOptionKind {
        AllowOnce => "allow_once",
        AllowAlways => "allow_always",
        RejectOnce => "reject_once",
        RejectAlways => "reject_always",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PermissionOption {
    pub option_id: String,
    pub name: String,
    pub kind: PermissionOptionKind,
}

open_enum! {
    pub enum InteractionStatus {
        Pending => "pending",
        Resolved => "resolved",
        Expired => "expired",
        Cancelled => "cancelled",
    }
}

/// A client's answer to an interaction request, tagged by `type`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type")]
pub enum InteractionOutcome {
    /// Pick one of the permission options.
    #[serde(rename = "selected")]
    Selected { option_id: String },
    /// Answer a question.
    #[serde(rename = "answer")]
    Answer { text: String },
    /// Decline to answer (the agent sees a cancellation).
    #[serde(rename = "dismissed")]
    Dismissed,
    #[serde(untagged)]
    Unknown(UnknownTagged),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InteractionResolved {
    pub request_id: String,
    pub status: InteractionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<InteractionOutcome>,
    /// Principal id that answered, for "already handled elsewhere" hints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_by: Option<String>,
}

/// Agent-side updates the host does not model yet (usage, commands, config, ...).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AgentNotice {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_event_type_is_tolerated() {
        let raw = r#"{"seq":5,"v":3,"ts":1,"type":"future.thing","data":{"x":1},"fallback":{"kind":"card","summary":"New thing"}}"#;
        let e: Event = serde_json::from_str(raw).unwrap();
        assert!(matches!(&e.body, EventBody::Unknown(u) if u.kind == "future.thing"));
        assert_eq!(e.fallback.as_ref().unwrap().summary, "New thing");
        let back: Value = serde_json::to_value(&e).unwrap();
        assert_eq!(back, serde_json::from_str::<Value>(raw).unwrap());
    }

    #[test]
    fn unknown_union_members_are_tolerated() {
        let r: InteractionRequest =
            serde_json::from_str(r#"{"type":"oauth","url":"https://example.invalid"}"#).unwrap();
        assert!(matches!(r, InteractionRequest::Unknown(ref u) if u.kind == "oauth"));
        let o: InteractionOutcome = serde_json::from_str(r#"{"type":"selected","option_id":"a"}"#).unwrap();
        assert_eq!(o, InteractionOutcome::Selected { option_id: "a".into() });
        let k: PermissionOptionKind = serde_json::from_str("\"allow_for_session\"").unwrap();
        assert_eq!(k, PermissionOptionKind::Unknown("allow_for_session".into()));
    }
}
