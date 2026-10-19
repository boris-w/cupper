//! IT-gw-subscribe: subscriptions, replays, snapshot pages, slow clients,
//! send-time coalescing and history pages, through a gateway on a real
//! socket, over streams on a real `host.db`, on a manual clock. Clients read
//! with a decoder that refuses frames over `limits.max_frame_bytes`.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use cupper_protocol::event::EventBody;
use cupper_protocol::ids::SessionId;
use cupper_protocol::sync::{ResyncReason, StreamId, SubscribeMode};
use serde_json::{Value, json};

use crate::clock::{Clock, ManualClock};
use crate::gateway::testkit::{Client, TestGateway, initialize_params};
use crate::gateway::{AuditEvent, Routes};
use crate::store::Readers;
use crate::store::streams::{Batch, Retention, Streams, StreamsConfig};
use crate::store::testutil::{TempDir, body, end, host_writer, item, project, turn};

/// A gateway serving the streams of a fresh `host.db`.
struct Fixture {
    gw: TestGateway,
    clock: ManualClock,
    streams: Streams,
    _dir: TempDir,
}

impl Fixture {
    fn new(retention: Retention) -> Self {
        let (dir, clock) = (TempDir::new("subscribe"), ManualClock::new());
        let (writer, _) = host_writer(&dir, &clock);
        let readers = Readers::open(&dir.host_db()).unwrap();
        let idle = Duration::from_secs(600);
        let config = StreamsConfig {
            epoch: 1,
            idle,
            retention,
        };
        let shared: Arc<dyn Clock> = Arc::new(clock.clone());
        let streams = Streams::new(writer, readers.clone(), Arc::clone(&shared), config);
        let mut routes = Routes::default();
        routes.streams(streams.clone(), readers);
        let gw = TestGateway::with(TestGateway::config(None), shared, routes);
        Self {
            gw,
            clock,
            streams,
            _dir: dir,
        }
    }

    /// Appends `bodies` to `stream` in one batch.
    async fn append(&self, stream: &StreamId, bodies: Vec<EventBody>) {
        let mut writer = self.streams.open(stream).await.unwrap().writer().unwrap();
        let batch = Batch {
            bodies,
            ..Batch::default()
        };
        writer.append(batch).await.unwrap();
    }

    /// A `peer` client that offered `features` and asked for `coalesce_ms`.
    async fn connect(&self, features: Value, coalesce_ms: u32) -> Peer {
        let mut client = Client::connect(&self.gw.socket).await;
        let nonce = client.hello().await;
        let mut params = initialize_params(&nonce, json!({"type": "peer"}));
        params["features"] = features;
        params["coalesce_ms"] = json!(coalesce_ms);
        client.call("initialize", params).await.unwrap();
        Peer {
            client,
            early: VecDeque::new(),
            next_id: 100,
        }
    }

    /// The next `stream.subscribed` record (Q16).
    async fn subscribed(&mut self) -> Subscribed {
        loop {
            if let AuditEvent::Subscribed {
                stream,
                mode,
                reason,
                lag,
            } = self.gw.record().await.event
            {
                return (stream, mode, reason, lag);
            }
        }
    }
}

type Subscribed = (StreamId, SubscribeMode, Option<ResyncReason>, Option<u64>);

/// A client that keeps the notifications that come before a response.
struct Peer {
    client: Client,
    early: VecDeque<Value>,
    next_id: i64,
}

impl Peer {
    /// Sends a request without waiting for its response; returns its id.
    async fn send(&mut self, method: &str, params: Value) -> i64 {
        self.next_id += 1;
        let request = json!({"jsonrpc": "2.0", "id": self.next_id, "method": method,
            "params": params});
        self.client.send(&request).await;
        self.next_id
    }

    /// The response to request `id`: its `result`, or its `error`.
    async fn response(&mut self, id: i64) -> Result<Value, Value> {
        loop {
            let message = self.client.next().await.expect("the connection is open");
            if message["id"] == id {
                return match message.get("error") {
                    Some(error) => Err(error.clone()),
                    None => Ok(message["result"].clone()),
                };
            }
            assert!(message.get("id").is_none(), "{message}");
            self.early.push_back(message);
        }
    }

    async fn call(&mut self, method: &str, params: Value) -> Result<Value, Value> {
        let id = self.send(method, params).await;
        self.response(id).await
    }

    /// The next notification.
    async fn note(&mut self) -> Value {
        match self.early.pop_front() {
            Some(note) => note,
            None => self.client.next().await.expect("the connection is open"),
        }
    }

    /// `stream/subscribe`, and with mode `snapshot` the params of its pages.
    async fn subscribe(&mut self, params: Value) -> (Value, Vec<Value>) {
        let result = self.call("stream/subscribe", params).await.unwrap();
        let mut pages = Vec::new();
        while result["mode"] == "snapshot" && pages.last().is_none_or(|p: &Value| p["last"] != true)
        {
            let note = self.note().await;
            assert_eq!(note["method"], "stream/snapshot", "{note}");
            pages.push(note["params"].clone());
        }
        (result, pages)
    }

    /// The params of the `stream/events` that come next, all of `stream`,
    /// up to the one with event `seq`.
    async fn events(&mut self, stream: &str, seq: u64) -> Vec<Value> {
        let mut messages = Vec::new();
        while seqs(&messages).last() != Some(&seq) {
            let note = self.note().await;
            assert_eq!(note["method"], "stream/events", "{note}");
            assert_eq!(note["params"]["stream"], stream);
            messages.push(note["params"].clone());
        }
        messages
    }

    /// Nothing is sent before the response to a `ping`.
    async fn quiet(&mut self) {
        assert_eq!(self.call("ping", json!({})).await, Ok(json!({})));
        assert!(self.early.is_empty(), "{:?}", self.early);
    }
}

/// The seqs of the events of `stream/events` params.
fn seqs(messages: &[Value]) -> Vec<u64> {
    let events = messages
        .iter()
        .flat_map(|m| m["events"].as_array().unwrap());
    events.map(|event| event["seq"].as_u64().unwrap()).collect()
}

fn session(n: u128) -> StreamId {
    StreamId::Session(SessionId::from_uuid(n))
}

fn delta(text: &str) -> EventBody {
    body(json!({"type": "item.delta", "data": {"item_id": "i_1", "field": "text", "text": text}}))
}

/// Lets the gateway's tasks run.
async fn settle() {
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn it_gw_subscribe_snapshot_then_live_and_replay_after_a_cursor() {
    use SubscribeMode::*;
    let mut fx = Fixture::new(Retention::DESIGN);
    let index = StreamId::Index;
    fx.append(&index, (1..=3).map(project).collect()).await;
    let mut a = fx.connect(json!([]), 0).await;
    let (result, pages) = a.subscribe(json!({"stream": "index"})).await;
    let snapshot = json!({"stream": "index", "mode": "snapshot", "at_seq": 3, "head_seq": 3,
        "epoch": 1});
    assert_eq!(result, snapshot);
    assert_eq!(pages.len(), 1);
    let page = &pages[0];
    assert_eq!(
        (&page["page"], &page["at_seq"], &page["epoch"]),
        (&json!(0), &json!(3), &json!(1))
    );
    assert_eq!(page["ops"].as_array().unwrap().len(), 3);
    fx.append(&index, vec![project(4)]).await;
    assert_eq!(seqs(&a.events("index", 4).await), [4]);

    // A cursor in the log replays what follows it, then live events.
    let mut b = fx.connect(json!([]), 0).await;
    let cursor = json!({"stream": "index", "epoch": 1, "after_seq": 2});
    let (result, pages) = b.subscribe(cursor).await;
    let replay = json!({"stream": "index", "mode": "replay", "at_seq": 2, "head_seq": 4,
        "epoch": 1});
    assert_eq!((result, pages), (replay, Vec::new()));
    assert_eq!(seqs(&b.events("index", 4).await), [3, 4]);
    fx.append(&index, vec![project(5)]).await;
    for peer in [&mut a, &mut b] {
        assert_eq!(seqs(&peer.events("index", 5).await), [5]);
    }
    assert_eq!(fx.subscribed().await, (index.clone(), Snapshot, None, None));
    assert_eq!(fx.subscribed().await, (index, Replay, None, Some(2)));
}

#[tokio::test]
async fn it_gw_subscribe_epoch_and_cursor_rules() {
    use ResyncReason::*;
    use SubscribeMode::*;
    let mut fx = Fixture::new(Retention::DESIGN);
    let index = StreamId::Index;
    fx.append(&index, (1..=5).map(project).collect()).await;
    let mut peer = fx.connect(json!([]), 0).await;
    let cases = [
        (
            json!({"epoch": 1, "after_seq": 9}),
            Snapshot,
            Some(CursorAhead),
            5,
            None,
        ),
        (
            json!({"epoch": 2, "after_seq": 1}),
            Snapshot,
            Some(EpochChanged),
            5,
            None,
        ),
        // Half a cursor is none.
        (json!({"epoch": 1}), Snapshot, None, 5, None),
        (json!({"after_seq": 1}), Snapshot, None, 5, None),
        (
            json!({"epoch": 1, "after_seq": 5}),
            Replay,
            None,
            5,
            Some(0),
        ),
        (
            json!({"epoch": 1, "after_seq": 0}),
            Replay,
            None,
            0,
            Some(5),
        ),
    ];
    for (mut params, mode, reason, at_seq, lag) in cases {
        params["stream"] = json!("index");
        let (result, pages) = peer.subscribe(params.clone()).await;
        assert_eq!(result["mode"], json!(mode), "{params}");
        assert_eq!(
            result.get("reason"),
            reason.as_ref().map(|r| json!(r)).as_ref()
        );
        assert_eq!(
            (&result["at_seq"], &result["head_seq"]),
            (&json!(at_seq), &json!(5))
        );
        assert_eq!(pages.is_empty(), mode == Replay);
        assert_eq!(fx.subscribed().await, (index.clone(), mode, reason, lag));
    }
    // Each subscription replaced the one before: the last replays, once.
    assert_eq!(seqs(&peer.events("index", 5).await), [1, 2, 3, 4, 5]);
    fx.append(&index, vec![project(6)]).await;
    assert_eq!(seqs(&peer.events("index", 6).await), [6]);
    peer.quiet().await;
}

#[tokio::test]
async fn it_gw_subscribe_cursor_expired_once_the_log_is_pruned() {
    let retention = Retention {
        events: 2,
        age: Duration::ZERO,
    };
    let fx = Fixture::new(retention);
    let s1 = session(1);
    let name = s1.to_string();
    fx.append(&s1, vec![turn(1), item("i_1", 1), end(1)]).await;
    // The checkpoint of turn 2 lets 1..=3 go, now that they are old.
    fx.clock.advance(Duration::from_secs(1));
    fx.append(&s1, vec![turn(2), end(2)]).await;
    let mut peer = fx.connect(json!([]), 0).await;
    let (result, pages) = peer
        .subscribe(json!({"stream": name, "epoch": 1, "after_seq": 2}))
        .await;
    assert_eq!(
        (&result["reason"], &result["at_seq"]),
        (&json!("cursor_expired"), &json!(5))
    );
    assert_eq!(pages.last().unwrap()["last"], true);
    let (result, _) = peer
        .subscribe(json!({"stream": name, "epoch": 1, "after_seq": 3}))
        .await;
    assert_eq!(result["mode"], "replay");
    assert_eq!(seqs(&peer.events(&name, 5).await), [4, 5]);
}

#[tokio::test]
async fn it_gw_subscribe_gap_found_while_replaying_resets_rebuilt() {
    let retention = Retention {
        events: 2,
        age: Duration::ZERO,
    };
    let mut fx = Fixture::new(retention);
    let (s1, s2) = (session(1), session(2));
    let (one, two) = (s1.to_string(), s2.to_string());
    fx.append(&s1, vec![turn(1), end(1)]).await;
    fx.append(&s2, vec![turn(1), item("i_1", 1)]).await;
    let mut peer = fx.connect(json!([]), 0).await;
    peer.subscribe(json!({"stream": two})).await;
    // The client reads nothing for a while: the writer blocks on the socket,
    // and the replay waits in the queue.
    let text = "x".repeat(16_000);
    fx.append(&s2, (0..64).map(|_| delta(&text)).collect())
        .await;
    let cursor = json!({"stream": one, "epoch": 1, "after_seq": 0});
    let id = peer.send("stream/subscribe", cursor.clone()).await;
    assert_eq!(fx.subscribed().await.0, s2);
    assert_eq!(fx.subscribed().await.1, SubscribeMode::Replay);
    // Meanwhile the checkpoint of turn 2 lets 1..=2 go.
    fx.clock.advance(Duration::from_secs(1));
    fx.append(&s1, vec![turn(2), end(2)]).await;

    assert_eq!(
        seqs(&peer.events(&two, 66).await),
        (3..=66).collect::<Vec<_>>()
    );
    assert_eq!(peer.response(id).await.unwrap()["mode"], "replay");
    let reset = json!({"stream": one, "reason": "rebuilt"});
    let note = peer.note().await;
    assert_eq!(
        (&note["method"], &note["params"]),
        (&json!("stream/reset"), &reset)
    );
    peer.quiet().await;
    // The cursor no longer fits the log.
    let (result, _) = peer.subscribe(cursor).await;
    assert_eq!(result["reason"], "cursor_expired");
}

#[tokio::test]
async fn it_gw_subscribe_snapshot_pages_and_replays_fit_frames() {
    let fx = Fixture::new(Retention::DESIGN);
    let s1 = session(1);
    let name = s1.to_string();
    let text: String = (0..200_000u32)
        .map(|i| char::from(b'a' + (i % 26) as u8))
        .collect();
    let deltas = text
        .as_bytes()
        .chunks(15_000)
        .map(|chunk| delta(std::str::from_utf8(chunk).unwrap()));
    fx.append(
        &s1,
        [vec![turn(1), item("i_1", 1)], deltas.collect()].concat(),
    )
    .await;
    let head = 16;

    let mut a = fx.connect(json!([]), 0).await;
    let (_, pages) = a.subscribe(json!({"stream": name})).await;
    assert!(pages.len() > 2, "{}", pages.len());
    for (n, page) in pages.iter().enumerate() {
        let expected = (json!(n), json!(n + 1 == pages.len()), json!(head));
        assert_eq!(
            (
                page["page"].clone(),
                page["last"].clone(),
                page["at_seq"].clone()
            ),
            expected
        );
    }
    let ops = pages
        .iter()
        .flat_map(|page| page["ops"].as_array().unwrap());
    let deltas = ops.filter(|op| op["type"] == "item.delta");
    let rebuilt: String = deltas
        .map(|op| op["data"]["text"].as_str().unwrap())
        .collect();
    assert!(rebuilt == text, "the deltas of the pages rebuild the text");

    let mut b = fx.connect(json!([]), 0).await;
    b.subscribe(json!({"stream": name, "epoch": 1, "after_seq": 0}))
        .await;
    let messages = b.events(&name, head).await;
    assert!(messages.len() > 2, "{}", messages.len());
    assert_eq!(seqs(&messages), (1..=head).collect::<Vec<_>>());
}

#[tokio::test]
async fn it_gw_subscribe_slow_consumer_is_reset_and_resumes_by_its_cursor() {
    let fx = Fixture::new(Retention::DESIGN);
    let s1 = session(1);
    let name = s1.to_string();
    fx.append(&s1, vec![turn(1), item("i_1", 1)]).await;
    let mut peer = fx.connect(json!([]), 0).await;
    peer.subscribe(json!({"stream": name})).await;
    // 6.4 MB the client does not read: past `limits.max_queue_bytes`.
    let text = "x".repeat(16_000);
    for _ in 0..20 {
        fx.append(&s1, (0..20).map(|_| delta(&text)).collect())
            .await;
    }
    let head = 402;
    let mut seen = Vec::new();
    let reset = loop {
        let note = peer.note().await;
        if note["method"] != "stream/events" {
            break note;
        }
        seen.extend(seqs(&[note["params"].clone()]));
    };
    let slow = json!({"stream": name, "reason": "slow_consumer"});
    assert_eq!(
        (&reset["method"], &reset["params"]),
        (&json!("stream/reset"), &slow)
    );
    let last = *seen.last().unwrap();
    assert!(last < head, "{last}");
    assert_eq!(seen, (3..=last).collect::<Vec<_>>());
    peer.quiet().await;

    let cursor = json!({"stream": name, "epoch": 1, "after_seq": last});
    assert_eq!(peer.subscribe(cursor).await.0["mode"], "replay");
    let rest = seqs(&peer.events(&name, head).await);
    assert_eq!(rest, (last + 1..=head).collect::<Vec<_>>());
}

#[tokio::test]
async fn it_gw_subscribe_coalesces_live_events_within_the_window() {
    let fx = Fixture::new(Retention::DESIGN);
    let s1 = session(1);
    let name = s1.to_string();
    fx.append(&s1, vec![turn(1), item("i_1", 1)]).await;
    let mut ranges = fx.connect(json!(["stream.coalesce"]), 100).await;
    let mut plain = fx.connect(json!([]), 100).await;
    for peer in [&mut ranges, &mut plain] {
        peer.subscribe(json!({"stream": name})).await;
    }
    fx.append(&s1, vec![delta("a"), delta("b")]).await;
    // Both writers took the first events and wait out the window, in which
    // more come.
    settle().await;
    assert!(fx.clock.next_deadline().is_some());
    fx.append(&s1, vec![delta("c"), delta("d")]).await;
    fx.clock.advance(Duration::from_millis(100));

    let merged = ranges.events(&name, 6).await;
    assert_eq!(merged.len(), 1);
    let event = &merged[0]["events"][0];
    assert_eq!(merged[0]["events"].as_array().unwrap().len(), 1);
    let range = (&event["from"], &event["seq"], &event["data"]["text"]);
    assert_eq!(range, (&json!(3), &json!(6), &json!("abcd")));
    // Without `stream.coalesce`, the events still go out together.
    let apart = plain.events(&name, 6).await;
    assert_eq!((apart.len(), seqs(&apart)), (1, vec![3, 4, 5, 6]));
    assert!(apart[0]["events"][0].get("from").is_none());
}

#[tokio::test]
async fn it_gw_subscribe_up_to_64_and_unsubscribed_streams_stay_quiet() {
    let fx = Fixture::new(Retention::DESIGN);
    for n in 1..=64 {
        fx.append(&session(n), vec![turn(1)]).await;
    }
    let name = |n| json!(session(n).to_string());
    let mut peer = fx.connect(json!([]), 0).await;
    // A session stream without events is not found; the index is there from
    // the start.
    let missing = json!({"stream": name(99)});
    let err = peer.call("stream/subscribe", missing).await.unwrap_err();
    assert_eq!(err["data"]["code"], "not_found");
    let (result, pages) = peer.subscribe(json!({"stream": "index"})).await;
    let empty = (&result["head_seq"], &pages[0]["last"], &pages[0]["ops"]);
    assert_eq!(empty, (&json!(0), &json!(true), &json!([])));

    // An unsubscribe right behind its subscribe applies after it.
    let id = peer
        .send("stream/subscribe", json!({"stream": name(2)}))
        .await;
    let gone = peer
        .send("stream/unsubscribe", json!({"stream": name(2)}))
        .await;
    assert_eq!(peer.response(id).await.unwrap()["mode"], "snapshot");
    assert_eq!(peer.response(gone).await, Ok(json!({})));
    peer.early
        .retain(|note| note["method"] != "stream/snapshot");
    fx.append(&session(2), vec![end(1)]).await;
    peer.quiet().await;

    for n in 1..=63 {
        peer.subscribe(json!({"stream": name(n)})).await;
    }
    let err = peer
        .call("stream/subscribe", json!({"stream": name(64)}))
        .await;
    assert_eq!(err.unwrap_err()["data"]["code"], "rate_limited");
    // Subscribing again replaces; unsubscribing frees a place.
    peer.subscribe(json!({"stream": name(1)})).await;
    let unsubscribe = peer.call("stream/unsubscribe", json!({"stream": name(2)}));
    assert_eq!(unsubscribe.await, Ok(json!({})));
    peer.subscribe(json!({"stream": name(64)})).await;
    fx.append(&session(2), vec![turn(2)]).await;
    fx.append(&session(1), vec![end(1)]).await;
    let one = session(1).to_string();
    assert_eq!(seqs(&peer.events(&one, 2).await), [2]);
    peer.quiet().await;
}

#[tokio::test]
async fn it_gw_subscribe_history_pages_before_the_window() {
    let fx = Fixture::new(Retention::DESIGN);
    let s1 = session(1);
    let id = json!(SessionId::from_uuid(1));
    // Turn 1 is larger than a page; turns 3..=22 fill the window.
    let big = (0..5).map(|_| delta(&"y".repeat(16_000)));
    let first = [vec![turn(1), item("i_1", 1)], big.collect(), vec![end(1)]];
    fx.append(&s1, first.concat()).await;
    for n in 2..=22 {
        let item = item(&format!("i_{n}"), n);
        fx.append(&s1, vec![turn(n), item, end(n)]).await;
    }
    let mut peer = fx.connect(json!(["history.page"]), 0).await;
    let page = peer.call("history/page", json!({"session_id": id})).await;
    let page = page.unwrap();
    assert_eq!(page["ops"][0]["data"]["turn_no"], 2);
    assert_eq!(page["next"], json!({"turn_no": 2}));

    // Whatever the client asks, a page stays within 48,000 bytes; the rest
    // of the turn follows from `op_index`.
    let params = json!({"session_id": id, "cursor": page["next"], "max_bytes": 1_000_000});
    let page = peer.call("history/page", params).await.unwrap();
    let bytes = serde_json::to_vec(&page["ops"]).unwrap().len();
    assert!(
        bytes <= 48_000 && page["next"]["op_index"].is_u64(),
        "{bytes}"
    );
    let mut ops = page["ops"].as_array().unwrap().clone();
    let mut next = page["next"].clone();
    while !next.is_null() {
        let params = json!({"session_id": id, "cursor": next, "max_bytes": 100});
        let page = peer.call("history/page", params).await.unwrap();
        assert_eq!(page["ops"].as_array().unwrap().len(), 1);
        ops.extend(page["ops"].as_array().unwrap().iter().cloned());
        next = page["next"].clone();
    }
    assert_eq!(ops[0]["data"]["turn_no"], 1);
    let text: String = ops
        .iter()
        .filter(|op| op["type"] == "item.delta")
        .map(|op| op["data"]["text"].as_str().unwrap())
        .collect();
    assert_eq!(text, "y".repeat(80_000));

    let missing = json!({"session_id": SessionId::from_uuid(9)});
    let err = peer.call("history/page", missing).await.unwrap_err();
    assert_eq!(err["data"]["code"], "not_found");
}
