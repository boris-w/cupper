//! Claude's extraction layer (design 5.6.3): what claude-agent-acp sends,
//! as the SDK parsed it, becomes driver events, frame by frame. It knows
//! nothing of turns or cancelling. Per session it keeps only what later frames
//! leave out: the workspace (tool calls carry no cwd), the last Markdown plan
//! (a plan confirmation may come without one) and the config options (a mode
//! change names only the new mode).
//!
//! - **Closed enums** (K19): an unknown option kind counts as the strictest
//!   ([`ask`]); the rest is as [`crate::driver::extract`] reads it.
//! - **Commands** (K22): Claude streams a command's parameters in updates and
//!   gives its output once at the end, in `_meta.terminal_output`, after an
//!   "Exit code N" line that is dropped; the code is in `_meta.terminal_exit`.

pub mod ask;
pub mod capabilities;
pub mod options;

use std::path::PathBuf;

use agent_client_protocol::Error;
use agent_client_protocol::schema::v1::{SessionConfigOption, SessionUpdate, ToolCallContent};
use cupper_protocol::annotations::WirePath;
use cupper_protocol::capabilities::ConfigOption;
use cupper_protocol::item::{DiffChange, ToolDiff, ToolKind};
use cupper_protocol::text::{MAX_LABEL_CHARS, truncate_chars};
use serde_json::Value;

use super::api::{
    AgentError, AgentErrorClass, AgentErrorForm, DriverEvent, NormUpdate, ToolOutput,
};
use super::extract::{
    Session, command, stat, texts, tool_kind, tool_status, tool_update, wire_path,
};

/// What one Claude session's frames are read against.
#[derive(Debug, Clone)]
pub struct ClaudeSession(Session);

impl ClaudeSession {
    /// `cwd`: the workspace the session was opened in; `config`: its options.
    pub fn new(cwd: PathBuf, config: Vec<ConfigOption>) -> Self {
        Self(Session::new(cwd, config, capabilities::TIERS))
    }

    /// The events of one `session/update`.
    pub fn update(&mut self, update: &SessionUpdate) -> Vec<DriverEvent> {
        let event = |update| vec![DriverEvent::Update(update)];
        match update {
            SessionUpdate::ToolCall(call) => {
                let tool_call_id = call.tool_call_id.to_string();
                let tool_kind = tool_kind(call.kind);
                let execute = tool_kind == ToolKind::Execute;
                let cwd = || WirePath(self.0.cwd().to_string_lossy().into_owned());
                let mut events = event(NormUpdate::ToolStart {
                    tool_call_id: tool_call_id.clone(),
                    tool_kind,
                    title: call.title.clone(),
                    paths: self.0.paths(&call.locations),
                    command: command(call.raw_input.as_ref()),
                    cwd: execute.then(cwd),
                });
                // Claude's edits start with their diff.
                if let Some(diffs) = self.diffs(&call.content) {
                    events.extend(event(tool_update(tool_call_id, Some(diffs))));
                }
                events
            }
            SessionUpdate::ToolCallUpdate(call) => {
                let (fields, meta) = (&call.fields, call.meta.as_ref());
                let content = fields.content.as_deref().unwrap_or_default();
                let tool_call_id = call.tool_call_id.to_string();
                let terminal = |key| meta?.get(key);
                let exit_code = terminal("terminal_exit").and_then(|exit| exit.get("exit_code"));
                let update = NormUpdate::ToolUpdate {
                    tool_call_id: tool_call_id.clone(),
                    title: fields.title.clone(),
                    command: command(fields.raw_input.as_ref()),
                    paths: fields.locations.as_ref().map(|l| self.0.paths(l)),
                    status: fields.status.and_then(|s| tool_status(s, meta)),
                    output: (terminal("terminal_output").and_then(|o| o.get("data")))
                        .and_then(Value::as_str)
                        .map(without_exit_line)
                        .or_else(|| texts(content))
                        .map(ToolOutput::Replace),
                    exit_code: exit_code
                        .and_then(Value::as_i64)
                        .and_then(|code| i32::try_from(code).ok()),
                    diffs: self.diffs(content),
                };
                match update == tool_update(tool_call_id, None) {
                    true => Vec::new(),
                    false => event(update),
                }
            }
            _ => self.0.update(update),
        }
    }

    /// The options as the agent now reports them (`config_option_update`,
    /// or the reply to `session/set_config_option`).
    pub fn set_config(&mut self, options: &[SessionConfigOption]) -> DriverEvent {
        self.0.set_config(options)
    }

    /// The session's options as last known.
    pub fn config(&self) -> &[ConfigOption] {
        self.0.config()
    }

    /// The plan of a confirmation that carries none: the last `plan_update`.
    pub fn last_plan(&self) -> Option<&str> {
        self.0.last_plan()
    }

    /// Diffs as Claude gives them: snippets with context (design 5.6.3).
    fn diffs(&self, content: &[ToolCallContent]) -> Option<Vec<ToolDiff>> {
        let diffs: Vec<ToolDiff> = (content.iter())
            .filter_map(|c| match c {
                ToolCallContent::Diff(diff) => Some(ToolDiff {
                    path: wire_path(self.0.cwd(), &diff.path),
                    change: match diff.old_text {
                        None => DiffChange::Add,
                        Some(_) => DiffChange::Update,
                    },
                    hunks: None,
                    stat: stat(diff.old_text.as_deref().unwrap_or_default(), &diff.new_text),
                }),
                _ => None,
            })
            .collect();
        (!diffs.is_empty()).then_some(diffs)
    }
}

/// A JSON-RPC error of Claude's adapter, classified (design 5.6.3).
pub fn agent_error(error: &Error) -> AgentError {
    let data = error
        .data
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_default();
    let code = i32::from(error.code);
    let class = match code {
        -32000 => AgentErrorClass::NotLoggedIn,
        _ if data.contains("Session not found") => AgentErrorClass::SessionNotFound,
        -32603 if error.message.contains("exited unexpectedly") => AgentErrorClass::ProcessExited,
        // The SDK fails what is in flight once the adapter's output ends.
        _ if data.contains("incoming_transport_closed") => AgentErrorClass::AdapterExited,
        _ => AgentErrorClass::Other,
    };
    let agent_code = code.into();
    AgentError {
        class,
        form: AgentErrorForm::Rpc { agent_code },
        message: truncate_chars(&error.message, MAX_LABEL_CHARS),
    }
}

/// A command's output without the "Exit code N" line Claude puts first.
fn without_exit_line(data: &str) -> String {
    let exit_line = |line: &str| {
        let code = line.strip_prefix("Exit code ").unwrap_or_default();
        !code.is_empty() && code.bytes().all(|b| b.is_ascii_digit())
    };
    match data.split_once('\n') {
        Some((first, rest)) if exit_line(first) => rest.to_owned(),
        None if exit_line(data) => String::new(),
        _ => data.to_owned(),
    }
}
