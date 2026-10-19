//! The method level (design 5.2.5) and the rules that read only a method's
//! parameters: security-related settings and SEC-9's narrowing of
//! `session/set_config`.

use cupper_protocol::capabilities::ConfigOptionId;
use cupper_protocol::domain::PrincipalKind;
use cupper_protocol::methods::admin::{
    AuthElevate, SettingsPatch, SettingsUpdate, SettingsUpdateParams,
};
use cupper_protocol::methods::git::{GitCommit, GitPush, GitRevert, GitStage, GitUnstage};
use cupper_protocol::methods::handshake::Principal;
use cupper_protocol::methods::project::{AgentsRefresh, AgentsRestart, ProjectAdd, ProjectRemove};
use cupper_protocol::methods::session::{
    SessionArchive, SessionRename, SessionRestore, SessionSetConfigParams,
};
use cupper_protocol::rpc::{Method, Scope};
use serde::Serialize;
use serde_json::{Map, Value};

use super::{Denial, require};

/// What SEC-9 keeps from a remote device although it holds the scope: no
/// project added, no file, git state or setting written (design 5.8.3).
/// `session/set_config` narrows by option instead, see [`set_config`].
const SEC9: &[&str] = &[
    ProjectAdd::NAME,
    ProjectRemove::NAME,
    AgentsRefresh::NAME,
    AgentsRestart::NAME,
    SessionRename::NAME,
    SessionArchive::NAME,
    SessionRestore::NAME,
    GitStage::NAME,
    GitUnstage::NAME,
    GitRevert::NAME,
    GitCommit::NAME,
    GitPush::NAME,
    SettingsUpdate::NAME,
];

/// The method level for method `name`, whose `SCOPE` is `scope` (`None`:
/// any principal).
pub fn method(principal: &Principal, name: &str, scope: Option<&Scope>) -> Result<(), Denial> {
    if let Some(scope) = scope {
        require(principal, scope)?;
    }
    if name == AuthElevate::NAME && principal.kind != PrincipalKind::Cli {
        return Err(Denial::CliOnly);
    }
    if principal.kind == PrincipalKind::RemoteDevice && SEC9.contains(&name) {
        return Err(Denial::Sec9);
    }
    Ok(())
}

/// `settings/update`: a security-related key takes `privileged` (design 5.2.11).
pub fn settings_update(principal: &Principal, params: &SettingsUpdateParams) -> Result<(), Denial> {
    if security_settings(&params.patch).is_empty() {
        Ok(())
    } else {
        require(principal, &Scope::Privileged)
    }
}

/// The security-related keys in `patch` with their new values. Every key of
/// the patch is named here, so a new key does not compile until it is sorted.
pub(super) fn security_settings(patch: &SettingsPatch) -> Map<String, Value> {
    let SettingsPatch {
        claude_cli_path,
        codex_cli_path,
        env_passthrough,
        verbose_diagnostics,
        worktrees_root: _,
        notify_needs_action: _,
        notify_turn_completed: _,
        notify_error: _,
        terminal_app: _,
        editor_app: _,
        default_agent: _,
        idle_exit_minutes: _,
    } = patch;
    let mut keys = Map::new();
    put(&mut keys, "agents.claude.cli_path", claude_cli_path);
    put(&mut keys, "agents.codex.cli_path", codex_cli_path);
    put(&mut keys, "agents.env_passthrough", env_passthrough);
    put(&mut keys, "diagnostics.verbose", verbose_diagnostics);
    keys
}

fn put<T: Serialize>(keys: &mut Map<String, Value>, key: &str, value: &Option<T>) {
    if let Some(value) = value {
        keys.insert(key.to_owned(), serde_json::json!(value));
    }
}

/// `session/set_config`: SEC-9 lets a remote device change the model, the
/// reasoning effort and the permission mode only (MB-9); the permission mode
/// then follows [`tier_change`](super::tier_change), so it only tightens.
pub fn set_config(principal: &Principal, params: &SessionSetConfigParams) -> Result<(), Denial> {
    use ConfigOptionId::{Model, PermissionMode, ReasoningEffort};
    let narrowed = !matches!(params.option, Model | ReasoningEffort | PermissionMode);
    if principal.kind == PrincipalKind::RemoteDevice && narrowed {
        Err(Denial::Sec9)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use cupper_protocol::methods::admin::{Settings, SettingsPatch};
    use serde_json::json;

    use super::*;
    use crate::policy::testkit::column;

    #[test]
    fn unit_policy_matrix_remote_set_config_by_option() {
        // The matrix covers the other columns and the model; here every option.
        let params = |option: &str| SessionSetConfigParams {
            command_id: "c-1".parse().unwrap(),
            session_id: "s_1".parse().unwrap(),
            option: ConfigOptionId::from(option),
            value: "x".into(),
        };
        let remote = column("remote_device");
        for option in ["model", "reasoning_effort", "permission_mode"] {
            assert_eq!(set_config(&remote, &params(option)), Ok(()), "{option}");
        }
        for option in ["plan_mode", "fast_mode"] {
            assert_eq!(
                set_config(&remote, &params(option)),
                Err(Denial::Sec9),
                "{option}"
            );
        }
    }

    fn patch(raw: Value) -> SettingsUpdateParams {
        serde_json::from_value(json!({"command_id": "c-1", "patch": raw})).unwrap()
    }

    #[test]
    fn unit_policy_matrix_security_keys_take_privileged() {
        let security = [
            json!({"agents.claude.cli_path": "/opt/claude"}),
            json!({"agents.codex.cli_path": null}),
            json!({"agents.env_passthrough": ["API_KEY"]}),
            json!({"diagnostics.verbose": false}),
            json!({"notifications.error": true, "diagnostics.verbose": true}),
        ];
        let ordinary = json!({"worktrees.root": "/w", "host.idle_exit_minutes": 5});
        for principal in [column("UI window"), column("cli + write")] {
            for raw in &security {
                let refused = settings_update(&principal, &patch(raw.clone()));
                assert_eq!(refused, Err(Denial::Scope(Scope::Privileged)), "{raw}");
            }
            assert_eq!(
                settings_update(&principal, &patch(ordinary.clone())),
                Ok(())
            );
        }
        for raw in security {
            assert_eq!(settings_update(&column("UI control"), &patch(raw)), Ok(()));
        }
    }

    #[test]
    fn unit_policy_matrix_every_setting_is_sorted() {
        // A full patch: each key of 5.2.11 once, security-related or not.
        let all: Settings = serde_json::from_value(json!({
            "agents.claude.cli_path": "/c", "agents.codex.cli_path": "/x",
            "agents.env_passthrough": [], "worktrees.root": "/w",
            "notifications.needs_action": true, "notifications.turn_completed": true,
            "notifications.error": true, "apps.terminal": "t", "apps.editor": "e",
            "sessions.default_agent": "claude", "host.idle_exit_minutes": 5,
            "diagnostics.verbose": false}))
        .unwrap();
        let full: SettingsPatch =
            serde_json::from_value(serde_json::to_value(all).unwrap()).unwrap();
        let keys: Vec<String> = security_settings(&full)
            .into_iter()
            .map(|(k, _)| k)
            .collect();
        assert_eq!(
            keys,
            [
                "agents.claude.cli_path",
                "agents.codex.cli_path",
                "agents.env_passthrough",
                "diagnostics.verbose",
            ]
        );
    }
}
