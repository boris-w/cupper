//! Streams (design 5.2.3, 5.2.8): every stream's seqs, view and subscribers
//! over the event log, reworked from SP-4a's `hub`, whose invariant holds: a
//! subscriber sees every seq after its start once, in order.
//!
//! - One [`StreamWriter`] per stream appends in three steps (`append`).
//! - [`StreamHandle::subscribe`] reads the head and the view under the stream
//!   lock and registers a [`Sink`]. Framing and replay reads are T033's.
//! - [`Streams::open`] loads a stream lazily: its checkpoint and the events
//!   after it; for a checkpoint of another view version, the latest turn
//!   snapshots instead (design 5.4.6).
//! - A stream with no handle left leaves memory after
//!   [`StreamsConfig::idle`]; idle streams are swept whenever one is opened.

mod append;
mod state;
mod sync;
#[cfg(test)]
mod tests;
mod view;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use cupper_protocol::limits::MAX_SAFE_INTEGER;
use cupper_protocol::sync::{Resume, StreamId};
use cupper_protocol::view::Window;
use rusqlite::Connection;

pub use append::{Appended, Batch, INDEX_CHECKPOINT_EVERY, StreamWriter};
pub use state::{Published, Push, Sink, Start};
use state::{State, Stream};
use sync::{Mutex, MutexGuard, lock};
pub use view::StreamView;

use super::log;
use super::upcast::{CheckpointFate, UpcastError, checkpoint_fate};
use super::{ReadError, Readers, WriteError, Writer};
use crate::clock::{Clock, MonoTime};
use crate::durations::{Durations, STREAM_IDLE};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamsConfig {
    /// `meta.data_epoch` of this run, for subscriptions (design 5.2.8).
    pub epoch: u64,
    /// How long an unused stream stays in memory.
    pub idle: Duration,
    pub retention: Retention,
}

impl StreamsConfig {
    pub fn new(epoch: u64, durations: &Durations) -> Self {
        Self {
            epoch,
            idle: durations.get(STREAM_IDLE),
            retention: Retention::DESIGN,
        }
    }
}

/// The stream registry. Clones share it; the host has one.
#[derive(Debug, Clone)]
pub struct Streams {
    registry: Arc<Registry>,
}

#[derive(Debug)]
struct Registry {
    writer: Writer,
    readers: Readers,
    clock: Arc<dyn Clock>,
    config: StreamsConfig,
    /// Held to look up, register and sweep only; never during I/O.
    map: Mutex<Map>,
}

#[derive(Debug, Default)]
struct Map {
    loaded: HashMap<StreamId, Loaded>,
    /// Streams evicted so far: a load that saw this change may be stale.
    evictions: u64,
}

#[derive(Debug)]
struct Loaded {
    stream: Arc<Stream>,
    /// Since when no handle is left; `None` while one is.
    idle_since: Option<MonoTime>,
}

impl Streams {
    pub fn new(
        writer: Writer,
        readers: Readers,
        clock: Arc<dyn Clock>,
        config: StreamsConfig,
    ) -> Self {
        Self {
            registry: Arc::new(Registry {
                writer,
                readers,
                clock,
                config,
                map: Mutex::default(),
            }),
        }
    }

    /// The stream `id`, loaded from the log unless it is in memory.
    pub async fn open(&self, id: &StreamId) -> Result<StreamHandle, StreamError> {
        loop {
            let evictions = {
                let mut map = self.registry.lock();
                if let Some(handle) = self.registry.handle(&mut map, id) {
                    return Ok(handle);
                }
                map.evictions
            };
            let of = id.clone();
            let state = self
                .registry
                .readers
                .read(move |conn| Ok(load(conn, &of)))
                .await??;
            let mut map = self.registry.lock();
            if let Some(handle) = self.registry.handle(&mut map, id) {
                return Ok(handle);
            }
            // Otherwise a writer may have loaded, appended to and left the
            // stream while this load ran, which would then be behind the log.
            if map.evictions == evictions {
                let stream = Arc::new(Stream::new(id.clone(), state));
                let idle_since = None;
                map.loaded.insert(id.clone(), Loaded { stream, idle_since });
                return Ok(self.registry.handle(&mut map, id).expect("just loaded"));
            }
        }
    }
}

impl Registry {
    fn lock(&self) -> MutexGuard<'_, Map> {
        lock(&self.map)
    }

    /// A handle to `id`, if it is in memory once idle streams are evicted.
    fn handle(self: &Arc<Self>, map: &mut Map, id: &StreamId) -> Option<StreamHandle> {
        let now = self.clock.now_mono();
        let before = map.loaded.len();
        map.loaded.retain(|_, loaded| {
            let idle = loaded
                .idle_since
                .map(|since| now.saturating_duration_since(since));
            idle.is_none_or(|idle| idle < self.config.idle)
        });
        map.evictions += (before - map.loaded.len()) as u64;
        let loaded = map.loaded.get_mut(id)?;
        loaded.idle_since = None;
        Some(StreamHandle {
            stream: Some(Arc::clone(&loaded.stream)),
            registry: Arc::clone(self),
        })
    }
}

/// A stream in memory. While a handle, or a writer or subscription made from
/// one, is left, the stream stays loaded.
#[derive(Debug, Clone)]
pub struct StreamHandle {
    /// Taken only by `drop`.
    stream: Option<Arc<Stream>>,
    registry: Arc<Registry>,
}

impl StreamHandle {
    pub fn id(&self) -> &StreamId {
        &self.stream().id
    }

    /// The stream's one writer, unless another one has it.
    pub fn writer(&self) -> Result<StreamWriter, StreamError> {
        let (next_seq, checkpoint_at) = self
            .stream()
            .attach_writer()
            .ok_or(StreamError::WriterTaken)?;
        Ok(StreamWriter::new(self.clone(), next_seq, checkpoint_at))
    }

    /// Subscribes `sink` from `resume` (design 5.3.3). `start` runs under
    /// the stream lock and must only queue: the response, then the snapshot
    /// pages or a note to replay `at_seq + 1..=head_seq` from the log; live
    /// events reach `sink` after it.
    pub fn subscribe(
        &self,
        sink: Arc<dyn Sink>,
        resume: Option<Resume>,
        start: impl FnOnce(Start),
    ) -> Subscription {
        let epoch = self.registry.config.epoch;
        let id = self.stream().subscribe(sink, resume, epoch, start);
        Subscription {
            handle: self.clone(),
            id,
        }
    }

    /// Runs `read` on the view and the head, under the stream lock.
    pub fn read<T>(&self, read: impl FnOnce(&StreamView, u64) -> T) -> T {
        self.stream().read(read)
    }

    fn stream(&self) -> &Stream {
        self.stream.as_ref().expect("taken only by drop")
    }

    fn registry(&self) -> &Registry {
        &self.registry
    }
}

impl Drop for StreamHandle {
    /// The last handle starts the idle time. Handles are only made under the
    /// registry lock, and dropped under it here, so the count is exact.
    fn drop(&mut self) {
        let Some(stream) = self.stream.take() else {
            return;
        };
        let mut map = self.registry.lock();
        if let Some(loaded) = map.loaded.get_mut(&stream.id)
            && Arc::ptr_eq(&loaded.stream, &stream)
        {
            drop(stream);
            if Arc::strong_count(&loaded.stream) == 1 {
                loaded.idle_since = Some(self.registry.clock.now_mono());
            }
        }
    }
}

/// A registered subscriber. Dropping it unsubscribes.
#[derive(Debug)]
#[must_use = "dropping a subscription unsubscribes"]
pub struct Subscription {
    handle: StreamHandle,
    id: u64,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.handle.stream().unsubscribe(self.id);
    }
}

/// The stream `id` as the log has it: the checkpoint and the events after it,
/// read in one transaction.
fn load(conn: &Connection, id: &StreamId) -> Result<State, StreamError> {
    let read = conn.unchecked_transaction()?;
    let name = id.to_string();
    let mut view = StreamView::empty(id);
    let (mut after, mut checkpoint_at) = (0, 0);
    if let Some((at_seq, view_v, json)) = log::checkpoint(&read, &name)? {
        (after, checkpoint_at) = (at_seq, at_seq);
        match (checkpoint_fate(view_v), id) {
            (CheckpointFate::Use, _) => view = StreamView::from_json(id, &json)?,
            // The latest turns end where the checkpoint stands, written with
            // it. What only the view had (configuration, the tail, detached
            // requests) comes back with the events that set it next.
            (CheckpointFate::Discard, StreamId::Session(session)) => {
                for op in log::latest_turns(&read, session, Window::V1.turns)? {
                    let _ = view.fold(&op, &mut |_, _| {});
                }
            }
            // The index has no turns: what the log still keeps.
            (CheckpointFate::Discard, StreamId::Index) => after = 0,
        }
    }
    let mut head = checkpoint_at;
    log::read_events(&read, id, after, MAX_SAFE_INTEGER, |event| {
        head = head.max(event.seq);
        let _ = view.fold(&event.body, &mut |_, _| {});
    })?;
    let first_seq = log::first_seq(&read, &name)?.unwrap_or(head + 1);
    Ok(State::new(head, first_seq, checkpoint_at, view))
}
