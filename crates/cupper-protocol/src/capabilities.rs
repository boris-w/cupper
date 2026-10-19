//! Session capabilities (design 5.3.7): what the agent behind a session can
//! do, in one shape for both agents (AG-4). The new-session page uses the same
//! shape from the host's cache (ADR-09). A feature the agent lacks is `false`
//! or absent, and the client hides it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::domain::{AgentKind, Tier};
use crate::open_enum;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionCapabilities {
    pub agent: AgentKind,
    pub versions: Versions,
    pub prompt: PromptCapabilities,
    pub turn: TurnCapabilities,
    pub session: SessionLifecycle,
    pub config: Vec<ConfigOption>,
    /// Slash commands from the agent (CP-3).
    pub commands: Vec<SlashCommand>,
    pub interactions: InteractionCapabilities,
    pub extraction: ExtractionCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Versions {
    pub adapter: String,
    pub cli: String,
    pub cli_compat: CliCompat,
}

open_enum! {
    /// Where a CLI version falls against the range verified with the pinned
    /// adapter (design 5.6.6).
    pub enum CliCompat {
        Verified => "verified",
        /// Outside the range, with no known problem: a hint, not a block (AG-5).
        NewerUntested => "newer_untested",
        OlderUntested => "older_untested",
        /// Below a version known to fail, or the version could not be read.
        Unsupported => "unsupported",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PromptCapabilities {
    pub image: bool,
    pub file_ref: bool,
    pub max_image_bytes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TurnCapabilities {
    /// Always true: the host queues messages itself.
    pub queue: bool,
    pub steer: SteerCapability,
    /// Always true.
    pub interrupt: bool,
    /// False (Codex) means commands may keep running after an interrupt;
    /// they end as `abandoned` with `may_still_run`.
    pub interrupt_stops_commands: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SteerCapability {
    pub supported: bool,
    /// Steering has side effects (Claude), which the client explains once.
    pub side_effects: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionLifecycle {
    pub resume: bool,
    /// P1.
    pub load_history: bool,
    /// P1.
    pub fork: bool,
}

/// One configuration option, with values and names as the agent gives them (CP-4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConfigOption {
    pub id: ConfigOptionId,
    /// The agent's own option ID.
    pub agent_option_id: String,
    pub current: String,
    pub values: Vec<ConfigValue>,
}

open_enum! {
    /// cupper's name for an option (design 5.6.5); any other option keeps the
    /// agent's ID.
    pub enum ConfigOptionId {
        PermissionMode => "permission_mode",
        Model => "model",
        ReasoningEffort => "reasoning_effort",
        PlanMode => "plan_mode",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConfigValue {
    pub value: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The approval tier this value puts the session in, from the host's mapping.
    pub risk: Tier,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SlashCommand {
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InteractionCapabilities {
    /// Rejecting a permission ends the turn (Codex #556).
    pub reject_ends_turn: bool,
    pub question_forms: bool,
    pub plan_confirm: bool,
}

/// What the host can extract from this agent's updates (design 5.6.3); the
/// client degrades its display where it cannot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExtractionCapabilities {
    pub exit_code: bool,
    pub output_streaming: bool,
    pub diff: DiffExtraction,
    pub thinking: bool,
    pub plan: PlanExtraction,
    pub run_state: RunStateSource,
}

open_enum! {
    pub enum DiffExtraction {
        /// Snippets with context.
        Fragment => "fragment",
        PerHunk => "per_hunk",
        None => "none",
    }
}

open_enum! {
    pub enum PlanExtraction {
        /// Steps with their status.
        Entries => "entries",
        MarkdownOnly => "markdown_only",
    }
}

open_enum! {
    /// Whether the agent reports when it is running, or the host infers it.
    pub enum RunStateSource {
        AgentReported => "agent_reported",
        Inferred => "inferred",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::testutil::{open_values, roundtrip};

    #[test]
    fn unit_proto_events_index_capabilities_roundtrip() {
        let raw = json!({
            "agent": "claude",
            "versions": {"adapter": "0.91.0", "cli": "2.1.296", "cli_compat": "verified"},
            "prompt": {"image": true, "file_ref": true, "max_image_bytes": 10485760},
            "turn": {"queue": true, "steer": {"supported": true, "side_effects": true},
                     "interrupt": true, "interrupt_stops_commands": true},
            "session": {"resume": true, "load_history": false, "fork": false},
            "config": [
                {"id": "permission_mode", "agent_option_id": "mode", "current": "default", "values": [
                    {"value": "default", "name": "Default", "risk": "normal"},
                    {"value": "bypassPermissions", "name": "Bypass", "description": "No prompts",
                     "risk": "full_access"}]},
                {"id": "fast_mode", "agent_option_id": "fast_mode", "current": "off", "values": []},
            ],
            "commands": [{"name": "review", "description": "Review a PR", "input_hint": "PR number"},
                         {"name": "init", "description": "Write CLAUDE.md"}],
            "interactions": {"reject_ends_turn": false, "question_forms": true, "plan_confirm": true},
            "extraction": {"exit_code": true, "output_streaming": false, "diff": "per_file",
                           "thinking": true, "plan": "markdown_only", "run_state": "inferred"},
        });
        let caps: SessionCapabilities = roundtrip(&raw);
        assert!(caps.turn.interrupt_stops_commands);
        assert_eq!(
            caps.config[1].id,
            ConfigOptionId::Unknown("fast_mode".into())
        );
        assert_eq!(
            caps.extraction.diff,
            DiffExtraction::Unknown("per_file".into())
        );

        let mut partial = raw.clone();
        partial["turn"]
            .as_object_mut()
            .unwrap()
            .remove("interrupt_stops_commands");
        let err = serde_json::from_value::<SessionCapabilities>(partial).unwrap_err();
        assert!(
            err.to_string().contains("interrupt_stops_commands"),
            "{err}"
        );
    }

    #[test]
    fn unit_proto_events_index_capability_enums_are_open() {
        open_values::<CliCompat>(CliCompat::KNOWN);
        open_values::<ConfigOptionId>(ConfigOptionId::KNOWN);
        open_values::<DiffExtraction>(DiffExtraction::KNOWN);
        open_values::<PlanExtraction>(PlanExtraction::KNOWN);
        open_values::<RunStateSource>(RunStateSource::KNOWN);
    }
}
