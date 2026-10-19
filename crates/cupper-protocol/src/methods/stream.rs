//! Streams and content (design 5.3.3, 5.3.4): subscriptions, the events,
//! snapshots and history pages that follow, and blobs.

use std::borrow::Cow;
use std::marker::PhantomData;

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Empty;
use super::catalog::Catalog;
use crate::envelope::Event;
use crate::event::{Decoded, EventBody, Strictness};
use crate::ids::{BlobHash, SessionId};
use crate::open_enum;
use crate::sync::{Resume, ResyncReason, StreamId, SubscribeMode};

/// An event or op as a message carries it: the JSON as is, decoded one by
/// one, so that one malformed entry cannot fail the whole message (design
/// 5.3.11). Its schema is `T`'s.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Raw<T> {
    value: Value,
    #[serde(skip)]
    of: PhantomData<fn() -> T>,
}

impl<T> Raw<T> {
    pub fn from_value(value: Value) -> Self {
        Self {
            value,
            of: PhantomData,
        }
    }

    pub fn as_value(&self) -> &Value {
        &self.value
    }
}

impl<T: Serialize> Raw<T> {
    pub fn encode(value: &T) -> Result<Self, serde_json::Error> {
        serde_json::to_value(value).map(Self::from_value)
    }
}

impl Raw<Event<EventBody>> {
    /// See [`Event::decode`]; a client decodes tolerantly.
    pub fn decode(&self, strictness: Strictness) -> serde_json::Result<Decoded<Event<EventBody>>> {
        Event::decode(&self.value, strictness)
    }
}

impl Raw<EventBody> {
    /// See [`EventBody::decode`]; a client decodes tolerantly.
    pub fn decode(&self, strictness: Strictness) -> serde_json::Result<Decoded<EventBody>> {
        EventBody::decode(&self.value, strictness)
    }
}

impl<T: JsonSchema> JsonSchema for Raw<T> {
    fn inline_schema() -> bool {
        T::inline_schema()
    }

    fn schema_name() -> Cow<'static, str> {
        T::schema_name()
    }

    fn schema_id() -> Cow<'static, str> {
        T::schema_id()
    }

    fn json_schema(g: &mut SchemaGenerator) -> Schema {
        T::json_schema(g)
    }
}

method! {
    /// `stream/subscribe`: replay after a cursor, or a snapshot (design 5.3.3).
    request Subscribe = "stream/subscribe" (SubscribeParams) -> SubscribeResult {
        scope: Read,
        idempotency: Natural,
        milestone: M1a,
        errors: [NotFound, RateLimited],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SubscribeParams {
    pub stream: StreamId,
    /// The last seq the client applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_seq: Option<u64>,
    /// The `data_epoch` the cursor belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epoch: Option<u64>,
}

impl SubscribeParams {
    /// The cursor, for [`subscribe_mode`](crate::sync::subscribe_mode); a
    /// cursor needs both `after_seq` and `epoch`.
    pub fn resume(&self) -> Option<Resume> {
        Some(Resume {
            epoch: self.epoch?,
            after_seq: self.after_seq?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SubscribeResult {
    pub stream: StreamId,
    pub mode: SubscribeMode,
    /// Why a cursor did not replay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<ResyncReason>,
    /// The seq the snapshot stands at, or the replay starts after.
    pub at_seq: u64,
    pub head_seq: u64,
    pub epoch: u64,
}

method! {
    /// `stream/unsubscribe`.
    request Unsubscribe = "stream/unsubscribe" (StreamParams) -> Empty {
        scope: Read,
        idempotency: Natural,
        milestone: M1a,
        errors: [],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StreamParams {
    pub stream: StreamId,
}

method! {
    /// `stream/events`: live or replayed events, contiguous by seq.
    host_notification StreamEvents = "stream/events" (StreamEventsParams) { milestone: M1a }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StreamEventsParams {
    pub stream: StreamId,
    pub events: Vec<Raw<Event<EventBody>>>,
}

method! {
    /// `stream/snapshot`: one page of the state at `at_seq`, applied like events.
    host_notification StreamSnapshot = "stream/snapshot" (SnapshotParams) { milestone: M1a }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SnapshotParams {
    pub stream: StreamId,
    pub at_seq: u64,
    pub epoch: u64,
    /// From 0.
    pub page: u32,
    /// Events after `at_seq` follow this page.
    pub last: bool,
    pub ops: Vec<Raw<EventBody>>,
}

method! {
    /// `stream/reset`: the host dropped the subscription; resubscribe with the cursor.
    host_notification StreamReset = "stream/reset" (ResetParams) { milestone: M1a }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ResetParams {
    pub stream: StreamId,
    pub reason: ResetReason,
}

open_enum! {
    pub enum ResetReason {
        /// The connection's backlog passed `limits.max_queue_bytes`.
        SlowConsumer => "slow_consumer",
        /// The host rebuilt the stream.
        Rebuilt => "rebuilt",
    }
}

method! {
    /// `stream/ack`: what a remote client has applied (M3).
    client_notification StreamAck = "stream/ack" (AckParams) {
        scope: Read,
        idempotency: Natural,
        milestone: M3,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AckParams {
    pub stream: StreamId,
    pub seq: u64,
}

method! {
    /// `history/page`: turns before the window, newest first (design 5.3.3).
    request HistoryPage = "history/page" (HistoryPageParams) -> HistoryPageResult {
        scope: Read,
        idempotency: Natural,
        milestone: M1a,
        errors: [NotFound],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HistoryPageParams {
    pub session_id: SessionId,
    /// Absent: start before the oldest turn in the window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<HistoryCursor>,
    /// At most `limits.max_history_page_bytes`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<u32>,
}

/// Without `op_index`: the turn before `turn_no`. With it: the rest of turn
/// `turn_no` from that op on, when the turn did not fit one page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HistoryCursor {
    pub turn_no: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub op_index: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HistoryPageResult {
    pub ops: Vec<Raw<EventBody>>,
    /// Absent at the first turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<HistoryCursor>,
}

method! {
    /// `blob/get`: the bytes come first as `0x02` chunks, then this result.
    request BlobGet = "blob/get" (BlobGetParams) -> BlobGetResult {
        scope: Read,
        idempotency: Natural,
        milestone: M1a,
        errors: [NotFound],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BlobGetParams {
    pub hash: BlobHash,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BlobGetResult {
    pub hash: BlobHash,
    /// The whole blob, in bytes.
    pub size: u64,
    pub offset: u64,
    /// Bytes sent.
    pub length: u64,
    /// `0x02` chunks sent.
    pub chunks: u32,
}

method! {
    /// `blob/put`: after the bytes as `0x02` chunks; addressed by content.
    request BlobPut = "blob/put" (BlobPutParams) -> BlobPutResult {
        scope: Write,
        idempotency: Natural,
        milestone: M1a,
        errors: [TooLarge, Conflict],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BlobPutParams {
    pub hash: BlobHash,
    pub size: u64,
    pub media_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BlobPutResult {
    pub hash: BlobHash,
}

pub fn register(c: &mut Catalog, g: &mut SchemaGenerator) {
    c.request::<Subscribe>(g);
    c.request::<Unsubscribe>(g);
    c.host_notification::<StreamEvents>(g);
    c.host_notification::<StreamSnapshot>(g);
    c.host_notification::<StreamReset>(g);
    c.client_notification::<StreamAck>(g);
    c.request::<HistoryPage>(g);
    c.request::<BlobGet>(g);
    c.request::<BlobPut>(g);
}

#[cfg(test)]
pub(super) mod tests {
    use serde_json::json;

    use super::*;
    use crate::methods::testkit::{notification, request};
    use crate::testutil::open_values;

    fn turn(n: u32) -> Value {
        json!({"type": "turn.started", "data": {"turn_id": format!("t_{n}"), "turn_no": n,
            "origin": "user", "started_at": 5}})
    }

    fn delta(text: &str) -> Value {
        json!({"type": "item.delta", "data": {"item_id": "i_1", "field": "text", "text": text}})
    }

    /// A known type whose data is malformed: `turn_no` is negative.
    fn broken() -> Value {
        json!({"type": "turn.started", "data": {"turn_id": "t_9", "turn_no": -1,
            "origin": "user", "started_at": 5}})
    }

    fn event(seq: u64, body: Value) -> Value {
        let mut event = json!({"seq": seq, "v": 1, "ts": 1700000000000_u64});
        event
            .as_object_mut()
            .unwrap()
            .extend(body.as_object().unwrap().clone());
        event
    }

    /// Typical values of every method in this group (design 5.3.3, 5.3.4).
    pub fn samples() -> Vec<&'static str> {
        let hash = "ab".repeat(32);
        let mut range = event(9, delta("ok"));
        range["from"] = json!(8);
        vec![
            request::<Subscribe>(
                json!({"stream": "session/s_1", "after_seq": 120, "epoch": 2}),
                json!({"stream": "session/s_1", "mode": "snapshot", "reason": "epoch_changed",
                    "at_seq": 98, "head_seq": 98, "epoch": 3}),
            ),
            request::<Subscribe>(
                json!({"stream": "index"}),
                json!({"stream": "index", "mode": "replay", "at_seq": 7, "head_seq": 9, "epoch": 1}),
            ),
            request::<Unsubscribe>(json!({"stream": "index"}), json!({})),
            notification::<StreamEvents>(json!({"stream": "session/s_1",
                "events": [event(7, turn(1)), range]})),
            notification::<StreamSnapshot>(
                json!({"stream": "session/s_1", "at_seq": 98, "epoch": 3,
                "page": 0, "last": true, "ops": [turn(1), delta("ok")]}),
            ),
            notification::<StreamReset>(json!({"stream": "index", "reason": "slow_consumer"})),
            notification::<StreamAck>(json!({"stream": "session/s_1", "seq": 98})),
            request::<HistoryPage>(
                json!({"session_id": "s_1", "cursor": {"turn_no": 21}, "max_bytes": 48000}),
                json!({"ops": [turn(20), delta("ok")], "next": {"turn_no": 19, "op_index": 1500}}),
            ),
            request::<HistoryPage>(json!({"session_id": "s_1"}), json!({"ops": []})),
            request::<BlobGet>(
                json!({"hash": hash, "offset": 0, "length": 65536}),
                json!({"hash": hash, "size": 70000, "offset": 0, "length": 65536, "chunks": 2}),
            ),
            request::<BlobGet>(
                json!({"hash": hash}),
                json!({"hash": hash, "size": 9, "offset": 0, "length": 9, "chunks": 1}),
            ),
            request::<BlobPut>(
                json!({"hash": hash, "size": 9, "media_type": "image/png"}),
                json!({"hash": hash}),
            ),
        ]
    }

    #[test]
    fn unit_proto_methods_core_stream_one_malformed_event_spares_the_rest() {
        let mut no_seq = event(9, delta("b"));
        no_seq.as_object_mut().unwrap().remove("seq");
        let raw = json!({"stream": "session/s_1",
            "events": [event(7, delta("a")), event(8, broken()), no_seq, event(10, delta("c"))]});
        let params: StreamEventsParams = serde_json::from_value(raw).unwrap();
        let decoded: Vec<_> = params
            .events
            .iter()
            .map(|e| e.decode(Strictness::Tolerant))
            .collect();
        let seqs: Vec<Option<u64>> = decoded
            .iter()
            .map(|d| d.as_ref().ok().map(|d| d.value.seq))
            .collect();
        assert_eq!(seqs, [Some(7), Some(8), None, Some(10)]);
        let degraded = decoded[1].as_ref().unwrap();
        assert!(matches!(&degraded.value.body, EventBody::Unknown(u) if u.kind == "turn.started"));
        let why = degraded.degraded.as_ref().unwrap().to_string();
        assert!(why.contains("invalid value"), "{why}");
        assert!(params.events[1].decode(Strictness::Strict).is_err());
        assert!(params.events[3].decode(Strictness::Strict).is_ok());

        // Snapshot pages and history pages decode op by op the same way.
        let ops = json!([turn(1), broken(), delta("ok"), {"data": {}}]);
        let page: SnapshotParams = serde_json::from_value(json!({"stream": "session/s_1",
            "at_seq": 9, "epoch": 1, "page": 0, "last": true, "ops": ops}))
        .unwrap();
        let history: HistoryPageResult = serde_json::from_value(json!({"ops": ops})).unwrap();
        for ops in [&page.ops, &history.ops] {
            let got: Vec<Option<(String, bool)>> = ops
                .iter()
                .map(|op| {
                    let op = op.decode(Strictness::Tolerant).ok()?;
                    Some((op.value.tag().to_owned(), op.degraded.is_some()))
                })
                .collect();
            let tag = |tag: &str, degraded| Some((tag.to_owned(), degraded));
            let (started, degraded) = (tag("turn.started", false), tag("turn.started", true));
            assert_eq!(got, [started, degraded, tag("item.delta", false), None]);
        }
    }

    #[test]
    fn unit_proto_methods_core_stream_raw_encodes_and_has_the_schema_of_its_type() {
        let body = EventBody::decode(&delta("ok"), Strictness::Strict)
            .unwrap()
            .value;
        let raw = Raw::encode(&body).unwrap();
        assert_eq!(raw.as_value(), &delta("ok"));
        assert_eq!(raw.decode(Strictness::Strict).unwrap().value, body);

        let mut g = crate::schema::generator();
        let ops = g.subschema_for::<Vec<Raw<EventBody>>>();
        let events = g.subschema_for::<Raw<Event<EventBody>>>();
        assert_eq!(ops, g.subschema_for::<Vec<EventBody>>());
        assert_eq!(events, g.subschema_for::<Event<EventBody>>());
        assert_eq!(events.get("$ref").unwrap(), "#/$defs/Event");
    }

    #[test]
    fn unit_proto_methods_core_stream_subscribe_cursor() {
        let params = |raw: Value| {
            serde_json::from_value::<SubscribeParams>(raw)
                .unwrap()
                .resume()
        };
        let full = params(json!({"stream": "index", "after_seq": 4, "epoch": 2}));
        assert_eq!(
            full,
            Some(Resume {
                epoch: 2,
                after_seq: 4
            })
        );
        assert_eq!(params(json!({"stream": "index", "after_seq": 4})), None);
        assert_eq!(params(json!({"stream": "index", "epoch": 2})), None);
        open_values::<ResetReason>(ResetReason::KNOWN);
    }
}
