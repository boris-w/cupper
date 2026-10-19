//! The policy (design 5.2.5, 5.8.2, 5.8.3): may this principal make this
//! request? Pure functions: no I/O, no clock; a time comes in as a parameter.
//! The host decides here whatever a client checked first.
//!
//! Two levels:
//!
//! 1. **Method level**, for every request and client notification:
//!    [`method()`] checks the method's `SCOPE`, refuses `auth/elevate` to every
//!    principal but `cli`, and keeps from a remote device the methods SEC-9
//!    takes away although it holds their scope.
//! 2. **Resource level**, by the parameters and the host's state, where the
//!    executor has them:
//!    - [`settings_update`]: a security-related key takes `privileged`;
//!    - [`set_config`]: SEC-9 lets a remote device change only the model, the
//!      reasoning effort and, by the tier rules, the permission mode;
//!    - [`tier_change`]: a looser tier from `session/create`'s `config`,
//!      `session/set_config` or an option's `mode_after` (design 5.8.2);
//!    - [`git_revert`], [`respond`]: the window's input evidence, the chosen
//!      option's `requires`, and the 400 ms after a request (design 5.7.8);
//!    - [`attach()`]: the project trust gate (design 5.2.6).
//!
//! `policy/describe` is [`Confirmable`] and [`describe()`].
//!
//! Cells of the matrix (5.8.3) the policy allows because the scope does, and
//! leaves the rest to others:
//! - "原生确认": the control connection holds `privileged`; the main process
//!   shows the native confirmation before it sends;
//! - "仅由主进程代发" (a window's `initialize`): it needs no scope, and the
//!   gateway takes it once per connection;
//! - "仅 … 构建": the host registers the method only in the builds of its
//!   `x-builds` (design 5.9.9).

mod attach;
mod describe;
mod input;
mod method;
mod tier;

#[cfg(test)]
mod testkit;

use cupper_protocol::domain::{ClientRole, PrincipalKind};
use cupper_protocol::error::{ErrorCode, ErrorReason, INVALID_PARAMS, RpcError};
use cupper_protocol::methods::handshake::Principal;
use cupper_protocol::rpc::Scope;

pub use attach::{ProjectConfig, attach};
pub use describe::{Confirmable, DescribeError, Facts, describe};
pub use input::{MIN_ANSWER_DELAY, Pending, git_revert, respond};
pub use method::{method, set_config, settings_update};
pub use tier::{ceiling, option_tier, tier_change};

/// Why the policy refuses, as a semantic code (design 5.3.8).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Denial {
    /// `forbidden/scope`, with the scope the principal lacks.
    #[error("needs scope {0}")]
    Scope(Scope),
    /// `forbidden/role`: `auth/elevate` from a principal other than `cli`.
    #[error("only cli asks for elevation")]
    CliOnly,
    /// `forbidden/role`: SEC-9 keeps this from a remote device.
    #[error("not from a remote device (SEC-9)")]
    Sec9,
    /// `forbidden/user_activation_required`: the window sent no input
    /// evidence, or evidence that does not fit the command.
    #[error("needs real user input that fits the command")]
    UserActivation,
    /// `conflict/too_early`: the window answered within
    /// [`MIN_ANSWER_DELAY`] of the request.
    #[error("answered too soon after the request")]
    TooEarly,
    /// Invalid params: the chosen option is not one of the request's.
    #[error("no such option in the request")]
    NoSuchOption,
    /// `unavailable/project_untrusted`.
    #[error("the project is not trusted")]
    ProjectUntrusted,
}

impl From<Denial> for RpcError {
    fn from(denial: Denial) -> Self {
        let message = denial.to_string();
        let error = |code, reason| RpcError::app(code, &message).with_reason(reason);
        match denial {
            Denial::Scope(scope) => error(ErrorCode::Forbidden, ErrorReason::Scope)
                .with_data(|data| data.scope = Some(scope)),
            Denial::CliOnly | Denial::Sec9 => error(ErrorCode::Forbidden, ErrorReason::Role),
            Denial::UserActivation => {
                error(ErrorCode::Forbidden, ErrorReason::UserActivationRequired)
            }
            Denial::TooEarly => error(ErrorCode::Conflict, ErrorReason::TooEarly),
            Denial::NoSuchOption => RpcError::new(INVALID_PARAMS, message),
            Denial::ProjectUntrusted => {
                error(ErrorCode::Unavailable, ErrorReason::ProjectUntrusted)
            }
        }
    }
}

/// `principal` holds `scope`, or the refusal.
fn require(principal: &Principal, scope: &Scope) -> Result<(), Denial> {
    if principal.scopes.contains(scope) {
        Ok(())
    } else {
        Err(Denial::Scope(scope.clone()))
    }
}

/// A `desktop_ui` connection other than the main process's own: the
/// window's rules apply to it, also when its role is unknown.
fn is_window(principal: &Principal) -> bool {
    principal.kind == PrincipalKind::DesktopUi && principal.role != Some(ClientRole::Control)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn unit_policy_matrix_denials_are_semantic_codes() {
        let wire = |denial: Denial| serde_json::to_value(RpcError::from(denial)).unwrap();
        let data = |denial| wire(denial)["data"].clone();
        assert_eq!(
            data(Denial::Scope(Scope::Privileged)),
            json!({"code": "forbidden", "reason": "scope", "scope": "privileged"})
        );
        for denial in [Denial::CliOnly, Denial::Sec9] {
            assert_eq!(data(denial), json!({"code": "forbidden", "reason": "role"}));
        }
        assert_eq!(
            data(Denial::UserActivation),
            json!({"code": "forbidden", "reason": "user_activation_required"})
        );
        assert_eq!(
            data(Denial::TooEarly),
            json!({"code": "conflict", "reason": "too_early"})
        );
        assert_eq!(
            data(Denial::ProjectUntrusted),
            json!({"code": "unavailable", "reason": "project_untrusted"})
        );
        let invalid = wire(Denial::NoSuchOption);
        assert_eq!(invalid["code"], INVALID_PARAMS);
        assert!(invalid.get("data").is_none());
    }

    #[test]
    fn unit_policy_matrix_unknown_desktop_role_counts_as_window() {
        let mut desktop = testkit::column("UI window");
        for role in [None, Some(ClientRole::Unknown("tab".into()))] {
            desktop.role = role;
            assert!(is_window(&desktop), "{:?}", desktop.role);
        }
        assert!(!is_window(&testkit::column("UI control")));
        assert!(!is_window(&testkit::column("test_harness")));
    }
}
