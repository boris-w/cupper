//! Principals, options and requests for the policy's tests.

use cupper_protocol::domain::{ClientRole, PrincipalKind};
use cupper_protocol::interaction::{
    InteractionOption, InteractionOutcome, InteractionRequest, Selected,
};
use cupper_protocol::methods::handshake::Principal;
use cupper_protocol::methods::interaction::InteractionRespondParams;
use cupper_protocol::rpc::Scope;
use serde_json::{Value, json};

/// The columns of 5.8.3, in its order.
pub const COLUMNS: [&str; 7] = [
    "UI control",
    "UI window",
    "cli",
    "cli + write",
    "cli + approve",
    "test_harness",
    "remote_device",
];

/// The principal of a column of 5.8.2 or 5.8.3, with its scopes.
pub fn column(name: &str) -> Principal {
    use PrincipalKind::{Cli, DesktopUi, RemoteDevice, TestHarness};
    use Scope::{Approve, Privileged, Read, Test, UiLocal, Write};
    let (id, kind, role, scopes) = match name {
        "UI control" => (
            "pr_desktop",
            DesktopUi,
            Some(ClientRole::Control),
            vec![Read, Write, Approve, Privileged, UiLocal],
        ),
        "UI window" => (
            "pr_desktop",
            DesktopUi,
            Some(ClientRole::Window),
            vec![Read, Write, Approve, UiLocal],
        ),
        "cli" => ("pr_cli", Cli, None, vec![Read]),
        "cli + write" => ("pr_cli", Cli, None, vec![Read, Write]),
        "cli + approve" => ("pr_cli", Cli, None, vec![Read, Approve]),
        "cli + write + approve" => ("pr_cli", Cli, None, vec![Read, Write, Approve]),
        "test_harness" => (
            "pr_test",
            TestHarness,
            None,
            vec![Read, Write, Approve, Privileged, UiLocal, Test],
        ),
        "remote_device" => (
            "pr_dev_phone",
            RemoteDevice,
            None,
            vec![Read, Write, Approve],
        ),
        _ => panic!("no column {name}"),
    };
    Principal {
        id: id.parse().unwrap(),
        kind,
        role,
        scopes,
    }
}

/// An option of `kind`, whose effect has `scope`, requiring `requires`.
pub fn option(id: &str, kind: &str, scope: &str, requires: &str) -> InteractionOption {
    serde_json::from_value(json!({"option_id": id, "name": id, "kind": kind,
        "effect": {"scope": scope, "persistent": false}, "requires": requires}))
    .unwrap()
}

/// `option` switching the session to `mode`, of `tier`.
pub fn with_mode(mut option: InteractionOption, mode: &str, tier: &str) -> InteractionOption {
    option.effect.mode_after = Some(mode.into());
    option.effect.tier_after = Some(tier.into());
    option
}

pub fn permission(options: Vec<InteractionOption>) -> InteractionRequest {
    serde_json::from_value(json!({"type": "permission", "title": "Run",
        "tool": {"tool_kind": "execute", "paths": []}, "options": options,
        "content_truncated": false}))
    .unwrap()
}

pub fn respond_params(
    outcome: InteractionOutcome,
    input: Option<Value>,
) -> InteractionRespondParams {
    let mut raw = json!({"command_id": "c-1", "session_id": "s_1", "request_id": "r_1",
        "request_digest": format!("d_{}", "f".repeat(64)), "outcome": outcome});
    if let Some(input) = input {
        raw["input"] = input;
    }
    serde_json::from_value(raw).unwrap()
}

pub fn selected(option_id: &str) -> InteractionOutcome {
    InteractionOutcome::Selected(Selected {
        option_id: option_id.into(),
    })
}
