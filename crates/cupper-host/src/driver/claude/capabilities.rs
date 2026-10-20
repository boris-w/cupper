//! What a Claude session can do (design 5.6.7, 5.3.7): from the `initialize`
//! response, the session's config options and its slash commands. Options
//! map to cupper's by category (design 5.6.5); until the tier table (T095),
//! every permission mode counts as `full_access`, as values missing from that
//! table do.

use agent_client_protocol::schema::v1::{
    AvailableCommand, AvailableCommandInput, InitializeResponse, SessionConfigKind,
    SessionConfigOption, SessionConfigOptionCategory as Category, SessionConfigSelectOptions,
};
use cupper_protocol::capabilities::{
    ConfigOption, ConfigOptionId, ConfigValue, DiffExtraction, ExtractionCapabilities,
    InteractionCapabilities, PlanExtraction, PromptCapabilities, RunStateSource,
    SessionCapabilities, SessionLifecycle, SlashCommand, SteerCapability, TurnCapabilities,
    Versions,
};
use cupper_protocol::domain::{AgentKind, Tier};
use cupper_protocol::limits;
use serde_json::Value;

/// The capabilities of a session on an adapter that answered `initialize`
/// with `init` (design 5.6.7). Fixed by the adapter rather than read: Claude
/// stops commands on interrupt, steering has side effects (K10), commands do
/// not stream (K22), diffs are snippets, the plan is Markdown only (K25) and
/// the run state is the host's inference.
pub fn capabilities(
    init: &InitializeResponse,
    versions: Versions,
    config: Vec<ConfigOption>,
    commands: Vec<SlashCommand>,
) -> SessionCapabilities {
    let agent = &init.agent_capabilities;
    // At the top of the response, not under `agentCapabilities` (T003).
    let steering = init.meta.as_ref().and_then(|meta| meta.get("steering"));
    let steer = steering.and_then(|steering| steering.get("supported"));
    SessionCapabilities {
        agent: AgentKind::Claude,
        versions,
        prompt: PromptCapabilities {
            image: agent.prompt_capabilities.image,
            // `resource_link` is part of every agent's baseline.
            file_ref: true,
            max_image_bytes: limits::V1.max_image_bytes,
        },
        turn: TurnCapabilities {
            queue: true,
            steer: SteerCapability {
                supported: steer.and_then(Value::as_bool).unwrap_or(false),
                side_effects: true,
            },
            interrupt: true,
            interrupt_stops_commands: true,
        },
        session: SessionLifecycle {
            resume: agent.session_capabilities.resume.is_some(),
            load_history: agent.load_session,
            // P1: not offered before it is built.
            fork: false,
        },
        config,
        commands,
        interactions: InteractionCapabilities {
            reject_ends_turn: false,
            question_forms: true,
            plan_confirm: true,
        },
        extraction: ExtractionCapabilities {
            exit_code: true,
            output_streaming: false,
            diff: DiffExtraction::Fragment,
            thinking: true,
            plan: PlanExtraction::MarkdownOnly,
            run_state: RunStateSource::Inferred,
        },
    }
}

/// The options the agent reports, by category; choices as the agent names
/// them (CP-4). Boolean options have no place among them yet.
pub fn config_options(options: &[SessionConfigOption]) -> Vec<ConfigOption> {
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
        let risk = match id {
            ConfigOptionId::PermissionMode => Tier::FullAccess,
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
                risk: risk.clone(),
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

#[cfg(test)]
mod tests {
    use cupper_protocol::capabilities::CliCompat;
    use cupper_protocol::recording::Body;

    use super::*;
    use crate::driver::claude::gold_a::{extract, recordings};

    /// What Claude's adapter answered to `initialize` and `session/new` in s01.
    fn s01() -> (InitializeResponse, Vec<SessionConfigOption>) {
        let (_, recording) = recordings(Some("s01-init-session")).remove(0);
        let mut results = (recording.frames.iter()).filter_map(|frame| match &frame.body {
            Body::Recv(msg) => Some(msg["result"].clone()),
            _ => None,
        });
        let init = results.find(|r| r["agentCapabilities"].is_object());
        let options = results.find_map(|r| r.get("configOptions").cloned());
        let init = serde_json::from_value(init.unwrap_or_default()).expect("initialize");
        let options = serde_json::from_value(options.unwrap_or_default()).expect("options");
        (init, options)
    }

    fn versions() -> Versions {
        let (adapter, cli) = ("0.91.0".to_owned(), "2.1.296".to_owned());
        let cli_compat = CliCompat::Verified;
        Versions {
            adapter,
            cli,
            cli_compat,
        }
    }

    #[test]
    fn unit_capabilities_from_initialize_and_the_session() {
        let (init, options) = s01();
        let caps = capabilities(&init, versions(), config_options(&options), Vec::new());
        // `steering.supported` is read from the top-level `_meta` (T003).
        assert!(caps.turn.steer.supported && caps.turn.steer.side_effects);
        assert!(caps.prompt.image && caps.session.resume && caps.session.load_history);
        assert!(caps.turn.interrupt_stops_commands && !caps.interactions.reject_ends_turn);
        assert!(caps.extraction.exit_code && !caps.extraction.output_streaming);
        let ids: Vec<_> = caps.config.iter().map(|o| o.id.clone()).collect();
        use ConfigOptionId as Id;
        assert_eq!(ids, [Id::PermissionMode, Id::Model, Id::ReasoningEffort]);
        assert_eq!(caps.config[0].current, "default");
        // Every permission mode is full access until the tier table (T095).
        let risks = |o: &ConfigOption| o.values.iter().map(|v| v.risk.clone()).collect::<Vec<_>>();
        assert!(
            risks(&caps.config[0])
                .iter()
                .all(|r| *r == Tier::FullAccess)
        );
        assert!(risks(&caps.config[1]).iter().all(|r| *r == Tier::Normal));

        let mut config = caps.config;
        assert!(set_mode(&mut config, "plan") && !set_mode(&mut config, "plan"));
        assert_eq!(config[0].current, "plan");
    }

    /// K25: TodoWrite is not available in SDK sessions, so Claude's plan is
    /// Markdown only, even when tasks are enabled (s02c).
    #[test]
    fn unit_capabilities_plan_is_markdown_only() {
        let (init, _) = s01();
        let caps = capabilities(&init, versions(), Vec::new(), Vec::new());
        assert_eq!(caps.extraction.plan, PlanExtraction::MarkdownOnly);
        for scenario in ["s02c-plan-tasks-enabled", "s02b-plan", "s05-questions"] {
            for (_, recording) in recordings(Some(scenario)) {
                let plans: Vec<Value> = (extract(&recording).iter())
                    .filter_map(|v| v.pointer("/event/update/plan").cloned())
                    .collect();
                assert!(plans.iter().all(|p| p["source"] == "markdown"), "{plans:?}");
            }
        }
    }
}
