//! Gateway sync on a host process (design 5.9.1, 5.9.8): IT-gw-sync
//! (SP-4a's end-to-end tests 1-7 and adapter isolation). The agent is the
//! claude-shaped fake agent; the tests talk to the host through the CLI
//! library's client and stream mirror, as the CLI's `harness` client.

mod common;

use std::path::Path;
use std::time::Duration;

use cupper_cli::client::{self, Client, ClientAuth, ClientError, Incoming, Options};
use cupper_cli::harness::HarnessKey;
use cupper_cli::mirror::{self, Mirror};
use cupper_host::clock::{Clock, SystemClock};
use cupper_host::gateway::testkit::{self, KEY, TempDir, initialize_params, key_auth};
use cupper_protocol::domain::BlobRef;
use cupper_protocol::frame::BlobChunkHeader;
use cupper_protocol::ids::{BlobHash, CommandId, SessionId};
use cupper_protocol::item::Item;
use cupper_protocol::methods::session::{SessionQueueResume, SessionSend};
use cupper_protocol::methods::stream::{
    BlobGet, BlobGetParams, BlobPut, BlobPutParams, StreamSubscribeResult,
};
use cupper_protocol::rpc::{Idempotency, Method, MethodError, Milestone, Scope};
use cupper_protocol::session::TurnStatus;
use cupper_protocol::sync::{StreamId, SubscribeMode};
use cupper_protocol::text::MAX_LABEL_CHARS;
use cupper_protocol::transport::socket_path;
use cupper_protocol::view::{IndexView, SessionView};
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
        for event in message["params"]["events"].as_array().unwrap() {
            let from = event.get("from").unwrap_or(&event["seq"]);
            assert_eq!(from.as_u64(), Some(seq + 1));
            seq = event["seq"].as_u64().unwrap();
        }
    }
    let head = fast.session.resume().unwrap();
    let again = json!({"stream": stream, "after_seq": seq, "epoch": head.epoch});
    let replay = slow.call("stream/subscribe", again).await.unwrap();
    assert_eq!(replay["mode"], "replay");
    while seq < head.after_seq {
        let message = slow.next().await.unwrap();
        for event in message["params"]["events"].as_array().unwrap() {
            let from = event.get("from").unwrap_or(&event["seq"]);
            assert_eq!(from.as_u64(), Some(seq + 1));
            seq = event["seq"].as_u64().unwrap();
        }
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
    let message = |text: &str, id: &CommandId| {
        json!({"command_id": id, "session_id": session, "mode": "queue",
            "content": [{"type": "text", "text": text}]})
    };

    let once = message("say once", &client::command_id());
    let first = a.request::<SendRaw>(once.clone()).await.unwrap();
    let again = a.request::<SendRaw>(once).await.unwrap();
    assert_eq!(
        (first.get("replayed"), &again["replayed"]),
        (None, &json!(true))
    );
    assert_eq!(first["queue_id"], again["queue_id"]);

    let id = client::command_id();
    let twice = message("say twice?", &id);
    let (x, y) = tokio::join!(
        a.request::<SendRaw>(twice.clone()),
        b.request::<SendRaw>(twice.clone()),
    );
    let (x, y) = (x.unwrap(), y.unwrap());
    assert_eq!(x["queue_id"], y["queue_id"]);
    assert!(x.get("replayed").is_some() ^ y.get("replayed").is_some());
    let other = a
        .request::<SendRaw>(message("say something else", &id))
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
/// cut, and a snapshot of it all comes in pages. The client refuses to send
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

    let script = "big 3000000; long_tool t1 100000; tool_done t1 40000; say end";
    send(&a.client, &session, script).await;
    assert_eq!(a.ended(2).await, TurnStatus::Completed);
    assert_eq!(said_in(a.view(), 2).len(), 3_000_000 + 3);
    let tool = a.view().items.values().find_map(|i| match &i.item.item {
        Item::ToolCall(call) => Some(call.title.clone()),
        _ => None,
    });
    assert!(tool.unwrap().chars().count() <= MAX_LABEL_CHARS + 1);

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
    let mut command = std::process::Command::new(HOST);
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
