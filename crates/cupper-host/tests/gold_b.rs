//! GOLD-B (design 5.9.3): the host core in this process replays a recording
//! it drove itself (`source: host`), and the session's events, normalized
//! ([`normalize`]), must equal the golden: the same path under
//! `testdata/golden/gold-b/`. `UPDATE_GOLDEN=1` writes the goldens instead
//! (`cargo xtask ci` refuses to run with it set).
//!
//! The test plays the adapter over an in-memory transport
//! (`test_config.in_process`) and the user over the CLI library's client:
//!
//! - Each host message must match the next `send` frame by the replay's rules
//!   (T067: ids remapped, `cwd` and `path` as placeholders, `_meta` ignored).
//!   The frames after it go out once the virtual clock, which only the test
//!   moves (`test/clock_advance`), reaches their recorded `t` from that match.
//! - When the host has not sent the next frame by its recorded time, the
//!   test acts as the user: `session/create` (with the model and effort the
//!   host passed on) or `session/send` with the next prompt's text,
//!   `interaction/respond` with the recorded option,
//!   `session/interrupt` for `session/cancel`. Nothing else is a user's.
//! - The host handles each frame the adapter writes before the next one goes
//!   out ([`Run::sync`]): frames recorded at the same `t` would otherwise
//!   race inside the host (an attach's answer and the first update take
//!   different ways to the session) and the goldens would not be stable.

mod common;
#[path = "gold_b/normalize.rs"]
mod normalize;

use std::collections::VecDeque;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use cupper_cli::client::{self, Client};
use cupper_fake_agent::replay::Mapping;
use cupper_host::clock::{Clock, SystemClock};
use cupper_host::driver::runtime::launch::{InProcess, InProcessAgent};
use cupper_host::gateway::testkit::{KEY, TempDir};
use cupper_host::startup::payload::Payload;
use cupper_host::supervisor::{Host, Start};
use cupper_protocol::envelope::Event;
use cupper_protocol::event::{EventBody, Strictness};
use cupper_protocol::ids::ProjectId;
use cupper_protocol::methods::handshake;
use cupper_protocol::methods::interaction::InteractionRespond;
use cupper_protocol::methods::session::{SessionCreate, SessionInterrupt, SessionSend};
use cupper_protocol::recording::{Body, Frame, INSTANCE_EXITED, Recording, Source, Usage};
use cupper_protocol::rpc::{Idempotency, Method, MethodError, Milestone, Scope};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines};
use tokio::io::{ReadHalf, WriteHalf};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use common::*;
use normalize::{Normalizer, first_difference};

fn testdata() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata")
}

/// The files under `dir`, sorted.
fn files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for path in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = path.path();
        match path.is_dir() {
            true => found.extend(files(&path)),
            false => found.push(path),
        }
    }
    found.sort();
    found
}

/// The host-driven recordings for GOLD-B, by path under `testdata/recordings/`.
fn recordings() -> Vec<(PathBuf, Recording)> {
    let base = testdata().join("recordings");
    let parsed = files(&base).into_iter().map(|path| {
        let recording = Recording::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
        (path.strip_prefix(&base).unwrap().to_owned(), recording)
    });
    let header =
        |r: &Recording| r.header.source == Source::Host && r.header.usage.contains(&Usage::GoldB);
    parsed.filter(|(_, r)| header(r)).collect()
}

/// Fails the test when `fut` takes longer than [`PATIENCE`].
async fn within<T>(what: &str, fut: impl Future<Output = T>) -> T {
    let clock = SystemClock::new();
    tokio::select! {
        out = fut => out,
        () = clock.timer_at(clock.now_mono() + PATIENCE) => panic!("not in time: {what}"),
    }
}

/// The adapter instance the test plays.
struct Player {
    lines: Lines<BufReader<ReadHalf<DuplexStream>>>,
    write: WriteHalf<DuplexStream>,
    exit: oneshot::Sender<i32>,
    /// Host messages read ahead of their frames.
    pending: VecDeque<Value>,
}

impl Player {
    fn new(agent: InProcessAgent) -> Self {
        let (read, write) = tokio::io::split(agent.stream);
        let (lines, exit) = (BufReader::new(read).lines(), agent.exit);
        let pending = VecDeque::new();
        Self {
            lines,
            write,
            exit,
            pending,
        }
    }

    async fn write(&mut self, line: &str) {
        self.write
            .write_all(format!("{line}\n").as_bytes())
            .await
            .unwrap();
    }

    async fn read(&mut self) -> Value {
        let line = within("a line from the host", self.lines.next_line()).await;
        serde_json::from_str(&line.unwrap().expect("the host keeps stdin open")).unwrap()
    }

    /// Answers the host's requests until it closes stdin, then exits 0:
    /// what an adapter does as the host stops.
    fn finish(mut self) {
        tokio::spawn(async move {
            while let Ok(Some(line)) = self.lines.next_line().await {
                let message: Value = serde_json::from_str(&line).unwrap();
                if message.get("method").is_some() && message.get("id").is_some() {
                    let reply = json!({"jsonrpc": "2.0", "id": message["id"], "result": {}});
                    self.write(&reply.to_string()).await;
                }
            }
            drop(self.exit);
        });
    }
}

/// The host in this process, the user's client and the adapter's player.
struct Run {
    data_dir: PathBuf,
    client: Client,
    host: JoinHandle<cupper_host::exit::HostExit>,
    agents: mpsc::UnboundedReceiver<InProcessAgent>,
    player: Option<Player>,
    mapping: Mapping,
    /// The virtual clock, in ms from the start: only the test moves it.
    now: u64,
    syncs: u64,
    commands: Vec<JoinHandle<Result<(), client::ClientError>>>,
}

impl Run {
    async fn start(dir: &TempDir) -> Self {
        let data_dir = fake_agent_data(dir);
        let (agents_in, agents) = mpsc::unbounded_channel();
        let mut payload: Payload = Payload::default();
        let config = json!({"clock": "manual", "login_env": {}});
        payload.test_config = serde_json::from_value(config).unwrap();
        payload.test_config.in_process = Some(InProcess(agents_in));
        payload.harness_key = Some(serde_json::from_value(json!(proof_hex())).unwrap());
        let start = Start {
            data_dir: data_dir.clone(),
            payload,
            #[cfg(feature = "dev-auth")]
            launcher: 0,
        };
        let host = Host::start(start).await.unwrap();
        let host = tokio::spawn(host.run());
        let client = connect(&data_dir).await;
        Self {
            data_dir,
            client,
            host,
            agents,
            player: None,
            mapping: Mapping::default(),
            now: 0,
            syncs: 0,
            commands: Vec::new(),
        }
    }

    /// Makes sure that the host has handled everything the adapter wrote, and
    /// that the test has read everything the host wrote to it, as of now.
    ///
    /// The barrier does not cover the SDK's own tasks between the transport
    /// and the runtime's mailbox (design 5.2.16), so the test asks the host
    /// something it does not know. SDK 3.3.0's `incoming_protocol_actor`
    /// hands each message on before it takes the next, and its answer goes
    /// out after whatever the host queued before: once the `method not
    /// found` is back, the adapter's lines are in the host's mailboxes and
    /// the host's earlier lines are read. Then the barrier settles. A newer
    /// SDK that dispatches otherwise fails
    /// `gold_b_fake_sdk_answers_an_unknown_request_after_what_came_before`
    /// first.
    async fn sync(&mut self) {
        self.ask_unknown().await;
        advance(&self.client, 0).await;
        self.ask_unknown().await;
    }

    async fn ask_unknown(&mut self) {
        let Some(player) = &mut self.player else {
            return;
        };
        self.syncs += 1;
        let id = format!("gold-b-sync-{}", self.syncs);
        let ask = json!({"jsonrpc": "2.0", "id": id, "method": "_cupper/gold_b_sync"});
        player.write(&ask.to_string()).await;
        loop {
            let message = player.read().await;
            if message["id"] == id.as_str() {
                assert_eq!(message["error"]["code"], -32601, "{message}");
                return;
            }
            player.pending.push_back(message);
        }
    }

    /// Moves the clock to `at` (ms from the start), if it is ahead.
    async fn until(&mut self, at: u64) {
        if at > self.now {
            self.sync().await;
            advance(&self.client, at - self.now).await;
            self.now = at;
        }
    }

    /// Plays the recording's segments, one adapter instance each.
    async fn replay(&mut self, recording: &Recording) {
        for frames in recording.segments() {
            let mut anchor = (self.now, frames[0].t);
            for (at, frame) in frames.iter().enumerate() {
                let due = anchor.0 + frame.t.saturating_sub(anchor.1);
                match &frame.body {
                    Body::Send(expected) => {
                        self.expect(frames, at, expected, due).await;
                        anchor = (self.now, frame.t);
                    }
                    Body::Recv(message) => {
                        self.until(due).await;
                        let line = self.mapping.outgoing(frame.seq, message);
                        let line = line.unwrap_or_else(|err| panic!("{err}")).to_string();
                        self.player.as_mut().expect("an adapter").write(&line).await;
                        self.sync().await;
                    }
                    Body::RecvNonjson(line) => {
                        self.until(due).await;
                        self.player.as_mut().expect("an adapter").write(line).await;
                        self.sync().await;
                    }
                    Body::Note(note) if frame.body.event() == Some(INSTANCE_EXITED) => {
                        self.until(due).await;
                        let code = note.get("code").and_then(Value::as_i64).unwrap_or(0);
                        let player = self.player.take().expect("an adapter");
                        drop(player.write);
                        let _ = player.exit.send(i32::try_from(code).unwrap());
                    }
                    Body::Stderr(_) | Body::Note(_) => {}
                }
            }
        }
    }

    /// Waits for the host to send `frames[at]`: by itself, once the clock
    /// is at its time `due`, or once the user acted.
    async fn expect(&mut self, frames: &[Frame], at: usize, expected: &Value, due: u64) {
        let seq = frames[at].seq;
        let (mut waited, mut acted) = (false, None);
        loop {
            if self.player.is_none()
                && let Ok(agent) = self.agents.try_recv()
            {
                self.player = Some(Player::new(agent));
            }
            if let Some(message) = self.player.as_mut().and_then(|p| p.pending.pop_front()) {
                let checked = self.mapping.check(expected, &message);
                return checked.unwrap_or_else(|diff| panic!("frame {seq}: {diff}"));
            }
            self.sync().await;
            let pending = self.player.as_ref().is_some_and(|p| !p.pending.is_empty());
            if pending || (self.player.is_none() && !self.agents.is_empty()) {
                continue;
            }
            if !waited {
                waited = true;
                self.until(due).await;
                continue;
            }
            let clock = SystemClock::new();
            match acted {
                None => {
                    self.act(frames, at, expected).await;
                    acted = Some(clock.now_mono() + PATIENCE);
                }
                Some(give_up) => assert!(
                    clock.now_mono() < give_up,
                    "frame {seq}: the host does not send {}",
                    expected.get("method").unwrap_or(&json!("its answer"))
                ),
            }
            wait(&clock).await;
        }
    }

    /// What the user does so that the host sends `expected`.
    async fn act(&mut self, frames: &[Frame], at: usize, expected: &Value) {
        let session = column(&self.data_dir, "SELECT session_id FROM sessions");
        let session = session.first().cloned();
        let id = client::command_id();
        let user = connect(&self.data_dir).await;
        let command = match expected["method"].as_str() {
            Some("session/cancel") => {
                let params = json!({"command_id": id, "session_id": session});
                spawn_as::<SessionInterrupt>(user, params)
            }
            None => {
                let session = session.expect("a session").parse().unwrap();
                let n = column(
                    &self.data_dir,
                    "SELECT CAST(count(*) AS TEXT) FROM interactions",
                );
                let asked = events(&self.data_dir, &session, "interaction.requested", 1).await;
                assert_eq!(asked.len().to_string(), n[0]);
                let option = &expected["result"]["outcome"]["optionId"];
                let outcome = json!({"type": "selected", "option_id": option});
                let asked = asked.last().unwrap();
                let params = json!({"command_id": id, "session_id": session,
                    "request_id": asked["request_id"], "request_digest": asked["digest"],
                    "outcome": outcome});
                spawn_as::<InteractionRespond>(user, params)
            }
            Some(_) => {
                let text = next_prompt(&frames[at..]);
                let message = json!([{"type": "text", "text": text}]);
                match session {
                    None => {
                        let params = json!({"command_id": id, "agent": "claude",
                            "project_id": ProjectId::from_uuid(1),
                            "workspace": {"kind": "local"}, "message": message,
                            "config": config_of(frames)});
                        spawn_as::<SessionCreate>(user, params)
                    }
                    Some(session) => {
                        let params = json!({"command_id": id, "session_id": session,
                            "mode": "queue", "content": message});
                        spawn_as::<SessionSend>(user, params)
                    }
                }
            }
        };
        self.commands.push(command);
    }

    /// The session's events, decoded strictly: a known type with data the
    /// protocol does not allow fails the test.
    async fn events(mut self) -> Vec<Value> {
        self.sync().await;
        advance(&self.client, 1_000).await;
        let session = column(&self.data_dir, "SELECT session_id FROM sessions");
        let sql = format!(
            "SELECT json FROM events WHERE stream = 'session/{}' ORDER BY seq",
            session[0]
        );
        let rows = column(&self.data_dir, &sql);
        let events: Vec<Value> = rows
            .iter()
            .map(|r| serde_json::from_str(r).unwrap())
            .collect();
        for (n, event) in events.iter().enumerate() {
            let decoded = Event::<EventBody>::decode(event, Strictness::Strict);
            decoded.unwrap_or_else(|err| panic!("event {}: {err}: {event}", n + 1));
        }
        for command in std::mem::take(&mut self.commands) {
            within("a command's reply", command).await.unwrap().unwrap();
        }
        self.stop().await;
        events
    }

    async fn stop(mut self) {
        if let Some(player) = self.player.take() {
            player.finish();
        }
        let params = json!({"command_id": client::command_id(), "force": true});
        let exit = self.client.request::<Raw<handshake::HostExit>>(params);
        exit.await.unwrap();
        let exited = within("the host's exit", self.host).await.unwrap();
        assert_eq!(exited, cupper_host::exit::HostExit::Ok);
    }
}

/// `M` with its params and result as JSON.
struct Raw<M>(PhantomData<M>);

impl<M: Method> Method for Raw<M> {
    const NAME: &'static str = M::NAME;
    const SCOPE: Option<Scope> = M::SCOPE;
    const IDEMPOTENCY: Idempotency = M::IDEMPOTENCY;
    const MILESTONE: Milestone = M::MILESTONE;
    const ERRORS: &'static [MethodError] = M::ERRORS;
    type Params = Value;
    type Result = Value;
}

/// Sends `M` over `client` in a task of its own: its reply may wait for
/// what the test still has to play.
fn spawn_as<M: Method + Send + 'static>(
    client: Client,
    params: Value,
) -> JoinHandle<Result<(), client::ClientError>> {
    tokio::spawn(async move { client.request::<Raw<M>>(params).await.map(drop) })
}

/// What the session was created with, as the host passed it on in `frames`:
/// a config option's first value (`xtask compat` gives a real run's model
/// and effort).
fn config_of(frames: &[Frame]) -> Value {
    let mut config = serde_json::Map::new();
    for frame in frames {
        let Body::Send(message) = &frame.body else {
            continue;
        };
        let params = &message["params"];
        let field = match params["configId"].as_str() {
            _ if message["method"] != "session/set_config_option" => continue,
            Some("model") => "model",
            Some("effort") => "reasoning_effort",
            _ => continue,
        };
        config.entry(field).or_insert(params["value"].clone());
    }
    Value::Object(config)
}

/// The text of the next `session/prompt` from `frames` on.
fn next_prompt(frames: &[Frame]) -> String {
    let prompt = frames.iter().find_map(|frame| match &frame.body {
        Body::Send(message) if message["method"] == "session/prompt" => Some(message),
        _ => None,
    });
    let blocks = prompt.expect("a prompt to send")["params"]["prompt"]
        .as_array()
        .unwrap();
    blocks.iter().filter_map(|b| b["text"].as_str()).collect()
}

fn proof_hex() -> String {
    cupper_protocol::proof::to_hex(&KEY)
}

/// `actual` against the golden `golden` of `name`: the first event and
/// field that differ.
fn compare(name: &Path, golden: &str, actual: &[String]) -> Result<(), String> {
    let lines = golden.lines().zip(actual).enumerate();
    for (n, (golden, actual)) in lines {
        if golden != actual {
            let parse = |line: &str| serde_json::from_str::<Value>(line).unwrap();
            let found = first_difference(&parse(golden), &parse(actual));
            let (field, golden, actual) = found.unwrap_or_default();
            return Err(format!(
                "{}: event {}: `{field}` is {actual}, the golden has {golden}",
                name.display(),
                n + 1
            ));
        }
    }
    let counts = (actual.len(), golden.lines().count());
    match counts.0 == counts.1 {
        true => Ok(()),
        false => Err(format!(
            "{}: {} events, the golden has {}",
            name.display(),
            counts.0,
            counts.1
        )),
    }
}

/// Replays the recording at `scenario`, its path under `testdata/recordings/`
/// without `.jsonl`, and compares, or writes, its golden.
async fn check(scenario: &str) {
    let found = recordings()
        .into_iter()
        .find(|(name, _)| name.with_extension("") == Path::new(scenario));
    let (name, recording) = found.unwrap_or_else(|| panic!("no recording of {scenario}"));
    let dir = TempDir::new("gold-b");
    let mut run = Run::start(&dir).await;
    run.replay(&recording).await;
    let lines = Normalizer::new().events(run.events().await);
    let path = testdata().join("golden/gold-b").join(&name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            lines.iter().map(|l| format!("{l}\n")).collect::<String>(),
        )
        .unwrap();
        return;
    }
    let golden = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("no golden for {name:?}; write it with UPDATE_GOLDEN=1"));
    if let Err(diff) = compare(&name, &golden, &lines) {
        panic!("{diff}");
    }
}

macro_rules! gold_b {
    ($($test:ident => $scenario:literal,)*) => {
        $(
            #[tokio::test(flavor = "multi_thread")]
            async fn $test() {
                check($scenario).await;
            }
        )*
        const SCENARIOS: &[&str] = &[$($scenario),*];
    };
}

gold_b! {
    gold_b_fake_normal_turn => "cupper-fake-agent/0.0.0/fake/normal-turn",
    gold_b_fake_approval => "cupper-fake-agent/0.0.0/fake/approval",
    gold_b_fake_cancel => "cupper-fake-agent/0.0.0/fake/cancel",
    gold_b_claude_normal_turn => "claude-agent-acp/0.91.0/2.1.296/normal-turn",
    gold_b_claude_approval => "claude-agent-acp/0.91.0/2.1.296/approval",
    gold_b_claude_cancel => "claude-agent-acp/0.91.0/2.1.296/cancel",
}

/// Every GOLD-B recording has a test above and a golden, and every golden
/// has its recording (design 5.9.3).
#[test]
fn gold_b_fake_recordings_and_goldens_match_up() {
    let recorded: Vec<PathBuf> = recordings().into_iter().map(|(name, _)| name).collect();
    let mut named: Vec<PathBuf> = SCENARIOS
        .iter()
        .map(|s| PathBuf::from(s).with_extension("jsonl"))
        .collect();
    named.sort();
    assert_eq!(recorded, named);
    let base = testdata().join("golden/gold-b");
    let goldens: Vec<PathBuf> = (files(&base).into_iter())
        .map(|path| path.strip_prefix(&base).unwrap().to_owned())
        .collect();
    assert_eq!(goldens, recorded);
}

/// A golden changed in one field of one event: the comparison names both.
#[test]
fn gold_b_fake_a_changed_golden_names_the_event_and_field() {
    let name = Path::new("cupper-fake-agent/0.0.0/fake/normal-turn.jsonl");
    let golden = std::fs::read_to_string(testdata().join("golden/gold-b").join(name)).unwrap();
    let actual: Vec<String> = golden.lines().map(str::to_owned).collect();
    let mut changed: Vec<Value> = actual
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let n = changed
        .iter()
        .position(|e| e["type"] == "turn.started")
        .unwrap();
    changed[n]["data"]["turn_no"] = json!(9);
    let changed: String = changed.iter().map(|e| format!("{e}\n")).collect();
    assert_eq!(compare(name, &golden, &actual), Ok(()));
    let diff = compare(name, &changed, &actual).unwrap_err();
    assert!(
        diff.contains(&format!(
            "event {}: `data.turn_no` is 1, the golden has 9",
            n + 1
        )),
        "{diff}"
    );
}
