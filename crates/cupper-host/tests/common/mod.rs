//! What the process-level tests share: a host with the harness key, the
//! harness client, the fake agent, and waiting on the store. Each test crate
//! uses part of it.
#![allow(dead_code, reason = "each test crate that includes it uses part of it")]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use cupper_cli::client::{self, Client, ClientAuth, Options};
use cupper_cli::harness::HarnessKey;
use cupper_cli::verify::HostCheck;
use cupper_host::clock::{Clock, SystemClock};
use cupper_host::driver::runtime::launch;
use cupper_host::gateway::testkit::{KEY, TempDir};
use cupper_host::store::{Database, MIGRATIONS, migrate, open_reader, open_writer};
use cupper_protocol::ids::{ProjectId, SessionId};
use cupper_protocol::methods::session::{SessionCreate, SessionSend};
use cupper_protocol::proof;
use cupper_protocol::transport::socket_path;
use rusqlite::Connection;
use serde_json::json;

pub const HOST: &str = env!("CARGO_BIN_EXE_cupper-host");
/// How long the host may take to come up or to exit.
pub const PATIENCE: Duration = Duration::from_secs(30);

/// Starts the host on `data_dir` with [`KEY`] as `harness_key`, no login shell.
pub fn spawn(data_dir: &Path) -> Child {
    spawn_with(data_dir, json!({"login_env": {}}))
}

pub fn spawn_with(data_dir: &Path, test_config: serde_json::Value) -> Child {
    spawn_from(Path::new(HOST), data_dir, test_config)
}

pub fn spawn_from(program: &Path, data_dir: &Path, test_config: serde_json::Value) -> Child {
    let mut child = Command::new(program)
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

pub async fn wait(clock: &SystemClock) {
    clock
        .timer_at(clock.now_mono() + Duration::from_millis(20))
        .await;
}

/// The `harness` client, once the host listens.
pub async fn connect(data_dir: &Path) -> client::Client {
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
pub async fn exit_code(child: &mut Child) -> i32 {
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

pub fn terminate(child: &Child) {
    let status = Command::new("/bin/kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(status.success());
}

/// A data directory holding `host.db` as this host creates it.
pub fn migrated(dir: &TempDir) -> (PathBuf, Connection) {
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

pub fn column(data_dir: &Path, sql: &str) -> Vec<String> {
    query(&data_dir.join("host.db"), sql)
}

pub fn query(db: &Path, sql: &str) -> Vec<String> {
    let conn = open_reader(db).unwrap();
    let mut query = conn.prepare(sql).unwrap();
    let rows = query.query_map([], |row| row.get(0)).unwrap();
    rows.collect::<Result<_, _>>().unwrap()
}

/// A host with the fake agent on a trusted project in `dir`, more of
/// `test_config` in `config`; its data directory and the harness client.
pub async fn fake_agent_host(dir: &TempDir, config: serde_json::Value) -> (PathBuf, Child, Client) {
    fake_agent_host_from(Path::new(HOST), dir, config).await
}

pub async fn fake_agent_host_from(
    host: &Path,
    dir: &TempDir,
    config: serde_json::Value,
) -> (PathBuf, Child, Client) {
    let (data_dir, conn) = migrated(dir);
    let sql = "INSERT INTO projects (project_id, path, name, position, added_at, trust_state)
               VALUES (?1, ?2, 'p', 0, 1, 'trusted')";
    let path = dir.path().to_str().unwrap();
    conn.execute(sql, [&ProjectId::from_uuid(1).to_string(), path])
        .unwrap();
    let program = launch::fake_agent();
    let mut test_config = json!({"agents": {"claude": {"program": program}}, "login_env": {}});
    test_config
        .as_object_mut()
        .unwrap()
        .extend(config.as_object().unwrap().clone());
    let host = spawn_from(host, &data_dir, test_config);
    let client = connect(&data_dir).await;
    (data_dir, host, client)
}

/// `session/create` with the message `text`; the session.
pub async fn create(client: &Client, text: &str) -> SessionId {
    let params = json!({"command_id": client::command_id(), "agent": "claude",
        "project_id": ProjectId::from_uuid(1), "workspace": {"kind": "local"},
        "message": [{"type": "text", "text": text}]});
    let params = serde_json::from_value(params).unwrap();
    client
        .request::<SessionCreate>(params)
        .await
        .unwrap()
        .session_id
}

/// `session/send` of the message `text`.
pub async fn send(client: &Client, session: &SessionId, text: &str) {
    let params = json!({"command_id": client::command_id(), "session_id": session,
        "mode": "queue", "content": [{"type": "text", "text": text}]});
    let params = serde_json::from_value(params).unwrap();
    client.request::<SessionSend>(params).await.unwrap();
}

/// Waits until `sql` reads `expected` in `db` of `data_dir`.
pub async fn until(data_dir: &Path, db: &str, sql: &str, expected: &[&str]) {
    let clock = SystemClock::new();
    let give_up = clock.now_mono() + PATIENCE;
    while query(&data_dir.join(db), sql) != expected {
        assert!(clock.now_mono() < give_up, "{sql}: not {expected:?}");
        wait(&clock).await;
    }
}

/// A host a test started: it exits when the test ends, passed or failed.
pub struct Running(pub Child);

impl Drop for Running {
    /// SIGTERM, not SIGKILL: the orderly exit stops the host's adapters and
    /// their process trees too (T008 fix 1), which a killed host leaves.
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let pid = self.0.id().to_string();
            let _ = Command::new("/bin/kill").args(["-TERM", &pid]).status();
        }
        let _ = self.0.wait();
    }
}

pub async fn advance(client: &client::Client, ms: u64) {
    use cupper_protocol::annotations::DurationMs;
    use cupper_protocol::methods::admin::{TestClockAdvance, TestClockAdvanceParams};
    let params = TestClockAdvanceParams { ms: DurationMs(ms) };
    client.request::<TestClockAdvance>(params).await.unwrap();
}
