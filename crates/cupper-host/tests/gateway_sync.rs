//! Gateway sync, history and notification intents on a host process
//! (design 5.9.1, 5.9.8): IT-gw-sync (SP-4a's end-to-end tests 1-7 and
//! adapter isolation), IT-NFR2-epoch, IT-history-from-host,
//! IT-notification-intent and IT-host-orphaned. The agent is the
//! claude-shaped fake agent; the tests talk to the host through the CLI
//! library's client and stream mirror, as the CLI's `harness` client.

mod common;

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use cupper_cli::client::{self, Client, ClientAuth, ClientError, Incoming, Options};
use cupper_cli::harness::HarnessKey;
use cupper_cli::mirror::{self, Mirror};
use cupper_host::clock::{Clock, SystemClock};
use cupper_host::gateway::testkit::{self, KEY, TempDir, initialize_params, key_auth};
use cupper_protocol::domain::{BlobContent, BlobRef, Content};
use cupper_protocol::event::Strictness;
use cupper_protocol::frame::BlobChunkHeader;
use cupper_protocol::ids::{BlobHash, CommandId, SessionId};
use cupper_protocol::item::Item;
use cupper_protocol::methods::handshake::{ClientPresence, ClientPresenceParams};
use cupper_protocol::methods::session::{SessionInterrupt, SessionQueueResume, SessionSend};
use cupper_protocol::methods::stream::{
    BlobGet, BlobGetParams, BlobPut, BlobPutParams, HistoryPage, HistoryPageParams,
    StreamSubscribeParams, StreamSubscribeResult,
};
use cupper_protocol::proof;
use cupper_protocol::rpc::{Idempotency, Method, MethodError, Milestone, Scope};
use cupper_protocol::session::TurnStatus;
use cupper_protocol::sync::{ResyncReason, StreamId, SubscribeMode};
use cupper_protocol::text::MAX_LABEL_CHARS;
use cupper_protocol::transport::socket_path;
use cupper_protocol::view::{IndexView, SessionView, View};
use cupper_sys::{ProcessGroup, Signal, StartTime};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use common::*;

/// A harness client that mirrors one session and the index, as a UI does.
struct Peer {
    client: Client,
    stream: StreamId,
    session: Mirror<SessionView>,
    index: Mirror<IndexView>,
    /// The host's answers to the subscriptions of the session.
    subscribed: Vec<StreamSubscribeResult>,
    /// Range events among the session's events.
    ranges: usize,
    /// Chunks of `blob/get`.
    blob: Vec<u8>,
}

impl Peer {
    async fn new(client: Client, session: &SessionId) -> Self {
        Self::with(client, Mirror::session(session.clone())).await
    }

    /// Subscribes `session` with its cursor, if it has one.
    async fn with(client: Client, session: Mirror<SessionView>) -> Self {
        let mut peer = Self {
            client,
            stream: session.stream().clone(),
            session,
            index: Mirror::index(),
            subscribed: Vec::new(),
            ranges: 0,
            blob: Vec::new(),
        };
        mirror::subscribe(&peer.client, &mut peer.session)
            .await
            .unwrap();
        mirror::subscribe(&peer.client, &mut peer.index)
            .await
            .unwrap();
        peer
    }

    fn view(&self) -> &SessionView {
        self.session.view()
    }

    /// Folds what the host sends until `done` holds.
    async fn until(&mut self, done: impl Fn(&Self) -> bool) {
        let clock = SystemClock::new();
        let mut give_up = clock.timer_at(clock.now_mono() + PATIENCE);
        while !done(self) {
            let incoming = tokio::select! {
                incoming = self.client.next() => incoming.expect("the connection is open"),
                () = &mut give_up => panic!("not done in time"),
            };
            self.take(&incoming).await;
        }
    }

    async fn take(&mut self, incoming: &Incoming) {
        match incoming {
            Incoming::Subscribed { stream, result } if *stream == self.stream => {
                self.subscribed.push(result.clone().unwrap());
            }
            Incoming::Events(events) if events.stream == self.stream => {
                let ranges = events.events.iter();
                self.ranges += ranges
                    .filter(|e| e.as_value().get("from").is_some())
                    .count();
            }
            Incoming::Blob { data, .. } => self.blob.extend_from_slice(data),
            _ => {}
        }
        let client = &self.client;
        mirror::fold(client, &mut self.session, incoming)
            .await
            .unwrap();
        mirror::fold(client, &mut self.index, incoming)
            .await
            .unwrap();
    }

    /// Waits until turn `n` has ended; its status.
    async fn ended(&mut self, n: u32) -> TurnStatus {
        self.until(|peer| status(peer.view(), n).is_some()).await;
        status(self.view(), n).unwrap()
    }

    /// The whole blob `hash`, fetched with `blob/get`.
    async fn fetch(&mut self, hash: &BlobHash) -> Vec<u8> {
        self.blob.clear();
        let params = BlobGetParams {
            hash: hash.clone(),
            offset: None,
            length: None,
        };
        let got = self.client.request::<BlobGet>(params).await.unwrap();
        let size = usize::try_from(got.size).unwrap();
        self.until(|peer| peer.blob.len() >= size).await;
        std::mem::take(&mut self.blob)
    }
}

/// Turn `n`'s status, once it has ended.
fn status(view: &SessionView, n: u32) -> Option<TurnStatus> {
    let turn = view.turns.iter().find(|t| t.started.turn_no == n)?;
    turn.completed.as_ref().map(|c| c.status.clone())
}

/// What the agent said in turn `n`.
fn said_in(view: &SessionView, n: u32) -> String {
    let turn = view.turns.iter().find(|t| t.started.turn_no == n).unwrap();
    let items = view.items.values();
    let ours = items.filter(|i| i.item.turn_id.as_ref() == Some(&turn.started.turn_id));
    let messages = ours.filter(|i| matches!(i.item.item, Item::AgentMessage));
    messages.map(|i| i.text.as_str()).collect()
}

/// Views compared without a multi-megabyte dump when they differ.
fn same(a: &SessionView, b: &SessionView) {
    let (a, b) = (
        serde_json::to_value(a).unwrap(),
        serde_json::to_value(b).unwrap(),
    );
    let first = || {
        a.as_object()
            .unwrap()
            .keys()
            .find(|k| a.get(*k) != b.get(*k))
            .cloned()
    };
    assert!(a == b, "the views differ at {:?}", first());
}

/// IT-gw-sync, SP-4a test 1: two clients subscribed at once see the same
/// session and index, and a late one's snapshot is the same too. Content
/// over a frame goes by blob: one client puts it, the other gets it.
#[tokio::test(flavor = "multi_thread")]
async fn it_gw_sync_two_clients_see_the_same_stream() {
    let dir = TempDir::new("sync-same");
    let (data_dir, host, client) = fake_agent_host(&dir, json!({})).await;
    let _host = Running(host);
    let session = create(&client, "say hi").await;
    let mut a = Peer::new(client, &session).await;
    let mut b = Peer::new(connect(&data_dir).await, &session).await;
    a.ended(1).await;
    b.ended(1).await;

    let script = "flood_mixed 300 40; tool t1 read Read big file; tool_done t1 40000; say done";
    send(&a.client, &session, script).await;
    for peer in [&mut a, &mut b] {
        assert_eq!(peer.ended(2).await, TurnStatus::Completed);
        peer.until(|p| summary(p, &session) == "unread").await;
    }
    same(a.view(), b.view());
    assert_eq!(a.index.view().sessions, b.index.view().sessions);
    assert!(said_in(a.view(), 2).ends_with("done"));
    // The tool's 40,000 characters are over the inline limit: a blob.
    let output = tool_output(a.view());
    assert_eq!((output.blob.size, output.preview.len()), (40_000, 4_000));
    assert_eq!(a.fetch(&output.blob.hash).await, vec![b'a'; 40_000]);

    let mut late = Peer::new(connect(&data_dir).await, &session).await;
    late.until(|p| p.session.is_live()).await;
    same(late.view(), a.view());

    // A message too large for a frame goes as a `text_blob`.
    let text = "x".repeat(100_000);
    let blob = put(&a.client, text.as_bytes()).await;
    let content = json!([{"type": "text_blob", "blob": blob}]);
    let params = json!({"command_id": client::command_id(), "session_id": session,
        "mode": "queue", "content": content});
    a.client
        .request::<SessionSend>(typed(params))
        .await
        .unwrap();
    b.ended(3).await;
    let message = b
        .view()
        .items
        .values()
        .rev()
        .find_map(|i| match &i.item.item {
            Item::UserMessage(message) => Some(serde_json::to_value(&message.content).unwrap()),
            _ => None,
        });
    assert_eq!(message.unwrap(), content);
    let hash = typed(blob["hash"].clone());
    assert_eq!(b.fetch(&hash).await, text.as_bytes());
}

/// Where the tool call's output went.
fn tool_output(view: &SessionView) -> BlobContent {
    let output = view.items.values().find_map(|i| match &i.item.item {
        Item::ToolCall(call) => call.output.clone(),
        _ => None,
    });
    match output {
        Some(Content::Blob(blob)) => blob,
        other => panic!("not a blob: {other:?}"),
    }
}

/// The session's status in the index mirror.
fn summary(peer: &Peer, session: &SessionId) -> String {
    let sessions = &peer.index.view().sessions;
    sessions
        .get(session)
        .map_or("-".into(), |s| s.status.to_string())
}

/// Puts `bytes` with `blob/put`; its blob reference.
async fn put(client: &Client, bytes: &[u8]) -> Value {
    let hash: [u8; 32] = Sha256::digest(bytes).into();
    let max = BlobChunkHeader::MAX_DATA;
    for (n, chunk) in bytes.chunks(max).enumerate() {
        let offset = n * max;
        let last = offset + chunk.len() == bytes.len();
        let header = BlobChunkHeader {
            hash,
            offset: offset as u64,
            last,
        };
        client.send_blob_chunk(&header, chunk).await.unwrap();
    }
    let params = BlobPutParams {
        hash: BlobHash::from_sha256(&hash),
        size: bytes.len() as u64,
        media_type: "text/plain".into(),
    };
    let put = client.request::<BlobPut>(params.clone()).await.unwrap();
    assert_eq!(put.hash, params.hash);
    serde_json::to_value(BlobRef {
        hash: params.hash,
        size: params.size,
        media_type: params.media_type,
    })
    .unwrap()
}

/// The pending request of the session, once there is one.
async fn pending(peer: &mut Peer) -> Value {
    let asked = |p: &Peer| p.view().requests.values().find(|r| r.is_pending()).cloned();
    peer.until(|p| asked(p).is_some()).await;
    serde_json::to_value(asked(peer).unwrap().requested).unwrap()
}

/// IT-gw-sync, SP-4a test 2: two clients answer the same request at once;
/// the first answer wins and the other gets `conflict`. The read-only
/// `cli` principal may not answer at all.
#[tokio::test(flavor = "multi_thread")]
async fn it_gw_sync_first_answer_wins() {
    let dir = TempDir::new("sync-first");
    let (data_dir, host, client) = fake_agent_host(&dir, json!({})).await;
    let _host = Running(host);
    let script = "tool t1 execute Run tests; permission t1; say finished";
    let session = create(&client, script).await;
    let mut a = Peer::new(client, &session).await;
    let mut b = Peer::new(connect(&data_dir).await, &session).await;
    let asked = pending(&mut a).await;
    assert_eq!(pending(&mut b).await, asked);

    let cli = connect_with(&data_dir, Options::default()).await;
    let allow = json!({"type": "selected", "option_id": "allow-once"});
    let denied = respond(&cli, &session, &asked, allow.clone()).await;
    assert_eq!(refused(denied).0, "forbidden");

    let (x, y) = tokio::join!(
        respond(&a.client, &session, &asked, allow.clone()),
        respond(&b.client, &session, &asked, allow.clone()),
    );
    let lost = match (x, y) {
        (Ok(()), lost @ Err(_)) | (lost @ Err(_), Ok(())) => lost,
        other => panic!("not one winner: {other:?}"),
    };
    assert_eq!(
        refused(lost),
        ("conflict".into(), "already_resolved".into())
    );
    for peer in [&mut a, &mut b] {
        assert_eq!(peer.ended(1).await, TurnStatus::Completed);
    }
    assert!(said_in(a.view(), 1).contains("permission:allow-once"));
    same(a.view(), b.view());
}

/// IT-gw-sync, SP-4a test 3: what a client missed while away comes back
/// on reconnect, from its cursor, with no gap and nothing twice (the mirror
/// checks every seq); contiguous deltas come as range events. A client
/// with a coalescing window gets ranges live.
#[tokio::test(flavor = "multi_thread")]
async fn it_gw_sync_reconnect_replays_without_gaps() {
    let dir = TempDir::new("sync-replay");
    let (data_dir, host, client) = fake_agent_host(&dir, json!({})).await;
    let _host = Running(host);
    let session = create(&client, "say first").await;
    let mut away = Peer::new(client, &session).await;
    away.ended(1).await;
    let cursor = away.session.clone();
    drop(away);

    let mut writer = Peer::new(connect(&data_dir).await, &session).await;
    send(
        &writer.client,
        &session,
        "flood 3000 7; think pondering; flood 50 3; say end",
    )
    .await;
    writer.ended(2).await;
    let mut back = Peer::with(connect(&data_dir).await, cursor).await;
    back.until(|p| p.session.is_live() && status(p.view(), 2).is_some())
        .await;
    assert_eq!(back.subscribed[0].mode, SubscribeMode::Replay);
    same(back.view(), writer.view());
    assert!(back.ranges > 0, "no range events in the replay");
    assert_eq!(said_in(back.view(), 2).len(), 3000 * 7 + 50 * 3 + 3);

    let options = Options {
        auth: ClientAuth::Harness(HarnessKey::new(KEY)),
        coalesce_ms: Some(200),
        ..Options::default()
    };
    let mut windowed = Peer::new(connect_with(&data_dir, options).await, &session).await;
    windowed.until(|p| p.session.is_live()).await;
    send(
        &writer.client,
        &session,
        "flood 400 5; sleep 300; flood 400 5",
    )
    .await;
    windowed.ended(3).await;
    writer.ended(3).await;
    assert!(windowed.ranges > 0, "no range events live");
    same(windowed.view(), writer.view());
}

/// IT-gw-sync, SP-4a test 4: a client that stops reading does not hold the
/// agent up. Its backlog passes `max_queue_bytes`, the host resets its
/// subscription instead of waiting, and from its cursor it catches up.
#[tokio::test(flavor = "multi_thread")]
async fn it_gw_sync_slow_client_does_not_block_the_agent() {
    let dir = TempDir::new("sync-slow");
    let (data_dir, host, client) = fake_agent_host(&dir, json!({})).await;
    let _host = Running(host);
    let session = create(&client, "say hi").await;
    let mut fast = Peer::new(client, &session).await;
    fast.ended(1).await;
    let script = "flood 5000 1000; say end";
    let clock = SystemClock::new();
    let took = async |fast: &mut Peer, n| {
        let started = clock.now_mono();
        send(&fast.client, &session, script).await;
        assert_eq!(fast.ended(n).await, TurnStatus::Completed);
        clock.now_mono().saturating_duration_since(started)
    };
    let baseline = took(&mut fast, 2).await;

    // A client that subscribes and then reads nothing.
    let socket = socket_path(&data_dir, &cupper_sys::user_temp_dir().unwrap());
    let mut slow = testkit::Client::connect(&socket).await;
    let nonce = slow.hello().await;
    let (auth, _) = key_auth("harness", &KEY, &nonce, std::process::id() as i32);
    slow.call("initialize", initialize_params(&nonce, auth))
        .await
        .unwrap();
    let stream = format!("session/{session}");
    let subscribed = slow
        .call("stream/subscribe", json!({"stream": stream}))
        .await;
    let mut seq = subscribed.unwrap()["at_seq"].as_u64().unwrap();
    while slow.next().await.unwrap()["params"]["last"] != true {}

    let stalled = took(&mut fast, 3).await;
    assert!(
        stalled < baseline * 3 + Duration::from_secs(2),
        "a stalled client slowed the agent: {stalled:?} against {baseline:?}"
    );
    // What reached the socket comes in order, then the reset.
    loop {
        let message = slow.next().await.unwrap();
        if message["method"] == "stream/reset" {
            assert_eq!(message["params"]["reason"], "slow_consumer");
            break;
        }
        follow(&message, &mut seq);
    }
    let head = fast.session.resume().unwrap();
    let again = json!({"stream": stream, "after_seq": seq, "epoch": head.epoch});
    let replay = slow.call("stream/subscribe", again).await.unwrap();
    assert_eq!(replay["mode"], "replay");
    while seq < head.after_seq {
        let message = slow.next().await.unwrap();
        follow(&message, &mut seq);
    }
}

/// Checks that the events of `message` come right after `seq`, which moves
/// on to the last of them.
fn follow(message: &Value, seq: &mut u64) {
    for event in message["params"]["events"].as_array().unwrap() {
        let from = event.get("from").unwrap_or(&event["seq"]);
        assert_eq!(from.as_u64(), Some(*seq + 1));
        *seq = event["seq"].as_u64().unwrap();
    }
}

/// The adapter's process group and its descendants' as the host recorded
/// them; whatever of them is left at the end is killed, once checked by
/// pgid and start time.
struct Leftovers(Vec<ProcessGroup>);

impl Leftovers {
    fn of(data_dir: &Path) -> Self {
        let row = column(data_dir, "SELECT pgid || ' ' || start_us FROM adapters");
        let (pgid, start) = row[0].split_once(' ').unwrap();
        let leader = ProcessGroup {
            pgid: pgid.parse().unwrap(),
            start: StartTime::from_micros(start.parse().unwrap()),
        };
        Self(leader.with_descendants())
    }

    fn alive(&self) -> bool {
        self.0.iter().any(ProcessGroup::is_alive)
    }
}

impl Drop for Leftovers {
    fn drop(&mut self) {
        for group in &self.0 {
            let _ = group.signal(Signal::Kill);
        }
    }
}

/// Waits until `done` holds, polling.
async fn eventually(what: &str, done: impl Fn() -> bool) {
    let clock = SystemClock::new();
    let give_up = clock.now_mono() + PATIENCE;
    while !done() {
        assert!(clock.now_mono() < give_up, "not in time: {what}");
        wait(&clock).await;
    }
}

/// IT-gw-sync, SP-4a test 5: `kill -9` of the host mid-turn. The next host
/// reaps the adapter's leftovers, ends the turn as interrupted and the
/// request as expired; the client comes back from its cursor. The queue
/// waits for the user (T089), and then the agent session goes on.
#[tokio::test(flavor = "multi_thread")]
async fn it_gw_sync_kill9_recovery_and_resume() {
    let dir = TempDir::new("sync-kill9");
    let data_dir = fake_agent_data(&dir);
    let mut host = Running(spawn_with(&data_dir, fake_agent_config(json!({}))));
    let session = create(&connect(&data_dir).await, "whoami").await;
    let mut a = Peer::new(connect(&data_dir).await, &session).await;
    a.ended(1).await;
    let first = said_in(a.view(), 1);
    let agent_session = first.split_whitespace().next().unwrap().to_owned();
    assert!(first.contains("resumed=false"), "{first}");

    // A turn that leaves a grandchild behind and waits for an answer, and
    // a message queued after it.
    send(
        &a.client,
        &session,
        "spawn_child; tool t1 edit Edit file; permission t1",
    )
    .await;
    let asked = pending(&mut a).await;
    send(&a.client, &session, "whoami").await;
    a.until(|p| {
        p.view()
            .queue
            .as_ref()
            .is_some_and(|q| q.entries.len() == 1)
    })
    .await;
    let cursor = a.session.clone();
    drop(a);
    let leftovers = Leftovers::of(&data_dir);
    host.0.kill().unwrap();
    host.0.wait().unwrap();
    // The adapter reads EOF and exits; its grandchild stays.
    eventually("the grandchild outlives the host", || leftovers.alive()).await;

    let _host = Running(spawn_with(&data_dir, fake_agent_config(json!({}))));
    let mut a = Peer::with(connect(&data_dir).await, cursor).await;
    eventually("the leftovers are reaped", || !leftovers.alive()).await;
    let allow = json!({"type": "selected", "option_id": "allow-once"});
    let late = respond(&a.client, &session, &asked, allow).await;
    assert_eq!(refused(late).0, "expired");
    a.until(|p| {
        let paused = p.view().queue.as_ref().and_then(|q| q.paused.as_ref());
        status(p.view(), 2).is_some() && paused.is_some()
    })
    .await;
    assert_eq!(a.subscribed[0].mode, SubscribeMode::Replay);
    assert_eq!(status(a.view(), 2), Some(TurnStatus::Interrupted));
    let request = a.view().requests.values().next().unwrap();
    assert_eq!(request.status().to_string(), "expired");
    let queue = a.view().queue.clone().unwrap();
    assert_eq!(queue.paused.unwrap().reason.to_string(), "turn_interrupted");
    a.until(|p| summary(p, &session) == "unread").await;

    let resume = json!({"command_id": client::command_id(), "session_id": session});
    let resumed = a.client.request::<SessionQueueResume>(typed(resume)).await;
    resumed.unwrap();
    assert_eq!(a.ended(3).await, TurnStatus::Completed);
    let again = said_in(a.view(), 3);
    assert!(again.starts_with(&agent_session), "{again} after {first}");
    assert!(again.contains("resumed=true"), "{again}");
}

/// `session/send` with its result as the host sent it, `replayed` included.
struct SendRaw;

impl Method for SendRaw {
    const NAME: &'static str = SessionSend::NAME;
    const SCOPE: Option<Scope> = SessionSend::SCOPE;
    const IDEMPOTENCY: Idempotency = SessionSend::IDEMPOTENCY;
    const MILESTONE: Milestone = SessionSend::MILESTONE;
    const ERRORS: &'static [MethodError] = SessionSend::ERRORS;
    type Params = Value;
    type Result = Value;
}

/// IT-gw-sync, SP-4a test 6: a command sent again with its `command_id`
/// runs once: repeated, sent twice at once on two connections, and sent
/// again after a restart. The same ID with other parameters is refused.
#[tokio::test(flavor = "multi_thread")]
async fn it_gw_sync_command_retries_are_idempotent() {
    let dir = TempDir::new("sync-retry");
    let data_dir = fake_agent_data(&dir);
    let mut host = Running(spawn_with(&data_dir, fake_agent_config(json!({}))));
    let a = connect(&data_dir).await;
    let b = connect(&data_dir).await;
    let session = create(&a, "say hi").await;
    let with_id = |text: &str, id: &CommandId| {
        json!({"command_id": id, "session_id": session, "mode": "queue",
            "content": [{"type": "text", "text": text}]})
    };

    let once = with_id("say once", &client::command_id());
    let first = a.request::<SendRaw>(once.clone()).await.unwrap();
    let again = a.request::<SendRaw>(once).await.unwrap();
    assert_eq!(
        (first.get("replayed"), &again["replayed"]),
        (None, &json!(true))
    );
    assert_eq!(first["queue_id"], again["queue_id"]);

    let id = client::command_id();
    let twice = with_id("say twice?", &id);
    let (x, y) = tokio::join!(
        a.request::<SendRaw>(twice.clone()),
        b.request::<SendRaw>(twice.clone()),
    );
    let (x, y) = (x.unwrap(), y.unwrap());
    assert_eq!(x["queue_id"], y["queue_id"]);
    assert!(x.get("replayed").is_some() ^ y.get("replayed").is_some());
    let other = a
        .request::<SendRaw>(with_id("say something else", &id))
        .await;
    assert_eq!(refused(other).0, "idempotency_mismatch");
    until(
        &data_dir,
        "host.db",
        "SELECT CAST(count(*) AS TEXT) FROM turns WHERE status = 'completed'",
        &["3"],
    )
    .await;

    terminate(&host.0);
    assert_eq!(exit_code(&mut host.0).await, 0);
    let _host = Running(spawn_with(&data_dir, fake_agent_config(json!({}))));
    let after = connect(&data_dir).await;
    let replayed = after.request::<SendRaw>(twice).await.unwrap();
    assert_eq!(
        (&replayed["replayed"], &replayed["queue_id"]),
        (&json!(true), &x["queue_id"])
    );
    let mut check = Peer::new(after, &session).await;
    check.until(|p| p.session.is_live()).await;
    assert_eq!(check.view().turns.len(), 3);
    let messages = check.view().items.values();
    let messages = messages.filter(|i| matches!(i.item.item, Item::UserMessage(_)));
    assert_eq!(messages.count(), 3);
}

/// IT-gw-sync, SP-4a test 7: whatever the agent sends, frames stay within
/// the limit: a 3 MB message comes as deltas, a 100,000-character title is
/// cut, a 500 kB tool output is a blob, and a snapshot of it all comes in
/// pages: the subscriber then and a later one stay subscribed (T177). The client refuses to send
/// a frame over the limit.
#[tokio::test(flavor = "multi_thread")]
async fn it_gw_sync_frames_stay_within_limit() {
    let dir = TempDir::new("sync-frames");
    let (data_dir, host, client) = fake_agent_host(&dir, json!({})).await;
    let _host = Running(host);
    let session = create(&client, "say hi").await;
    let mut a = Peer::new(client, &session).await;
    a.ended(1).await;
    let large = message(&session, &"x".repeat(100_000));
    let refused = a.client.request::<SessionSend>(typed(large)).await;
    assert!(
        matches!(refused, Err(ClientError::Encode(_))),
        "{refused:?}"
    );

    let script = "big 3000000; long_tool t1 100000; tool_done t1 500000; say end";
    send(&a.client, &session, script).await;
    assert_eq!(a.ended(2).await, TurnStatus::Completed);
    assert_eq!(said_in(a.view(), 2).len(), 3_000_000 + 3);
    let tool = a.view().items.values().find_map(|i| match &i.item.item {
        Item::ToolCall(call) => Some(call.title.clone()),
        _ => None,
    });
    assert!(tool.unwrap().chars().count() <= MAX_LABEL_CHARS + 1);
    assert_eq!(tool_output(a.view()).blob.size, 500_000);
    assert_eq!(a.subscribed.len(), 1, "never reset");

    // The frame decoder of each client refuses frames over the limit, and
    // the connection closes then; the mirrors here got everything.
    let mut late = Peer::new(connect(&data_dir).await, &session).await;
    late.until(|p| p.session.is_live()).await;
    same(late.view(), a.view());
}

/// IT-gw-sync, adapter isolation (ADR-05): the host holds its database,
/// its socket and client connections, yet the adapter inherits fds 0-2
/// only; of the host's environment it gets the allowed variables only.
#[tokio::test(flavor = "multi_thread")]
async fn it_gw_sync_adapter_gets_only_stdio_and_allowed_env() {
    let dir = TempDir::new("sync-isolation");
    let data_dir = fake_agent_data(&dir);
    let mut command = Command::new(HOST);
    command
        .env("CUPPER_PROBE", "1")
        .env("UNLISTED_TEST_VAR", "1");
    let login = json!({"login_env": {"PATH": "/usr/bin:/bin"}});
    let payload = payload(fake_agent_config(login));
    let _host = Running(start(&mut command, &data_dir, &payload));
    let _other = connect(&data_dir).await;
    let session = create(&connect(&data_dir).await, "probe").await;
    let mut a = Peer::new(connect(&data_dir).await, &session).await;
    a.ended(1).await;
    let said = said_in(a.view(), 1);
    let (fds, env) = said.split_once(" env=").unwrap();
    assert_eq!(fds, "fds=0,1,2");
    let names: Vec<&str> = env.split(',').collect();
    assert!(names.iter().all(|n| !n.starts_with("CUPPER_")), "{env}");
    assert!(!names.contains(&"UNLISTED_TEST_VAR"), "{env}");
    assert!(names.contains(&"PATH") && names.contains(&"HOME"), "{env}");
}

/// `session/send` params of `text`.
fn message(session: &SessionId, text: &str) -> Value {
    json!({"command_id": client::command_id(), "session_id": session, "mode": "queue",
        "content": [{"type": "text", "text": text}]})
}

/// Boot session UUIDs the host is told it runs in.
const BOOTS: [&str; 4] = [
    "8D4E1A0C-0000-4000-8000-00000000000A",
    "8D4E1A0C-0000-4000-8000-00000000000B",
    "8D4E1A0C-0000-4000-8000-00000000000C",
    "8D4E1A0C-0000-4000-8000-00000000000D",
];

/// The host with the fake agent, on boot `boot`.
fn boot(data_dir: &Path, boot: &str) -> Running {
    let config = fake_agent_config(json!({"boot_session_uuid": boot}));
    Running(spawn_with(data_dir, config))
}

/// The orderly exit.
async fn stop(mut host: Running) {
    terminate(&host.0);
    assert_eq!(exit_code(&mut host.0).await, 0);
}

fn kill9(mut host: Running) {
    host.0.kill().unwrap();
    host.0.wait().unwrap();
}

/// A new connection subscribing with the mirror `away`, once it is live;
/// its `data_epoch` from `initialize`.
async fn back(data_dir: &Path, away: Mirror<SessionView>) -> (Peer, u64) {
    let client = connect(data_dir).await;
    let epoch = client.initialized().host.data_epoch;
    let mut peer = Peer::with(client, away).await;
    peer.until(|p| p.session.is_live()).await;
    (peer, epoch)
}

/// How the last subscription of the session came back.
fn came_back(peer: &Peer) -> (SubscribeMode, Option<ResyncReason>, u64) {
    let last = peer.subscribed.last().unwrap();
    (last.mode.clone(), last.reason.clone(), last.epoch)
}

/// IT-NFR2-epoch (design 5.9.8, minutes D5) on one data directory through
/// six starts: `data_epoch` goes up only after an unclean exit across a
/// reboot. `kill -9` on the same boot keeps it, and so does a clean exit
/// before a reboot; `kill -9` and then a reboot counts as a power loss and
/// costs one more full snapshot. A cursor past the head is `cursor_ahead`.
/// A power loss that took the latest commits away: the client's old epoch
/// gets a snapshot, and the seqs the host hands out again are taken as new.
#[tokio::test(flavor = "multi_thread")]
async fn it_nfr2_epoch_moves_only_when_commits_may_be_lost() {
    use SubscribeMode::{Replay, Snapshot};
    let dir = TempDir::new("epoch");
    let data_dir = fake_agent_data(&dir);
    let host = boot(&data_dir, BOOTS[0]);
    let client = connect(&data_dir).await;
    let epoch = client.initialized().host.data_epoch;
    let session = create(&client, "say one").await;
    let mut peer = Peer::new(client, &session).await;
    peer.ended(1).await;
    assert_eq!(came_back(&peer).2, epoch);

    kill9(host);
    let host = boot(&data_dir, BOOTS[0]);
    let (mut peer, now) = back(&data_dir, peer.session).await;
    assert_eq!((came_back(&peer), now), ((Replay, None, epoch), epoch));
    // The same epoch, a cursor past the head.
    let head = peer.session.resume().unwrap();
    let ahead = StreamSubscribeParams {
        stream: peer.stream.clone(),
        after_seq: Some(head.after_seq + 5),
        epoch: Some(epoch),
    };
    peer.client.subscribe(ahead).await.unwrap();
    peer.until(|p| p.subscribed.len() == 2 && p.session.is_live())
        .await;
    let ahead = Some(ResyncReason::CursorAhead);
    assert_eq!(came_back(&peer), (Snapshot, ahead, epoch));

    stop(host).await;
    let host = boot(&data_dir, BOOTS[1]);
    let (peer, now) = back(&data_dir, peer.session).await;
    assert_eq!((came_back(&peer), now), ((Replay, None, epoch), epoch));

    kill9(host);
    let host = boot(&data_dir, BOOTS[2]);
    let (mut peer, now) = back(&data_dir, peer.session).await;
    let changed = Some(ResyncReason::EpochChanged);
    assert_eq!(now, epoch + 1);
    assert_eq!(came_back(&peer), (Snapshot, changed.clone(), now));

    // The power loss: a copy of the store at seq M, as the disk may keep
    // it, while the host runs on to seq N.
    let copy = dir.path().join("at-m.db");
    let store = cupper_host::store::open_reader(&data_dir.join("host.db")).unwrap();
    store
        .execute("VACUUM INTO ?1", [copy.to_str().unwrap()])
        .unwrap();
    let at_m = (
        peer.session.resume().unwrap().after_seq,
        peer.view().clone(),
    );
    send(&peer.client, &session, "say two").await;
    peer.ended(2).await;
    let at_n = peer.session.resume().unwrap().after_seq;
    stop(host).await;
    for wal in ["host.db-wal", "host.db-shm"] {
        let _ = std::fs::remove_file(data_dir.join(wal));
    }
    std::fs::copy(&copy, data_dir.join("host.db")).unwrap();
    let _host = boot(&data_dir, BOOTS[3]);
    let (mut peer, now) = back(&data_dir, peer.session).await;
    assert_eq!(now, epoch + 2);
    assert_eq!(came_back(&peer), (Snapshot, changed, now));
    assert_eq!(peer.session.resume().unwrap().after_seq, at_m.0);
    same(peer.view(), &at_m.1);
    // From M + 1 the seqs of before are handed out again; the mirror takes
    // them.
    send(&peer.client, &session, "say two again").await;
    peer.ended(2).await;
    let head = peer.session.resume().unwrap().after_seq;
    assert!(at_m.0 < at_n && at_m.0 < head, "{} {at_n} {head}", at_m.0);
    assert_eq!(said_in(peer.view(), 2), "two again");
    let (fresh, _) = back(&data_dir, Mirror::session(session)).await;
    same(fresh.view(), peer.view());
}

/// IT-history-from-host (PS-8, K8): after a restart the history comes from
/// the host's turn snapshots. A client with nothing cached subscribes to
/// the window of 20 turns and pages the older ones with `history/page`;
/// together they are what a client saw live. No adapter starts: nothing
/// comes from the agent's replay.
#[tokio::test(flavor = "multi_thread")]
async fn it_history_from_host_pages_the_turns_before_the_window() {
    let dir = TempDir::new("history");
    let data_dir = fake_agent_data(&dir);
    let host = boot(&data_dir, BOOTS[0]);
    let client = connect(&data_dir).await;
    let session = create(&client, "say turn 1").await;
    let mut live = Peer::new(client, &session).await;
    for n in 2..=22 {
        send(&live.client, &session, &format!("say turn {n}")).await;
    }
    live.ended(22).await;
    stop(host).await;

    let _host = boot(&data_dir, BOOTS[0]);
    let (fresh, _) = back(&data_dir, Mirror::session(session.clone())).await;
    let window = fresh.view().turns.iter().map(|t| t.started.turn_no);
    assert_eq!(window.collect::<Vec<_>>(), (3..=22).collect::<Vec<_>>());
    let mut view = fresh.view().clone();
    let mut cursor = None;
    loop {
        let params = HistoryPageParams {
            session_id: session.clone(),
            cursor,
            max_bytes: None,
        };
        let page = fresh.client.request::<HistoryPage>(params).await.unwrap();
        for op in &page.ops {
            view.apply(&op.decode(Strictness::Strict).unwrap().value)
                .unwrap();
        }
        cursor = page.next;
        if cursor.is_none() {
            break;
        }
    }
    same(&view, live.view());
    assert_eq!(said_in(&view, 1), "turn 1");
    let adapters = "SELECT CAST(count(*) AS TEXT) FROM adapters";
    assert_eq!(column(&data_dir, adapters), ["1"]);
}

/// Waits until the index holds `want`, the reasons of its intents in order.
async fn intents(data_dir: &Path, want: &[&str]) {
    let sql = "SELECT json_extract(json, '$.data.reason') FROM events
               WHERE stream = 'index' AND type = 'notification.intent' ORDER BY seq";
    until(data_dir, "host.db", sql, want).await;
}

/// Sends `text` and waits for it to end as the session's `n`th turn.
async fn turn(client: &Client, data_dir: &Path, session: &SessionId, text: &str, n: usize) {
    send(client, session, text).await;
    events(data_dir, session, "turn.completed", n).await;
}

/// `client/presence` of the `dev_key` desktop connection `ui`; the ping
/// after it makes sure the host has read it.
async fn presence(ui: &mut testkit::Client, foreground: bool, session: &SessionId) {
    let params = json!({"foreground": foreground, "session_id": session});
    let notification = json!({"jsonrpc": "2.0", "method": "client/presence", "params": params});
    ui.send(&notification).await;
    ui.call("ping", Value::Null).await.unwrap();
}

/// IT-notification-intent (AP-3): needing action, a turn completed and a
/// turn failed each bring one `notification.intent` once the 100 ms of
/// coalescing are over; a request answered within them brings none, nor
/// does an interrupted turn. The presence of the harness's client counts
/// for nothing (security #16); a desktop window in front on the session
/// keeps intents back while it watches (`dev-auth` builds, which have the
/// desktop's `dev_key`).
#[tokio::test(flavor = "multi_thread")]
async fn it_notification_intent_once_per_change_unless_watched() {
    let dir = TempDir::new("intent");
    let data_dir = fake_agent_data(&dir);
    let mut payload = payload(fake_agent_config(json!({"clock": "manual"})));
    if cfg!(feature = "dev-auth") {
        payload["dev_key"] = json!(proof::to_hex(&KEY));
    }
    let _host = Running(start(&mut Command::new(HOST), &data_dir, &payload));
    let client = connect(&data_dir).await;
    let allow = json!({"type": "selected", "option_id": "allow-once"});

    let session = create(&client, "permission t1; say done").await;
    let asked = events(&data_dir, &session, "interaction.requested", 1).await;
    advance(&client, 100).await;
    intents(&data_dir, &["needs_action"]).await;
    respond(&client, &session, &asked[0], allow.clone())
        .await
        .unwrap();
    events(&data_dir, &session, "turn.completed", 1).await;
    advance(&client, 100).await;
    let mut want = vec!["needs_action", "turn_completed"];
    intents(&data_dir, &want).await;

    // Answered within the 100 ms: only the end of the turn.
    send(&client, &session, "permission t2; say again").await;
    let asked = events(&data_dir, &session, "interaction.requested", 2).await;
    respond(&client, &session, &asked[1], allow).await.unwrap();
    events(&data_dir, &session, "turn.completed", 2).await;
    advance(&client, 100).await;
    want.push("turn_completed");
    intents(&data_dir, &want).await;

    // Interrupted: none.
    send(&client, &session, "wait_cancel").await;
    events(&data_dir, &session, "turn.started", 3).await;
    let interrupt = json!({"command_id": client::command_id(), "session_id": session});
    let interrupted = client.request::<SessionInterrupt>(typed(interrupt)).await;
    interrupted.unwrap();
    events(&data_dir, &session, "turn.completed", 3).await;
    advance(&client, 100).await;
    let mut ended = 3;

    if cfg!(feature = "dev-auth") {
        let socket = socket_path(&data_dir, &cupper_sys::user_temp_dir().unwrap());
        let mut ui = testkit::Client::connect(&socket).await;
        let nonce = ui.hello().await;
        let (auth, _) = key_auth("dev_key", &KEY, &nonce, std::process::id() as i32);
        let mut params = initialize_params(&nonce, auth);
        params["client"]["kind"] = json!("desktop");
        params["role"] = json!("control");
        ui.call("initialize", params).await.unwrap();
        presence(&mut ui, true, &session).await;
        turn(&client, &data_dir, &session, "say watched", 4).await;
        advance(&client, 100).await;
        presence(&mut ui, false, &session).await;
        turn(&client, &data_dir, &session, "say unwatched", 5).await;
        advance(&client, 100).await;
        want.push("turn_completed");
        intents(&data_dir, &want).await;
        ended = 5;
    }

    let params = ClientPresenceParams {
        foreground: true,
        session_id: Some(session.clone()),
    };
    client.notify::<ClientPresence>(params).await.unwrap();
    turn(&client, &data_dir, &session, "exit 1", ended + 1).await;
    advance(&client, 100).await;
    want.push("error");
    intents(&data_dir, &want).await;
}

/// IT-host-orphaned (P-19, T059's rule, `dev-auth` builds): the process
/// that started the host exits while a turn waits for an answer. The host
/// starts no new sessions or turns, lets the turn finish, and then exits
/// in order.
#[cfg(feature = "dev-auth")]
#[tokio::test(flavor = "multi_thread")]
async fn it_host_orphaned_finishes_the_turn_then_exits() {
    use std::io::{BufRead, BufReader, Write};
    use std::process::Stdio;

    use cupper_protocol::annotations::DurationMs;
    use cupper_protocol::ids::ProjectId;
    use cupper_protocol::methods::admin::{TestClockAdvance, TestClockAdvanceParams};
    use cupper_protocol::methods::session::SessionCreate;
    use cupper_sys::ExitWatch;

    /// A host this test did not start itself: SIGTERM at the end if it still
    /// runs, once its pid and start time say it is the same process.
    struct Adopted {
        pid: i32,
        start: StartTime,
    }

    impl Drop for Adopted {
        fn drop(&mut self) {
            let same = cupper_sys::process_info(self.pid).is_some_and(|p| p.start == self.start);
            if let (true, Ok(exit)) = (same, ExitWatch::new(self.pid)) {
                let pid = self.pid.to_string();
                let _ = Command::new("/bin/kill").args(["-TERM", &pid]).status();
                let _ = exit.wait(PATIENCE);
            }
        }
    }

    let dir = TempDir::new("orphaned");
    let data_dir = fake_agent_data(&dir);
    // sh starts the host and turns into `sleep`, the host's parent until
    // the test ends it.
    let script =
        r#"exec 3<&0; "$0" "$1" "$2" <&3 3<&- >/dev/null 2>&1 & echo $!; exec /bin/sleep 3600"#;
    let sh = Command::new("/bin/sh")
        .args(["-c", script, HOST, "--data-dir"])
        .arg(&data_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn();
    let mut parent = Running(sh.unwrap());
    let config = fake_agent_config(json!({"clock": "manual"}));
    writeln!(parent.0.stdin.take().unwrap(), "{}", payload(config)).unwrap();
    let mut line = String::new();
    let stdout = parent.0.stdout.take().unwrap();
    BufReader::new(stdout).read_line(&mut line).unwrap();
    let pid: i32 = line.trim().parse().unwrap();
    let host = cupper_sys::process_info(pid).unwrap();
    assert_eq!(host.ppid as u32, parent.0.id());
    let _host = Adopted {
        pid,
        start: host.start,
    };
    let exited = ExitWatch::new(pid).unwrap();

    let client = connect(&data_dir).await;
    let session = create(
        &client,
        "tool t1 execute Run tests; permission t1; say done",
    )
    .await;
    let asked = events(&data_dir, &session, "interaction.requested", 1).await;
    terminate(&parent.0);
    parent.0.wait().unwrap();
    let adopted = || cupper_sys::process_info(pid).is_some_and(|p| p.ppid == 1);
    eventually("launchd adopts the host", adopted).await;
    // The host looks at its parent every `supervisor_observe`.
    advance(&client, 1_000).await;
    let send = client.request::<SessionSend>(typed(message(&session, "say more")));
    assert_eq!(refused(send.await).0, "unavailable");
    let params = json!({"command_id": client::command_id(), "agent": "claude",
        "project_id": ProjectId::from_uuid(1), "workspace": {"kind": "local"},
        "message": [{"type": "text", "text": "say new"}]});
    let created = client.request::<SessionCreate>(typed(params)).await;
    assert_eq!(refused(created).0, "unavailable");

    let allow = json!({"type": "selected", "option_id": "allow-once"});
    respond(&client, &session, &asked[0], allow).await.unwrap();
    let ended = events(&data_dir, &session, "turn.completed", 1).await;
    assert_eq!(ended[0]["status"], "completed");
    // It may be gone before the answer to the advance.
    let observe = TestClockAdvanceParams {
        ms: DurationMs(1_000),
    };
    let _ = client.request::<TestClockAdvance>(observe).await;
    let status = tokio::task::spawn_blocking(move || exited.wait(PATIENCE));
    let status = status.await.unwrap().unwrap().expect("the host exits");
    assert_eq!(status.code(), Some(0));
    let exiting = "SELECT detail_json FROM audit WHERE event = 'host.exiting'";
    let exiting: Value = serde_json::from_str(&column(&data_dir, exiting)[0]).unwrap();
    assert_eq!(exiting, json!({"force": false, "reason": "orphaned"}));
    let left = "SELECT CAST(count(*) AS TEXT) FROM adapters WHERE ended_at IS NULL";
    assert_eq!(column(&data_dir, left), ["0"]);
    let turns = "SELECT CAST(count(*) AS TEXT) FROM turns";
    assert_eq!(column(&data_dir, turns), ["1"]);
}
