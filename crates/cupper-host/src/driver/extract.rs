//! What the agents' extraction layers share (design 5.6.3, 5.6.5): the state
//! a session's frames are read against, the updates every adapter sends alike,
//! config options by category, and small readers of ACP values. Each agent's
//! layer reads tool calls and its own `_meta` itself and hands the rest of a
//! session's updates to [`Session::update`].
//!
//! - **Closed enums** (K19): where the tap replaced a value the SDK cannot
//!   parse, the original under `_meta.cupper.original` decides: an unknown
//!   stop reason is passed on as it is, an unknown tool status changes
//!   nothing.
//! - **Tool kinds**: one the SDK does not know arrives as `other` (the SDK's
//!   own fallback; tool kinds are not among K19's fields), and so does a
//!   missing one.
//! - **Unknown updates** fail the SDK's parse and never get here: they stay
//!   raw frames.
//! - The replayed history of `session/load` (`user_message_chunk` and the
//!   rest) is not history (K8); only `user_message_chunk` is dropped here,
//!   since PS-5's import is not in M1.

use std::path::{Path, PathBuf};

use agent_client_protocol::schema::v1::{
    AvailableCommand, AvailableCommandInput, ContentBlock, CreateElicitationRequest,
    ElicitationMode, ElicitationScope, Meta, PlanUpdateContent, PromptResponse, SessionConfigKind,
    SessionConfigOption, SessionConfigOptionCategory as Category, SessionConfigSelectOptions,
    SessionUpdate, ToolCallContent, ToolCallLocation, ToolCallStatus, ToolKind as AcpToolKind,
};
use cupper_protocol::annotations::WirePath;
use cupper_protocol::capabilities::{ConfigOption, ConfigOptionId, ConfigValue, SlashCommand};
use cupper_protocol::domain::{Content, Cost, TextContent, Tier};
use cupper_protocol::index::TitleSource;
use cupper_protocol::item::{LineStat, Plan, PlanEntry, PlanSource, ToolDiff, ToolKind};
use cupper_protocol::session::{StopReason, TurnUsage, UsageUpdated};
use serde::Serialize;
use serde_json::Value;

use super::api::{AskRequest, CompactionPhase, DriverEvent, FormAsk, NormUpdate, ToolStatus};
use super::tap::k19;

/// An agent's permission modes by approval tier (design 5.6.5).
pub type Tiers = &'static [(&'static str, Tier)];

/// What one session's frames are read against: the workspace (paths are
/// shown relative to it), the last Markdown plan (a plan confirmation may
/// come without one) and the config options (a mode change names only the
/// new mode), with the agent's tiers for its modes.
#[derive(Debug, Clone)]
pub struct Session {
    cwd: PathBuf,
    plan: Option<String>,
    config: Vec<ConfigOption>,
    tiers: Tiers,
}

impl Session {
    pub fn new(cwd: PathBuf, config: Vec<ConfigOption>, tiers: Tiers) -> Self {
        let plan = None;
        Self {
            cwd,
            plan,
            config,
            tiers,
        }
    }

    /// The events of an update every adapter sends alike; none for a tool
    /// call, which the agent's layer reads.
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
                vec![DriverEvent::CommandsChanged(commands(
                    &update.available_commands,
                ))]
            }
            SessionUpdate::CurrentModeUpdate(update) => {
                let mode = update.current_mode_id.to_string();
                match set_mode(&mut self.config, &mode) {
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
        self.config = config_options(options, self.tiers);
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

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn paths(&self, locations: &[ToolCallLocation]) -> Vec<WirePath> {
        let paths = locations.iter().map(|l| wire_path(&self.cwd, &l.path));
        paths.collect()
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

/// `elicitation/create` in form mode: the tool call it is about, and the form,
/// which the interaction broker maps (design 5.3.6). The URL mode is not in
/// M1 (design 5.6.4).
pub fn elicitation(request: &CreateElicitationRequest) -> Option<(Option<String>, AskRequest)> {
    let ElicitationMode::Form(form) = &request.mode else {
        return None;
    };
    let tool_call_id = match &form.scope {
        ElicitationScope::Session(scope) => scope.tool_call_id.as_ref().map(ToString::to_string),
        _ => None,
    };
    let form = FormAsk {
        message: request.message.clone(),
        schema: serde_json::to_value(&form.requested_schema).ok()?,
    };
    Some((tool_call_id, AskRequest::Form(form)))
}

/// The options the agent reports, by category; choices as the agent names
/// them (CP-4), permission modes at their tier. Boolean options have no place
/// among them yet.
pub fn config_options(options: &[SessionConfigOption], tiers: Tiers) -> Vec<ConfigOption> {
    let option = |option: &SessionConfigOption| {
        let SessionConfigKind::Select(select) = &option.kind else {
            return None;
        };
        let agent_option_id = option.id.to_string();
        let id = match &option.category {
            Some(Category::Mode) => ConfigOptionId::PermissionMode,
            Some(Category::Model) => ConfigOptionId::Model,
            Some(Category::ThoughtLevel) => ConfigOptionId::ReasoningEffort,
            _ => ConfigOptionId::from(agent_option_id.as_str()),
        };
        let risk = |value: &str| match id {
            ConfigOptionId::PermissionMode => tier(tiers, value),
            _ => Tier::Normal,
        };
        let choices = match &select.options {
            SessionConfigSelectOptions::Ungrouped(choices) => choices.iter().collect(),
            SessionConfigSelectOptions::Grouped(groups) => {
                groups.iter().flat_map(|group| &group.options).collect()
            }
            _ => Vec::new(),
        };
        let values = (choices.into_iter())
            .map(|choice| ConfigValue {
                value: choice.value.to_string(),
                name: choice.name.clone(),
                description: choice.description.clone(),
                risk: risk(&choice.value.to_string()),
            })
            .collect();
        Some(ConfigOption {
            id,
            agent_option_id,
            current: select.current_value.to_string(),
            values,
        })
    };
    options.iter().filter_map(option).collect()
}

/// The tier of a permission mode; one the table does not have counts as
/// `full_access` until the table knows it.
pub fn tier(tiers: Tiers, mode: &str) -> Tier {
    let row = tiers.iter().find(|(known, _)| *known == mode);
    row.map_or(Tier::FullAccess, |(_, tier)| tier.clone())
}

/// `current_mode_update`: sets the permission mode; false when there is none
/// to set.
pub fn set_mode(config: &mut [ConfigOption], mode: &str) -> bool {
    let option = config
        .iter_mut()
        .find(|o| o.id == ConfigOptionId::PermissionMode);
    match option {
        Some(option) if option.current != mode => {
            option.current = mode.to_owned();
            true
        }
        _ => false,
    }
}

pub fn commands(list: &[AvailableCommand]) -> Vec<SlashCommand> {
    let command = |command: &AvailableCommand| SlashCommand {
        name: command.name.clone(),
        description: command.description.clone(),
        input_hint: match &command.input {
            Some(AvailableCommandInput::Unstructured(input)) => Some(input.hint.clone()),
            _ => None,
        },
    };
    list.iter().map(command).collect()
}

pub fn tool_update(tool_call_id: String, diffs: Option<Vec<ToolDiff>>) -> NormUpdate {
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
pub fn tool_kind(kind: AcpToolKind) -> ToolKind {
    wire(&kind).as_str().into()
}

/// `None` for a status the tap had to replace: an unknown status changes nothing.
pub fn tool_status(status: ToolCallStatus, meta: Option<&Meta>) -> Option<ToolStatus> {
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

pub fn command(raw_input: Option<&Value>) -> Option<String> {
    raw_input?.get("command")?.as_str().map(str::to_owned)
}

pub fn text(block: &ContentBlock) -> Option<String> {
    match block {
        ContentBlock::Text(text) => Some(text.text.clone()),
        _ => None,
    }
}

/// The text blocks of a tool's content, as its output.
pub fn texts(content: &[ToolCallContent]) -> Option<String> {
    let parts: Vec<String> = (content.iter())
        .filter_map(|c| match c {
            ToolCallContent::Content(c) => text(&c.content),
            _ => None,
        })
        .collect();
    (!parts.is_empty()).then(|| parts.concat())
}

pub fn content_text(text: &str) -> Content {
    Content::Text(TextContent {
        text: text.to_owned(),
    })
}

/// Relative to the workspace when inside it (design 5.3.5).
pub fn wire_path(cwd: &Path, path: &Path) -> WirePath {
    let shown = path.strip_prefix(cwd).unwrap_or(path);
    WirePath(shown.to_string_lossy().into_owned())
}

/// Lines added and removed, past the lines both texts start and end with.
pub fn stat(old: &str, new: &str) -> LineStat {
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
pub fn wire(value: &impl Serialize) -> String {
    match serde_json::to_value(value) {
        Ok(Value::String(name)) => name,
        _ => String::new(),
    }
}
