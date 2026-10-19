//! Client-side sync rules (design 5.3.3): stream names and the contiguity check.

use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use crate::envelope::Event;
use crate::ids::SessionId;

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
