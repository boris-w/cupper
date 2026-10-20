//! Dispatching at the gateway: IT-command-order, UNIT-unsupported-set, and
//! the policy (UNIT-policy-matrix) and the ledger (UNIT-ledger) as the gateway
//! applies them. [`Dispatch`] runs as the read loop runs it, with principals
//! made in the crate, on routes started on a real `host.db` with stub ports;
//! one test runs the read loop itself on a real socket with `peer`.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use cupper_protocol::annotations::{X_BUILDS, X_MILESTONE};
use cupper_protocol::domain::{ClientRole, PrincipalKind};
use cupper_protocol::ids::{ConnId, SessionId};
use cupper_protocol::methods::admin::SettingsUpdate;
use cupper_protocol::methods::catalog::catalog;
use cupper_protocol::methods::git::GitRevert;
use cupper_protocol::methods::session::SessionSetConfig;
use cupper_protocol::rpc::{Milestone, Scope};
use cupper_protocol::schema::generator;
use futures::FutureExt;
use futures::future::{self, BoxFuture};
use serde_json::{Value, json};
use tokio::sync::oneshot;

use super::*;
use crate::clock::{ManualClock, SystemClock};
use crate::durations::Durations;
use crate::gateway::Routes;
use crate::gateway::testkit::{Client, Peer, TempDir, TestGateway};
use crate::quiescence::Quiescence;
use crate::store::streams::{Streams, StreamsConfig};
use crate::store::{
    Database, MIGRATIONS, Readers, WRITER, Writer, WriterConfig, migrate, open_reader, open_writer,
};
use crate::supervisor::control::Control;

/// A port that keeps each call and its reply channel for the test to answer.
#[derive(Debug, Default)]
struct Stub {
    calls: Mutex<VecDeque<Delivered>>,
    /// `opened <conn> <principal>` and `closed <conn>`, in order.
    clients: Mutex<Vec<String>>,
}

#[derive(Debug)]
struct Delivered {
    call: Call,
    reply: oneshot::Sender<Outcome>,
}

impl Delivered {
    fn answer(self, outcome: Outcome) {
        // A connection that closed reads nothing.
        let _ = self.reply.send(outcome);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap()
}

impl Stub {
    /// The calls delivered so far, oldest first.
    fn take(&self) -> Vec<Delivered> {
        lock(&self.calls).drain(..).collect()
    }

    fn clients(&self) -> Vec<String> {
        lock(&self.clients).clone()
    }
}

impl Port for Stub {
    fn deliver(&self, call: Call) -> Reply {
        let (reply, answer) = oneshot::channel();
        lock(&self.calls).push_back(Delivered { call, reply });
        Box::pin(answer.map(|outcome| outcome.unwrap_or_else(|_| Err(internal()))))
    }
}

impl SessionPort for Stub {
    /// Two turns run; nothing else is known.
    fn facts(&self, _: &Confirmable) -> BoxFuture<'static, Result<Facts, RpcError>> {
        let facts = Facts {
            running_turns: 2,
            ..Facts::default()
        };
        Box::pin(future::ready(Ok(facts)))
    }
}

impl IndexPort for Stub {
    fn opened(&self, client: Opened) {
        let line = format!("opened {} {}", client.conn, client.principal.id);
        lock(&self.clients).push(line);
    }

    fn closed(&self, conn: &ConnId) {
        lock(&self.clients).push(format!("closed {conn}"));
    }
}

fn internal() -> RpcError {
    RpcError::app(ErrorCode::Internal, "the stub dropped the call")
}

/// `host.db` in a fresh directory, its writer, and stub ports.
struct Fixture {
    dir: TempDir,
    clock: ManualClock,
    writer: Writer,
    sessions: Arc<Stub>,
    index: Arc<Stub>,
}

impl Fixture {
    fn new() -> Self {
        let (dir, clock) = (TempDir::new("dispatch"), ManualClock::new());
        let mut conn = open_writer(&dir.path().join("host.db"), Database::Host).unwrap();
        migrate(&mut conn, MIGRATIONS, &dir.path().join("backups"), &clock).unwrap();
        let quiescence = Quiescence::new();
        quiescence.register(WRITER);
        let linger = Duration::ZERO;
        let (writer, _) = Writer::spawn(conn, WriterConfig { linger }, &quiescence).unwrap();
        Self {
            dir,
            clock,
            writer,
            sessions: Arc::default(),
            index: Arc::default(),
        }
    }

    /// The routes as the host starts them, the ledger loaded from the table;
    /// call within a runtime.
    fn start(&self) -> Routes {
        let host_db = self.dir.path().join("host.db");
        let conn = open_reader(&host_db).unwrap();
        let clock = Arc::new(self.clock.clone());
        let (sessions, index) = (Arc::clone(&self.sessions), Arc::clone(&self.index));
        let durations = Durations::default();
        let started = Routes::start(
            &conn,
            self.writer.clone(),
            clock.clone(),
            &durations,
            sessions,
            index,
            &Quiescence::new(),
        );
        let mut routes = started.unwrap().0;
        let readers = Readers::open(&host_db).unwrap();
        let config = StreamsConfig::new(1, &durations);
        let streams = Streams::new(self.writer.clone(), readers.clone(), clock, config);
        routes.streams(streams, readers);
        let (supervisor, _) = tokio::sync::mpsc::unbounded_channel();
        Arc::new(Control::new(supervisor, Arc::new([]))).route(&mut routes);
        // As the host's test interface (T047) in `test-harness` builds.
        if cfg!(feature = "test-harness") {
            use cupper_protocol::methods::admin::{TestClockAdvance, TestRunMaintenance, TestSeed};
            let port = || Arc::clone(&self.index) as Arc<dyn Port>;
            routes.add::<TestClockAdvance>(port());
            routes.add::<TestRunMaintenance>(port());
            routes.add::<TestSeed>(port());
        }
        routes
    }
}

fn principal(
    id: &str,
    kind: PrincipalKind,
    role: Option<ClientRole>,
    scopes: &[Scope],
) -> Principal {
    Principal {
        id: id.parse().unwrap(),
        kind,
        role,
        scopes: scopes.to_vec(),
    }
}

fn window() -> Principal {
    use Scope::*;
    let role = Some(ClientRole::Window);
    principal(
        "pr_desktop",
        PrincipalKind::DesktopUi,
        role,
        &[Read, Write, Approve, UiLocal],
    )
}

fn control() -> Principal {
    use Scope::*;
    let (kind, role) = (PrincipalKind::DesktopUi, Some(ClientRole::Control));
    principal(
        "pr_desktop",
        kind,
        role,
        &[Read, Write, Approve, Privileged, UiLocal],
    )
}

/// Every scope, as `pr_test` has.
fn harness() -> Principal {
    use Scope::*;
    let scopes = [Read, Write, Approve, Privileged, UiLocal, Test];
    principal("pr_test", PrincipalKind::TestHarness, None, &scopes)
}

fn session(n: u128) -> SessionId {
    SessionId::from_uuid(n)
}

/// `session/send` params of command `command` to session `n`.
fn send(n: u128, command: &str) -> Value {
    json!({"command_id": command, "session_id": session(n), "mode": "queue",
        "content": [{"type": "text", "text": "hi"}]})
}

/// `data.code` and `data.reason` of an error, `""` when missing.
fn code(error: &Value) -> (&str, &str) {
    let field = |name| error["data"][name].as_str().unwrap_or("");
    (field("code"), field("reason"))
}

/// What each delivered call was: method, session and command.
fn seen(delivered: &[Delivered]) -> Vec<(&str, String, String)> {
    let text = |params: &Value, key: &str| params[key].as_str().unwrap_or("").to_owned();
    let call = |d: &Delivered| {
        let params = &d.call.params;
        (
            d.call.method,
            text(params, "session_id"),
            text(params, "command_id"),
        )
    };
    delivered.iter().map(call).collect()
}

#[tokio::test]
async fn it_command_order_per_session_in_arrival_order_and_sessions_apart() {
    let fixture = Fixture::new();
    let routes = fixture.start();
    let mut peer = Peer::new(window());
    let a = peer.send(&routes, "session/send", send(1, "a"));
    let interrupt = json!({"command_id": "b", "session_id": session(1)});
    let b = peer.send(&routes, "session/interrupt", interrupt);
    let c = peer.send(&routes, "session/send", send(2, "c"));
    let d = peer.send(&routes, "session/get", json!({"session_id": session(1)}));
    let mut calls = fixture.sessions.take();
    let (s1, s2) = (session(1).to_string(), session(2).to_string());
    assert_eq!(
        seen(&calls),
        [
            ("session/send", s1.clone(), "a".into()),
            ("session/interrupt", s1.clone(), "b".into()),
            ("session/send", s2, "c".into()),
            ("session/get", s1, String::new()),
        ]
    );
    // Commands carry their first run's ticket; a query has none.
    let tickets: Vec<bool> = calls.iter().map(|d| d.call.ticket.is_some()).collect();
    assert_eq!(tickets, [true, true, true, false]);
    assert_eq!(calls[0].call.principal, window());

    // Session 2 is answered while session 1 still waits: replies go out as
    // they come, not in the order of the requests.
    let (d_call, c_call) = (calls.pop().unwrap(), calls.pop().unwrap());
    c_call.answer(Ok(json!({"disposition": "started"})));
    assert_eq!(peer.answered().await, [c]);
    assert_eq!(peer.reply(c).await, Ok(json!({"disposition": "started"})));
    d_call.answer(Ok(json!({"summary": {}})));
    calls.pop().unwrap().answer(Err(internal()));
    let mut answered = peer.answered().await;
    answered.sort_unstable();
    assert_eq!(answered, [b, d]);
    calls
        .pop()
        .unwrap()
        .answer(Ok(json!({"disposition": "queued"})));
    assert_eq!(peer.reply(a).await, Ok(json!({"disposition": "queued"})));
    let err = peer.reply(b).await.unwrap_err();
    assert_eq!(code(&err), ("internal", ""));
}

#[tokio::test]
async fn it_command_order_65th_request_in_flight_is_rate_limited() {
    let fixture = Fixture::new();
    let routes = fixture.start();
    let mut peer = Peer::new(window());
    let ids: Vec<i64> = (0..64)
        .map(|n| peer.send(&routes, "session/send", send(1, &format!("c-{n}"))))
        .collect();
    let mut held = fixture.sessions.take();
    assert_eq!(held.len(), 64);

    // Any request counts, and the refused one never reaches the session.
    for (method, params) in [("session/send", send(1, "c-64")), ("ping", json!({}))] {
        let err = peer.call(&routes, method, params).await.unwrap_err();
        assert_eq!(code(&err), ("rate_limited", ""));
        assert!(err["data"].get("retry_after_ms").is_none(), "{err}");
    }
    assert!(fixture.sessions.take().is_empty());

    // A reply frees a place.
    held.remove(0).answer(Ok(json!({})));
    assert_eq!(peer.reply(ids[0]).await, Ok(json!({})));
    let next = peer.send(&routes, "session/send", send(1, "c-64"));
    let delivered = fixture.sessions.take();
    assert_eq!(seen(&delivered)[0].2, "c-64");
    let err = peer.call(&routes, "ping", json!({})).await.unwrap_err();
    assert_eq!(code(&err), ("rate_limited", ""));
    // A call its executor drops is answered all the same.
    drop((held, delivered));
    assert_eq!(code(&peer.reply(next).await.unwrap_err()), ("internal", ""));
}

/// Why a method may still answer `unsupported` (design 5.3.11, rulings
/// before T033). The task that brings an executor deletes its rows; by T099
/// no M1a method is left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Why {
    /// A method of a later milestone.
    Later(Milestone),
    /// The task that brings its executor.
    Until(&'static str),
    /// Served only in the builds of its `x-builds`, which this one is not.
    Builds,
}

const ALLOWED: &[(&str, Why)] = {
    use Milestone::{M1b, M2, M3};
    use Why::{Builds, Later, Until};
    &[
        ("agents/capabilities", Until("T055")),
        ("agents/refresh", Until("T055")),
        ("agents/restart", Later(M1b)),
        ("auth/elevate", Until("T038")),
        ("auth/elevation_get", Until("T038")),
        ("auth/elevation_respond", Until("T038")),
        ("blob/get", Until("T029")),
        ("blob/put", Until("T029")),
        ("diag/export", Later(M2)),
        // Builds `test-harness` and `dev-auth` only.
        ("diag/recording_export", Until("T116")),
        ("files/search", Later(M1b)),
        ("git/branches", Later(M1b)),
        ("git/commit", Later(M1b)),
        ("git/diff", Later(M1b)),
        ("git/file_diff", Later(M1b)),
        ("git/push", Later(M1b)),
        ("git/revert", Later(M1b)),
        ("git/stage", Later(M1b)),
        ("git/status", Later(M1b)),
        ("git/unstage", Later(M1b)),
        ("host/prepare_upgrade", Until("T183")),
        ("project/remove", Later(M1b)),
        ("session/archive", Later(M1b)),
        ("session/confirm_config", Until("T044")),
        ("session/list_archived", Later(M1b)),
        ("session/queue_edit", Later(M1b)),
        ("session/rename", Later(M1b)),
        ("session/restore", Later(M1b)),
        ("session/set_config", Later(M1b)),
        ("settings/get", Until("T036")),
        ("settings/update", Until("T036")),
        ("stream/ack", Later(M3)),
        // Build `test-harness` only.
        ("test/clock_advance", Builds),
        ("test/run_maintenance", Builds),
        ("test/seed", Builds),
        ("workspace/open", Later(M1b)),
        ("worktree/check", Later(M1b)),
    ]
};

/// Valid params of each client notification.
fn notification_params(name: &str) -> Value {
    match name {
        "client/presence" => json!({"foreground": true}),
        "stream/ack" => json!({"stream": "index", "seq": 1}),
        _ => panic!("no params for {name}"),
    }
}

#[tokio::test]
async fn unit_unsupported_set_is_what_the_milestones_allow() {
    let fixture = Fixture::new();
    let routes = fixture.start();
    let catalog = catalog(&mut generator());
    // Every scope, so that only the table decides.
    let mut peer = Peer::new(harness());
    let mut unsupported = Vec::new();
    for name in catalog.requests.keys() {
        // The handshake's, before anything is dispatched.
        if *name == "initialize" {
            continue;
        }
        let id = peer.send(&routes, name, json!({}));
        for call in [&fixture.sessions, &fixture.index].map(|stub| stub.take()) {
            call.into_iter().for_each(|d| d.answer(Ok(json!({}))));
        }
        if peer
            .reply(id)
            .await
            .is_err_and(|e| code(&e).0 == "unsupported")
        {
            unsupported.push(*name);
        }
    }
    for name in catalog.client_notifications.keys() {
        peer.dispatch
            .notification(&routes, name, notification_params(name));
        if fixture.index.take().is_empty() && fixture.sessions.take().is_empty() {
            unsupported.push(*name);
        }
    }
    unsupported.sort_unstable();
    let harness = cfg!(feature = "test-harness");
    let allowed = ALLOWED
        .iter()
        .filter(|(_, why)| !(harness && *why == Why::Builds));
    let allowed: Vec<&str> = allowed.map(|(name, _)| *name).collect();
    assert_eq!(unsupported, allowed);

    // Each reason holds: the milestone is the method's, and an executor
    // still to come is for M1a or M1b.
    let entries = catalog.requests.iter().chain(&catalog.client_notifications);
    let milestones: BTreeMap<&str, &str> = entries
        .map(|(name, entry)| (*name, entry[X_MILESTONE].as_str().unwrap()))
        .collect();
    for (name, why) in ALLOWED {
        let milestone = milestones[name];
        match why {
            Why::Later(later) => assert_eq!(milestone, later.as_str(), "{name}"),
            Why::Until(_) | Why::Builds => {
                assert!(["M1a", "M1b"].contains(&milestone), "{name}")
            }
        }
    }
}

#[tokio::test]
async fn unit_unsupported_set_gated_methods_only_in_their_builds() {
    use cupper_protocol::methods::admin::DiagRecordingExport;
    // `test/clock_advance` as the fixture routes it, like the host.
    let fixture = Fixture::new();
    let mut routes = fixture.start();
    routes.add::<DiagRecordingExport>(Arc::clone(&fixture.index) as Arc<dyn Port>);
    let harness_build = cfg!(feature = "test-harness");
    let cases = [
        ("test/clock_advance", json!({"ms": 5}), harness_build),
        (
            "diag/recording_export",
            json!({"session_id": session(1)}),
            harness_build || cfg!(feature = "dev-auth"),
        ),
    ];
    let catalog = catalog(&mut generator());
    let mut peer = Peer::new(harness());
    for (name, params, served) in cases {
        assert!(catalog.requests[name].get(X_BUILDS).is_some(), "{name}");
        let id = peer.send(&routes, name, params);
        fixture
            .index
            .take()
            .into_iter()
            .for_each(|d| d.answer(Ok(json!({}))));
        match peer.reply(id).await {
            Ok(_) => assert!(served, "{name}"),
            Err(err) => assert_eq!((code(&err).0, served), ("unsupported", false), "{name}"),
        }
    }
}

#[tokio::test]
async fn unit_policy_matrix_gateway_refuses_before_anything_runs() {
    use Scope::*;
    let fixture = Fixture::new();
    let mut routes = fixture.start();
    let to = || Arc::clone(&fixture.sessions) as Arc<dyn Port>;
    routes.add::<SettingsUpdate>(to());
    routes.add::<SessionSetConfig>(to());
    routes.add::<GitRevert>(to());
    let cli = principal("pr_cli", PrincipalKind::Cli, None, &[Read]);
    let remote = [Read, Write, Approve];
    let remote = principal("pr_dev_1", PrincipalKind::RemoteDevice, None, &remote);
    let revert = json!({"command_id": "c-1", "session_id": session(1), "diff_token": "dt_1",
        "target": {"all": true}});
    let verbose = json!({"command_id": "c-1", "patch": {"diagnostics.verbose": true}});
    let plan = json!({"command_id": "c-1", "session_id": session(1), "option": "plan_mode",
        "value": "on"});
    let describe = json!({"method": "ping", "params": {}});
    let cases = [
        // The method's scope.
        (cli, "session/send", send(1, "c-1"), "forbidden/scope/write"),
        (
            window(),
            "policy/describe",
            describe,
            "forbidden/scope/privileged",
        ),
        // The rules that read only the params.
        (
            window(),
            "settings/update",
            verbose,
            "forbidden/scope/privileged",
        ),
        (remote, "session/set_config", plan, "forbidden/role"),
        (
            window(),
            "git/revert",
            revert,
            "forbidden/user_activation_required",
        ),
        // Params that do not parse.
        (
            window(),
            "session/send",
            json!({"command_id": "c-1"}),
            "-32602",
        ),
    ];
    for (principal, method, params, expected) in cases {
        let mut peer = Peer::new(principal);
        let err = peer.call(&routes, method, params).await.unwrap_err();
        assert_eq!(denial(&err), expected, "{method}: {err}");
    }
    assert!(fixture.sessions.take().is_empty(), "nothing ran");

    // What the rules allow runs.
    let mut peer = Peer::new(window());
    let ordinary = json!({"command_id": "c-2", "patch": {"worktrees.root": "/w"}});
    peer.send(&routes, "settings/update", ordinary);
    assert_eq!(seen(&fixture.sessions.take())[0].0, "settings/update");

    // A notification has no reply: a refused one is dropped.
    let presence = json!({"foreground": true, "session_id": session(1)});
    for (scopes, delivered) in [(&[][..], 0), (&[Read][..], 1)] {
        let peer = Peer::new(principal("pr_cli", PrincipalKind::Cli, None, scopes));
        peer.dispatch
            .notification(&routes, "client/presence", presence.clone());
        peer.dispatch
            .notification(&routes, "client/presence", json!({}));
        peer.dispatch
            .notification(&routes, "client/nope", presence.clone());
        let calls = fixture.index.take();
        assert_eq!(calls.len(), delivered);
        if let Some(call) = calls.first() {
            assert_eq!(
                (call.call.method, &call.call.params),
                ("client/presence", &presence)
            );
            assert!(call.call.ticket.is_none());
        }
    }
}

/// `code/reason/scope` of an error's data; without data, the JSON-RPC code.
fn denial(err: &Value) -> String {
    let data = &err["data"];
    if data.is_null() {
        return err["code"].to_string();
    }
    let fields = ["code", "reason", "scope"].map(|key| data[key].as_str());
    fields.into_iter().flatten().collect::<Vec<_>>().join("/")
}

#[tokio::test]
async fn unit_policy_matrix_gateway_describe_reads_the_session_ports_facts() {
    let fixture = Fixture::new();
    let routes = fixture.start();
    let mut peer = Peer::new(control());
    let describe = |method: &str, params: Value| json!({"method": method, "params": params});
    let exit = describe("host/exit", json!({"command_id": "c-1", "force": true}));
    assert_eq!(
        peer.call(&routes, "policy/describe", exit).await,
        Ok(json!({"action": "interrupt_turns", "details": {"running_turns": 2}}))
    );
    let digest = format!("d_{}", "a".repeat(64));
    let trust = json!({"command_id": "c-1", "project_id": "p_1", "trust_digest": digest});
    let trust = peer
        .call(&routes, "policy/describe", describe("project/trust", trust))
        .await;
    assert_eq!(trust.unwrap()["action"], "trust_project");
    let plain = describe("settings/update", json!({"command_id": "c-1", "patch": {}}));
    let err = peer
        .call(&routes, "policy/describe", plain)
        .await
        .unwrap_err();
    assert_eq!(err["code"], INVALID_PARAMS);
}

/// Runs `session/send` command `command` of `peer` and answers it with
/// `outcome`, recorded first.
async fn run(fixture: &Fixture, routes: &Routes, peer: &mut Peer, command: &str, outcome: Value) {
    let id = peer.send(routes, "session/send", send(1, command));
    let Delivered { call, reply } = fixture.sessions.take().pop().unwrap();
    let completion = call.ticket.unwrap().complete(Ok(outcome.clone()));
    completion.commit().await.unwrap();
    let _ = reply.send(Ok(outcome.clone()));
    assert_eq!(peer.reply(id).await, Ok(outcome));
}

#[tokio::test]
async fn unit_ledger_gateway_replays_repeats_also_after_a_restart() {
    let fixture = Fixture::new();
    let routes = fixture.start();
    let mut peer = Peer::new(window());
    let queued = json!({"disposition": "queued", "queue_id": "q_1"});
    run(&fixture, &routes, &mut peer, "c-1", queued.clone()).await;
    let replayed = json!({"disposition": "queued", "queue_id": "q_1", "replayed": true});
    assert_eq!(
        peer.call(&routes, "session/send", send(1, "c-1")).await,
        Ok(replayed.clone())
    );

    // A host that starts again replays from the table, to the same
    // principal on another connection.
    let restarted = fixture.start();
    let mut again = Peer::new(control());
    let repeat = again.call(&restarted, "session/send", send(1, "c-1")).await;
    assert_eq!(repeat, Ok(replayed));
    assert!(fixture.sessions.take().is_empty(), "a repeat never runs");

    // Another method or other params under the same command_id.
    let other = again.call(&restarted, "session/send", send(2, "c-1")).await;
    assert_eq!(code(&other.unwrap_err()), ("idempotency_mismatch", ""));
    let interrupt = json!({"command_id": "c-1", "session_id": session(1)});
    let other = again.call(&restarted, "session/interrupt", interrupt).await;
    assert_eq!(code(&other.unwrap_err()), ("idempotency_mismatch", ""));
}

#[tokio::test]
async fn unit_ledger_gateway_repeat_waits_for_the_first_run() {
    let fixture = Fixture::new();
    let routes = fixture.start();
    let mut peer = Peer::new(window());
    let first = peer.send(&routes, "session/send", send(1, "c-1"));
    let repeat = peer.send(&routes, "session/send", send(1, "c-1"));
    let Delivered { call, reply } = fixture.sessions.take().pop().unwrap();
    assert!(peer.answered().await.is_empty(), "both wait");
    assert!(
        fixture.sessions.take().is_empty(),
        "the repeat is not delivered"
    );

    let started = json!({"disposition": "started", "turn_id": "t_1"});
    call.ticket
        .unwrap()
        .complete(Ok(started.clone()))
        .commit()
        .await
        .unwrap();
    let _ = reply.send(Ok(started.clone()));
    assert_eq!(peer.reply(first).await, Ok(started));
    let replayed = json!({"disposition": "started", "turn_id": "t_1", "replayed": true});
    assert_eq!(peer.reply(repeat).await, Ok(replayed));
}

#[tokio::test]
async fn unit_ledger_gateway_outcome_unknown_after_an_external_run_without_outcome() {
    let fixture = Fixture::new();
    let routes = fixture.start();
    let mut peer = Peer::new(window());
    let first = peer.send(&routes, "session/send", send(1, "c-1"));
    let Delivered { call, reply } = fixture.sessions.take().pop().unwrap();
    let mut ticket = call.ticket.unwrap();
    ticket.mark_external().await.unwrap();
    // The executor stops before the outcome is recorded.
    drop((ticket, reply));
    assert_eq!(
        code(&peer.reply(first).await.unwrap_err()),
        ("internal", "")
    );
    let repeat = peer.call(&routes, "session/send", send(1, "c-1")).await;
    assert_eq!(code(&repeat.unwrap_err()), ("conflict", "outcome_unknown"));
    assert!(fixture.sessions.take().is_empty());
}

#[tokio::test]
async fn it_command_order_through_the_read_loop_and_clients_open_and_close() {
    let fixture = Fixture::new();
    let config = TestGateway::config(None);
    let mut gw = TestGateway::with(config, Arc::new(SystemClock::new()), fixture.start());
    let mut client = Client::connect(&gw.socket).await;
    client.handshake(json!({"type": "peer"})).await.unwrap();
    assert_eq!(fixture.index.clients(), ["opened c_1 pr_cli"]);

    let request = |id: i64, method: &str, params: Value| json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
    let get = |n| json!({"session_id": session(n)});
    let presence = json!({"jsonrpc": "2.0", "method": "client/presence",
        "params": {"foreground": false}});
    for message in [
        request(1, "session/get", get(1)),
        request(2, "session/get", get(1)),
        request(3, "session/get", get(2)),
        presence,
        request(4, "session/send", send(1, "c-1")),
        request(5, "nope/nope", json!({})),
        json!({"jsonrpc": "2.0", "id": 6, "method": "ping"}),
    ] {
        client.send(&message).await;
    }
    // Answered at once, in order; the queries wait for their session.
    let id_and_code = |reply: Value| (reply["id"].clone(), code(&reply["error"]).0.to_owned());
    for (id, expected) in [(4, "forbidden"), (5, "unsupported"), (6, "")] {
        let reply = client.next().await.unwrap();
        assert_eq!(id_and_code(reply), (json!(id), expected.to_owned()));
    }
    let mut calls = fixture.sessions.take();
    let sessions: Vec<String> = seen(&calls).into_iter().map(|(_, s, _)| s).collect();
    assert_eq!(sessions, [1, 1, 2].map(|n| session(n).to_string()));
    let presence = fixture.index.take();
    assert_eq!(presence[0].call.params, json!({"foreground": false}));
    assert_eq!(presence[0].call.conn.as_str(), "c_1");

    for (call, id) in [(2, 3), (1, 2), (0, 1)] {
        calls.remove(call).answer(Ok(json!({"n": id})));
        let reply = client.next().await.unwrap();
        assert_eq!(
            (&reply["id"], &reply["result"]),
            (&json!(id), &json!({"n": id}))
        );
    }
    drop(client);
    assert_eq!(gw.closed().await, crate::gateway::CloseReason::PeerClosed);
    assert_eq!(fixture.index.clients(), ["opened c_1 pr_cli", "closed c_1"]);
}
