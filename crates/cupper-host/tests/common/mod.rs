//! What the process-level tests share: a host with the harness key, the
//! harness client, the fake agent, and waiting on the store. Each test crate
//! uses part of it.
#![allow(dead_code, reason = "each test crate that includes it uses part of it")]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use cupper_cli::client::{self, Client, ClientAuth, ClientError, Options};
use cupper_cli::harness::HarnessKey;
use cupper_cli::verify::HostCheck;
use cupper_host::clock::{Clock, SystemClock};
use cupper_host::driver::runtime::launch;
use cupper_host::gateway::testkit::{KEY, TempDir};
use cupper_host::store::{Database, MIGRATIONS, migrate, open_reader, open_writer};
use cupper_protocol::ids::{ProjectId, SessionId};
use cupper_protocol::methods::interaction::InteractionRespond;
use cupper_protocol::methods::session::{SessionCreate, SessionSend};
use cupper_protocol::proof;
use cupper_protocol::transport::socket_path;
use rusqlite::Connection;
use serde_json::{Value, json};

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
    start(&mut Command::new(program), data_dir, &payload(test_config))
}

/// The startup payload: [`KEY`] as `harness_key`, and `test_config`.
pub fn payload(test_config: serde_json::Value) -> Value {
    json!({"harness_key": proof::to_hex(&KEY), "test_config": test_config})
}

/// Runs `command` with `--data-dir <data_dir>`, `payload` on its stdin.
pub fn start(command: &mut Command, data_dir: &Path, payload: &Value) -> Child {
    let mut child = command
        .arg("--data-dir")
        .arg(data_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start the host");
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
    let options = Options {
        auth: ClientAuth::Harness(HarnessKey::new(KEY)),
        ..Options::default()
    };
    connect_with(data_dir, options).await
}

pub async fn connect_with(data_dir: &Path, options: Options) -> client::Client {
    let clock = SystemClock::new();
    let give_up = clock.now_mono() + PATIENCE;
    loop {
        match connect_once(data_dir, options.clone()).await {
            Ok(client) => return client,
            Err(err) => assert!(clock.now_mono() < give_up, "cannot connect: {err}"),
        }
        wait(&clock).await;
    }
}

/// One attempt at the client of `options`.
pub async fn connect_once(
    data_dir: &Path,
    options: Options,
) -> Result<client::Client, ClientError> {
    let socket = socket_path(data_dir, &cupper_sys::user_temp_dir().unwrap());
    client::Client::connect(&socket, &HostCheck::Harness, options).await
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
    let data_dir = fake_agent_data(dir);
    let host = spawn_from(host, &data_dir, fake_agent_config(config));
    let client = connect(&data_dir).await;
    (data_dir, host, client)
}

/// A data directory in `dir` with `dir` as a trusted project.
pub fn fake_agent_data(dir: &TempDir) -> PathBuf {
    let (data_dir, conn) = migrated(dir);
    let sql = "INSERT INTO projects (project_id, path, name, position, added_at, trust_state)
               VALUES (?1, ?2, 'p', 0, 1, 'trusted')";
    let path = dir.path().to_str().unwrap();
    conn.execute(sql, [&ProjectId::from_uuid(1).to_string(), path])
        .unwrap();
    data_dir
}

/// `test_config` with the fake agent and no login shell, more in `config`.
pub fn fake_agent_config(config: serde_json::Value) -> Value {
    let program = launch::fake_agent();
    let mut test_config = json!({"agents": {"claude": {"program": program}}, "login_env": {}});
    test_config
        .as_object_mut()
        .unwrap()
        .extend(config.as_object().unwrap().clone());
    test_config
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

/// `value` as `T`.
pub fn typed<T: serde::de::DeserializeOwned>(value: Value) -> T {
    serde_json::from_value(value).unwrap()
}

/// The `data` of the session's events of type `kind`, once there are `n`.
pub async fn events(data_dir: &Path, session: &SessionId, kind: &str, n: usize) -> Vec<Value> {
    let sql = format!(
        "SELECT json_extract(json, '$.data') FROM events
         WHERE stream = 'session/{session}' AND type = '{kind}' ORDER BY seq"
    );
    let clock = SystemClock::new();
    let give_up = clock.now_mono() + PATIENCE;
    loop {
        let rows = query(&data_dir.join("host.db"), &sql);
        if rows.len() >= n {
            return rows
                .iter()
                .map(|row| serde_json::from_str(row).unwrap())
                .collect();
        }
        assert!(clock.now_mono() < give_up, "{kind}: {rows:?}");
        wait(&clock).await;
    }
}

/// Answers `asked` with `outcome`.
pub async fn respond(
    client: &Client,
    session: &SessionId,
    asked: &Value,
    outcome: Value,
) -> Result<(), ClientError> {
    let params = json!({"command_id": client::command_id(), "session_id": session,
        "request_id": asked["request_id"], "request_digest": asked["digest"],
        "outcome": outcome});
    client
        .request::<InteractionRespond>(typed(params))
        .await
        .map(drop)
}

/// Waits until the audit holds `want` of `events`, with their principals.
pub async fn audited(data_dir: &Path, events: &str, want: &[&str]) {
    let sql = format!(
        "SELECT event || ' ' || coalesce(principal, '-') FROM audit
         WHERE event IN ({events}) ORDER BY id"
    );
    until(data_dir, "host.db", &sql, want).await;
}

/// The error codes of a request the host refused: `data.code` and its reason.
pub fn refused<T: std::fmt::Debug>(answered: Result<T, ClientError>) -> (String, String) {
    let Err(ClientError::Rpc(err)) = answered else {
        panic!("refused: {answered:?}")
    };
    let code = err.app_code().map(ToString::to_string).unwrap_or_default();
    (
        code,
        err.reason().map(ToString::to_string).unwrap_or_default(),
    )
}

/// `M` with its params and result as JSON, e.g. to see `replayed`.
pub struct Raw<M>(std::marker::PhantomData<M>);

impl<M: cupper_protocol::rpc::Method> cupper_protocol::rpc::Method for Raw<M> {
    const NAME: &'static str = M::NAME;
    const SCOPE: Option<cupper_protocol::rpc::Scope> = M::SCOPE;
    const IDEMPOTENCY: cupper_protocol::rpc::Idempotency = M::IDEMPOTENCY;
    const MILESTONE: cupper_protocol::rpc::Milestone = M::MILESTONE;
    const ERRORS: &'static [cupper_protocol::rpc::MethodError] = M::ERRORS;
    type Params = Value;
    type Result = Value;
}
