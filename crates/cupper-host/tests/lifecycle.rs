//! IT-lifecycle: the host as a process (design 5.2.13). What the previous run
//! left running ends at startup, `host/exit` and SIGTERM exit in order, and
//! a host that cannot start exits with the code of the startup contract, a
//! session runs a turn with the fake agent, and read marks and presence reach
//! the index. IT-project-add-trust adds and
//! trusts a project in a git repository built here; IT-clock-advance and
//! IT-seed drive the test interface; IT-self-check and IT-idle-exit the
//! supervisor's watch; IT-runtime-startup the runtime installer. The tests talk to the host as the CLI's
//! `harness` client.

use std::io::Write;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use cupper_cli::client::{self, Client, ClientAuth, ClientError, Incoming, Options};
use cupper_cli::harness::HarnessKey;
use cupper_cli::verify::HostCheck;
use cupper_host::clock::{Clock, SystemClock};
use cupper_host::diag::log::log_dir;
use cupper_host::driver::runtime::launch;
use cupper_host::gateway::listen::HostLock;
use cupper_host::gateway::testkit::{KEY, TempDir};
use cupper_host::store::meta::{self, MetaKey};
use cupper_host::store::{Database, MIGRATIONS, migrate, open_reader, open_writer};
use cupper_protocol::error::ErrorReason;
use cupper_protocol::ids::{ProjectId, SessionId};
use cupper_protocol::methods::Empty;
use cupper_protocol::methods::admin::{
    AuthElevationGet, AuthElevationRespond, DiagRecordingExport, TestClockAdvance,
};
use cupper_protocol::methods::handshake::{
    ClientPresence, ClientPresenceParams, HostExit, HostExitParams, HostStatus,
};
use cupper_protocol::methods::session::{
    SessionCreate, SessionInterrupt, SessionMarkRead, SessionMarkReadParams, SessionSend,
};
use cupper_protocol::methods::stream::StreamSubscribe;
use cupper_protocol::proof;
use cupper_protocol::recording::{Body, INSTANCE_EXITED, INSTANCE_STARTED, Recording};
use cupper_protocol::transport::socket_path;
use rusqlite::Connection;
use serde_json::{Value, json};

const HOST: &str = env!("CARGO_BIN_EXE_cupper-host");
/// How long the host may take to come up or to exit.
const PATIENCE: Duration = Duration::from_secs(30);

/// Starts the host on `data_dir` with [`KEY`] as `harness_key`, no login shell.
fn spawn(data_dir: &Path) -> Child {
    spawn_with(data_dir, json!({"login_env": {}}))
}

fn spawn_with(data_dir: &Path, test_config: serde_json::Value) -> Child {
    spawn_from(Path::new(HOST), data_dir, test_config)
}

fn spawn_from(program: &Path, data_dir: &Path, test_config: serde_json::Value) -> Child {
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
    query(&data_dir.join("host.db"), sql)
}

fn query(db: &Path, sql: &str) -> Vec<String> {
    let conn = open_reader(db).unwrap();
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
    // And an adapter, in a session of its own, with a descendant's group
    // that ignores SIGTERM: this test's children, nobody else's.
    let start = |script: &str| {
        let mut command = cupper_sys::Command::new("/bin/sh");
        command.args(["-c", script]).new_session();
        cupper_sys::spawn(&command).unwrap()
    };
    let mut adapter = start("exec /bin/sleep 60");
    let mut tool = start("trap '' TERM; /bin/sleep 60; :");
    let groups = json!([[tool.pid(), tool.start().as_micros()]]).to_string();
    conn.execute(
        "INSERT INTO adapters (instance_id, agent, pid, pgid, start_us, adapter_version,
             node_version, started_at, groups)
           VALUES ('a_1', 'claude', ?1, ?1, ?2, '1', '1', 1, ?3)",
        (adapter.pid(), adapter.start().as_micros(), groups),
    )
    .unwrap();
    drop(conn);

    let config = json!({"login_env": {}, "durations": {"reap_term_wait_ms": 200}});
    let mut host = spawn_with(&data_dir, config);
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
    let signals = [adapter.wait().unwrap(), tool.wait().unwrap()].map(|s| s.signal());
    assert_eq!(signals, [Some(15), Some(9)], "SIGTERM, then SIGKILL");
    assert_eq!(rows("SELECT exit_status FROM adapters"), ["reaped"]);
    terminate(&host);
    assert_eq!(exit_code(&mut host).await, 0);
}

/// `session/mark_read` moves `last_read_seq` forward only, and no further
/// than the last turn's end; `client/presence` is served.
#[tokio::test]
async fn it_lifecycle_read_marks_and_presence_reach_the_index() {
    let dir = TempDir::new("read-marks");
    let (data_dir, conn) = migrated(&dir);
    conn.execute_batch(
        "INSERT INTO projects (project_id, path, name, position, added_at)
           VALUES ('p_1', '/p', 'p', 0, 1);
         INSERT INTO sessions (session_id, project_id, agent, workspace_kind, workspace_path,
             state, last_turn_end_seq, last_read_seq, created_at, updated_at)
           VALUES ('s_1', 'p_1', 'claude', 'local', '/p', 'idle', 9, 2, 1, 1);",
    )
    .unwrap();
    drop(conn);

    let mut host = spawn(&data_dir);
    let client = connect(&data_dir).await;
    let session_id: SessionId = "s_1".parse().unwrap();
    let presence = ClientPresenceParams {
        foreground: true,
        session_id: Some(session_id.clone()),
    };
    client.notify::<ClientPresence>(presence).await.unwrap();
    let mut marks = Vec::new();
    for up_to_seq in [5, 3, 100] {
        let params = SessionMarkReadParams {
            session_id: session_id.clone(),
            up_to_seq,
        };
        client.request::<SessionMarkRead>(params).await.unwrap();
        marks.extend(column(
            &data_dir,
            "SELECT CAST(last_read_seq AS TEXT) FROM sessions",
        ));
    }
    assert_eq!(marks, ["5", "5", "9"]);
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

/// A host with the fake agent on a trusted project in `dir`, more of
/// `test_config` in `config`; its data directory and the harness client.
async fn fake_agent_host(dir: &TempDir, config: serde_json::Value) -> (PathBuf, Child, Client) {
    fake_agent_host_from(Path::new(HOST), dir, config).await
}

async fn fake_agent_host_from(
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
async fn create(client: &Client, text: &str) -> SessionId {
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
async fn send(client: &Client, session: &SessionId, text: &str) {
    let params = json!({"command_id": client::command_id(), "session_id": session,
        "mode": "queue", "content": [{"type": "text", "text": text}]});
    let params = serde_json::from_value(params).unwrap();
    client.request::<SessionSend>(params).await.unwrap();
}

/// Waits until `sql` reads `expected` in `db` of `data_dir`.
async fn until(data_dir: &Path, db: &str, sql: &str, expected: &[&str]) {
    let clock = SystemClock::new();
    let give_up = clock.now_mono() + PATIENCE;
    while query(&data_dir.join(db), sql) != expected {
        assert!(clock.now_mono() < give_up, "{sql}: not {expected:?}");
        wait(&clock).await;
    }
}

#[tokio::test]
async fn it_lifecycle_runs_a_turn_with_the_fake_agent() {
    let dir = TempDir::new("turn");
    let (data_dir, mut host, client) = fake_agent_host(&dir, json!({})).await;
    create(&client, "say hello").await;
    until(
        &data_dir,
        "host.db",
        "SELECT status FROM turns",
        &["completed"],
    )
    .await;
    terminate(&host);
    assert_eq!(exit_code(&mut host).await, 0);
    // The adapter stopped with the host.
    let sql = "SELECT instance_id FROM adapters WHERE ended_at IS NOT NULL";
    assert_eq!(column(&data_dir, sql).len(), 1);
    // The index heard of the session, its turn and the agent.
    let index = |data: &str, kind: &str| {
        let sql = format!(
            "SELECT json_extract(json, '$.data.{data}') FROM events
                           WHERE stream = 'index' AND type = '{kind}' ORDER BY seq"
        );
        column(&data_dir, &sql)
    };
    let statuses = index("status", "session.upserted");
    assert_eq!(statuses.last().map(String::as_str), Some("unread"));
    assert_eq!(index("reason", "notification.intent"), ["turn_completed"]);
    assert!(!index("agents", "agents.updated").is_empty());
}

/// UNIT-replay end to end: SP-1's Claude turn `s02b-plan`, played by the
/// fake agent, runs to its end on a real host. The SP-1 client is not the
/// host (design 5.9.2), so its own requests give way to the host's:
/// `initialize` takes the host's parameters, and the two
/// `set_config_option` of SP-1's safety policy go with their responses.
/// Every other frame plays as recorded and must match.
#[tokio::test]
async fn unit_replay_a_host_runs_an_sp1_claude_turn() {
    let dir = TempDir::new("replay");
    let sp1 = "../../testdata/recordings/claude-agent-acp/0.91.0/2.1.296/s02b-plan.jsonl";
    let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(sp1));
    let mut recording = Recording::parse(&text.unwrap()).unwrap();
    let config = |m: &Value| m["method"] == "session/set_config_option";
    let sends = recording
        .frames
        .iter()
        .filter_map(|frame| match &frame.body {
            Body::Send(m) => Some(m),
            _ => None,
        });
    let ids: Vec<Value> = sends
        .filter(|m| config(m))
        .map(|m| m["id"].clone())
        .collect();
    recording.frames.retain(|frame| match &frame.body {
        Body::Send(m) => !config(m),
        Body::Recv(m) => m.get("method").is_some() || !ids.contains(&m["id"]),
        _ => true,
    });
    let client = json!({"name": "cupper", "version": env!("CARGO_PKG_VERSION")});
    let capabilities = json!({"fs": {"readTextFile": false, "writeTextFile": false},
        "terminal": false, "auth": {"terminal": false}, "elicitation": {"form": {}}, "plan": {}});
    let mut prompt = String::new();
    for frame in &mut recording.frames {
        match &mut frame.body {
            Body::Send(m) if m["method"] == "initialize" => {
                m["params"] = json!({"protocolVersion": 1, "clientCapabilities": capabilities,
                    "clientInfo": client});
            }
            Body::Send(m) if m["method"] == "session/prompt" => {
                prompt = m["params"]["prompt"][0]["text"]
                    .as_str()
                    .unwrap()
                    .to_owned();
            }
            _ => {}
        }
    }
    let path = dir.path().join("s02b-plan.jsonl");
    std::fs::write(&path, recording.to_jsonl()).unwrap();
    let agent = json!({"program": launch::fake_agent(), "args": ["--replay", path]});
    let (data_dir, mut host, client) =
        fake_agent_host(&dir, json!({"agents": {"claude": agent}})).await;
    create(&client, &prompt).await;
    let turns = "SELECT status FROM turns";
    until(&data_dir, "host.db", turns, &["completed"]).await;
    terminate(&host);
    assert_eq!(exit_code(&mut host).await, 0);
}

/// A recording of two instances: the first writes to stderr and exits, the
/// second starts for the next message. The export reads back, and
/// `tools/sanitize` takes it.
#[tokio::test]
async fn it_recording_export_two_instances_read_back_and_sanitize() {
    let dir = TempDir::new("recording");
    let (data_dir, mut host, client) =
        fake_agent_host(&dir, json!({"durations": {"adapter_backoff_ms": 1}})).await;
    let session = create(&client, "say hi; stderr oops").await;
    let turns = "SELECT status FROM turns ORDER BY turn_no";
    until(&data_dir, "host.db", turns, &["completed"]).await;
    for (text, statuses) in [
        ("exit 0", &["completed", "failed"][..]),
        ("say again", &["completed", "failed", "completed"]),
    ] {
        send(&client, &session, text).await;
        until(&data_dir, "host.db", turns, statuses).await;
    }
    // Raw frames hold no barrier: wait for the notes to land.
    let notes = "SELECT json_extract(frame, '$.event') FROM raw_frames WHERE dir = 'note'
                 ORDER BY ts, seq";
    let events = ["instance_started", "instance_exited", "instance_started"];
    until(&data_dir, "raw.db", notes, &events).await;

    let params = serde_json::from_value(json!({"session_id": session})).unwrap();
    let blob = client
        .request::<DiagRecordingExport>(params)
        .await
        .unwrap()
        .blob;
    let hash = blob.hash.to_string();
    let path = data_dir.join("blobs/sha256").join(&hash[..2]).join(&hash);
    let text = std::fs::read_to_string(path).unwrap();
    let recording = Recording::parse(&text).unwrap();
    let segments: Vec<_> = recording.segments().collect();
    assert_eq!(segments.len(), 2);
    let first = segments[0].iter().map(|frame| &frame.body);
    assert!(
        first
            .clone()
            .any(|body| *body == Body::Stderr("oops".into()))
    );
    assert!(
        first
            .clone()
            .any(|body| body.event() == Some(INSTANCE_EXITED))
    );
    for segment in &segments {
        assert_eq!(segment[0].body.event(), Some(INSTANCE_STARTED));
    }

    // Into a directory of its own, then through `tools/sanitize`.
    let (input, output) = (dir.path().join("in"), dir.path().join("out"));
    std::fs::create_dir(&input).unwrap();
    std::fs::write(input.join("recording.jsonl"), &text).unwrap();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let real = std::fs::canonicalize(dir.path()).unwrap();
    let sanitized = Command::new("node")
        .current_dir(&repo)
        .args(["tools/sanitize/src/main.ts", "sanitize", "--in"])
        .args([
            &input,
            Path::new("--out"),
            &output,
            Path::new("--base"),
            dir.path(),
        ])
        .args([Path::new("--base"), &real])
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(sanitized.success());
    terminate(&host);
    assert_eq!(exit_code(&mut host).await, 0);
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
    let info = ProjectTrustInfoParams {
        project_id: project_id.clone(),
    };
    let info = client.request::<ProjectTrustInfo>(info).await.unwrap();
    assert_eq!(info.state, TrustState::Stale);
    let resolved = std::fs::canonicalize(&app).unwrap();
    assert_eq!(
        info.path.0,
        resolved.to_str().unwrap(),
        "the directory the host resolved"
    );
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
    // A link that leads nowhere fails the scan; the audit has each step.
    let gone = app.join(".claude/agents/gone.md");
    std::os::unix::fs::symlink(dir.path().join("nowhere"), gone).unwrap();
    let info = ProjectTrustInfoParams { project_id };
    assert!(client.request::<ProjectTrustInfo>(info).await.is_err());
    let trust = "SELECT substr(event, 15) || ' ' || coalesce(principal, '-') FROM audit
                 WHERE event LIKE 'project.trust.%' ORDER BY id";
    let steps = ["trusted pr_test", "stale -", "scan_failed -"];
    until(&data_dir, "host.db", trust, &steps).await;
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

/// A host a test started: killed when the test ends, passed or failed.
struct Running(Child);

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// IT-audit-retention: what the gateway records lands in the `audit` table,
/// in its formal shape, and in `host.log`; `test/run_maintenance` keeps 90
/// days of it, on the manual clock.
#[tokio::test]
async fn it_audit_retention_records_land_and_last_90_days() {
    use cupper_protocol::methods::admin::{TestRunMaintenance, TestRunMaintenanceParams};

    let dir = TempDir::new("audit");
    let data_dir = dir.path().join("data");
    let test_config = json!({"login_env": {}, "clock": "manual"});
    let mut host = Running(spawn_with(&data_dir, test_config));
    let client = connect(&data_dir).await;
    drop(connect(&data_dir).await);
    let events = "SELECT event || ' ' || coalesce(principal, '-') FROM audit ORDER BY id";
    let opened = ["conn.accepted -", "conn.initialized pr_test"];
    let closed = ["conn.closed pr_test"];
    // A `dev-auth` host says it runs with weak authentication.
    let weak = ["host.weak_auth -"];
    let started = [
        &["host.started -"][..],
        &weak[..usize::from(cfg!(feature = "dev-auth"))],
    ];
    let first = [&started.concat()[..], &opened, &opened, &closed].concat();
    until(&data_dir, "host.db", events, &first).await;
    let log = std::fs::read_to_string(data_dir.join("logs/host.log")).unwrap();
    let logged: Vec<_> = (log.lines())
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .filter(|line| line["message"] == "audit")
        .map(|line| line["fields"]["event"].as_str().unwrap().to_owned())
        .collect();
    let first_events: Vec<_> = first.iter().map(|e| e.split(' ').next().unwrap()).collect();
    assert_eq!(&logged[..first.len()], first_events);

    // Two records from about 90 days ago, one just past the limit: the
    // manual clock decides which `test/run_maintenance` drops.
    let now: u64 = query(
        &data_dir.join("host.db"),
        "SELECT CAST(min(ts) AS TEXT) FROM audit",
    )[0]
    .parse()
    .unwrap();
    let days_90 = 90 * 24 * 3_600_000;
    let old = Connection::open(data_dir.join("host.db")).unwrap();
    old.busy_timeout(PATIENCE).unwrap();
    for (ts, event) in [(now - days_90 - 1, "old"), (now - days_90 + 1, "edge")] {
        let sql = "INSERT INTO audit (ts, event) VALUES (?1, ?2)";
        old.execute(sql, rusqlite::params![ts, event]).unwrap();
    }
    let task = || TestRunMaintenanceParams {
        task: serde_json::from_value(json!("audit")).unwrap(),
    };
    let kept = |edge: &[&'static str]| [&first[..], edge].concat();
    client.request::<TestRunMaintenance>(task()).await.unwrap();
    assert_eq!(query(&data_dir.join("host.db"), events), kept(&["edge -"]));
    advance(&client, 2).await;
    client.request::<TestRunMaintenance>(task()).await.unwrap();
    assert_eq!(query(&data_dir.join("host.db"), events), kept(&[]));
    // The orderly exit stores its own record and the last connection's.
    terminate(&host.0);
    assert_eq!(exit_code(&mut host.0).await, 0);
    let last = [&kept(&["host.exiting -"])[..], &closed].concat();
    assert_eq!(query(&data_dir.join("host.db"), events), last);
    let why = "SELECT json_extract(detail_json, '$.reason') FROM audit
               WHERE event IN ('host.exiting', 'conn.closed') ORDER BY id";
    let reasons = ["peer_closed", "terminated", "host_exiting"];
    assert_eq!(query(&data_dir.join("host.db"), why), reasons);
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

/// The lines an elevation probe wrote to `out`, once there are `n`.
async fn answers(out: &Path, n: usize) -> Vec<serde_json::Value> {
    let clock = SystemClock::new();
    let give_up = clock.now_mono() + PATIENCE;
    loop {
        let text = std::fs::read_to_string(out).unwrap_or_default();
        let lines: Vec<_> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        if lines.len() >= n {
            return lines;
        }
        assert!(clock.now_mono() < give_up, "{}: {lines:?}", out.display());
        wait(&clock).await;
    }
}

/// IT-elevate on a real host with the fake agent, on the manual clock: the
/// adapter's descendants are refused; a request from elsewhere waits, a
/// second one is rate limited, and two minutes on the first has expired;
/// one granted can write; one whose connection closes ends with it.
#[tokio::test]
async fn it_elevate_lineage_waits_limits_expiry_grant_and_close() {
    let dir = TempDir::new("elevate");
    let (data_dir, mut host, client) = fake_agent_host(&dir, json!({"clock": "manual"})).await;
    let probe = launch::fake_agent().with_file_name("cupper-elevate-probe");
    let out = |name: &str| dir.path().join(name);
    let args = |name: &str| {
        let (data, out) = (data_dir.display(), out(name));
        format!(
            "{} --data-dir {data} --out {}",
            probe.display(),
            out.display()
        )
    };
    // A tool command in a session of its own, then an MCP server or hook.
    let session = create(&client, &format!("spawn {} --setsid", args("tool"))).await;
    send(&client, &session, &format!("spawn {}", args("mcp"))).await;
    for name in ["tool", "mcp"] {
        let refused = &answers(&out(name), 1).await[0]["elevate"]["error"]["data"];
        assert_eq!(
            (&refused["code"], &refused["reason"]),
            (&json!("forbidden"), &json!("lineage"))
        );
    }

    let start = |name: &str| {
        let args = args(name);
        Command::new(&probe)
            .args(args.split(' ').skip(1))
            .spawn()
            .unwrap()
    };
    let events = |kind| {
        format!(
            "SELECT json_extract(json, '$.data.elevation_id') FROM events
        WHERE stream = 'index' AND type = 'elevation.{kind}' ORDER BY seq"
        )
    };
    let requested = || query(&data_dir.join("host.db"), &events("requested"));
    let until_requested = |n| async move {
        let clock = SystemClock::new();
        while requested().len() < n {
            wait(&clock).await;
        }
        requested().pop().unwrap()
    };
    let code = |answer: &serde_json::Value| answer["elevate"]["error"]["data"]["code"].clone();
    let mut first = start("first");
    until_requested(1).await;
    let mut second = start("second");
    assert_eq!(code(&answers(&out("second"), 1).await[0]), "rate_limited");
    let advance = json!({"ms": 120_000});
    client
        .request::<TestClockAdvance>(serde_json::from_value(advance).unwrap())
        .await
        .unwrap();
    assert_eq!(code(&answers(&out("first"), 1).await[0]), "timeout");

    let mut third = start("third");
    let elevation_id = until_requested(2).await;
    let get = serde_json::from_value(json!({"elevation_id": elevation_id})).unwrap();
    let verified = client
        .request::<AuthElevationGet>(get)
        .await
        .unwrap()
        .verified;
    let probe_path = std::fs::canonicalize(&probe).unwrap();
    assert_eq!((verified.pid, verified.bundled_cli), (third.id(), false));
    assert_eq!(verified.executable.0, probe_path.display().to_string());
    let respond = json!({"command_id": client::command_id(), "elevation_id": elevation_id,
        "decision": "grant", "scopes": ["write"]});
    let respond = serde_json::from_value(respond).unwrap();
    client
        .request::<AuthElevationRespond>(respond)
        .await
        .unwrap();
    let granted = answers(&out("third"), 2).await;
    assert_eq!(granted[0]["elevate"]["ok"]["granted"], json!(["write"]));
    assert_eq!(granted[1]["write"]["error"]["data"]["code"], "not_found");

    let mut fourth = start("fourth");
    until_requested(3).await;
    terminate(&fourth);
    // Each request resolved, the last as its connection closed.
    let ids = requested();
    let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
    until(&data_dir, "host.db", &events("resolved"), &ids).await;
    let decisions = "SELECT json_extract(json, '$.data.decision') FROM events
        WHERE stream = 'index' AND type = 'elevation.resolved' ORDER BY seq";
    let decided = query(&data_dir.join("host.db"), decisions);
    assert_eq!(decided, ["expired", "granted", "expired"]);
    for probe in [&mut first, &mut second, &mut third, &mut fourth] {
        probe.wait().unwrap();
    }
    terminate(&host);
    assert_eq!(exit_code(&mut host).await, 0);
}

/// The e2e build of `cupper`, beside the fake agent.
fn cupper() -> PathBuf {
    launch::fake_agent().with_file_name("cupper")
}

/// `cupper --data-dir <data_dir> --json <args>`: its exit code and lines.
fn cupper_once(data_dir: &Path, args: &[&str], stdin: &str) -> (i32, Vec<serde_json::Value>) {
    let mut child = Command::new(cupper())
        .arg("--data-dir")
        .arg(data_dir)
        .arg("--json")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    let lines = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    (output.status.code().unwrap(), lines)
}

/// A child of the case, terminated and waited for when dropped: a failed
/// case leaves no process behind.
struct Reaped(Child);

impl Drop for Reaped {
    fn drop(&mut self) {
        if let Ok(None) = self.0.try_wait() {
            terminate(&self.0);
            let _ = self.0.wait();
        }
    }
}

/// `cupper repl`, fed one command at a time.
struct Repl {
    child: Reaped,
    lines: tokio::sync::mpsc::UnboundedReceiver<serde_json::Value>,
}

impl Repl {
    fn start(data_dir: &Path) -> Self {
        let mut child = Command::new(cupper())
            .arg("--data-dir")
            .arg(data_dir)
            .args(["--json", "repl"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let (tx, lines) = tokio::sync::mpsc::unbounded_channel();
        let stdout = std::io::BufReader::new(child.stdout.take().unwrap());
        std::thread::spawn(move || {
            use std::io::BufRead;
            for line in stdout.lines().map_while(Result::ok) {
                let _ = tx.send(serde_json::from_str(&line).unwrap());
            }
        });
        let child = Reaped(child);
        Self { child, lines }
    }

    fn send(&mut self, line: &str) {
        let stdin = self.child.0.stdin.as_mut().unwrap();
        writeln!(stdin, "{line}").unwrap();
    }

    /// What `line` printed, and its exit code.
    async fn run(&mut self, line: &str) -> (Vec<serde_json::Value>, u64) {
        self.send(line);
        self.output().await
    }

    async fn output(&mut self) -> (Vec<serde_json::Value>, u64) {
        let mut lines = Vec::new();
        loop {
            let line = self.lines.recv().await.expect("repl output");
            match line["repl"]["exit"].as_u64() {
                Some(exit) => return (lines, exit),
                None => lines.push(line),
            }
        }
    }
}

/// IT-cli-commands: `cupper --json` on a real host with the fake agent, on
/// the manual clock, which nothing advances. Reading needs nothing; writing
/// needs `elevate`, granted here on the harness connection as the dialog
/// would, then holds for the `repl`'s connection.
#[tokio::test]
async fn it_cli_commands_read_elevate_write_and_answer() {
    let dir = TempDir::new("cli");
    let (data_dir, host, client) = fake_agent_host(&dir, json!({"clock": "manual"})).await;
    let mut host = Reaped(host);
    let session = create(&client, "say hi").await;
    let turns = "SELECT status FROM turns ORDER BY turn_no";
    until(&data_dir, "host.db", turns, &["completed"]).await;
    let s = session.to_string();

    let (code, status) = cupper_once(&data_dir, &["status"], "");
    assert_eq!((code, status[0]["version"].is_string()), (0, true));
    let key = proof::to_hex(&KEY) + "\n";
    let harness = cupper_once(&data_dir, &["--harness-key-fd", "0", "status"], &key);
    assert_eq!(harness.0, 0);
    // The index writes what changed after 100 ms, on this clock too.
    let advance = serde_json::from_value(json!({"ms": 1_000})).unwrap();
    client.request::<TestClockAdvance>(advance).await.unwrap();
    let (code, sessions) = cupper_once(&data_dir, &["sessions"], "");
    assert_eq!(
        (code, &sessions[0]["session_id"]),
        (0, &json!(s)),
        "{sessions:?}"
    );
    assert_eq!(cupper_once(&data_dir, &["history", &s], "").0, 0);
    // Writing as the read-only `cli` principal.
    let (code, refused) = cupper_once(&data_dir, &["send", &s, "hello"], "");
    let error = &refused[0]["error"];
    assert_eq!(
        (code, &error["code"], &error["reason"]),
        (1, &json!("forbidden"), &json!("scope"))
    );

    let mut watch = Command::new(cupper());
    watch
        .arg("--data-dir")
        .arg(&data_dir)
        .args(["--json", "watch", &s]);
    let mut watch = Reaped(watch.stdout(Stdio::piped()).spawn().unwrap());
    let mut repl = Repl::start(&data_dir);
    repl.send("elevate --scopes write,approve");
    let requested = "SELECT json_extract(json, '$.data.elevation_id') FROM events
        WHERE stream = 'index' AND type = 'elevation.requested'";
    let clock = SystemClock::new();
    let elevation_id = loop {
        if let Some(id) = query(&data_dir.join("host.db"), requested).pop() {
            break id;
        }
        wait(&clock).await;
    };
    let grant = json!({"command_id": client::command_id(), "elevation_id": elevation_id,
        "decision": "grant", "scopes": ["write", "approve"]});
    client
        .request::<AuthElevationRespond>(serde_json::from_value(grant).unwrap())
        .await
        .unwrap();
    let (granted, code) = repl.output().await;
    assert_eq!(
        (code, &granted[0]["granted"]),
        (0, &json!(["write", "approve"]))
    );

    // A permission approved, another rejected, a question answered.
    for (script, answer) in [
        ("permission p1 execute", "approve"),
        ("permission p2 execute", "reject"),
        ("question colour?", "answer"),
    ] {
        let (sent, code) = repl.run(&format!("send {s} \"{script}\"")).await;
        assert_eq!(
            (code, &sent[0]["disposition"]),
            (0, &json!("started")),
            "{script}"
        );
        let asked = loop {
            let (lines, _) = repl.run(&format!("pending {s}")).await;
            if let Some(asked) = lines.into_iter().next() {
                break asked;
            }
        };
        let request = asked["request_id"].as_str().unwrap().to_owned();
        let line = match answer {
            "answer" => {
                let field = &asked["request"]["fields"][0]["field_id"];
                format!(
                    "answer {s} {request} --field {}=blue",
                    field.as_str().unwrap()
                )
            }
            verb => format!("{verb} {s} {request}"),
        };
        let (answered, code) = repl.run(&line).await;
        assert_eq!(
            (code, &answered[0]["request_id"]),
            (0, &json!(request)),
            "{line}"
        );
        // The agent says what it got.
        let said = match &answered[0]["outcome"]["option_id"] {
            serde_json::Value::String(option) => format!("permission:{option}"),
            _ => "answer:blue".to_owned(),
        };
        let says = format!(
            "SELECT 'said' FROM events WHERE stream = 'session/{s}'
            AND json LIKE '%{said}%' LIMIT 1"
        );
        until(&data_dir, "host.db", &says, &["said"]).await;
    }

    // A new session, a message waited for, and an interrupt.
    let project = dir.path().display().to_string();
    let (created, code) = repl
        .run(&format!(
            "new --agent claude --project {project} \"say new\""
        ))
        .await;
    let other = created[0]["session_id"].as_str().unwrap().to_owned();
    assert_eq!(code, 0);
    let (waited, code) = repl
        .run(&format!("send {other} \"say again\" --wait"))
        .await;
    assert_eq!((code, &waited[1]["status"]), (0, &json!("completed")));
    repl.send(&format!("send {s} wait_cancel"));
    repl.output().await;
    let (interrupted, code) = repl.run(&format!("interrupt {s}")).await;
    assert_eq!((code, interrupted[0]["turn_id"].is_string()), (0, true));
    repl.send("exit");
    assert_eq!(repl.child.0.wait().unwrap().code(), Some(0));

    // `watch` saw the turns of the first session.
    terminate(&watch.0);
    let mut seen = String::new();
    std::io::Read::read_to_string(&mut watch.0.stdout.take().unwrap(), &mut seen).unwrap();
    assert!(seen.contains("\"turn.started\"") && seen.contains("\"turn.completed\""));
    terminate(&host.0);
    assert_eq!(exit_code(&mut host.0).await, 0);
}

/// The harness client asking for the blob features, once the host listens.
async fn connect_for_blobs(data_dir: &Path) -> client::Client {
    use cupper_protocol::methods::handshake::Feature;
    let socket = socket_path(data_dir, &cupper_sys::user_temp_dir().unwrap());
    let options = Options {
        auth: ClientAuth::Harness(HarnessKey::new(KEY)),
        features: vec![Feature::BlobGet, Feature::BlobPut],
        ..Options::default()
    };
    let client = client::Client::connect(&socket, &HostCheck::Harness, options).await;
    let client = client.expect("the host listens");
    assert!(client.initialized().features.contains(&Feature::BlobPut));
    client
}

/// Sends `data` as the chunks of `hash`, up to `upto` of them.
async fn send_chunks(client: &client::Client, hash: [u8; 32], data: &[u8], upto: usize) {
    use cupper_protocol::frame::BlobChunkHeader;
    let pieces: Vec<&[u8]> = data.chunks(BlobChunkHeader::MAX_DATA).collect();
    for (n, piece) in pieces.iter().enumerate().take(upto) {
        let offset = (n * BlobChunkHeader::MAX_DATA) as u64;
        let header = BlobChunkHeader {
            hash,
            offset,
            last: n + 1 == pieces.len(),
        };
        client.send_blob_chunk(&header, piece).await.unwrap();
    }
}

/// IT-gw-blob: a blob of three chunks put and got through the gateway of a
/// real host, whole and by range; a size or hash that does not match, a size
/// past the limit and an unknown hash refused; an upload cut off by a
/// disconnect leaves nothing behind.
#[tokio::test]
async fn it_gw_blob_put_and_get_through_the_gateway() {
    use cupper_protocol::error::ErrorCode;
    use cupper_protocol::ids::BlobHash;
    use cupper_protocol::methods::stream::{BlobGet, BlobGetParams, BlobPut, BlobPutParams};
    use sha2::{Digest as _, Sha256};

    let dir = TempDir::new("blobs");
    let data_dir = dir.path().join("data");
    let mut host = spawn_with(&data_dir, json!({}));
    drop(connect(&data_dir).await);
    let data: Vec<u8> = (0..150_000u32).map(|n| (n % 251) as u8).collect();
    let raw: [u8; 32] = Sha256::digest(&data).into();
    let hash = BlobHash::from_sha256(&raw);
    let put = |hash: &BlobHash, size: u64| BlobPutParams {
        hash: hash.clone(),
        size,
        media_type: "application/octet-stream".into(),
    };
    let code = |err: client::ClientError| match err {
        client::ClientError::Rpc(err) => err.data.map(|data| data.code),
        other => panic!("{other}"),
    };

    // Cut off halfway: the next connection puts the same blob as if new.
    let cut = connect_for_blobs(&data_dir).await;
    send_chunks(&cut, raw, &data, 2).await;
    drop(cut);
    let mut client = connect_for_blobs(&data_dir).await;
    send_chunks(&client, raw, &data, 3).await;
    let stored = client
        .request::<BlobPut>(put(&hash, 150_000))
        .await
        .unwrap();
    assert_eq!(stored.hash, hash);

    for (offset, length, want) in [
        (None, None, &data[..]),
        (Some(70_000), Some(10), &data[70_000..70_010]),
    ] {
        let params = BlobGetParams {
            hash: hash.clone(),
            offset,
            length,
        };
        let got = client.request::<BlobGet>(params).await.unwrap();
        let mut bytes = Vec::new();
        for _ in 0..got.chunks {
            let Some(Incoming::Blob { header, data }) = client.next().await else {
                panic!("a chunk comes ahead of the response");
            };
            assert_eq!(
                (header.hash, header.offset),
                (raw, got.offset + bytes.len() as u64)
            );
            bytes.extend_from_slice(&data);
        }
        assert_eq!((got.size, bytes.as_slice()), (150_000, want));
    }

    // The same bytes under a size or a hash of something else.
    send_chunks(&client, raw, &data, 3).await;
    let err = client
        .request::<BlobPut>(put(&hash, 149_999))
        .await
        .unwrap_err();
    assert_eq!(code(err), Some(ErrorCode::Conflict));
    let other = BlobHash::from_sha256(&[7; 32]);
    send_chunks(&client, [7; 32], &data, 3).await;
    let err = client
        .request::<BlobPut>(put(&other, 150_000))
        .await
        .unwrap_err();
    assert_eq!(code(err), Some(ErrorCode::Conflict));
    let err = client
        .request::<BlobPut>(put(&other, 33 << 20))
        .await
        .unwrap_err();
    assert_eq!(code(err), Some(ErrorCode::TooLarge));
    let params = BlobGetParams {
        hash: other,
        offset: None,
        length: None,
    };
    let err = client.request::<BlobGet>(params).await.unwrap_err();
    assert_eq!(code(err), Some(ErrorCode::NotFound));

    terminate(&host);
    assert_eq!(exit_code(&mut host).await, 0);
}

/// IT-idle-exit: a connection keeps the host up; with nobody connected it
/// exits in order once `supervisor_idle` has passed. The stream methods are
/// served on the way.
#[tokio::test]
async fn it_idle_exit_once_nobody_is_connected_for_the_idle_time() {
    let dir = TempDir::new("idle");
    let data_dir = dir.path().join("data");
    let durations = json!({"supervisor_idle_ms": 300, "supervisor_observe_ms": 20});
    let mut host = spawn_with(&data_dir, json!({"login_env": {}, "durations": durations}));
    let client = connect(&data_dir).await;
    let params = serde_json::from_value(json!({"stream": "index"})).unwrap();
    let subscribed = client.request::<StreamSubscribe>(params).await.unwrap();
    assert_eq!(subscribed.stream.to_string(), "index");
    let clock = SystemClock::new();
    let until = clock.now_mono() + Duration::from_millis(900);
    while clock.now_mono() < until {
        wait(&clock).await;
    }
    assert!(host.try_wait().unwrap().is_none(), "connected, it stays");
    drop(client);
    assert_eq!(exit_code(&mut host).await, 0);
    let (start, clean) = (MetaKey::LastStartAt, MetaKey::LastCleanExitAt);
    assert!(meta_ms(&data_dir, clean) >= meta_ms(&data_dir, start));
    let log = std::fs::read_to_string(log_dir(&data_dir).join("host.log")).unwrap();
    let exits: Vec<serde_json::Value> = log
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|line: &serde_json::Value| line["message"] == "orderly exit")
        .collect();
    assert_eq!(exits.len(), 1, "{log}");
    assert_eq!(exits[0]["fields"]["reason"], "idle");
}

/// IT-self-check: the host runs from a copy of its executable; with the copy
/// gone, the next connection finds the code on disk changed. New sessions are
/// refused and the host says an upgrade waits; once the running turn ends it
/// exits with `upgrade`.
#[tokio::test]
async fn it_self_check_a_changed_executable_makes_the_host_wait_for_an_upgrade() {
    let dir = TempDir::new("self-check");
    let exe = dir.path().join("cupper-host");
    std::fs::copy(HOST, &exe).unwrap();
    let config = json!({"durations": {"supervisor_observe_ms": 20}});
    let (data_dir, mut host, mut client) = fake_agent_host_from(&exe, &dir, config).await;
    let session = create(&client, "wait_cancel").await;
    until(
        &data_dir,
        "host.db",
        "SELECT status FROM turns",
        &["running"],
    )
    .await;
    std::fs::remove_file(&exe).unwrap();
    drop(connect(&data_dir).await);
    let clock = SystemClock::new();
    while !client
        .request::<HostStatus>(Empty {})
        .await
        .unwrap()
        .pending_upgrade
    {
        wait(&clock).await;
    }
    let params = json!({"command_id": client::command_id(), "agent": "claude",
        "project_id": ProjectId::from_uuid(1), "workspace": {"kind": "local"},
        "message": [{"type": "text", "text": "say hi"}]});
    let refused = client.request::<SessionCreate>(serde_json::from_value(params).unwrap());
    let Err(ClientError::Rpc(err)) = refused.await else {
        panic!("a new session while an upgrade waits");
    };
    assert_eq!(err.reason(), Some(&ErrorReason::PendingUpgrade), "{err}");
    let sql = "SELECT CAST(json_extract(json, '$.data.pending_upgrade') AS TEXT) FROM events
               WHERE stream = 'index' AND type = 'host.updated' ORDER BY seq DESC LIMIT 1";
    until(&data_dir, "host.db", sql, &["1"]).await;
    let audit = "SELECT event FROM audit WHERE event = 'host.pending_upgrade'";
    until(&data_dir, "host.db", audit, &["host.pending_upgrade"]).await;

    let params = json!({"command_id": client::command_id(), "session_id": session});
    let interrupt = client.request::<SessionInterrupt>(serde_json::from_value(params).unwrap());
    // Its reply races the exit it brings about.
    let _ = interrupt.await;
    loop {
        match client.next().await {
            Some(Incoming::Other { method, params }) if method == "host/shutdown" => {
                assert_eq!(params["reason"], "upgrade");
                break;
            }
            Some(_) => {}
            None => panic!("no host/shutdown"),
        }
    }
    assert_eq!(exit_code(&mut host).await, 0);
}

/// Stands in for node running the bundling script (as in IT-runtime-download):
/// it copies the entry to the bundle.
const NODE: &str = "#!/bin/sh\n[ -f \"$2\" ] || exit 9\n/bin/cat \"$3/$4\" > \"$3/$5\"\n";
const ENTRY: &[u8] = b"export const adapter = 1;";

/// A host in `CupperHost.app` made in `dir`, with a fake node, esbuild-wasm
/// and the bundling inputs, the manifest that pins them, and a registry
/// serving the one package of the lockfile; started once `prepare` ran.
async fn bundled_host(dir: &TempDir, prepare: impl FnOnce(&Path)) -> (PathBuf, Running) {
    use base64::Engine as _;
    use sha2::{Digest as _, Sha256, Sha512};
    let contents = dir.path().join("CupperHost.app/Contents");
    let (exe, data_dir) = (contents.join("MacOS/cupper-host"), dir.path().join("data"));
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    std::fs::copy(HOST, &exe).unwrap();
    let mut tarball = tar::Builder::new(flate2::write::GzEncoder::new(
        Vec::new(),
        Default::default(),
    ));
    let mut header = tar::Header::new_gnu();
    header.set_size(ENTRY.len() as u64);
    header.set_mode(0o644);
    tarball
        .append_data(&mut header, "package/index.js", ENTRY)
        .unwrap();
    let tarball = tarball.into_inner().unwrap().finish().unwrap();
    let sri = base64::engine::general_purpose::STANDARD.encode(Sha512::digest(&tarball));
    let lock = json!({"lockfileVersion": 3, "packages": {"": {"name": "root"},
        "node_modules/a": {"resolved": "https://registry.npmjs.org/a/-/a-1.0.0.tgz",
            "integrity": format!("sha512-{sri}")}}});
    let mut manifest: serde_json::Value =
        serde_json::from_str(include_str!("../../../runtime/manifest.json")).unwrap();
    let files = [
        ("Helpers/node", NODE.as_bytes().to_vec(), 0o755),
        (
            "Resources/runtime/esbuild-wasm-0.1.0/lib/main.js",
            b"main".to_vec(),
            0o644,
        ),
        (
            "Resources/runtime/locks/claude.json",
            lock.to_string().into_bytes(),
            0o644,
        ),
        (
            "Resources/runtime/bundle-claude-acp.mjs",
            b"// bundles".to_vec(),
            0o644,
        ),
    ];
    manifest["files"] = json!({});
    for (path, bytes, mode) in files {
        let path_on_disk = contents.join(path);
        std::fs::create_dir_all(path_on_disk.parent().unwrap()).unwrap();
        std::fs::write(&path_on_disk, &bytes).unwrap();
        let permissions = std::os::unix::fs::PermissionsExt::from_mode(mode);
        std::fs::set_permissions(&path_on_disk, permissions).unwrap();
        manifest["files"][path] = json!({"sha256": format!("{:x}", Sha256::digest(&bytes))});
    }
    manifest["node"]["version"] = json!("1.0.0");
    manifest["esbuild_wasm"]["version"] = json!("0.1.0");
    manifest["claude_agent_acp"] = json!({"version": "0.9.0", "lockfile": "locks/claude.json",
        "entry": "node_modules/a/index.js", "bundle_sha256": format!("{:x}", Sha256::digest(ENTRY))});
    let manifest_path = dir.path().join("manifest.json");
    std::fs::write(&manifest_path, manifest.to_string()).unwrap();
    prepare(&contents);

    let registry = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", registry.local_addr().unwrap());
    tokio::spawn(async move {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        while let Ok((mut stream, _)) = registry.accept().await {
            let mut head = [0; 4096];
            let _ = stream.read(&mut head).await;
            let reply = format!(
                "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                tarball.len()
            );
            let _ = stream
                .write_all(&[reply.as_bytes(), &tarball].concat())
                .await;
        }
    });
    let runtime = json!({"registry_url": url, "manifest": manifest_path});
    let host = spawn_from(
        &exe,
        &data_dir,
        json!({"login_env": {}, "runtime": runtime}),
    );
    (data_dir, Running(host))
}

/// The latest `agents.updated`'s `adapter.<field>` of Claude, as text.
const ADAPTER_FIELD: &str =
    "SELECT coalesce(json_extract(json, '$.data.agents[0].adapter.' || ?), '-')
    FROM events WHERE stream = 'index' AND type = 'agents.updated' ORDER BY seq DESC LIMIT 1";

fn adapter(data_dir: &Path, field: &str) -> Vec<String> {
    let conn = open_reader(&data_dir.join("host.db")).unwrap();
    let mut query = conn.prepare(ADAPTER_FIELD).unwrap();
    let rows = query.query_map([field], |row| row.get(0)).unwrap();
    rows.collect::<Result<_, _>>().unwrap_or_default()
}

/// IT-runtime-startup: a host in a bundle installs its runtime from the
/// registry, and the index shows it ready; with a bundled file changed it
/// waits for an upgrade instead, and having nothing to finish, exits.
#[tokio::test]
async fn it_runtime_startup_installs_from_the_registry_or_waits_for_an_upgrade() {
    let dir = TempDir::new("runtime");
    let (data_dir, mut host) = bundled_host(&dir, |_| ()).await;
    drop(connect(&data_dir).await);
    let clock = SystemClock::new();
    let give_up = clock.now_mono() + PATIENCE;
    while adapter(&data_dir, "runtime") != ["ready"] {
        assert!(
            clock.now_mono() < give_up,
            "{:?}",
            adapter(&data_dir, "runtime")
        );
        wait(&clock).await;
    }
    let installed = data_dir.join("runtime/claude-agent-acp/0.9.0/adapter.mjs");
    assert_eq!(std::fs::read(installed).unwrap(), ENTRY);
    terminate(&host.0);
    assert_eq!(exit_code(&mut host.0).await, 0);

    let dir = TempDir::new("runtime-changed");
    let changed = |contents: &Path| {
        std::fs::write(
            contents.join("Resources/runtime/esbuild-wasm-0.1.0/lib/main.js"),
            "x",
        )
        .unwrap()
    };
    let (data_dir, mut host) = bundled_host(&dir, changed).await;
    assert_eq!(exit_code(&mut host.0).await, 0);
    let upgrade = "SELECT CAST(json_extract(json, '$.data.pending_upgrade') AS TEXT) FROM events
               WHERE stream = 'index' AND type = 'host.updated' ORDER BY seq DESC LIMIT 1";
    assert_eq!(column(&data_dir, upgrade), ["1"]);
    assert_eq!(adapter(&data_dir, "failure"), ["bundle_changed"]);
}
