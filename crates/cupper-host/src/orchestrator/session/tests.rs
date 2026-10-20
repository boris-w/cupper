//! IT-orch-lifecycle: session commands from the gateway's dispatch, through
//! the policy and the ledger, to the session actors and the store, with an
//! in-process agent, on `ManualClock`. The fixture serves IT-orch-interaction
//! too.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cupper_protocol::capabilities::SessionCapabilities;
use cupper_protocol::domain::{AgentKind, PrincipalKind, PromptContent};
use cupper_protocol::event::EventBody;
use cupper_protocol::ids::{AdapterInstanceId, ProjectId, SessionId, TurnId};
use cupper_protocol::index::PendingKind;
use cupper_protocol::limits::{MAX_SAFE_INTEGER, V1};
use cupper_protocol::methods::handshake::Principal;
use cupper_protocol::rpc::Scope;
use cupper_protocol::session::StopReason;
use cupper_protocol::sync::StreamId;
use futures::FutureExt;
use futures::future::BoxFuture;
use serde_json::{Value, json};
use tokio::sync::{Notify, mpsc, oneshot};

use super::{IndexNotes, SessionDeps, SessionManager};
use crate::clock::{Clock, IdGen, ManualClock};
use crate::driver::api::{
    AgentError, AgentPort, AgentSessionId, Attach, AttachInfo, AttachTarget, DriverCommand,
    DriverEvent, NormUpdate, ProjectConfigPolicy, PromptTag,
};
use crate::durations::Durations;
use crate::gateway::testkit::Peer;
use crate::gateway::{AuditRecord, Routes};
use crate::ledger::Ledger;
use crate::orchestrator::testkit::text;
use crate::quiescence::Quiescence;
use crate::store::streams::{Published, Push, Sink, Streams, StreamsConfig};
use crate::store::testutil::{TempDir, host_writer, query};
use crate::store::{Durability, Readers, WriterThread, log, open_reader};

type Attached = oneshot::Sender<Result<AttachInfo, AgentError>>;

/// What the session asked of its agent, in order.
#[derive(Debug)]
enum Seen {
    Attach(Attach, mpsc::UnboundedSender<DriverEvent>, Attached),
    Command(DriverCommand),
}

#[derive(Debug)]
struct Agent(mpsc::UnboundedSender<Seen>);

impl AgentPort for Agent {
    fn attach(
        &self,
        attach: Attach,
        events: mpsc::UnboundedSender<DriverEvent>,
    ) -> BoxFuture<'static, Result<AttachInfo, AgentError>> {
        let (reply, answer) = oneshot::channel();
        self.0.send(Seen::Attach(attach, events, reply)).unwrap();
        Box::pin(answer.map(Result::unwrap))
    }

    fn send(&self, command: DriverCommand) {
        self.0.send(Seen::Command(command)).unwrap();
    }
}

/// Wakes the test when the stream publishes.
#[derive(Debug, Default)]
struct Wake(Notify);

impl Sink for Wake {
    fn push(&self, _: &StreamId, _: &Arc<Published>) -> Push {
        self.0.notify_one();
        Push::Queued
    }

    fn reset(&self, _: &StreamId) {}
}

/// Claude's, with three permission modes; the host's cache has them too.
pub(super) fn caps() -> SessionCapabilities {
    let mode = |value, risk| json!({"value": value, "name": value, "risk": risk});
    serde_json::from_value(json!({
        "agent": "claude",
        "versions": {"adapter": "1", "cli": "1", "cli_compat": "verified"},
        "prompt": {"image": true, "file_ref": true, "max_image_bytes": 1},
        "turn": {"queue": true, "steer": {"supported": false, "side_effects": false},
                 "interrupt": true, "interrupt_stops_commands": true},
        "session": {"resume": true, "load_history": false, "fork": false},
        "config": [{"id": "permission_mode", "agent_option_id": "mode", "current": "default",
            "values": [mode("default", "normal"), mode("auto", "elevated"),
                       mode("bypass", "full_access")]}],
        "commands": [],
        "interactions": {"reject_ends_turn": false, "question_forms": true, "plan_confirm": true},
        "extraction": {"exit_code": true, "output_streaming": false, "diff": "per_file",
                       "thinking": true, "plan": "markdown_only", "run_state": "inferred"}
    }))
    .unwrap()
}

pub(super) fn principal(id: &str, kind: PrincipalKind, scopes: &[Scope]) -> Principal {
    let value = json!({"id": id, "kind": kind, "scopes": scopes});
    serde_json::from_value(value).unwrap()
}

pub(super) fn harness() -> Principal {
    use Scope::*;
    let scopes = [Read, Write, Approve, Privileged, UiLocal, Test];
    principal("pr_test", PrincipalKind::TestHarness, &scopes)
}

pub(super) fn create(command: &str) -> Value {
    json!({"command_id": command, "agent": "claude", "project_id": ProjectId::from_uuid(1),
        "workspace": {"kind": "local"}, "message": text("hi")})
}

pub(super) fn send(session: &SessionId, command: &str, message: &str) -> Value {
    json!({"command_id": command, "session_id": session, "mode": "queue",
        "content": text(message)})
}

pub(super) fn on(session: &SessionId, command: &str) -> Value {
    json!({"command_id": command, "session_id": session})
}

/// `data.code` and `data.reason` of an error, space-separated.
pub(super) fn code(error: Value) -> String {
    let field = |name| error["data"][name].as_str().unwrap_or("").to_owned();
    format!("{} {}", field("code"), field("reason"))
}

/// The event types, space-separated.
pub(super) fn tags(bodies: &[EventBody]) -> String {
    let tags: Vec<&str> = bodies.iter().map(EventBody::tag).collect();
    tags.join(" ")
}

pub(super) fn done(tag: PromptTag, stop_reason: StopReason) -> DriverEvent {
    let usage = None;
    DriverEvent::PromptDone {
        tag,
        stop_reason,
        usage,
    }
}

/// The data of the session's events of type `tag`, as JSON.
pub(super) fn data(bodies: &[EventBody], tag: &str) -> Vec<Value> {
    let of = bodies.iter().filter(|b| b.tag() == tag);
    of.map(|b| json!(b)["data"].clone()).collect()
}

/// The index's notes: each session's pending count, in order.
#[derive(Debug, Default)]
pub(super) struct Index(pub Mutex<Vec<u32>>);

impl IndexNotes for Index {
    fn pending(&self, _: &SessionId, count: u32, _: Option<PendingKind>) {
        self.0.lock().unwrap().push(count);
    }
}

pub(super) struct Fixture {
    dir: TempDir,
    pub clock: ManualClock,
    routes: Routes,
    streams: Streams,
    seen: mpsc::UnboundedReceiver<Seen>,
    /// The attached session's events.
    events: Option<mpsc::UnboundedSender<DriverEvent>>,
    pub audit: mpsc::UnboundedReceiver<AuditRecord>,
    pub index: Arc<Index>,
    _writer: WriterThread,
}

impl Fixture {
    /// The host's sessions on a fresh `host.db` with a trusted project and
    /// Claude's capabilities cached.
    pub async fn new(name: &str) -> Self {
        let (dir, clock) = (TempDir::new(name), ManualClock::new());
        let (writer, thread) = host_writer(&dir, &clock);
        let project = ProjectId::from_uuid(1).to_string();
        let cached = json!(caps()).to_string();
        let seed = move |conn: &rusqlite::Connection| {
            conn.execute(
                "INSERT INTO projects (project_id, path, name, position, added_at, trust_state)
                 VALUES (?1, '/work/app', 'app', 0, 0, 'trusted')",
                [project],
            )?;
            conn.execute(
                "INSERT INTO agent_capabilities (agent, adapter_version, captured_at, json)
                 VALUES ('claude', '1', 0, ?1)",
                [cached],
            )
        };
        writer.submit(Durability::Normal, seed).await.unwrap();
        let shared: Arc<dyn Clock> = Arc::new(clock.clone());
        let readers = Readers::open(&dir.host_db()).unwrap();
        let durations = Durations::default();
        let config = StreamsConfig::new(1, &durations);
        let streams = Streams::new(writer.clone(), readers.clone(), Arc::clone(&shared), config);
        let (agent, seen) = mpsc::unbounded_channel();
        let agent = Arc::new(Agent(agent)) as Arc<dyn AgentPort>;
        let ((records, audit), index) = (mpsc::unbounded_channel(), Arc::new(Index::default()));
        let deps = SessionDeps {
            streams: streams.clone(),
            writer: writer.clone(),
            readers,
            clock: Arc::clone(&shared),
            ids: Arc::new(IdGen::seeded(Arc::clone(&shared), 1)),
            durations: durations.clone(),
            agents: HashMap::from([(AgentKind::Claude, agent)]),
            quiescence: Quiescence::new(),
            audit: records,
            index: Arc::clone(&index) as Arc<dyn IndexNotes>,
        };
        let conn = open_reader(&dir.host_db()).unwrap();
        let mut routes = Routes::new(Ledger::load(&conn, writer, shared, &durations).unwrap());
        routes.sessions(Arc::new(SessionManager::new(deps)));
        let (events, _writer) = (None, thread);
        Self {
            dir,
            clock,
            routes,
            streams,
            seen,
            events,
            audit,
            index,
            _writer,
        }
    }

    pub async fn call(&self, by: Principal, method: &str, params: Value) -> Result<Value, Value> {
        Peer::new(by).call(&self.routes, method, params).await
    }

    /// The result of a request from the test harness, which holds every scope.
    pub async fn ok(&self, method: &str, params: Value) -> Value {
        self.call(harness(), method, params).await.unwrap()
    }

    pub async fn err(&self, method: &str, params: Value) -> String {
        code(self.call(harness(), method, params).await.unwrap_err())
    }

    /// Answers the next attach with the agent's session `agent-1`.
    pub async fn attach(&mut self) -> Attach {
        let Some(Seen::Attach(attach, events, reply)) = self.seen.recv().await else {
            panic!("an attach")
        };
        let session = AgentSessionId("agent-1".into());
        let (instance, capabilities) = (AdapterInstanceId::from_uuid(1), caps());
        reply
            .send(Ok(AttachInfo {
                session,
                instance,
                capabilities,
            }))
            .unwrap();
        self.events = Some(events);
        attach
    }

    pub async fn command(&mut self) -> DriverCommand {
        match self.seen.recv().await {
            Some(Seen::Command(command)) => command,
            other => panic!("{other:?}"),
        }
    }

    pub async fn prompt(&mut self) -> (PromptTag, PromptContent) {
        match self.command().await {
            DriverCommand::Prompt { tag, content, .. } => (tag, content),
            other => panic!("{other:?}"),
        }
    }

    pub fn emit(&self, event: DriverEvent) {
        self.events.as_ref().unwrap().send(event).unwrap();
    }

    /// A session with the message `hi`, its agent attached and its prompt sent.
    pub async fn started(&mut self) -> SessionId {
        let created = self.ok("session/create", create("c-1")).await;
        self.attach().await;
        self.prompt().await;
        serde_json::from_value(created["session_id"].clone()).unwrap()
    }

    /// The session's events in the log, once there are `n` of type `tag`.
    pub async fn events(&self, session: &SessionId, tag: &str, n: usize) -> Vec<EventBody> {
        let stream = StreamId::Session(session.clone());
        let handle = self.streams.open(&stream).await.unwrap();
        let wake = Arc::new(Wake::default());
        let _subscription = handle.subscribe(Arc::clone(&wake) as Arc<dyn Sink>, None, |_| {});
        loop {
            let conn = open_reader(&self.dir.host_db()).unwrap();
            let mut bodies = Vec::new();
            let all = |e: cupper_protocol::envelope::Event<_>| bodies.push(e.body);
            log::read_events(&conn, &stream, 0, MAX_SAFE_INTEGER, all).unwrap();
            if bodies.iter().filter(|b| b.tag() == tag).count() >= n {
                return bodies;
            }
            wake.0.notified().await;
        }
    }

    /// One text column of `sql`'s rows.
    pub fn rows(&self, sql: &str) -> Vec<String> {
        query(&self.dir.host_db(), sql, |row| row.get(0))
    }
}

#[tokio::test]
async fn it_orch_lifecycle_create_runs_the_first_message_as_a_turn() {
    let mut f = Fixture::new("create").await;
    let created = f.ok("session/create", create("c-1")).await;
    let again = f.ok("session/create", create("c-1")).await;
    let replayed = json!({"session_id": created["session_id"], "replayed": true});
    assert_eq!(again, replayed);
    let session: SessionId = serde_json::from_value(created["session_id"].clone()).unwrap();
    let attach = f.attach().await;
    assert_eq!(attach.target, AttachTarget::New);
    assert_eq!(attach.cwd.to_str(), Some("/work/app"));
    assert_eq!(attach.project_config, ProjectConfigPolicy::Load);
    let (tag, content) = f.prompt().await;
    assert_eq!(content, text("hi"));
    let (message_id, text) = (Some("m1".into()), "hello".into());
    let update = NormUpdate::AgentText { message_id, text };
    f.emit(DriverEvent::Update(update));
    f.emit(done(tag, StopReason::EndTurn));

    let bodies = f.events(&session, "turn.completed", 1).await;
    let expected = "session.configured workspace.ready turn.started item.started item.completed \
        snapshot.recorded session.capabilities item.started item.delta item.completed \
        snapshot.recorded turn.completed";
    assert_eq!(tags(&bodies), expected);
    let snapshots = data(&bodies, "snapshot.recorded");
    let skipped = |s: &Value| s["status"] == "skipped" && s["reason"].is_null();
    assert!(snapshots.iter().all(skipped));
    let completed = &data(&bodies, "turn.completed")[0];
    assert_eq!(completed["status"], "completed");
    assert_eq!(
        completed["final_item_ids"].as_array().map(Vec::len),
        Some(1)
    );

    // The rows, the turn's snapshot and the checkpoint commit with the events.
    let sessions = "SELECT state || ' ' || (initial_prompt_json IS NULL) || ' ' || agent_session_id \
        || ' ' || title || ' ' || last_turn_end_seq FROM sessions";
    assert_eq!(f.rows(sessions), ["idle 1 agent-1 hi 12"]);
    let turns = "SELECT turn_no || ' ' || origin || ' ' || status || ' ' || snapshot_status \
        || ' ' || first_seq || '-' || last_seq FROM turns";
    assert_eq!(f.rows(turns), ["1 user completed skipped 3-12"]);
    let saved = "SELECT (SELECT count(*) FROM turn_snapshots) || ' ' || at_seq FROM checkpoints";
    assert_eq!(f.rows(saved), ["1 12"]);

    let got = f.ok("session/get", json!({"session_id": session})).await;
    let summary = &got["summary"];
    let shown = [
        &summary["status"],
        &summary["title"],
        &summary["title_source"],
    ];
    assert_eq!(shown, ["unread", "hi", "prompt"]);
    let workspace = json!({"kind": "local", "path": "/work/app"});
    assert_eq!(got["workspace"], workspace);
    let session = json!({"session_id": session});
    assert_eq!(f.ok("session/capabilities", session).await, json!(caps()));
}

#[tokio::test]
async fn it_orch_lifecycle_send_queues_while_a_turn_runs() {
    let mut f = Fixture::new("queue").await;
    let session = f.started().await;
    let sent = f.ok("session/send", send(&session, "s-1", "next")).await;
    assert_eq!(sent["disposition"], "queued");
    for n in 2..=V1.max_queued_messages {
        let more = send(&session, &format!("s-{n}"), "more");
        f.ok("session/send", more).await;
    }
    let full = send(&session, "s-full", "more");
    assert_eq!(f.err("session/send", full).await, "too_large queue_full");
    let head = "SELECT command_id || ' ' || content_json FROM queue_entries WHERE position = 0";
    let content = json!(text("next"));
    assert_eq!(f.rows(head), [format!("s-1 {content}")]);

    // The turn ends; the head of the queue starts the next one.
    f.emit(done(PromptTag(1), StopReason::EndTurn));
    assert_eq!(f.prompt().await, (PromptTag(2), text("next")));
    let bodies = f.events(&session, "turn.started", 2).await;
    let started = data(&bodies, "turn.started");
    let origins: Vec<&Value> = started.iter().map(|t| &t["origin"]).collect();
    assert_eq!(origins, ["user", "queued"]);
    let queued = data(&bodies, "queue.updated");
    assert_eq!(queued[0]["entries"][0]["content"], content);
    let message = data(&bodies, "item.started").pop().unwrap();
    assert_eq!(message["content"], content);
    assert!(message["queue_id"].is_string());
    let left = "SELECT count(*) || ' ' || max(position) FROM queue_entries";
    assert_eq!(f.rows(left), ["49 48"]);
}

#[tokio::test]
async fn it_orch_lifecycle_interrupt_ends_the_turn_and_pauses_the_queue() {
    // The agent returns `cancelled`, or the host gives up after 10 s.
    for deadline in [false, true] {
        let mut f = Fixture::new("interrupt").await;
        let session = f.started().await;
        f.ok("session/send", send(&session, "s-1", "next")).await;
        let mut other = on(&session, "i-0");
        other["expected_turn_id"] = json!(TurnId::from_uuid(9));
        let mismatch = f.err("session/interrupt", other).await;
        assert_eq!(mismatch, "conflict turn_mismatch");
        let interrupted = f.ok("session/interrupt", on(&session, "i-1")).await;
        assert!(interrupted["turn_id"].is_string());
        assert!(matches!(f.command().await, DriverCommand::Cancel { .. }));
        match deadline {
            true => f.clock.advance(Duration::from_secs(10)),
            false => f.emit(done(PromptTag(1), StopReason::Cancelled)),
        }
        let bodies = f.events(&session, "turn.completed", 1).await;
        let end = &data(&bodies, "turn.completed")[0];
        let reason = ["cancelled", "host_timeout"][usize::from(deadline)];
        assert_eq!([&end["status"], &end["stop_reason"]], ["cancelled", reason]);
        let queue = data(&bodies, "queue.updated").pop().unwrap();
        assert_eq!(queue["paused"]["reason"], "turn_cancelled");
        let paused = f.rows("SELECT queue_paused FROM sessions");
        assert_eq!(paused, ["turn_cancelled"]);

        // Resumed, the queue runs its message; a prompt the agent never
        // answered closes and resumes its session first (K1).
        let resumed = f.ok("session/queue_resume", on(&session, "r-1")).await;
        assert_eq!(resumed, json!({}));
        if deadline {
            assert!(matches!(f.command().await, DriverCommand::Close { .. }));
            let target = AttachTarget::Resume(AgentSessionId("agent-1".into()));
            assert_eq!(f.attach().await.target, target);
        }
        assert_eq!(f.prompt().await.1, text("next"));
    }
}

#[tokio::test]
async fn it_orch_lifecycle_create_checks_workspace_agent_project_and_tier() {
    use Scope::{Approve, Read, Write};
    let mut f = Fixture::new("checks").await;
    let cli = principal("pr_cli", PrincipalKind::Cli, &[Read, Write, Approve]);
    // A mode the mapping does not know is full access.
    let refused = [
        (
            "config",
            json!({"permission_mode": "bypass"}),
            "forbidden scope",
        ),
        (
            "config",
            json!({"permission_mode": "yolo"}),
            "forbidden scope",
        ),
        ("project_id", json!(ProjectId::from_uuid(9)), "not_found "),
        (
            "workspace",
            json!({"kind": "worktree", "base_ref": "main"}),
            "unsupported ",
        ),
        ("agent", json!("codex"), "unavailable "),
    ];
    for (n, (key, value, expected)) in refused.into_iter().enumerate() {
        let mut params = create(&format!("c-{n}"));
        params[key] = value;
        let refused = f.call(cli.clone(), "session/create", params).await;
        assert_eq!(code(refused.unwrap_err()), expected, "{n}");
    }
    let unknown = send(&SessionId::from_uuid(9), "s-1", "hi");
    assert_eq!(f.err("session/send", unknown).await, "not_found ");
    assert_eq!(f.rows("SELECT count(*) || '' FROM sessions"), ["0"]);

    // The main process describes a create in full access before it confirms.
    let mut bypass = create("c-bypass");
    bypass["config"] = json!({"permission_mode": "bypass"});
    let described = json!({"method": "session/create", "params": bypass});
    let description = f.ok("policy/describe", described).await;
    assert_eq!(description["action"], "full_access");

    // `approve` loosens to `elevated`; the attach applies the mode.
    let mut auto = create("c-auto");
    auto["config"] = json!({"permission_mode": "auto"});
    f.call(cli, "session/create", auto).await.unwrap();
    let config = f.attach().await.config;
    assert_eq!(config, [("permission_mode".into(), "auto".into())].into());
}
