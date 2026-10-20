//! `policy/describe`: what a command the main process confirms natively
//! would do, for the dialog it shows before it sends the command (design
//! 5.7.8, 5.8.3). An action and its details, as codes and wire values; the
//! main process words them.
//!
//! The handler parses the command with [`Confirmable::parse`], looks up the
//! [`Facts`] its variant needs, and calls [`describe`]. The details by action:
//!
//! | action | command | details |
//! |---|---|---|
//! | `trust_project` | `project/trust` | `project_id`, `trust_digest` |
//! | `confirm_config` | `session/confirm_config` | `session_id`, `config_digest` |
//! | `full_access` | `session/create` | `agent`, `project_id`, `mode` |
//! | | `session/set_config` | `session_id`, `mode` |
//! | | `interaction/respond` | as `privileged_option`, and `mode` when the option has one |
//! | `privileged_option` | `interaction/respond` | `session_id`, `request_id`, `option_id`, `kind`, `effect` |
//! | `security_setting` | `settings/update` | `settings`: the security-related keys with their new values |
//! | `grant_elevation` | `auth/elevation_respond` with `grant` | `elevation_id`, `scopes` |
//! | `interrupt_turns` | `host/exit` with `force`, `host/prepare_upgrade` with `mode: force` | `running_turns` |
//!
//! A command that needs no confirmation in the form given (a tighter mode, an
//! ordinary setting, a denial, no running turns) is
//! [`DescribeError::NotConfirmable`].

use cupper_protocol::annotations::WirePath;
use cupper_protocol::capabilities::ConfigOptionId;
use cupper_protocol::domain::Tier;
use cupper_protocol::error::{INVALID_PARAMS, RpcError};
use cupper_protocol::interaction::{InteractionOption, InteractionOutcome};
use cupper_protocol::methods::admin::{
    AuthElevationRespond, AuthElevationRespondParams, ConfirmAction, ElevationAnswer,
    PolicyDescribeParams, PolicyDescribeResult, SettingsUpdate, SettingsUpdateParams,
};
use cupper_protocol::methods::handshake::{
    HostExit, HostExitParams, HostPrepareUpgrade, HostPrepareUpgradeParams, UpgradeMode,
};
use cupper_protocol::methods::interaction::{InteractionRespond, InteractionRespondParams};
use cupper_protocol::methods::project::{ProjectTrust, ProjectTrustParams};
use cupper_protocol::methods::session::{
    SessionConfirmConfig, SessionConfirmConfigParams, SessionCreate, SessionCreateParams,
    SessionSetConfig, SessionSetConfigParams,
};
use cupper_protocol::rpc::{Method, Scope};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

use super::input::requires;
use super::method::security_settings;
use super::option_tier;
use super::tier::rank;

/// A command the main process may confirm natively, parsed from
/// `policy/describe`'s `{method, params}`.
#[derive(Debug, Clone, PartialEq)]
pub enum Confirmable {
    ProjectTrust(ProjectTrustParams),
    ConfirmConfig(SessionConfirmConfigParams),
    Create(SessionCreateParams),
    SetConfig(SessionSetConfigParams),
    Respond(InteractionRespondParams),
    Settings(SettingsUpdateParams),
    Elevation(AuthElevationRespondParams),
    Exit(HostExitParams),
    Upgrade(HostPrepareUpgradeParams),
}

impl Confirmable {
    pub fn parse(described: &PolicyDescribeParams) -> Result<Self, DescribeError> {
        fn typed<T: DeserializeOwned>(params: &Value) -> Result<T, DescribeError> {
            T::deserialize(params).map_err(|err| DescribeError::Params(err.to_string()))
        }
        let params = &described.params;
        Ok(match described.method.as_str() {
            ProjectTrust::NAME => Self::ProjectTrust(typed(params)?),
            SessionConfirmConfig::NAME => Self::ConfirmConfig(typed(params)?),
            SessionCreate::NAME => Self::Create(typed(params)?),
            SessionSetConfig::NAME => Self::SetConfig(typed(params)?),
            InteractionRespond::NAME => Self::Respond(typed(params)?),
            SettingsUpdate::NAME => Self::Settings(typed(params)?),
            AuthElevationRespond::NAME => Self::Elevation(typed(params)?),
            HostExit::NAME => Self::Exit(typed(params)?),
            HostPrepareUpgrade::NAME => Self::Upgrade(typed(params)?),
            _ => return Err(DescribeError::NotConfirmable),
        })
    }
}

/// What some descriptions read of the host's state. The handler fills in
/// what the command's variant needs and leaves the rest at the default.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Facts {
    /// `session/create`, `session/set_config`: the tier of the permission
    /// mode asked for, by the agent's mapping. `None`: the mapping does not
    /// know the mode, which counts as `full_access` (design 5.6.5).
    pub mode_tier: Option<Tier>,
    /// `interaction/respond`: the chosen option, as the pending request has it.
    pub option: Option<InteractionOption>,
    /// `session/create`: the project's directory, from the index; `None` for
    /// a project the host does not know.
    pub project_path: Option<WirePath>,
    /// `host/exit`, `host/prepare_upgrade`.
    pub running_turns: u32,
}

/// Why `policy/describe` has no description; invalid params on the wire.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DescribeError {
    #[error("params of the described command: {0}")]
    Params(String),
    #[error("the command needs no native confirmation")]
    NotConfirmable,
    #[error("no such option in the pending request")]
    NoSuchOption,
}

impl From<DescribeError> for RpcError {
    fn from(err: DescribeError) -> Self {
        RpcError::new(INVALID_PARAMS, err.to_string())
    }
}

pub fn describe(
    command: &Confirmable,
    facts: &Facts,
) -> Result<PolicyDescribeResult, DescribeError> {
    use {ConfirmAction as A, Confirmable as C};
    let (action, details) = match command {
        C::ProjectTrust(p) => (
            A::TrustProject,
            map([
                ("project_id", json!(p.project_id)),
                ("trust_digest", json!(p.trust_digest)),
            ]),
        ),
        C::ConfirmConfig(p) => (
            A::ConfirmConfig,
            map([
                ("session_id", json!(p.session_id)),
                ("config_digest", json!(p.config_digest)),
            ]),
        ),
        C::Create(p) => {
            let config = p.config.as_ref();
            let mode = config.and_then(|c| c.permission_mode.as_ref());
            let mode = mode.ok_or(DescribeError::NotConfirmable)?;
            full_access(facts)?;
            let details = [
                ("agent", json!(p.agent)),
                ("project_id", json!(p.project_id)),
                ("project_path", json!(facts.project_path)),
            ];
            (A::FullAccess, with_mode(map(details), mode))
        }
        C::SetConfig(p) if p.option == ConfigOptionId::PermissionMode => {
            full_access(facts)?;
            (
                A::FullAccess,
                with_mode(map([("session_id", json!(p.session_id))]), &p.value),
            )
        }
        C::Respond(p) => respond(p, facts)?,
        C::Settings(p) => {
            let settings = security_settings(&p.patch);
            if settings.is_empty() {
                return Err(DescribeError::NotConfirmable);
            }
            (
                A::SecuritySetting,
                map([("settings", Value::Object(settings))]),
            )
        }
        C::Elevation(p) if p.decision == ElevationAnswer::Grant => (
            A::GrantElevation,
            map([
                ("elevation_id", json!(p.elevation_id)),
                ("scopes", json!(p.scopes)),
            ]),
        ),
        C::Exit(HostExitParams { force: true, .. })
        | C::Upgrade(HostPrepareUpgradeParams {
            mode: UpgradeMode::Force,
            ..
        }) if facts.running_turns > 0 => (
            A::InterruptTurns,
            map([("running_turns", json!(facts.running_turns))]),
        ),
        _ => return Err(DescribeError::NotConfirmable),
    };
    Ok(PolicyDescribeResult { action, details })
}

/// The mode asked for is `full_access`, or else nothing to confirm.
fn full_access(facts: &Facts) -> Result<(), DescribeError> {
    if facts.mode_tier.as_ref().is_none_or(full) {
        Ok(())
    } else {
        Err(DescribeError::NotConfirmable)
    }
}

/// `full_access`, or a tier this version does not know.
fn full(tier: &Tier) -> bool {
    rank(tier) == rank(&Tier::FullAccess)
}

/// `interaction/respond` choosing an option that switches to full access,
/// or a privileged one.
fn respond(
    params: &InteractionRespondParams,
    facts: &Facts,
) -> Result<(ConfirmAction, Map<String, Value>), DescribeError> {
    let InteractionOutcome::Selected(selected) = &params.outcome else {
        return Err(DescribeError::NotConfirmable);
    };
    let option = facts.option.as_ref();
    let option = option.filter(|option| option.option_id == selected.option_id);
    let option = option.ok_or(DescribeError::NoSuchOption)?;
    let details = map([
        ("session_id", json!(params.session_id)),
        ("request_id", json!(params.request_id)),
        ("option_id", json!(option.option_id)),
        ("kind", json!(option.kind)),
        ("effect", json!(option.effect)),
    ]);
    if option_tier(&option.effect).as_ref().is_some_and(full) {
        let details = match &option.effect.mode_after {
            Some(mode) => with_mode(details, mode),
            None => details,
        };
        Ok((ConfirmAction::FullAccess, details))
    } else if requires(option) == Scope::Privileged {
        Ok((ConfirmAction::PrivilegedOption, details))
    } else {
        Err(DescribeError::NotConfirmable)
    }
}

fn map<const N: usize>(entries: [(&str, Value); N]) -> Map<String, Value> {
    entries
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect()
}

fn with_mode(mut details: Map<String, Value>, mode: &str) -> Map<String, Value> {
    details.insert("mode".to_owned(), json!(mode));
    details
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::testkit::{option, with_mode as switching};

    fn parse(method: &str, params: Value) -> Result<Confirmable, DescribeError> {
        let method = method.into();
        Confirmable::parse(&PolicyDescribeParams { method, params })
    }

    /// `policy/describe`'s result for `method` with `params`, given `facts`.
    fn run(method: &str, params: Value, facts: &Facts) -> Result<Value, DescribeError> {
        let described = describe(&parse(method, params)?, facts)?;
        Ok(serde_json::to_value(described).unwrap())
    }

    fn digest() -> String {
        format!("d_{}", "a".repeat(64))
    }

    fn facts(mode_tier: Option<Tier>, running_turns: u32) -> Facts {
        Facts {
            mode_tier,
            running_turns,
            project_path: Some(WirePath("/w/app".into())),
            ..Facts::default()
        }
    }

    #[test]
    fn unit_policy_matrix_describe_each_action() {
        let (none, full) = (facts(None, 0), facts(Some(Tier::FullAccess), 0));
        for (method, params, facts, expected) in [
            (
                "project/trust",
                json!({"command_id": "c-1", "project_id": "p_1", "trust_digest": digest()}),
                &none,
                json!({"action": "trust_project",
                    "details": {"project_id": "p_1", "trust_digest": digest()}}),
            ),
            (
                "session/confirm_config",
                json!({"command_id": "c-1", "session_id": "s_1", "config_digest": digest()}),
                &none,
                json!({"action": "confirm_config",
                    "details": {"session_id": "s_1", "config_digest": digest()}}),
            ),
            (
                "session/create",
                json!({"command_id": "c-1", "agent": "claude", "project_id": "p_1",
                    "workspace": {"kind": "local"}, "message": [],
                    "config": {"permission_mode": "bypassPermissions"}}),
                &full,
                json!({"action": "full_access", "details": {"agent": "claude",
                    "project_id": "p_1", "project_path": "/w/app", "mode": "bypassPermissions"}}),
            ),
            (
                "session/set_config",
                json!({"command_id": "c-1", "session_id": "s_1", "option": "permission_mode",
                    "value": "agent-full-access"}),
                &full,
                json!({"action": "full_access",
                    "details": {"session_id": "s_1", "mode": "agent-full-access"}}),
            ),
            (
                "settings/update",
                json!({"command_id": "c-1", "patch": {"agents.codex.cli_path": "/opt/codex",
                    "diagnostics.verbose": true, "notifications.error": false}}),
                &none,
                json!({"action": "security_setting", "details": {"settings": {
                    "agents.codex.cli_path": "/opt/codex", "diagnostics.verbose": true}}}),
            ),
            (
                "auth/elevation_respond",
                json!({"command_id": "c-1", "elevation_id": "e_1", "decision": "grant",
                    "scopes": ["write"]}),
                &none,
                json!({"action": "grant_elevation",
                    "details": {"elevation_id": "e_1", "scopes": ["write"]}}),
            ),
            (
                "host/exit",
                json!({"command_id": "c-1", "force": true}),
                &facts(None, 2),
                json!({"action": "interrupt_turns", "details": {"running_turns": 2}}),
            ),
            (
                "host/prepare_upgrade",
                json!({"command_id": "c-1", "mode": "force"}),
                &facts(None, 1),
                json!({"action": "interrupt_turns", "details": {"running_turns": 1}}),
            ),
        ] {
            assert_eq!(run(method, params, facts), Ok(expected), "{method}");
        }
    }

    /// Describes choosing `chosen` (by `option_id`) in a pending request.
    fn choose(option_id: &str, chosen: InteractionOption) -> Result<Value, DescribeError> {
        let params = json!({"command_id": "c-1", "session_id": "s_1", "request_id": "r_1",
            "request_digest": digest(), "outcome": {"type": "selected", "option_id": option_id}});
        let facts = Facts {
            option: Some(chosen),
            ..Facts::default()
        };
        run("interaction/respond", params, &facts)
    }

    #[test]
    fn unit_policy_matrix_describe_options() {
        let always = option("always", "allow_always", "persistent", "privileged");
        assert_eq!(
            choose("always", always),
            Ok(
                json!({"action": "privileged_option", "details": {"session_id": "s_1",
                "request_id": "r_1", "option_id": "always", "kind": "allow_always",
                "effect": {"scope": "persistent", "persistent": false}}})
            )
        );
        // ExitPlanMode's options (design 5.3.6): to auto, and to bypass.
        let to = |id, mode, tier| {
            let option = option(id, "allow_always", "mode_switch", "privileged");
            choose(id, switching(option, mode, tier)).unwrap()
        };
        let auto = to("exit-plan-auto", "auto", "elevated");
        assert_eq!(
            (&auto["action"], auto["details"].get("mode")),
            (&json!("privileged_option"), None)
        );
        let bypass = to("exit-plan-bypass", "bypassPermissions", "full_access");
        assert_eq!(bypass["action"], "full_access");
        assert_eq!(bypass["details"]["mode"], "bypassPermissions");
        assert_eq!(bypass["details"]["effect"]["tier_after"], "full_access");
        // An unknown kind is privileged whatever the table says.
        let odd = option("maybe", "allow_maybe", "once", "approve");
        assert_eq!(choose("maybe", odd).unwrap()["action"], "privileged_option");
    }

    #[test]
    fn unit_policy_matrix_describe_refuses_what_needs_no_confirmation() {
        let set = |option| {
            json!({"command_id": "c-1", "session_id": "s_1", "option": option,
            "value": "acceptEdits"})
        };
        let create = json!({"command_id": "c-1", "agent": "codex", "project_id": "p_1",
            "workspace": {"kind": "local"}, "message": []});
        let deny = json!({"command_id": "c-1", "elevation_id": "e_1", "decision": "deny",
            "scopes": []});
        let (none, running) = (facts(None, 0), facts(None, 1));
        for (method, params, facts) in [
            ("ping", json!({}), &none),
            (
                "session/set_config",
                set("permission_mode"),
                &facts(Some(Tier::Elevated), 0),
            ),
            ("session/set_config", set("model"), &none),
            ("session/create", create, &none),
            (
                "settings/update",
                json!({"command_id": "c-1", "patch": {"worktrees.root": "/w"}}),
                &none,
            ),
            ("auth/elevation_respond", deny, &none),
            (
                "host/exit",
                json!({"command_id": "c-1", "force": false}),
                &running,
            ),
            (
                "host/exit",
                json!({"command_id": "c-1", "force": true}),
                &none,
            ),
            (
                "host/prepare_upgrade",
                json!({"command_id": "c-1", "mode": "wait"}),
                &running,
            ),
        ] {
            let got = run(method, params.clone(), facts);
            assert_eq!(got, Err(DescribeError::NotConfirmable), "{method} {params}");
        }
        let once = option("allow-once", "allow_once", "once", "approve");
        assert_eq!(
            choose("allow-once", once.clone()),
            Err(DescribeError::NotConfirmable)
        );
        assert_eq!(choose("other", once), Err(DescribeError::NoSuchOption));

        // A mode the mapping does not know counts as full access.
        let unknown = run("session/set_config", set("permission_mode"), &none);
        assert_eq!(unknown.unwrap()["action"], "full_access");
        let malformed = parse("project/trust", json!({"project_id": "p_1"}));
        assert!(
            matches!(malformed, Err(DescribeError::Params(_))),
            "{malformed:?}"
        );
        let rpc = RpcError::from(DescribeError::NotConfirmable);
        assert_eq!(rpc.code, INVALID_PARAMS);
    }
}
