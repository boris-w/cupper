//! `AgentStatus` (design 5.6.2, AG-2, AG-5): what the settings and the
//! new-session pages show of an agent, from its launch, its runtime and its
//! login state. A semantic code becomes the field it is about: no CLI found
//! leaves `cli` out, a CLI the manifest's range does not cover is in
//! `cli.compat`, a runtime that fails its check is `adapter.runtime: failed`,
//! and `not_logged_in` is `auth.state: logged_out`. The index shows it
//! (T034, T100); until then nothing hears it.

use std::fmt;

use cupper_protocol::annotations::WirePath;
use cupper_protocol::domain::AgentKind;
use cupper_protocol::index::{
    AdapterStatus, AgentState, AgentStatus, AuthState, AuthStatus, CliStatus, RuntimeState,
};

use super::launch::{Launch, Program};
use crate::driver::acp::AuthStatusUpdate;
use crate::driver::api::AgentErrorClass;

/// Where the index hears of the agents (T100).
pub trait AgentNotes: Send + Sync + fmt::Debug {
    fn agent(&self, status: AgentStatus);
}

/// The index until T034: it hears nothing.
#[derive(Debug)]
pub struct NoAgentNotes;

impl AgentNotes for NoAgentNotes {
    fn agent(&self, _: AgentStatus) {}
}

/// Whether the adapter runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Running {
    No,
    Starting,
    Yes,
}

/// The login state the agent last reported (K16: kind and plan only).
pub fn auth(update: &AuthStatusUpdate) -> AuthStatus {
    let status = &update.auth_status;
    let state = match status.kind.as_str() {
        "none" => AuthState::LoggedOut,
        _ => AuthState::LoggedIn,
    };
    let plan = status
        .account
        .as_ref()
        .and_then(|account| account.plan.clone());
    let kind = Some(status.kind.clone());
    AuthStatus { state, kind, plan }
}

pub fn status(launch: &Launch, running: Running, auth: &AuthStatus) -> AgentStatus {
    let runtime = match (&launch.program, launch.unavailable) {
        (_, Some(AgentErrorClass::RuntimeFailed)) => RuntimeState::Failed,
        (Program::Runtime { node, bundle }, _) if !node.exists() || !bundle.exists() => {
            RuntimeState::Missing
        }
        _ => RuntimeState::Ready,
    };
    let state = match (launch.unavailable, running) {
        (Some(_), _) => AgentState::Unavailable,
        (None, Running::No) => AgentState::Idle,
        (None, Running::Starting) => AgentState::Starting,
        (None, Running::Yes) => AgentState::Running,
    };
    let cli = launch.cli.as_ref().map(|cli| CliStatus {
        path: WirePath(cli.path.display().to_string()),
        version: (!cli.version.is_empty()).then(|| cli.version.clone()),
        compat: launch.versions.cli_compat.clone(),
        install: cli.install.clone(),
    });
    AgentStatus {
        agent: AgentKind::Claude,
        state,
        cli,
        adapter: AdapterStatus {
            version: launch.versions.adapter.clone(),
            runtime,
            failure: None,
        },
        auth: auth.clone(),
        // The runtime's latest sample, which it adds.
        rss_bytes: None,
        config_dir: launch.config_dir.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use cupper_protocol::capabilities::{CliCompat, Versions};
    use cupper_protocol::index::ConfigDirState;
    use serde_json::json;

    use super::*;

    fn launch(unavailable: Option<AgentErrorClass>) -> Launch {
        let versions = Versions {
            adapter: "0.91.0".into(),
            cli: "2.1.296".into(),
            cli_compat: CliCompat::OlderUntested,
        };
        Launch {
            program: Program::Runtime {
                node: PathBuf::from("/missing/node"),
                bundle: PathBuf::from("/missing/bundle"),
            },
            env: cupper_sys::Env::new(),
            versions,
            cli: None,
            config_dir: ConfigDirState::Rejected,
            unavailable,
        }
    }

    #[test]
    fn agent_status_maps_semantic_codes_to_fields() {
        let (state, kind, plan) = (AuthState::Unchecked, None, None);
        let unchecked = AuthStatus { state, kind, plan };
        let of = |unavailable| {
            serde_json::to_value(status(&launch(unavailable), Running::No, &unchecked)).unwrap()
        };
        let missing = of(Some(AgentErrorClass::CliMissing));
        assert_eq!(
            missing,
            json!({"agent": "claude", "state": "unavailable", "config_dir": "rejected",
            "adapter": {"version": "0.91.0", "runtime": "missing"}, "auth": {"state": "unchecked"}})
        );
        let failed = of(Some(AgentErrorClass::RuntimeFailed));
        assert_eq!(failed["adapter"]["runtime"], "failed");

        let auth_of = |status| {
            let update = serde_json::from_value(json!({"authStatus": status})).unwrap();
            serde_json::to_value(auth(&update)).unwrap()
        };
        let none = json!({"state": "logged_out", "kind": "none"});
        assert_eq!(
            auth_of(json!({"kind": "none", "label": "Not logged in"})),
            none
        );
        let account = json!({"kind": "account", "account": {"plan": "max", "email": "e"}});
        let logged_in = json!({"state": "logged_in", "kind": "account", "plan": "max"});
        assert_eq!(auth_of(account), logged_in);
    }
}
