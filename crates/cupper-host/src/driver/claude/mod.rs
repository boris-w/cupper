//! Claude's extraction layer (design 5.6.3): what claude-agent-acp sends,
//! as the SDK parsed it, becomes driver events, frame by frame. It knows
//! nothing of turns or cancelling. Per session it keeps only what later frames
//! leave out: the workspace (tool calls carry no cwd), the last Markdown plan
//! (a plan confirmation may come without one) and the config options (a mode
//! change names only the new mode).
//!
//! - **Closed enums** (K19): where the tap replaced a value the SDK cannot
//!   parse, the original under `_meta.cupper.original` decides: an unknown
//!   stop reason is passed on as it is, an unknown tool status changes
//!   nothing, an unknown option kind counts as the strictest ([`ask`]).
//! - **Tool kinds**: one the SDK does not know arrives as `other` (the SDK's
//!   own fallback; tool kinds are not among K19's fields), and so does a
//!   missing one.
//! - **Unknown updates** fail the SDK's parse and never get here: they stay
//!   raw frames.
//! - **Commands** (K22): Claude streams a command's parameters in updates and
//!   gives its output once at the end, in `_meta.terminal_output`, after an
//!   "Exit code N" line that is dropped; the code is in `_meta.terminal_exit`.
//! - The replayed history of `session/load` (`user_message_chunk` and the
//!   rest) is not history (K8); only `user_message_chunk` is dropped here,
//!   since PS-5's import is not in M1.

pub mod ask;
pub mod capabilities;
#[cfg(test)]
mod gold_a;
pub mod options;

use std::path::{Path, PathBuf};

use agent_client_protocol::Error;
use agent_client_protocol::schema::v1::{
    ContentBlock, Meta, PlanUpdateContent, PromptResponse, SessionConfigOption, SessionUpdate,
    ToolCallContent, ToolCallLocation, ToolCallStatus, ToolKind as AcpToolKind,
};
use cupper_protocol::annotations::WirePath;
use cupper_protocol::capabilities::ConfigOption;
use cupper_protocol::domain::{Content, Cost, TextContent};
use cupper_protocol::index::TitleSource;
use cupper_protocol::item::{
    DiffChange, LineStat, Plan, PlanEntry, PlanSource, ToolDiff, ToolKind,
};
use cupper_protocol::session::{StopReason, TurnUsage, UsageUpdated};
use cupper_protocol::text::{MAX_LABEL_CHARS, truncate_chars};
use serde::Serialize;
use serde_json::Value;

use super::api::{
    AgentError, AgentErrorClass, AgentErrorForm, CompactionPhase, DriverEvent, NormUpdate,
    ToolOutput, ToolStatus,
};
use super::tap::k19;

/// What one Claude session's frames are read against.
#[derive(Debug, Clone)]
pub struct ClaudeSession {
    cwd: PathBuf,
    plan: Option<String>,
    config: Vec<ConfigOption>,
}

impl ClaudeSession {
    /// `cwd`: the workspace the session was opened in; `config`: its options.
    pub fn new(cwd: PathBuf, config: Vec<ConfigOption>) -> Self {
        let plan = None;
        Self { cwd, plan, config }
    }

    /// The events of one `session/update`.
    pub fn update(&mut self, update: &SessionUpdate) -> Vec<DriverEvent> {
        let event = |update| vec![DriverEvent::Update(update)];
        match update {
            SessionUpdate::AgentMessageChunk(chunk) => text(&chunk.content)
                .map(|text| {
                    let message_id = chunk.message_id.as_ref().map(ToString::to_string);
                    event(NormUpdate::AgentText { message_id, text })
                })
                .unwrap_or_default(),
            SessionUpdate::AgentThoughtChunk(chunk) => text(&chunk.content)
                .map(|text| event(NormUpdate::Thought { text }))
                .unwrap_or_default(),
            SessionUpdate::ToolCall(call) => {
                let tool_call_id = call.tool_call_id.to_string();
                let tool_kind = tool_kind(call.kind);
                let execute = tool_kind == ToolKind::Execute;
                let mut events = event(NormUpdate::ToolStart {
                    tool_call_id: tool_call_id.clone(),
                    tool_kind,
                    title: call.title.clone(),
                    paths: self.paths(&call.locations),
                    command: command(call.raw_input.as_ref()),
                    cwd: execute.then(|| WirePath(self.cwd.to_string_lossy().into_owned())),
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
                    paths: fields.locations.as_ref().map(|l| self.paths(l)),
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
            SessionUpdate::Plan(plan) => {
                let entries = plan.entries.iter().map(|e| PlanEntry {
                    content: e.content.clone(),
                    status: wire(&e.status).as_str().into(),
                    priority: Some(wire(&e.priority).as_str().into()),
                });
                event(NormUpdate::Plan(Plan {
                    source: PlanSource::Entries,
                    entries: Some(entries.collect()),
                    markdown: None,
                }))
            }
            SessionUpdate::PlanUpdate(update) => match &update.plan {
                PlanUpdateContent::Markdown(plan) => {
                    self.plan = Some(plan.content.clone());
                    event(NormUpdate::Plan(Plan {
                        source: PlanSource::Markdown,
                        entries: None,
                        markdown: Some(content_text(&plan.content)),
                    }))
                }
                _ => Vec::new(),
            },
            SessionUpdate::AvailableCommandsUpdate(update) => {
                let commands = capabilities::commands(&update.available_commands);
                vec![DriverEvent::CommandsChanged(commands)]
            }
            SessionUpdate::CurrentModeUpdate(update) => {
                let mode = update.current_mode_id.to_string();
                match capabilities::set_mode(&mut self.config, &mode) {
                    true => vec![DriverEvent::ConfigChanged(self.config.clone())],
                    false => Vec::new(),
                }
            }
            SessionUpdate::ConfigOptionUpdate(update) => {
                vec![self.set_config(&update.config_options)]
            }
            SessionUpdate::SessionInfoUpdate(update) => match update.title.value() {
                Some(title) => vec![DriverEvent::TitleChanged {
                    title: title.clone(),
                    source: TitleSource::Agent,
                }],
                None => Vec::new(),
            },
            SessionUpdate::UsageUpdate(update) => vec![DriverEvent::Usage(UsageUpdated {
                context_used: update.used,
                context_size: update.size,
                cost: (update.cost.as_ref()).map(|c| Cost {
                    amount: c.amount,
                    currency: c.currency.clone(),
                }),
            })],
            SessionUpdate::CompactionUpdate(update) => {
                let phase = match wire(&update.status).as_str() {
                    "in_progress" => CompactionPhase::Started,
                    "completed" => CompactionPhase::Completed,
                    _ => return Vec::new(),
                };
                let summary = update.summary.value().and_then(|blocks| {
                    let parts: Vec<String> = blocks.iter().filter_map(text).collect();
                    (!parts.is_empty()).then(|| parts.concat())
                });
                event(NormUpdate::Compaction { phase, summary })
            }
            _ => Vec::new(),
        }
    }

    /// The options as the agent now reports them (`config_option_update`,
    /// or the reply to `session/set_config_option`).
    pub fn set_config(&mut self, options: &[SessionConfigOption]) -> DriverEvent {
        self.config = capabilities::config_options(options);
        DriverEvent::ConfigChanged(self.config.clone())
    }

    /// The session's options as last known.
    pub fn config(&self) -> &[ConfigOption] {
        &self.config
    }

    /// The plan of a confirmation that carries none: the last `plan_update`.
    pub fn last_plan(&self) -> Option<&str> {
        self.plan.as_deref()
    }

    fn paths(&self, locations: &[ToolCallLocation]) -> Vec<WirePath> {
        let paths = locations.iter().map(|l| wire_path(&self.cwd, &l.path));
        paths.collect()
    }

    /// Diffs as Claude gives them: snippets with context (design 5.6.3).
    fn diffs(&self, content: &[ToolCallContent]) -> Option<Vec<ToolDiff>> {
        let diffs: Vec<ToolDiff> = (content.iter())
            .filter_map(|c| match c {
                ToolCallContent::Diff(diff) => Some(ToolDiff {
                    path: wire_path(&self.cwd, &diff.path),
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

/// A prompt's result: its stop reason, the agent's original when the tap
/// rewrote it (K19), and the tokens of the turn.
pub fn prompt_done(response: &PromptResponse) -> (StopReason, Option<TurnUsage>) {
    let original = k19::original(response.meta.as_ref(), "stopReason").and_then(Value::as_str);
    let stop = original.map_or_else(|| wire(&response.stop_reason), str::to_owned);
    let usage = response.usage.as_ref().map(|u| TurnUsage {
        input: u.input_tokens,
        output: u.output_tokens,
        cached: u.cached_read_tokens,
        cost: None,
    });
    (stop.as_str().into(), usage)
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

fn tool_update(tool_call_id: String, diffs: Option<Vec<ToolDiff>>) -> NormUpdate {
    NormUpdate::ToolUpdate {
        tool_call_id,
        title: None,
        command: None,
        paths: None,
        status: None,
        output: None,
        exit_code: None,
        diffs,
    }
}

/// The SDK's fallback for an unknown kind is `other` already.
fn tool_kind(kind: AcpToolKind) -> ToolKind {
    wire(&kind).as_str().into()
}

/// `None` for a status the tap had to replace: an unknown status changes nothing.
fn tool_status(status: ToolCallStatus, meta: Option<&Meta>) -> Option<ToolStatus> {
    if k19::original(meta, "status").is_some() {
        return None;
    }
    match status {
        ToolCallStatus::Pending => Some(ToolStatus::Pending),
        ToolCallStatus::InProgress => Some(ToolStatus::InProgress),
        ToolCallStatus::Completed => Some(ToolStatus::Completed),
        ToolCallStatus::Failed => Some(ToolStatus::Failed),
        _ => None,
    }
}

fn command(raw_input: Option<&Value>) -> Option<String> {
    raw_input?.get("command")?.as_str().map(str::to_owned)
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

fn text(block: &ContentBlock) -> Option<String> {
    match block {
        ContentBlock::Text(text) => Some(text.text.clone()),
        _ => None,
    }
}

/// The text blocks of a tool's content, as its output.
fn texts(content: &[ToolCallContent]) -> Option<String> {
    let parts: Vec<String> = (content.iter())
        .filter_map(|c| match c {
            ToolCallContent::Content(c) => text(&c.content),
            _ => None,
        })
        .collect();
    (!parts.is_empty()).then(|| parts.concat())
}

fn content_text(text: &str) -> Content {
    Content::Text(TextContent {
        text: text.to_owned(),
    })
}

/// Relative to the workspace when inside it (design 5.3.5).
fn wire_path(cwd: &Path, path: &Path) -> WirePath {
    let shown = path.strip_prefix(cwd).unwrap_or(path);
    WirePath(shown.to_string_lossy().into_owned())
}

/// Lines added and removed, past the lines both texts start and end with.
fn stat(old: &str, new: &str) -> LineStat {
    let (old, new): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    let head = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let tail = (old[head..].iter().rev())
        .zip(new[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let count = |lines: &[&str]| u32::try_from(lines.len() - head - tail).unwrap_or(u32::MAX);
    LineStat {
        added: count(&new),
        removed: count(&old),
    }
}

/// An SDK enum's wire name.
fn wire(value: &impl Serialize) -> String {
    match serde_json::to_value(value) {
        Ok(Value::String(name)) => name,
        _ => String::new(),
    }
}
