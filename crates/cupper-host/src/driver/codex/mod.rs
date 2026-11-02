//! Codex's extraction layer (design 5.6.3): what codex-acp 2.2.2 sends to a
//! client that does not declare JetBrains AIR (Q31), as the SDK parsed it,
//! becomes driver events, frame by frame. It knows nothing of turns or
//! cancelling, and reads what both agents send alike as
//! [`crate::driver::extract`] does.
//!
//! - **Run state**: `session_info_update._meta.codex.threadStatus` is
//!   [`DriverEvent::RunState`]: `active` with its flags, `idle` and
//!   `notLoaded` idle, `systemError` failed.
//! - **Upstream retries** (K18): `_meta.codex.error` with `willRetry` is
//!   [`DriverEvent::UpstreamRetry`], counted by its "Reconnecting... n/m"
//!   (0 when it does not say). Without `willRetry` it is about another turn.
//! - **Text errors** (K17): from `systemError` until the thread is next
//!   `active`, a message chunk without a `messageId` is the adapter's text of
//!   the turn's error, not the model's, whose text always carries its item's
//!   ID. The prompt's `end_turn` then fails with it ([`CodexSession::prompt_done`]).
//! - **Commands**: an `execute` call has the command as its title (the
//!   adapter strips `sh -c`), runs in `rawInput.cwd`, streams its
//!   output in `_meta.terminal_output` chunks, each one more, and ends with
//!   `_meta.terminal_exit`. A read, search or listing has no terminal: its
//!   output is `content` text at the end and its exit code is in
//!   `rawOutput.exit_code`, where a command without output has it too.
//! - **Edits**: a diff per hunk, `_meta.kind` telling add, update or delete;
//!   their files are the call's paths.
//!   When the model runs `apply_patch` in the shell there is none (K24).
//! - **Plan review**: the adapter's update of its `plan-review:` call is left
//!   out; the confirmation is a request, not a tool call ([`ask`]).

pub mod ask;
pub mod capabilities;
pub mod options;

use std::path::PathBuf;

use agent_client_protocol::Error;
use agent_client_protocol::schema::v1::{
    PromptResponse, SessionConfigOption, SessionUpdate, ToolCallContent,
};
use cupper_protocol::annotations::WirePath;
use cupper_protocol::capabilities::ConfigOption;
use cupper_protocol::item::{DiffChange, ToolDiff, ToolKind};
use cupper_protocol::session::{StopReason, TurnUsage};
use cupper_protocol::text::{MAX_LABEL_CHARS, truncate_chars};
use serde_json::Value;

use super::api::{
    AgentError, AgentErrorClass, AgentErrorForm, DriverEvent, NormUpdate, RunState, ToolOutput,
};
use super::extract::{
    self, Session, command, stat, text, texts, tool_kind, tool_status, tool_update, wire_path,
};

/// What one Codex session's frames are read against.
#[derive(Debug, Clone)]
pub struct CodexSession {
    frames: Session,
    /// Since the thread reported `systemError`: the text of its error so far.
    failure: Option<String>,
}

impl CodexSession {
    /// `cwd`: the workspace the session was opened in; `config`: its options.
    pub fn new(cwd: PathBuf, config: Vec<ConfigOption>) -> Self {
        let frames = Session::new(cwd, config, capabilities::TIERS);
        let failure = None;
        Self { frames, failure }
    }

    /// The events of one `session/update`.
    pub fn update(&mut self, update: &SessionUpdate) -> Vec<DriverEvent> {
        let event = |update| vec![DriverEvent::Update(update)];
        match update {
            SessionUpdate::AgentMessageChunk(chunk) if chunk.message_id.is_none() => {
                match (&mut self.failure, text(&chunk.content)) {
                    (Some(failure), Some(text)) => failure.push_str(&text),
                    _ => return self.frames.update(update),
                }
                Vec::new()
            }
            SessionUpdate::ToolCall(call) => {
                let tool_call_id = call.tool_call_id.to_string();
                let tool_kind = tool_kind(call.kind);
                let diffs = self.diffs(&call.content);
                let paths = match (call.locations.is_empty(), &diffs) {
                    (true, Some(diffs)) => {
                        let mut paths: Vec<WirePath> =
                            diffs.iter().map(|d| d.path.clone()).collect();
                        paths.dedup();
                        paths
                    }
                    _ => self.frames.paths(&call.locations),
                };
                let execute = tool_kind == ToolKind::Execute;
                let raw_input = call.raw_input.as_ref();
                let title = Some(call.title.clone()).filter(|title| !title.is_empty());
                let cwd = raw_input
                    .and_then(|raw| raw.get("cwd"))
                    .and_then(Value::as_str);
                let cwd = cwd.map_or_else(|| self.frames.cwd().to_string_lossy(), Into::into);
                let mut events = event(NormUpdate::ToolStart {
                    tool_call_id: tool_call_id.clone(),
                    tool_kind,
                    title: call.title.clone(),
                    paths,
                    command: execute.then(|| title.or(command(raw_input))).flatten(),
                    cwd: execute.then(|| WirePath(cwd.into_owned())),
                });
                if diffs.is_some() {
                    events.extend(event(tool_update(tool_call_id, diffs)));
                }
                events
            }
            SessionUpdate::ToolCallUpdate(call) => {
                let tool_call_id = call.tool_call_id.to_string();
                if tool_call_id.starts_with("plan-review:") {
                    return Vec::new();
                }
                let (fields, meta) = (&call.fields, call.meta.as_ref());
                let content = fields.content.as_deref().unwrap_or_default();
                let terminal = |key| meta?.get(key);
                let raw_exit = fields
                    .raw_output
                    .as_ref()
                    .and_then(|raw| raw.get("exit_code"));
                let exit_code = (terminal("terminal_exit").and_then(|exit| exit.get("exit_code")))
                    .or(raw_exit)
                    .and_then(Value::as_i64)
                    .and_then(|code| i32::try_from(code).ok());
                let chunk = terminal("terminal_output").and_then(|output| output.get("data"));
                let update = NormUpdate::ToolUpdate {
                    tool_call_id: tool_call_id.clone(),
                    title: fields.title.clone(),
                    command: None,
                    paths: fields.locations.as_ref().map(|l| self.frames.paths(l)),
                    status: fields.status.and_then(|s| tool_status(s, meta)),
                    output: (chunk.and_then(Value::as_str))
                        .map(|data| ToolOutput::Append(data.to_owned()))
                        .or_else(|| texts(content).map(ToolOutput::Replace)),
                    exit_code,
                    diffs: self.diffs(content),
                };
                match update == tool_update(tool_call_id, None) {
                    true => Vec::new(),
                    false => event(update),
                }
            }
            SessionUpdate::SessionInfoUpdate(info) => {
                let codex = info.meta.as_ref().and_then(|meta| meta.get("codex"));
                let status = codex.and_then(|codex| codex.get("threadStatus"));
                let state = status.and_then(run_state);
                match state {
                    Some(RunState { active: true, .. }) => self.failure = None,
                    Some(RunState {
                        system_error: true, ..
                    }) => self.failure = Some(String::new()),
                    _ => {}
                }
                let retry = codex.and_then(|codex| codex.get("error")).and_then(retry);
                let mut events: Vec<DriverEvent> =
                    state.map(DriverEvent::RunState).into_iter().collect();
                events.extend(retry);
                events.extend(self.frames.update(update));
                events
            }
            _ => self.frames.update(update),
        }
    }

    /// The options as the agent now reports them (`config_option_update`,
    /// or the reply to `session/set_config_option`).
    pub fn set_config(&mut self, options: &[SessionConfigOption]) -> DriverEvent {
        self.frames.set_config(options)
    }

    /// The session's options as last known.
    pub fn config(&self) -> &[ConfigOption] {
        self.frames.config()
    }

    /// A prompt's result. One that ends after the thread failed is a failure
    /// with the error's text (K17), though Codex says `end_turn`.
    pub fn prompt_done(
        &mut self,
        response: &PromptResponse,
    ) -> Result<(StopReason, Option<TurnUsage>), AgentError> {
        match self.failure.take() {
            Some(text) => Err(text_error(&text)),
            None => Ok(extract::prompt_done(response)),
        }
    }

    /// Codex's diffs, one per hunk (design 5.6.3).
    fn diffs(&self, content: &[ToolCallContent]) -> Option<Vec<ToolDiff>> {
        let diffs: Vec<ToolDiff> = (content.iter())
            .filter_map(|c| match c {
                ToolCallContent::Diff(diff) => Some(diff),
                _ => None,
            })
            .map(|diff| {
                let kind = diff.meta.as_ref().and_then(|meta| meta.get("kind"));
                let old = diff.old_text.as_deref();
                ToolDiff {
                    path: wire_path(self.frames.cwd(), &diff.path),
                    change: match (kind.and_then(Value::as_str), old) {
                        (Some("delete"), _) => DiffChange::Delete,
                        (Some("add"), _) | (_, None) => DiffChange::Add,
                        _ => DiffChange::Update,
                    },
                    hunks: None,
                    stat: stat(old.unwrap_or_default(), &diff.new_text),
                }
            })
            .collect();
        (!diffs.is_empty()).then_some(diffs)
    }
}

/// A JSON-RPC error of Codex's adapter, classified (design 5.6.3). The
/// message stops at its first line: a `1001` goes on with the app-server's
/// stderr, which names the user's configuration.
pub fn agent_error(error: &Error) -> AgentError {
    let data = error.data.as_ref();
    let field = |key| data.and_then(|data| data.get(key));
    let code = i32::from(error.code);
    let class = match code {
        -32000 => AgentErrorClass::NotLoggedIn,
        1001 if field("restartable").and_then(Value::as_bool) == Some(true) => {
            AgentErrorClass::Restartable
        }
        1001 => AgentErrorClass::ProcessExited,
        _ if error.message.starts_with("Session not found") => AgentErrorClass::SessionNotFound,
        _ if field("codexErrorInfo").and_then(Value::as_str) == Some("usageLimitExceeded") => {
            AgentErrorClass::QuotaExhausted
        }
        // The SDK fails what is in flight once the adapter's output ends.
        _ if data.is_some_and(|data| data.to_string().contains("incoming_transport_closed")) => {
            AgentErrorClass::AdapterExited
        }
        _ => AgentErrorClass::Other,
    };
    let message = error.message.lines().next().unwrap_or_default();
    AgentError {
        class,
        form: AgentErrorForm::Rpc {
            agent_code: code.into(),
        },
        message: truncate_chars(message, MAX_LABEL_CHARS),
    }
}

/// The error of a failed thread, from the adapter's text: an OpenAI error
/// object's message, or the text itself.
fn text_error(text: &str) -> AgentError {
    let json = serde_json::from_str::<Value>(text.trim()).ok();
    let inner = json
        .as_ref()
        .and_then(|error| error.pointer("/error/message"));
    let message = inner.and_then(Value::as_str).unwrap_or(text.trim());
    let lower = message.to_lowercase();
    let class = match lower.contains("model") && lower.contains("not supported") {
        true => AgentErrorClass::ModelUnsupported,
        false => AgentErrorClass::Other,
    };
    AgentError {
        class,
        form: AgentErrorForm::Text,
        message: truncate_chars(message, MAX_LABEL_CHARS),
    }
}

fn run_state(status: &Value) -> Option<RunState> {
    let flags = status["activeFlags"].as_array();
    let flag = |name: &str| flags.is_some_and(|flags| flags.iter().any(|f| f == name));
    let (active, system_error) = match status["type"].as_str()? {
        "active" => (true, false),
        "idle" | "notLoaded" => (false, false),
        "systemError" => (false, true),
        _ => return None,
    };
    Some(RunState {
        active,
        waiting_on_approval: flag("waitingOnApproval"),
        waiting_on_user_input: flag("waitingOnUserInput"),
        system_error,
    })
}

fn retry(error: &Value) -> Option<DriverEvent> {
    if error["willRetry"] != true {
        return None;
    }
    let message = error["message"].as_str().unwrap_or_default();
    let counts = message.rsplit(' ').next().and_then(|n| n.split_once('/'));
    let count = |n: &str| n.parse().ok();
    let (attempt, max) = counts
        .and_then(|(attempt, max)| count(attempt).zip(count(max)))
        .unwrap_or((0, 0));
    Some(DriverEvent::UpstreamRetry { attempt, max })
}
