//! Streams (design 5.2.3, 5.2.8): every stream's seqs, view and subscribers
//! over the event log, reworked from SP-4a's `hub`, whose invariant holds: a
//! subscriber sees every seq after its start once, in order.

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the stream registry, next, puts it to use")
)]
mod state;
mod sync;
mod view;

use std::time::Duration;

pub use state::{Published, Push, Sink, Start};
pub use view::StreamView;

use super::upcast::UpcastError;
use super::{ReadError, WriteError};

#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    #[error(transparent)]
    Read(#[from] ReadError),
    #[error(transparent)]
    Write(#[from] WriteError),
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    #[error("JSON in the log does not parse or serialize")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Upcast(#[from] UpcastError),
    #[error("the log is corrupt: {0}")]
    Corrupt(String),
    #[error("the stream already has its writer")]
    WriterTaken,
}

/// What the log keeps of a stream before its checkpoint (design 5.4.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retention {
    /// The latest this many events before the checkpoint.
    pub events: u64,
    /// And every event younger than this.
    pub age: Duration,
}

impl Retention {
    pub const DESIGN: Self = Self {
        events: 10_000,
        age: Duration::from_secs(7 * 24 * 60 * 60),
    };
}
