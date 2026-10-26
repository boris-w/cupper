//! UNIT-settings: the defaults, what `host.db` holds over them, the checks,
//! and `settings/get` and `settings/update` through the gateway as the
//! desktop sends them.

use std::sync::atomic::{AtomicUsize, Ordering};

use cupper_protocol::domain::{ClientRole, PrincipalKind};
use cupper_protocol::methods::handshake::Principal;
use cupper_protocol::rpc::Scope;

use super::*;
use crate::clock::ManualClock;
use crate::durations::Durations;
use crate::gateway::Routes;
use crate::gateway::testkit::Peer;
use crate::ledger::Ledger;
use crate::store::open_reader;
use crate::store::testutil::{TempDir, host_writer, query};

fn desktop(role: ClientRole, scopes: &[Scope]) -> Principal {
    Principal {
        id: "pr_desktop".parse().unwrap(),
        kind: PrincipalKind::DesktopUi,
        role: Some(role),
        scopes: scopes.to_vec(),
    }
}

#[test]
fn unit_settings_rows_over_the_defaults_and_the_checks() {
    let defaults = defaults(Path::new("/Users/someone"));
    assert_eq!(
        defaults.worktrees_root.0,
        "/Users/someone/.cupper/worktrees"
    );
    assert!(defaults.notify_error && !defaults.verbose_diagnostics);
    // A row that holds, one of a key the host does not know, one that does
    // not fit.
    let rows = [
        ("notifications.error", "false"),
        ("no.such.key", "1"),
        ("host.idle_exit_minutes", "\"ten\""),
    ];
    let read = merged(&defaults, rows.map(|(k, v)| (k.into(), v.into())).into());
    assert!(!read.notify_error);
    assert_eq!(read.idle_exit_minutes, 10);

    let patch = |value| serde_json::from_value::<SettingsPatch>(value).unwrap();
    let refused = [
        json!({"agents.claude.cli_path": "bin/claude"}),
        json!({"worktrees.root": "w"}),
        json!({"agents.env_passthrough": ["API_KEY", "not-a-name"]}),
        json!({"host.idle_exit_minutes": 0}),
        json!({"apps.editor": ""}),
    ];
    for value in refused {
        assert!(apply(&read, &patch(value.clone())).is_err(), "{value}");
    }
    let (next, set) = apply(&read, &patch(json!({"notifications.error": true}))).unwrap();
    assert!(next.notify_error);
    assert_eq!(Value::Object(set), json!({"notifications.error": true}));
}

#[tokio::test]
async fn unit_settings_update_writes_its_keys_and_finds_the_agents_again() {
    let dir = TempDir::new("settings");
    let clock = ManualClock::new();
    let (writer, _thread) = host_writer(&dir, &clock);
    let conn = open_reader(&dir.host_db()).unwrap();
    let shared: Arc<dyn Clock> = Arc::new(clock);
    let home = Path::new("/Users/someone");
    let load = || SettingsHandle::load(&conn, writer.clone(), Arc::clone(&shared), home);
    let settings = load().unwrap();
    let found = Arc::new(AtomicUsize::new(0));
    let rediscover: Rediscover = {
        let found = Arc::clone(&found);
        Arc::new(move || {
            found.fetch_add(1, Ordering::Relaxed);
            async {}.boxed()
        })
    };
    let ledger = Ledger::load(
        &conn,
        writer.clone(),
        Arc::clone(&shared),
        &Durations::default(),
    );
    let mut routes = Routes::new(ledger.unwrap());
    let port = SettingsPort {
        settings: settings.clone(),
        rediscover,
    };
    let port = Arc::new(port) as Arc<dyn Port>;
    routes.add::<SettingsGet>(Arc::clone(&port));
    routes.add::<SettingsUpdate>(port);

    use Scope::{Approve, Privileged, Read, UiLocal, Write};
    let window = desktop(ClientRole::Window, &[Read, Write, Approve, UiLocal]);
    let control = desktop(ClientRole::Control, &[Read, Write, Approve, Privileged]);
    let update = |command: &str, patch| json!({"command_id": command, "patch": patch});
    // An ordinary key, from a window: written, and no agent reads it.
    let ordinary = update("c-1", json!({"notifications.error": false}));
    let done = Peer::new(window)
        .call(&routes, "settings/update", ordinary)
        .await;
    assert_eq!(done.unwrap()["settings"]["notifications.error"], false);
    assert_eq!(found.load(Ordering::Relaxed), 0);
    // A security-related key, from the privileged control connection: the
    // agents are found again before the reply.
    let passthrough = update("c-2", json!({"agents.env_passthrough": ["API_KEY"]}));
    let done = Peer::new(control.clone())
        .call(&routes, "settings/update", passthrough)
        .await;
    assert_eq!(
        done.unwrap()["settings"]["agents.env_passthrough"],
        json!(["API_KEY"])
    );
    assert_eq!(found.load(Ordering::Relaxed), 1);
    // A patch the checks refuse changes nothing.
    let relative = update("c-3", json!({"agents.codex.cli_path": "codex"}));
    let refused = Peer::new(control.clone())
        .call(&routes, "settings/update", relative)
        .await;
    assert_eq!(refused.unwrap_err()["code"], INVALID_PARAMS);
    let got = Peer::new(control)
        .call(&routes, "settings/get", json!({}))
        .await;
    assert_eq!(got.unwrap()["settings"], json!(settings.current()));
    assert_eq!(settings.current().codex_cli_path, CliPath(None));

    // The rows hold what changed, and a restart reads them back.
    let sql = "SELECT key || '=' || value FROM settings ORDER BY key";
    let rows = query(&dir.host_db(), sql, |row| row.get::<_, String>(0));
    assert_eq!(
        rows,
        [
            "agents.env_passthrough=[\"API_KEY\"]",
            "notifications.error=false"
        ]
    );
    assert_eq!(load().unwrap().current(), settings.current());
}
