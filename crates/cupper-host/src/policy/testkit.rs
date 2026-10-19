//! Principals for the policy's tests.

use cupper_protocol::domain::{ClientRole, PrincipalKind};
use cupper_protocol::methods::handshake::Principal;
use cupper_protocol::rpc::Scope;

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
