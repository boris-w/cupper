//! Client-side sync rules: stream names and the contiguity check.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use crate::events::Event;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StreamId {
    Index,
    Session(String),
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
        match s {
            "index" => Ok(StreamId::Index),
            _ => match s.strip_prefix("session/") {
                Some(id) if !id.is_empty() && !id.contains('/') => Ok(StreamId::Session(id.to_owned())),
                _ => Err(BadStreamId(s.to_owned())),
            },
        }
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
/// `event.from (or seq) == last_seq + 1`. On a gap the client must resubscribe
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

    pub fn accept(&mut self, event: &Event) -> Result<(), GapError> {
        let expected = self.last_seq + 1;
        if event.first_seq() != expected || event.seq < event.first_seq() {
            return Err(GapError { expected, from: event.first_seq(), seq: event.seq });
        }
        self.last_seq = event.seq;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_names() {
        assert_eq!("index".parse::<StreamId>().unwrap(), StreamId::Index);
        assert_eq!("session/s_1".parse::<StreamId>().unwrap(), StreamId::Session("s_1".into()));
        assert!("session/".parse::<StreamId>().is_err());
        assert!("session/a/b".parse::<StreamId>().is_err());
        assert_eq!(StreamId::Session("x".into()).to_string(), "session/x");
    }
}
