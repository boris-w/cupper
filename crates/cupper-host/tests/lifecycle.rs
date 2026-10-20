//! IT-lifecycle: the host as a process (design 5.2.13). What the previous run
//! left running ends at startup, `host/exit` and SIGTERM exit in order, and
//! a host that cannot start exits with the code of the startup contract, and
//! a session runs a turn with the fake agent. IT-project-add-trust adds and
//! trusts a project in a git repository built here; IT-clock-advance and
//! IT-seed drive the test interface. The tests talk to the host as the CLI's
//! `harness` client.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use cupper_cli::client::{self, ClientAuth, Incoming, Options};
use cupper_cli::harness::HarnessKey;
use cupper_cli::verify::HostCheck;
use cupper_host::clock::{Clock, SystemClock};
use cupper_host::driver::runtime::launch;
use cupper_host::gateway::listen::HostLock;
use cupper_host::gateway::testkit::{KEY, TempDir};
use cupper_host::store::meta::{self, MetaKey};
use cupper_host::store::{Database, MIGRATIONS, migrate, open_reader, open_writer};
use cupper_protocol::ids::ProjectId;
use cupper_protocol::methods::Empty;
use cupper_protocol::methods::handshake::{HostExit, HostExitParams, HostStatus};
use cupper_protocol::methods::session::SessionCreate;
use cupper_protocol::proof;
use cupper_protocol::transport::socket_path;
use rusqlite::Connection;
use serde_json::json;

const HOST: &str = env!("CARGO_BIN_EXE_cupper-host");
/// How long the host may take to come up or to exit.
const PATIENCE: Duration = Duration::from_secs(30);

/// Starts the host on `data_dir` with [`KEY`] as `harness_key`, no login shell.
fn spawn(data_dir: &Path) -> Child {
    spawn_with(data_dir, json!({"login_env": {}}))
}

fn spawn_with(data_dir: &Path, test_config: serde_json::Value) -> Child {
    let mut child = Command::new(HOST)
        .arg("--data-dir")
        .arg(data_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start the host");
    let payload = json!({"harness_key": proof::to_hex(&KEY), "test_config": test_config});
    writeln!(child.stdin.take().unwrap(), "{payload}").unwrap();
    child
}

async fn wait(clock: &SystemClock) {
    clock
        .timer_at(clock.now_mono() + Duration::from_millis(20))
        .await;
}

/// The `harness` client, once the host listens.
async fn connect(data_dir: &Path) -> client::Client {
    let socket = socket_path(data_dir, &cupper_sys::user_temp_dir().unwrap());
    let clock = SystemClock::new();
    let give_up = clock.now_mono() + PATIENCE;
    loop {
        let options = Options {
            auth: ClientAuth::Harness(HarnessKey::new(KEY)),
            ..Options::default()
        };
        match client::Client::connect(&socket, &HostCheck::Harness, options).await {
            Ok(client) => return client,
            Err(err) => assert!(clock.now_mono() < give_up, "cannot connect: {err}"),
        }
        wait(&clock).await;
    }
}

/// The host's exit code, once it exits.
async fn exit_code(child: &mut Child) -> i32 {
    let clock = SystemClock::new();
    let give_up = clock.now_mono() + PATIENCE;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status.code().expect("an exit, not a signal");
        }
        assert!(clock.now_mono() < give_up, "the host did not exit");
        wait(&clock).await;
    }
}

fn terminate(child: &Child) {
    let status = Command::new("/bin/kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(status.success());
}

/// A data directory holding `host.db` as this host creates it.
fn migrated(dir: &TempDir) -> (PathBuf, Connection) {
    let data_dir = dir.path().join("data");
    std::fs::create_dir(&data_dir).unwrap();
    let mut conn = open_writer(&data_dir.join("host.db"), Database::Host).unwrap();
    migrate(
        &mut conn,
        MIGRATIONS,
        &data_dir.join("backups"),
        &SystemClock::new(),
    )
    .unwrap();
    (data_dir, conn)
}

fn column(data_dir: &Path, sql: &str) -> Vec<String> {
    let conn = open_reader(&data_dir.join("host.db")).unwrap();
    let mut query = conn.prepare(sql).unwrap();
    let rows = query.query_map([], |row| row.get(0)).unwrap();
    rows.collect::<Result<_, _>>().unwrap()
}

fn meta_ms(data_dir: &Path, key: MetaKey) -> u64 {
    let conn = open_reader(&data_dir.join("host.db")).unwrap();
    meta::get(&conn, key).unwrap().unwrap().parse().unwrap()
}

#[tokio::test]
async fn it_lifecycle_startup_ends_what_the_last_run_left_running() {
    let dir = TempDir::new("left-running");
    let (data_dir, conn) = migrated(&dir);
    // A turn and an interaction were running when the last host died.
    conn.execute_batch(
        "INSERT INTO projects (project_id, path, name, position, added_at)
           VALUES ('p_1', '/p', 'p', 0, 1);
         INSERT INTO sessions (session_id, project_id, agent, workspace_kind, workspace_path,
             state, created_at, updated_at)
           VALUES ('s_1', 'p_1', 'claude', 'local', '/p', 'running', 1, 1),
                  ('s_2', 'p_1', 'claude', 'local', '/p', 'idle', 1, 1);
         INSERT INTO turns (session_id, turn_id, turn_no, origin, status, first_seq, started_at)
           VALUES ('s_1', 't_1', 1, 'user', 'completed', 1, 1),
                  ('s_1', 't_2', 2, 'user', 'running', 5, 2);
         INSERT INTO interactions (request_id, session_id, kind, digest, status,
             adapter_instance, request_json, created_at)
           VALUES ('r_1', 's_1', 'permission', 'd', 'resolved', 'a_1', '{}', 2),
                  ('r_2', 's_1', 'permission', 'd', 'pending', 'a_1', '{}', 3);",
    )
    .unwrap();
    drop(conn);

    let mut host = spawn(&data_dir);
    let client = connect(&data_dir).await;
    assert_eq!(client.initialized().principal.id.as_str(), "pr_test");
    let rows = |sql| column(&data_dir, sql);
    assert_eq!(
        rows(
            "SELECT turn_id || ' ' || status || ' ' || (ended_at IS NOT NULL) FROM turns ORDER BY turn_no"
        ),
        ["t_1 completed 0", "t_2 interrupted 1"]
    );
    assert_eq!(
        rows(
            "SELECT request_id || ' ' || status || ' ' || coalesce(reason, '-') FROM interactions ORDER BY request_id"
        ),
        ["r_1 resolved -", "r_2 expired host_restarted"]
    );
    assert_eq!(
        rows("SELECT session_id || ' ' || state FROM sessions ORDER BY session_id"),
        ["s_1 interrupted", "s_2 idle"]
    );
    terminate(&host);
    assert_eq!(exit_code(&mut host).await, 0);
}

/// How the test asks the host to exit.
#[derive(Debug, Clone, Copy)]
enum Trigger {
    HostExit,
    Sigterm,
}

#[tokio::test]
async fn it_lifecycle_host_exit_and_sigterm_exit_in_order() {
    for trigger in [Trigger::HostExit, Trigger::Sigterm] {
        let dir = TempDir::new("orderly");
        let data_dir = dir.path().join("data");
        let mut host = spawn(&data_dir);
        let mut client = connect(&data_dir).await;

        let status = client.request::<HostStatus>(Empty {}).await.unwrap();
        assert_eq!(status.version, env!("CARGO_PKG_VERSION"));
        assert!(!status.pending_upgrade);
        assert_eq!((status.running_turns, status.pending_interactions), (0, 0));
        assert_eq!(status.pid, Some(host.id()));
        let kinds: Vec<_> = status
            .clients
            .iter()
            .map(|c| c.principal_kind.as_str())
            .collect();
        assert_eq!(kinds, ["test_harness"]);

        match trigger {
            Trigger::HostExit => {
                let params = HostExitParams {
                    command_id: client::command_id(),
                    force: false,
                };
                client.request::<HostExit>(params).await.unwrap();
            }
            Trigger::Sigterm => terminate(&host),
        }
        let Some(Incoming::Other { method, params }) = client.next().await else {
            panic!("{trigger:?}: no host/shutdown");
        };
        assert_eq!(method, "host/shutdown", "{trigger:?}");
        assert_eq!(
            params,
            json!({"reason": "user_quit", "restart_expected": false})
        );
        assert_eq!(
            client.next().await,
            None,
            "{trigger:?}: the connection ends"
        );
        assert_eq!(exit_code(&mut host).await, 0, "{trigger:?}");

        let (start, clean) = (MetaKey::LastStartAt, MetaKey::LastCleanExitAt);
        assert!(
            meta_ms(&data_dir, clean) >= meta_ms(&data_dir, start),
            "{trigger:?}"
        );
        assert!(!data_dir.join("run/host.sock").exists(), "{trigger:?}");
        HostLock::acquire(&data_dir).expect("the lock is free again");
    }
}

#[tokio::test]
async fn it_lifecycle_runs_a_turn_with_the_fake_agent() {
    let dir = TempDir::new("turn");
    let (data_dir, conn) = migrated(&dir);
    let project = ProjectId::from_uuid(1).to_string();
    let sql = "INSERT INTO projects (project_id, path, name, position, added_at, trust_state)
               VALUES (?1, ?2, 'p', 0, 1, 'trusted')";
    let path = dir.path().to_str().unwrap();
    conn.execute(sql, [&project, path]).unwrap();
    let program = launch::fake_agent();
    let test_config = json!({"agents": {"claude": {"program": program}}, "login_env": {}});
    let mut host = spawn_with(&data_dir, test_config);
    let client = connect(&data_dir).await;
    let params = json!({"command_id": client::command_id(), "agent": "claude",
        "project_id": project, "workspace": {"kind": "local"},
        "message": [{"type": "text", "text": "say hello"}]});
    let params = serde_json::from_value(params).unwrap();
    client.request::<SessionCreate>(params).await.unwrap();
    let (clock, rows) = (SystemClock::new(), |sql| column(&data_dir, sql));
    let give_up = clock.now_mono() + PATIENCE;
    while rows("SELECT status FROM turns") != ["completed"] {
        assert!(clock.now_mono() < give_up, "the turn did not complete");
        wait(&clock).await;
    }
    terminate(&host);
    assert_eq!(exit_code(&mut host).await, 0);
    // The adapter stopped with the host.
    let ended = rows("SELECT instance_id FROM adapters WHERE ended_at IS NOT NULL");
    assert_eq!(ended.len(), 1);
}

#[tokio::test]
async fn it_lifecycle_startup_failures_exit_3_to_6() {
    // 3: another host holds the lock.
    let dir = TempDir::new("running");
    let data_dir = dir.path().join("data");
    let mut first = spawn(&data_dir);
    let _client = connect(&data_dir).await;
    assert_eq!(exit_code(&mut spawn(&data_dir)).await, 3);
    terminate(&first);
    assert_eq!(exit_code(&mut first).await, 0);

    // 4: the database comes from a newer host.
    let dir = TempDir::new("newer");
    let (data_dir, conn) = migrated(&dir);
    meta::set(&conn, MetaKey::SchemaVersion, "999").unwrap();
    drop(conn);
    assert_eq!(exit_code(&mut spawn(&data_dir)).await, 4);

    // 5: the data directory is a file, or host.db is unreadable.
    let dir = TempDir::new("file");
    let file = dir.path().join("data");
    std::fs::write(&file, b"").unwrap();
    assert_eq!(exit_code(&mut spawn(&file)).await, 5);
    let dir = TempDir::new("garbled");
    let (data_dir, conn) = migrated(&dir);
    meta::set(&conn, MetaKey::SchemaVersion, "one").unwrap();
    drop(conn);
    assert_eq!(exit_code(&mut spawn(&data_dir)).await, 5);

    // 6: a migration fails; here the first script meets a table it creates.
    let dir = TempDir::new("failing");
    let data_dir = dir.path().join("data");
    std::fs::create_dir(&data_dir).unwrap();
    let conn = Connection::open(data_dir.join("host.db")).unwrap();
    conn.execute_batch(
        "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         INSERT INTO meta VALUES ('schema_version', '0');
         CREATE TABLE projects (x);",
    )
    .unwrap();
    drop(conn);
    assert_eq!(exit_code(&mut spawn(&data_dir)).await, 6);
}

/// A workspace two levels below its git root, with configuration at both:
/// added, scanned up to the root, trusted, changed into `stale`, and, with
/// 1,500 agents, a result that still fits one frame.
#[tokio::test]
async fn it_project_add_trust_through_the_gateway() {
    use cupper_protocol::annotations::WirePath;
    use cupper_protocol::index::TrustState;
    use cupper_protocol::methods::project::{
        ProjectAdd, ProjectAddParams, ProjectTrust, ProjectTrustInfo, ProjectTrustInfoParams,
        ProjectTrustParams,
    };

    let dir = TempDir::new("projects");
    let (repo, app) = (dir.path().join("repo"), dir.path().join("repo/apps/web"));
    std::fs::create_dir_all(app.join(".claude/agents")).unwrap();
    let git = Command::new("/usr/bin/git")
        .args(["init", "-q"])
        .arg(&repo)
        .status();
    assert!(git.unwrap().success());
    let hooks =
        r#"{"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "acme-done"}]}]}}"#;
    std::fs::create_dir_all(repo.join(".claude")).unwrap();
    std::fs::write(repo.join(".claude/settings.json"), hooks).unwrap();
    std::fs::write(
        app.join(".mcp.json"),
        r#"{"mcpServers": {"acme": {"url": "https://acme.invalid"}}}"#,
    )
    .unwrap();
    let data_dir = dir.path().join("data");
    let mut host = spawn_with(&data_dir, json!({"login_env": {"PATH": "/usr/bin:/bin"}}));
    let client = connect(&data_dir).await;

    let path = WirePath(app.to_str().unwrap().into());
    let add = ProjectAddParams {
        command_id: client::command_id(),
        path,
    };
    let added = client.request::<ProjectAdd>(add).await.unwrap();
    assert_eq!(added.trust.state, TrustState::Untrusted);
    let found: Vec<_> = added
        .trust
        .findings
        .iter()
        .map(|f| (f.path.0.as_str(), f.detail.as_str()))
        .collect();
    assert_eq!(
        found,
        [
            ("../../.claude/settings.json", "Stop: acme-done"),
            (".mcp.json", "acme: https://acme.invalid")
        ]
    );
    let project_id = added.project_id;
    let trust = ProjectTrustParams {
        command_id: client::command_id(),
        project_id: project_id.clone(),
        trust_digest: added.trust.trust_digest,
    };
    let trusted = client.request::<ProjectTrust>(trust).await.unwrap();
    assert_eq!(trusted.trust_state, TrustState::Trusted);

    for n in 0..1_500 {
        std::fs::write(
            app.join(format!(".claude/agents/acme-agent-{n:04}.md")),
            "---",
        )
        .unwrap();
    }
    let info = ProjectTrustInfoParams { project_id };
    let info = client.request::<ProjectTrustInfo>(info).await.unwrap();
    assert_eq!(info.state, TrustState::Stale);
    let diff = info.diff_from_trusted.unwrap();
    assert!(
        diff.iter()
            .all(|change| change.path.0.starts_with(".claude/agents/"))
    );
    assert_eq!(diff.len() + info.diff_omitted.unwrap() as usize, 1_500);
    assert_eq!(
        info.findings.len() + info.findings_omitted.unwrap() as usize,
        1_502
    );
    let states = column(
        &data_dir,
        "SELECT trust_state || ' ' || (git_common_dir IS NOT NULL) FROM projects",
    );
    assert_eq!(states, ["stale 1"]);
    terminate(&host);
    assert_eq!(exit_code(&mut host).await, 0);
}

/// Adds the directory `dir`, empty and outside git, as a project.
async fn add_project(client: &client::Client, dir: &Path) {
    use cupper_protocol::annotations::WirePath;
    use cupper_protocol::methods::project::{ProjectAdd, ProjectAddParams};
    std::fs::create_dir_all(dir).unwrap();
    let path = WirePath(dir.to_str().unwrap().into());
    let params = ProjectAddParams {
        command_id: client::command_id(),
        path,
    };
    client.request::<ProjectAdd>(params).await.unwrap();
}

async fn advance(client: &client::Client, ms: u64) {
    use cupper_protocol::annotations::DurationMs;
    use cupper_protocol::methods::admin::{TestClockAdvance, TestClockAdvanceParams};
    let params = TestClockAdvanceParams { ms: DurationMs(ms) };
    client.request::<TestClockAdvance>(params).await.unwrap();
}

/// On a manual clock, `test/clock_advance` returns once what the timers set
/// off is done: a handshake deadline closed its connection, and the ledger's
/// hourly sweeps removed what passed its 48 hours, through the writer.
#[tokio::test]
async fn it_clock_advance_returns_once_the_host_settled() {
    let dir = TempDir::new("clock");
    let data_dir = dir.path().join("data");
    let config = json!({"clock": "manual", "login_env": {"PATH": "/usr/bin:/bin"}});
    let mut host = spawn_with(&data_dir, config);
    let client = connect(&data_dir).await;
    add_project(&client, &dir.path().join("work")).await;
    let commands = || column(&data_dir, "SELECT CAST(count(*) AS TEXT) FROM commands");
    assert_eq!(commands(), ["1"]);

    let socket = socket_path(&data_dir, &cupper_sys::user_temp_dir().unwrap());
    let mut silent = cupper_host::gateway::testkit::Client::connect(&socket).await;
    silent.hello().await;
    advance(&client, 9_999).await;
    advance(&client, 1).await;
    // Closed by the handshake deadline, before the advance returned.
    assert_eq!(silent.next().await, None);

    advance(&client, 49 * 3_600_000).await;
    assert_eq!(commands(), ["0"]);
    terminate(&host);
    assert_eq!(exit_code(&mut host).await, 0);
}

/// `test/seed` writes sessions through the stream writers, turn snapshots
/// and checkpoints included, and the index lists them.
#[tokio::test]
async fn it_seed_writes_sessions_the_normal_way() {
    use cupper_protocol::methods::admin::{SeedMix, SeedProfile, TestSeed, TestSeedParams};

    let dir = TempDir::new("seed");
    let data_dir = dir.path().join("data");
    let mut host = spawn(&data_dir);
    let client = connect(&data_dir).await;
    add_project(&client, &dir.path().join("work")).await;
    let profile = SeedProfile {
        sessions: 3,
        turns: 2,
        items_per_turn: 20,
        mix: SeedMix::Sp1,
        large_turn_items: Some(2_000),
    };
    client
        .request::<TestSeed>(TestSeedParams { profile })
        .await
        .unwrap();

    let count = |sql| column(&data_dir, sql);
    assert_eq!(count("SELECT CAST(count(*) AS TEXT) FROM sessions"), ["3"]);
    assert_eq!(count("SELECT CAST(count(*) AS TEXT) FROM turns"), ["7"]);
    assert_eq!(
        count("SELECT CAST(count(*) AS TEXT) FROM turn_snapshots"),
        ["7"]
    );
    let checkpoints =
        "SELECT CAST(count(*) AS TEXT) FROM checkpoints WHERE stream LIKE 'session/%'";
    assert_eq!(count(checkpoints), ["3"]);
    // The large turn: its 2,000 items start, and the messages have text.
    let large = "SELECT CAST(max(json_array_length(ops)) AS TEXT) FROM turn_snapshots";
    let ops: u32 = count(large)[0].parse().unwrap();
    assert!(ops > 4_000, "{ops}");
    let listed = "SELECT CAST(count(*) AS TEXT) FROM events WHERE stream = 'index' AND type = 'session.upserted'";
    assert_eq!(count(listed), ["3"]);
    terminate(&host);
    assert_eq!(exit_code(&mut host).await, 0);
}
