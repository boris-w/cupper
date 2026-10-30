//! What a Codex session can do (design 5.6.7, 5.3.7), and Codex's modes by
//! approval tier (design 5.6.5, Q25).

use agent_client_protocol::schema::v1::{InitializeResponse, SessionConfigOption};
use cupper_protocol::capabilities::{
    ConfigOption, DiffExtraction, ExtractionCapabilities, InteractionCapabilities, PlanExtraction,
    PromptCapabilities, RunStateSource, SessionCapabilities, SessionLifecycle, SlashCommand,
    SteerCapability, TurnCapabilities, Versions,
};
use cupper_protocol::domain::{AgentKind, Tier};
use cupper_protocol::limits;
use serde_json::Value;

use crate::driver::extract::{self, Tiers};

/// The capabilities of a session on an adapter that answered `initialize`
/// with `init` (design 5.6.7). Fixed by the adapter rather than read:
/// interrupting leaves commands running (K3), steering has no side effects,
/// a rejection ends the turn (K2), commands stream, diffs come per hunk, the
/// plan has entries when `update_plan` is available, and the thread reports
/// its state.
pub fn capabilities(
    init: &InitializeResponse,
    versions: Versions,
    config: Vec<ConfigOption>,
    commands: Vec<SlashCommand>,
) -> SessionCapabilities {
    let agent = &init.agent_capabilities;
    let steering = init.meta.as_ref().and_then(|meta| meta.get("steering"));
    let steer = steering.and_then(|steering| steering.get("supported"));
    SessionCapabilities {
        agent: AgentKind::Codex,
        versions,
        prompt: PromptCapabilities {
            image: agent.prompt_capabilities.image,
            file_ref: true,
            max_image_bytes: limits::V1.max_image_bytes,
        },
        turn: TurnCapabilities {
            queue: true,
            steer: SteerCapability {
                supported: steer.and_then(Value::as_bool).unwrap_or(false),
                side_effects: false,
            },
            interrupt: true,
            interrupt_stops_commands: false,
        },
        session: SessionLifecycle {
            resume: agent.session_capabilities.resume.is_some(),
            load_history: agent.load_session,
            fork: false,
        },
        config,
        commands,
        interactions: InteractionCapabilities {
            reject_ends_turn: true,
            question_forms: true,
            plan_confirm: true,
        },
        extraction: ExtractionCapabilities {
            exit_code: true,
            output_streaming: true,
            diff: DiffExtraction::PerHunk,
            thinking: true,
            plan: PlanExtraction::Entries,
            run_state: RunStateSource::AgentReported,
        },
    }
}

/// The session's options, Codex's modes at their tier; `collaboration_mode`
/// is cupper's `plan_mode`.
pub fn config_options(options: &[SessionConfigOption]) -> Vec<ConfigOption> {
    extract::config_options(options, TIERS)
}

/// codex-acp 2.2.2's modes by approval tier (design 5.6.5; T102 checked that
/// there are no others).
pub const TIERS: Tiers = &[
    ("read-only", Tier::Normal),
    ("workspace-write", Tier::Normal),
    ("agent", Tier::Elevated),
    ("agent-full-access", Tier::FullAccess),
];
