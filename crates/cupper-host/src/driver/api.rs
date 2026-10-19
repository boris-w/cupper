//! The driver's interface (design 5.6.2): the commands a session sends its
//! agent's `AgentRuntime` and the events it gets back. Only host and protocol
//! types appear here; ACP's stay inside `driver` (design 5.6).
//!
//! Commands and events are plain data, so pure cores can emit and take them.
//! When the session's actor hands a command to the runtime it adds the reply
//! channel of a request and, for `Attach`, its own event sender; replies and
//! events come back to its cores as inputs.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitStatus;

use cupper_protocol::annotations::WirePath;
use cupper_protocol::capabilities::{ConfigOption, SessionCapabilities, SlashCommand};
use cupper_protocol::domain::{FailureCode, PromptContent};
use cupper_protocol::ids::AdapterInstanceId;
use cupper_protocol::index::TitleSource;
use cupper_protocol::interaction::{InteractionOutcome, InteractionRequest};
use cupper_protocol::item::{Plan, ToolDiff, ToolKind};
use cupper_protocol::session::{StopReason, TurnUsage, UsageUpdated};
use thiserror::Error;

/// The agent's own ID of a session (ACP `sessionId`), opaque to the host.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AgentSessionId(pub String);

/// Names one prompt of a session. Its result carries the tag back, so a late
/// result of a prompt the host gave up on is not taken for the current one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PromptTag(pub u64);

/// Names a request from the agent (a permission or a question). The runtime
/// keeps the request's ACP responder under this ID until `Answer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AskId(pub u64);

/// What a session asks of its agent's runtime. "Request" marks the commands
/// with a reply.
#[derive(Debug, Clone, PartialEq)]
pub enum DriverCommand {
    /// Request, replied with `Result<AttachInfo, AgentError>`. Starts the
    /// adapter when needed, then `session/new` or `session/resume`, then
    /// applies `config` (also after a resume: the agent restores the mode of
    /// its transcript).
    Attach(Attach),
    /// `session/prompt`, answered by `PromptDone` or `PromptFailed` with the
    /// same tag. The runtime turns the content into ACP blocks: images as
    /// base64, file references as `resource_link`, review comments laid out
    /// as text.
    Prompt {
        session: AgentSessionId,
        tag: PromptTag,
        content: PromptContent,
    },
    /// Request, replied with `Result<SteerOutcome, AgentError>` within 5 s:
    /// `_session/steering` with `idleBehavior: promptRequired`.
    Steer {
        session: AgentSessionId,
        content: PromptContent,
    },
    /// `session/cancel`.
    Cancel { session: AgentSessionId },
    /// Request, replied with `Result<Vec<ConfigOption>, AgentError>`, the
    /// latest options: `session/set_config_option`.
    SetConfig {
        session: AgentSessionId,
        option: String,
        value: String,
    },
    /// `session/close`; the session leaves the runtime's routing table.
    Close { session: AgentSessionId },
    /// Answers an agent request; `AskAnswer::Cancelled` is ACP's `Cancelled`.
    Answer { ask: AskId, answer: AskAnswer },
    /// Request, replied with `Result<SessionCapabilities, AgentError>`, while
    /// the capability cache is empty (design 5.6.7).
    Probe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attach {
    pub target: AttachTarget,
    /// The workspace, as a real path (K13).
    pub cwd: PathBuf,
    /// Option values to apply, keyed by the option's `id` in the capabilities.
    pub config: BTreeMap<String, String>,
    pub project_config: ProjectConfigPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachTarget {
    New,
    Resume(AgentSessionId),
}

/// Follows the project's trust state (design 5.2.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectConfigPolicy {
    Load,
    /// Claude gets `settingSources: ["user"]`; Codex is `unavailable`
    /// (`project_untrusted`), since codex-acp cannot leave project
    /// configuration out.
    Ignore,
}

/// A session the runtime attached to. The capabilities carry the
/// configuration options and the slash commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachInfo {
    pub session: AgentSessionId,
    pub instance: AdapterInstanceId,
    pub capabilities: SessionCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SteerOutcome {
    Injected,
    StartedNewTurn,
    PromptRequired,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AskAnswer {
    Outcome(InteractionOutcome),
    Cancelled,
}

/// What the runtime reports to a session.
#[derive(Debug, Clone, PartialEq)]
pub enum DriverEvent {
    Update(NormUpdate),
    /// A classified request with its options' effects (design 5.6.4).
    Ask {
        ask: AskId,
        request: InteractionRequest,
    },
    /// The agent took a request back (`$/cancel_request`, K5).
    AskWithdrawn(AskId),
    PromptDone {
        tag: PromptTag,
        stop_reason: StopReason,
        usage: Option<TurnUsage>,
    },
    PromptFailed {
        tag: PromptTag,
        error: AgentError,
    },
    /// Codex's `threadStatus`. It never ends a turn while a prompt is in
    /// flight (K12).
    RunState(RunState),
    /// `config_option_update` and `current_mode_update`.
    ConfigChanged(Vec<ConfigOption>),
    /// `available_commands_update`, only when the list changed.
    CommandsChanged(Vec<SlashCommand>),
    TitleChanged {
        title: String,
        source: TitleSource,
    },
    /// `usage_update`.
    Usage(UsageUpdated),
    /// Codex retries upstream by itself (`_meta.codex.error`, `willRetry`).
    UpstreamRetry {
        attempt: u32,
        max: u32,
    },
    /// Sent to every session of the agent.
    AdapterExited {
        instance: AdapterInstanceId,
        status: ExitStatus,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunState {
    pub active: bool,
    pub waiting_on_approval: bool,
    pub waiting_on_user_input: bool,
    pub system_error: bool,
}

/// A `session/update` after extraction (design 5.6.3).
#[derive(Debug, Clone, PartialEq)]
pub enum NormUpdate {
    AgentText {
        message_id: Option<String>,
        text: String,
    },
    Thought {
        text: String,
    },
    ToolStart {
        tool_call_id: String,
        tool_kind: ToolKind,
        title: String,
        paths: Vec<WirePath>,
        command: Option<String>,
    },
    ToolUpdate {
        tool_call_id: String,
        title: Option<String>,
        status: Option<ToolStatus>,
        output: Option<ToolOutput>,
        exit_code: Option<i32>,
        diffs: Option<Vec<ToolDiff>>,
    },
    Plan(Plan),
    Compaction {
        phase: CompactionPhase,
        summary: Option<String>,
    },
}

/// ACP's tool call status, after the tap's K19 fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolOutput {
    /// More output: an `item.delta` with `field: output`.
    Append(String),
    /// The whole output, given at the end.
    Replace(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactionPhase {
    Started,
    Completed,
}

/// An agent error of any of the three forms (ADR-06), with cupper's class.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("agent error ({class:?}): {message}")]
pub struct AgentError {
    pub class: AgentErrorClass,
    pub form: AgentErrorForm,
    /// Truncated and redacted.
    pub message: String,
}

/// cupper's classes of agent errors (design 5.6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentErrorClass {
    NotLoggedIn,
    CliTooOld,
    SessionNotFound,
    ProcessExited,
    Restartable,
    ModelUnsupported,
    QuotaExhausted,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentErrorForm {
    /// A JSON-RPC error, with the agent's code.
    Rpc { agent_code: i64 },
    /// Error text in an agent message (K17).
    Text,
    /// An upstream error the agent retries by itself.
    Upstream,
}

impl AgentErrorClass {
    /// The class as it appears in `turn.completed.error_code` and `error` items.
    pub fn failure_code(self) -> FailureCode {
        match self {
            Self::NotLoggedIn => FailureCode::NotLoggedIn,
            Self::CliTooOld => FailureCode::CliTooOld,
            Self::SessionNotFound => FailureCode::SessionNotFound,
            Self::ProcessExited => FailureCode::ProcessExited,
            Self::Restartable => FailureCode::Restartable,
            Self::ModelUnsupported => FailureCode::ModelUnsupported,
            Self::QuotaExhausted => FailureCode::QuotaExhausted,
            Self::Other => FailureCode::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_error_classes_map_to_their_failure_codes() {
        use AgentErrorClass as C;
        let all = [
            C::NotLoggedIn,
            C::CliTooOld,
            C::SessionNotFound,
            C::ProcessExited,
            C::Restartable,
            C::ModelUnsupported,
            C::QuotaExhausted,
            C::Other,
        ];
        // Same wire names as design 5.6.2's classes.
        let codes: Vec<FailureCode> = all.iter().map(|c| c.failure_code()).collect();
        let named: Vec<FailureCode> = FailureCode::KNOWN[..8]
            .iter()
            .map(|name| FailureCode::from(*name))
            .collect();
        assert_eq!(codes, named);
    }
}
