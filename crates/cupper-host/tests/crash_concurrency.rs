//! Crash consistency, concurrency and egress on a host process (design
//! 5.9.1, 5.9.8): IT-NFR9-failpoints-core, IT-NFR9-crash-under-load,
//! IT-six-sessions-shared-adapter and IT-host-egress-seatbelt. The agent is
//! the claude-shaped fake agent.
//!
//! After every crash the host starts again and [`check`] asks of what is
//! left: the store passes `PRAGMA integrity_check`; each stream's seqs are
//! contiguous; what a client saw live before the crash is the start of the
//! log, seq for seq; each command sent, sent again with its `command_id`,
//! gives the ledger's answer, the first run's when it had committed; and
//! the first message, like the second, is in its session exactly once.

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use cupper_cli::client::{self, Client, ClientAuth, Incoming, Options};
use cupper_cli::harness::HarnessKey;
use cupper_host::clock::{Clock, SystemClock};
use cupper_host::gateway::testkit::{KEY, TempDir};
use cupper_protocol::methods::project::ProjectAdd;
use cupper_protocol::methods::session::{SessionCreate, SessionSend};
use cupper_protocol::methods::stream::StreamSubscribeParams;
use cupper_protocol::rpc::Method;
use cupper_protocol::sync::StreamId;
use serde_json::{Value, json};
use tokio::task::JoinHandle;

use common::*;

/// The scenario's messages.
const FIRST: &str = "say one";
const SECOND: &str = "say two";

/// What a client saw live, by stream and seq.
type Seen = BTreeMap<(String, u64), Value>;

/// The host with the fake agent on `data_dir`, `failpoints` armed.
fn host(data_dir: &Path, failpoints: Value) -> Running {
    let config = fake_agent_config(json!({"failpoints": failpoints}));
    Running(spawn_with(data_dir, config))
}

/// The harness client without range events: each event as the log has it.
fn plain() -> Options {
    Options {
        auth: ClientAuth::Harness(HarnessKey::new(KEY)),
        features: Vec::new(),
        coalesce_ms: None,
    }
}

/// The harness client, unless the host exits first.
async fn reach(data_dir: &Path, host: &mut Running) -> Option<Client> {
    let clock = SystemClock::new();
    let give_up = clock.now_mono() + PATIENCE;
    loop {
        if host.0.try_wait().unwrap().is_some() {
            return None;
        }
        if let Ok(client) = connect_once(data_dir, plain()).await {
            return Some(client);
        }
        assert!(
            clock.now_mono() < give_up,
            "the host neither listens nor exits"
        );
        wait(&clock).await;
    }
}

/// Records every live event until the connection closes, following each
/// session the index names.
fn watch(mut client: Client) -> JoinHandle<Seen> {
    tokio::spawn(async move {
        let mut seen = Seen::new();
        let mut followed = BTreeSet::new();
        let subscribe = |stream| StreamSubscribeParams {
            stream,
            after_seq: None,
            epoch: None,
        };
        if client.subscribe(subscribe(StreamId::Index)).await.is_err() {
            return seen;
        }
        while let Some(incoming) = client.next().await {
            let Incoming::Events(events) = incoming else {
                continue;
            };
            for event in &events.events {
                let event = event.as_value();
                let session = event["data"]["session_id"].as_str().map(str::to_owned);
                if let Some(session) = session.filter(|_| event["type"] == "session.upserted")
                    && followed.insert(session.clone())
                {
                    let stream = StreamId::Session(session.parse().unwrap());
                    let _ = client.subscribe(subscribe(stream)).await;
                }
                let at = (events.stream.to_string(), event["seq"].as_u64().unwrap());
                seen.insert(at, event.clone());
            }
        }
        seen
    })
}

/// Whether `n` turns have ended before the host exited.
async fn ended(data_dir: &Path, host: &mut Running, n: usize) -> bool {
    let sql = "SELECT CAST(count(*) AS TEXT) FROM turns WHERE status != 'running'";
    let clock = SystemClock::new();
    let give_up = clock.now_mono() + PATIENCE;
    loop {
        if host.0.try_wait().unwrap().is_some() {
            return false;
        }
        if column(data_dir, sql)[0].parse::<usize>().unwrap() >= n {
            return true;
        }
        assert!(clock.now_mono() < give_up, "{n} turns did not end");
        wait(&clock).await;
    }
}

/// Sends `method` with `params`, as JSON; a reply that takes longer than
/// [`PATIENCE`] fails the test.
async fn request(
    client: &Client,
    method: &str,
    params: Value,
) -> Result<Value, client::ClientError> {
    let reply = async {
        match method {
            ProjectAdd::NAME => client.request::<Raw<ProjectAdd>>(params).await,
            SessionCreate::NAME => client.request::<Raw<SessionCreate>>(params).await,
            SessionSend::NAME => client.request::<Raw<SessionSend>>(params).await,
            other => panic!("not in the scenario: {other}"),
        }
    };
    let clock = SystemClock::new();
    tokio::select! {
        reply = reply => reply,
        () = clock.timer_at(clock.now_mono() + PATIENCE) => panic!("no reply to {method}"),
    }
}

fn text(text: &str) -> Value {
    json!([{"type": "text", "text": text}])
}

/// The scenario: a project, a session with [`FIRST`], then [`SECOND`] once
/// the first turn ended. It stops where the host stops; `issued` gets each
/// command before it is sent. Whether it ran to the end.
async fn drive(
    data_dir: &Path,
    app: &Path,
    host: &mut Running,
    issued: &mut Vec<(&str, Value)>,
) -> bool {
    let Some(client) = reach(data_dir, host).await else {
        return false;
    };
    let add = json!({"command_id": client::command_id(), "path": app});
    issued.push((ProjectAdd::NAME, add.clone()));
    let Ok(added) = request(&client, ProjectAdd::NAME, add).await else {
        return false;
    };
    let create = json!({"command_id": client::command_id(), "agent": "claude",
        "project_id": added["project_id"], "workspace": {"kind": "local"},
        "message": text(FIRST)});
    issued.push((SessionCreate::NAME, create.clone()));
    let Ok(created) = request(&client, SessionCreate::NAME, create).await else {
        return false;
    };
    if !ended(data_dir, host, 1).await {
        return false;
    }
    let send = json!({"command_id": client::command_id(), "session_id": created["session_id"],
        "mode": "queue", "content": text(SECOND)});
    issued.push((SessionSend::NAME, send.clone()));
    request(&client, SessionSend::NAME, send).await.is_ok() && ended(data_dir, host, 2).await
}

/// The store after a crash: intact, its seqs contiguous, and what the
/// client saw the start of each stream.
fn consistent(data_dir: &Path, seen: &Seen) {
    assert_eq!(column(data_dir, "PRAGMA integrity_check"), ["ok"]);
    let gaps = "SELECT stream || ' ' || CAST(max(seq) - min(seq) + 1 - count(*) AS TEXT)
                FROM events GROUP BY stream";
    for gap in column(data_dir, gaps) {
        assert!(gap.ends_with(" 0"), "seqs missing: {gap}");
    }
    for ((stream, seq), event) in seen {
        let sql = format!("SELECT json FROM events WHERE stream = '{stream}' AND seq = {seq}");
        let logged = column(data_dir, &sql);
        let logged: Option<Value> = logged
            .first()
            .map(|json| serde_json::from_str(json).unwrap());
        assert_eq!(logged.as_ref(), Some(event), "{stream} at {seq}");
    }
}

/// The checks after a crash, on a host started again (see the module).
async fn check(data_dir: &Path, issued: &[(&str, Value)], seen: &Seen) {
    let mut host = host(data_dir, json!([]));
    let client = connect(data_dir).await;
    consistent(data_dir, seen);
    for (method, params) in issued {
        let again = request(&client, method, params.clone()).await;
        let again = again.unwrap_or_else(|err| panic!("{method} again: {err}"));
        if *method == SessionCreate::NAME {
            let sessions = column(data_dir, "SELECT session_id FROM sessions");
            assert_eq!(sessions, [again["session_id"].as_str().unwrap()]);
        }
    }
    let sent = issued
        .iter()
        .filter(|(method, _)| *method != ProjectAdd::NAME);
    for (_, params) in sent {
        let message = params.get("message").unwrap_or(&params["content"])[0]["text"].clone();
        let sql = format!(
            "SELECT CAST(count(*) AS TEXT) FROM events WHERE type = 'item.started'
               AND json_extract(json, '$.data.kind') = 'user_message'
               AND json_extract(json, '$.data.content[0].text') = {message}"
        );
        until(data_dir, "host.db", &sql, &["1"]).await;
    }
    let running = "SELECT CAST(count(*) AS TEXT) FROM turns WHERE status = 'running'";
    until(data_dir, "host.db", running, &["0"]).await;
    terminate(&host.0);
    assert_eq!(exit_code(&mut host.0).await, 0);
}

/// IT-NFR9-failpoints-core: the scenario with `point` armed at its 1st,
/// 2nd, 4th, ... hit, until it and the orderly exit after it run without
/// reaching it; after each crash, the checks.
async fn crash_at(point: &str) {
    for (crashes, hit) in (0..10).map(|n| 1_u64 << n).enumerate() {
        let dir = TempDir::new("failpoint");
        let (data_dir, app) = (dir.path().join("data"), dir.path().join("app"));
        std::fs::create_dir_all(&app).unwrap();
        let mut host = host(&data_dir, json!([{"name": point, "hit": hit}]));
        let watching = reach(&data_dir, &mut host).await.map(watch);
        let mut issued = Vec::new();
        // The orderly exit writes too: a crash there is checked the same way.
        let completed = drive(&data_dir, &app, &mut host, &mut issued).await;
        if completed {
            terminate(&host.0);
        }
        match exit_code(&mut host.0).await {
            0 if completed => {
                assert!(crashes > 0, "{point} was never reached");
                return;
            }
            code => assert_eq!(code, 99, "{point} at {hit}"),
        }
        let seen = match watching {
            Some(watching) => watching.await.unwrap(),
            None => Seen::new(),
        };
        check(&data_dir, &issued, &seen).await;
    }
    panic!("{point} is still reached at hit 512");
}

#[tokio::test(flavor = "multi_thread")]
async fn it_nfr9_failpoints_core_between_two_migrations() {
    crash_at("store.migration_committed").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn it_nfr9_failpoints_core_before_a_commit() {
    crash_at("store.before_commit").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn it_nfr9_failpoints_core_after_a_commit() {
    crash_at("store.after_commit").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn it_nfr9_failpoints_core_before_the_fan_out() {
    crash_at("store.before_publish").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn it_nfr9_failpoints_core_amid_a_turn_snapshot() {
    crash_at("store.snapshot_written").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn it_nfr9_failpoints_core_before_the_first_turn() {
    crash_at("session.created").await;
}

/// A host on a new data directory with its project added; the client.
async fn project(dir: &TempDir) -> (PathBuf, Running, Client, Value) {
    let (data_dir, app) = (dir.path().join("data"), dir.path().join("app"));
    std::fs::create_dir_all(&app).unwrap();
    let host = host(&data_dir, json!([]));
    let client = connect(&data_dir).await;
    let add = json!({"command_id": client::command_id(), "path": app});
    let added = request(&client, ProjectAdd::NAME, add).await.unwrap();
    (data_dir, host, client, added["project_id"].clone())
}

/// `session/create` in `project` with `message`; the session.
async fn start(client: &Client, project: &Value, message: &str) -> String {
    let create = json!({"command_id": client::command_id(), "agent": "claude",
        "project_id": project, "workspace": {"kind": "local"}, "message": text(message)});
    let created = request(client, SessionCreate::NAME, create).await.unwrap();
    created["session_id"].as_str().unwrap().to_owned()
}

/// IT-NFR9-crash-under-load: while an agent writes as fast as it can, the
/// host is killed (`kill -9`) five times over; after each start the store is
/// consistent with what a client saw, and at the end the session goes on.
#[tokio::test(flavor = "multi_thread")]
async fn it_nfr9_crash_under_load_kill9_again_and_again() {
    let dir = TempDir::new("under-load");
    let (data_dir, mut host, client, project) = project(&dir).await;
    // 200,000 fragments as fast as the agent can, 2,000 at a time: in one
    // flood it queues all of them before it writes the first, 1.8 s at a
    // load of 5 and 15 to 28 s at 100, which the wait below counted (T199).
    let load = vec!["flood 2000 20"; 100].join("; sleep 1; ");
    let session = start(&client, &project, &load).await;
    let deltas = format!(
        "SELECT count(*) FROM events WHERE stream = 'session/{session}' AND type = 'item.delta'"
    );
    let count = || query_count(&data_dir, &deltas);
    for round in 0..5 {
        let watching = watch(connect_with(&data_dir, plain()).await);
        let client = connect(&data_dir).await;
        if round > 0 {
            let send = json!({"command_id": client::command_id(), "session_id": session,
                "mode": "queue", "content": text(&load)});
            request(&client, SessionSend::NAME, send).await.unwrap();
        }
        let (clock, from) = (SystemClock::new(), count());
        let give_up = clock.now_mono() + PATIENCE;
        while count() < from + 200 {
            assert!(clock.now_mono() < give_up, "no load in round {round}");
            wait(&clock).await;
        }
        host.0.kill().unwrap();
        host.0.wait().unwrap();
        let seen = watching.await.unwrap();
        host = self::host(&data_dir, json!([]));
        connect(&data_dir).await;
        consistent(&data_dir, &seen);
    }
    let client = connect(&data_dir).await;
    let done = json!({"command_id": client::command_id(), "session_id": session,
        "mode": "queue", "content": text("say done")});
    request(&client, SessionSend::NAME, done).await.unwrap();
    let completed = "SELECT CAST(count(*) AS TEXT) FROM turns WHERE status = 'completed'";
    until(&data_dir, "host.db", completed, &["1"]).await;
    terminate(&host.0);
    assert_eq!(exit_code(&mut host.0).await, 0);
}

fn query_count(data_dir: &Path, sql: &str) -> u64 {
    let conn = cupper_host::store::open_reader(&data_dir.join("host.db")).unwrap();
    conn.query_row(sql, [], |row| row.get(0)).unwrap()
}

/// IT-six-sessions-shared-adapter (NFR-11, Q8 on the host's side): six
/// sessions prompt at once on the one adapter process. Each stream holds its
/// own agent session's output, whole, and no other's.
#[tokio::test(flavor = "multi_thread")]
async fn it_six_sessions_shared_adapter_streams_stay_apart() {
    let dir = TempDir::new("six");
    let (data_dir, host, client, project) = project(&dir).await;
    let _host = host;
    let script = "whoami; flood 40 10; whoami";
    let started = (0..6).map(|_| start(&client, &project, script));
    let sessions = futures::future::join_all(started).await;
    let completed = "SELECT CAST(count(*) AS TEXT) FROM turns WHERE status = 'completed'";
    until(&data_dir, "host.db", completed, &["6"]).await;
    assert_eq!(
        column(&data_dir, "SELECT CAST(count(*) AS TEXT) FROM adapters"),
        ["1"]
    );
    let mut agents = BTreeSet::new();
    for session in &sessions {
        let sql = format!("SELECT agent_session_id FROM sessions WHERE session_id = '{session}'");
        let agent = column(&data_dir, &sql).remove(0);
        let said = format!(
            "SELECT group_concat(json_extract(json, '$.data.text'), '') FROM events
             WHERE stream = 'session/{session}' AND type = 'item.delta'"
        );
        let said = column(&data_dir, &said).remove(0);
        let whoami = format!("session={agent} resumed=false segment=0");
        assert_eq!(said.matches("session=").count(), 2, "{session}: {said}");
        assert!(
            said.starts_with(&whoami) && said.ends_with(&whoami),
            "{session}: {said}"
        );
        assert_eq!(said.len(), 2 * whoami.len() + 40 * 10, "{session}");
        agents.insert(agent);
    }
    assert_eq!(agents.len(), 6);
}

/// IT-host-egress-seatbelt (SEC-6, minutes D13): the host, and the adapter it
/// starts, run under a Seatbelt profile that kills a process at its first
/// outbound connection. A session runs to the end and the host exits in
/// order, so nothing went out. The profile does kill one that connects out.
#[tokio::test(flavor = "multi_thread")]
async fn it_host_egress_seatbelt_a_session_runs_with_outbound_denied() {
    let dir = TempDir::new("seatbelt");
    let profile = dir.path().join("no-egress.sb");
    let rules = "(version 1)\n(allow default)\n\
                 (deny network-outbound (remote ip) (with send-signal SIGKILL))\n";
    std::fs::write(&profile, rules).unwrap();
    let sandboxed = |program: &str| {
        let mut command = Command::new("/usr/bin/sandbox-exec");
        command.arg("-f").arg(&profile).arg(program);
        command
    };
    let probe = sandboxed("/usr/bin/nc")
        .args(["-u", "-w", "1", "192.0.2.1", "9"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert_eq!(probe.signal(), Some(9), "the profile lets a connection out");

    let (data_dir, app) = (dir.path().join("data"), dir.path().join("app"));
    std::fs::create_dir_all(&app).unwrap();
    let payload = payload(fake_agent_config(json!({})));
    let mut host = Running(common::start(&mut sandboxed(HOST), &data_dir, &payload));
    let client = connect(&data_dir).await;
    let add = json!({"command_id": client::command_id(), "path": app});
    let project = request(&client, ProjectAdd::NAME, add).await.unwrap()["project_id"].clone();
    start(
        &client,
        &project,
        "say hi; tool t1 read Read notes; tool_done t1 10; say done",
    )
    .await;
    let completed = "SELECT status FROM turns";
    until(&data_dir, "host.db", completed, &["completed"]).await;
    terminate(&host.0);
    assert_eq!(exit_code(&mut host.0).await, 0);
    let exits = column(&data_dir, "SELECT coalesce(exit_status, '-') FROM adapters");
    assert!(exits.iter().all(|e| !e.contains("signal")), "{exits:?}");
}
