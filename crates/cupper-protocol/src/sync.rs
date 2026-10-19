//! Sync rules (design 5.3.3): stream names, how `stream/subscribe` starts a
//! stream, and the contiguity check a client applies to every event.

use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use crate::envelope::Event;
use crate::ids::SessionId;
use crate::open_enum;

/// `index` or `session/<session_id>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StreamId {
    Index,
    Session(SessionId),
}

impl fmt::Display for StreamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StreamId::Index => f.write_str("index"),
            StreamId::Session(id) => write!(f, "session/{id}"),
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("unknown stream name {0:?}")]
pub struct BadStreamId(pub String);

impl FromStr for StreamId {
    type Err = BadStreamId;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "index" {
            return Ok(StreamId::Index);
        }
        s.strip_prefix("session/")
            .and_then(|id| id.parse().ok())
            .map(StreamId::Session)
            .ok_or_else(|| BadStreamId(s.to_owned()))
    }
}

impl Serialize for StreamId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for StreamId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = <Cow<'de, str>>::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for StreamId {
    fn schema_name() -> Cow<'static, str> {
        "StreamId".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "string", "description": "`index` or `session/<session_id>`." })
    }
}

open_enum! {
    /// How `stream/subscribe` starts a stream.
    pub enum SubscribeMode {
        /// `stream/events` from `after_seq + 1`; the client keeps its view and cursor.
        Replay => "replay",
        /// `stream/snapshot` pages at `at_seq`, then events. Whatever the
        /// reason, the client drops the stream's view and cursor and rebuilds
        /// from the pages: after `epoch_changed` seqs may be reused, and
        /// nothing old is left to clash with them.
        Snapshot => "snapshot",
    }
}

open_enum! {
    /// Why a subscription with a cursor gets a snapshot.
    pub enum ResyncReason {
        /// The log no longer keeps the events after the cursor.
        CursorExpired => "cursor_expired",
        /// The host's `data_epoch` moved on after a power loss (design 5.2.8).
        EpochChanged => "epoch_changed",
        /// The cursor is past the host's head.
        CursorAhead => "cursor_ahead",
    }
}

/// A client's cursor: the `epoch` and `after_seq` of `stream/subscribe`.
/// A cursor needs both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resume {
    pub epoch: u64,
    pub after_seq: u64,
}

/// What the host's log holds of one stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogExtent {
    /// The host's `data_epoch`.
    pub epoch: u64,
    pub head_seq: u64,
    /// The oldest seq still kept; `head_seq + 1` when none is.
    pub first_seq: u64,
}

/// The four cases of `stream/subscribe`. The epoch goes first: across
/// epochs, seqs cannot be compared.
pub fn subscribe_mode(
    resume: Option<Resume>,
    log: &LogExtent,
) -> (SubscribeMode, Option<ResyncReason>) {
    let reason = match resume {
        None => return (SubscribeMode::Snapshot, None),
        Some(r) if r.epoch != log.epoch => ResyncReason::EpochChanged,
        Some(r) if r.after_seq > log.head_seq => ResyncReason::CursorAhead,
        Some(r) if r.after_seq + 1 < log.first_seq => ResyncReason::CursorExpired,
        Some(_) => return (SubscribeMode::Replay, None),
    };
    (SubscribeMode::Snapshot, Some(reason))
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("gap in stream: expected first seq {expected}, got from={from} seq={seq}")]
pub struct GapError {
    pub expected: u64,
    pub from: u64,
    pub seq: u64,
}

/// Tracks the last applied seq of one stream and enforces
/// `event.from (or seq) == last_seq + 1`. On a gap the client resubscribes
/// with its cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamCursor {
    last_seq: u64,
}

impl StreamCursor {
    /// Start after `seq` (a snapshot's `at_seq` or a replay's `after_seq`).
    pub fn at(seq: u64) -> Self {
        Self { last_seq: seq }
    }

    pub fn last_seq(&self) -> u64 {
        self.last_seq
    }

    pub fn accept<B>(&mut self, event: &Event<B>) -> Result<(), GapError> {
        let expected = self.last_seq + 1;
        if event.first_seq() != expected || event.seq < event.first_seq() {
            return Err(GapError {
                expected,
                from: event.first_seq(),
                seq: event.seq,
            });
        }
        self.last_seq = event.seq;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::open_values;

    /// A snapshot serves the window afresh whenever the cursor cannot be trusted.
    #[test]
    fn unit_view_window_subscribe_snapshots_unless_the_cursor_fits() {
        use ResyncReason::*;
        use SubscribeMode::*;
        let mode = |(epoch, head_seq, first_seq), cursor: Option<(u64, u64)>| {
            let log = LogExtent {
                epoch,
                head_seq,
                first_seq,
            };
            subscribe_mode(
                cursor.map(|(epoch, after_seq)| Resume { epoch, after_seq }),
                &log,
            )
        };
        let log = (3, 90, 41);
        assert_eq!(mode(log, None), (Snapshot, None));
        assert_eq!(mode(log, Some((3, 90))), (Replay, None));
        assert_eq!(
            mode(log, Some((3, 40))),
            (Replay, None),
            "from the oldest kept seq"
        );
        assert_eq!(mode(log, Some((3, 39))), (Snapshot, Some(CursorExpired)));
        assert_eq!(mode(log, Some((3, 91))), (Snapshot, Some(CursorAhead)));
        assert_eq!(mode(log, Some((2, 90))), (Snapshot, Some(EpochChanged)));
        assert_eq!(mode(log, Some((4, 9_000))), (Snapshot, Some(EpochChanged)));
        assert_eq!(
            mode((1, 0, 1), Some((1, 0))),
            (Replay, None),
            "an empty log"
        );
        open_values::<SubscribeMode>(SubscribeMode::KNOWN);
        open_values::<ResyncReason>(ResyncReason::KNOWN);
    }

    #[test]
    fn unit_proto_base_stream_names() {
        let s1: SessionId = "s_1".parse().unwrap();
        assert_eq!("index".parse::<StreamId>().unwrap(), StreamId::Index);
        assert_eq!(
            "session/s_1".parse::<StreamId>().unwrap(),
            StreamId::Session(s1.clone())
        );
        for bad in ["session/", "session/s_1/x", "session/t_1", "sessions"] {
            assert!(bad.parse::<StreamId>().is_err(), "{bad}");
        }
        let json = serde_json::to_string(&StreamId::Session(s1)).unwrap();
        assert_eq!(json, "\"session/s_1\"");
        assert_eq!(
            serde_json::from_str::<StreamId>(&json).unwrap().to_string(),
            "session/s_1"
        );
    }
}
