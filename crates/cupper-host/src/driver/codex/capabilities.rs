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

#[cfg(test)]
mod tests {
    use cupper_protocol::capabilities::CliCompat;
    use cupper_protocol::recording::Body;

    use super::*;
    use crate::driver::gold_a::{CODEX, recordings};

    /// What s01's adapter answered, and the tiers of its modes (Q25).
    #[test]
    fn unit_capabilities_codex_from_initialize_and_the_session() {
        let (_, recording) = recordings(&CODEX, Some("s01-init-session")).remove(0);
        let mut results = (recording.frames.iter()).filter_map(|frame| match &frame.body {
            Body::Recv(msg) => Some(msg["result"].clone()),
            _ => None,
        });
        let init = results.find(|r| r["agentCapabilities"].is_object());
        let init = serde_json::from_value(init.unwrap_or_default()).expect("initialize");
        let options = results.find_map(|r| r.get("configOptions").cloned());
        let options: Vec<SessionConfigOption> =
            serde_json::from_value(options.unwrap_or_default()).expect("options");
        let versions = Versions {
            adapter: "2.2.2".into(),
            cli: "0.159.2".into(),
            cli_compat: CliCompat::Verified,
        };
        let caps = capabilities(&init, versions, config_options(&options), Vec::new());
        assert!(caps.turn.steer.supported && !caps.turn.steer.side_effects);
        assert!(!caps.turn.interrupt_stops_commands && caps.interactions.reject_ends_turn);
        let ids: Vec<&str> = caps.config.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(
            ids,
            ["permission_mode", "plan_mode", "model", "reasoning_effort"]
        );
        let risks = |o: &ConfigOption| o.values.iter().map(|v| v.risk.clone()).collect::<Vec<_>>();
        use Tier::*;
        assert_eq!(
            risks(&caps.config[0]),
            [Normal, Normal, Elevated, FullAccess]
        );
        assert_eq!(risks(&caps.config[1]), [Normal, Normal]);
        assert_eq!(extract::tier(TIERS, "danger"), FullAccess);
    }
}
