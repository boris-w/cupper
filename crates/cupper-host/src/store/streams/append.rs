//! A stream's one writer and the three steps of an append (design 5.2.3):
//!
//! 1. The writer gives the events their seqs and serializes them. When the
//!    batch completes a turn, it also builds the view after the batch, the
//!    turn's snapshot and the checkpoint. No lock is held: only this writer
//!    changes the view, so a copy taken now is still current at step 3.
//! 2. The writer thread commits all of it in one transaction, together with
//!    the batch's own rows; the writer awaits the receipt, holding no lock.
//! 3. Under the stream lock the view moves on, the head advances and the
//!    subscribers get the events.
//!
//! If the host dies between steps 2 and 3, the next start loads the stream
//! from the log, where the batch is committed whole.

use std::sync::Arc;

use cupper_protocol::annotations::TimestampMs;
use cupper_protocol::envelope::{Cause, Event};
use cupper_protocol::event::EventBody;
use cupper_protocol::ids::TurnId;
use cupper_protocol::limits::MAX_SAFE_INTEGER;
use cupper_protocol::sync::StreamId;
use cupper_protocol::view::{SessionView, ViewError};
use rusqlite::Connection;

use super::state::{Committed, Published};
use super::{StreamError, StreamHandle};
use crate::failpoint;
use crate::store::Durability;
use crate::store::log::{self, TurnSnapshot};
use crate::store::upcast::Upcasters;

/// The index stream has no turns; it checkpoints once this many events have
/// come since its last checkpoint, so that loading it and its log stay short.
pub const INDEX_CHECKPOINT_EVERY: u64 = 1_000;

type Write = Box<dyn FnOnce(&Connection) -> rusqlite::Result<()> + Send>;

/// Events to append in one transaction: `Batch { bodies, ..Batch::default() }`.
#[derive(Default)]
pub struct Batch {
    pub bodies: Vec<EventBody>,
    /// The command behind every event of the batch.
    pub cause: Option<Cause>,
    pub durability: Durability,
    /// Runs in the batch's transaction after the events: the rows they go
    /// with, such as the command's result (design 5.2.3, rule 3). If it
    /// fails, nothing of the batch is committed. Without events it runs in
    /// a transaction of its own.
    pub with: Option<Write>,
}

/// An append that committed and was published.
#[derive(Debug)]
pub struct Appended {
    pub head: u64,
    /// Events the view could not apply. They are in the log and went out;
    /// every replay skips them alike. A writer that breaks the view's rules
    /// has a bug to log.
    pub rejected: Vec<(u64, ViewError)>,
}

/// The one writer of a stream (design 5.2.3, rule 1), from
/// [`StreamHandle::writer`]. Dropping it lets another writer in.
#[derive(Debug)]
pub struct StreamWriter {
    handle: StreamHandle,
    next_seq: u64,
    checkpoint_at: u64,
}

impl StreamWriter {
    pub(super) fn new(handle: StreamHandle, next_seq: u64, checkpoint_at: u64) -> Self {
        Self {
            handle,
            next_seq,
            checkpoint_at,
        }
    }

    pub fn head(&self) -> u64 {
        self.next_seq - 1
    }

    /// Appends `batch` in the three steps. On an error nothing was committed
    /// or published, and the seqs are given out again. A batch without
    /// events commits only its `with`, if it has one.
    pub async fn append(&mut self, batch: Batch) -> Result<Appended, StreamError> {
        let Batch {
            bodies,
            cause,
            durability,
            with,
        } = batch;
        if bodies.is_empty() {
            if let Some(with) = with {
                self.handle
                    .registry()
                    .writer
                    .submit(durability, with)
                    .await?;
            }
            return Ok(Appended {
                head: self.head(),
                rejected: Vec::new(),
            });
        }
        let registry = self.handle.registry();
        let now = registry.clock.now_wall_ms();

        // Step 1.
        let events = publishable(bodies, self.next_seq, now, cause)?;
        let last = events.last().expect("not empty").event.seq;
        let mut rejected = Vec::new();
        let mut snapshots = Vec::new();
        let mut checkpoint = None;
        if self.checkpoint_due(&events, last) {
            let mut view = self.handle.stream().read(|view, _| view.clone());
            for published in &events {
                let mut ended = |session_view: &SessionView, turn: &TurnId| {
                    if let StreamId::Session(session) = self.handle.id() {
                        snapshots.extend(TurnSnapshot::of(session, session_view, turn));
                    }
                };
                if let Err(err) = view.fold(&published.event.body, &mut ended) {
                    rejected.push((published.event.seq, err));
                }
            }
            checkpoint = Some((view.to_json()?, view));
        }
        let snapshots = snapshots
            .into_iter()
            .collect::<serde_json::Result<Vec<_>>>()?;

        // Step 2.
        let stream = self.handle.id().to_string();
        let (rows, retention) = (events.clone(), registry.config.retention);
        let json = checkpoint.as_ref().map(|(json, _)| json.clone());
        let receipt = registry.writer.submit(durability, move |conn| {
            log::insert_events(conn, &stream, &rows)?;
            for snapshot in &snapshots {
                snapshot.insert(conn)?;
            }
            let mut pruned = None;
            if let Some(json) = json {
                log::save_checkpoint(conn, &stream, last, &json, now)?;
                pruned = log::prune(conn, &stream, last, &retention, now)?;
            }
            if let Some(with) = with {
                with(conn)?;
            }
            Ok(pruned)
        });
        let pruned = receipt.await?;
        failpoint!(STORE_BEFORE_PUBLISH);

        // Step 3.
        let view = checkpoint.map(|(_, view)| view);
        if view.is_some() {
            self.checkpoint_at = last;
        }
        let committed = Committed {
            events,
            view,
            pruned,
        };
        rejected.extend(self.handle.stream().publish(committed));
        self.next_seq = last + 1;
        Ok(Appended {
            head: last,
            rejected,
        })
    }

    /// Writes the checkpoint of the view at the head now, unless one stands
    /// there already: the orderly exit's for every stream (design 5.2.8).
    pub async fn checkpoint(&mut self) -> Result<(), StreamError> {
        let head = self.head();
        if head == self.checkpoint_at {
            return Ok(());
        }
        let registry = self.handle.registry();
        let json = self.handle.stream().read(|view, _| view.to_json())?;
        let (stream, now) = (self.handle.id().to_string(), registry.clock.now_wall_ms());
        let saved = registry.writer.submit(Durability::Normal, move |conn| {
            log::save_checkpoint(conn, &stream, head, &json, now)
        });
        saved.await?;
        self.checkpoint_at = head;
        Ok(())
    }

    /// A session stream checkpoints when a turn completes (design 5.2.8); the
    /// index stream every [`INDEX_CHECKPOINT_EVERY`] events.
    fn checkpoint_due(&self, events: &[Arc<Published>], last: u64) -> bool {
        match self.handle.id() {
            StreamId::Session(_) => events
                .iter()
                .any(|p| matches!(p.event.body, EventBody::TurnCompleted(_))),
            StreamId::Index => last - self.checkpoint_at >= INDEX_CHECKPOINT_EVERY,
        }
    }
}

impl Drop for StreamWriter {
    fn drop(&mut self) {
        self.handle.stream().detach_writer();
    }
}

/// The events of `bodies` from seq `first` on, each at its type's current
/// version and serialized once.
fn publishable(
    bodies: Vec<EventBody>,
    first: u64,
    now_ms: u64,
    cause: Option<Cause>,
) -> serde_json::Result<Vec<Arc<Published>>> {
    let upcasters = Upcasters::host();
    bodies
        .into_iter()
        .zip(first..)
        .map(|(body, seq)| {
            assert!(seq <= MAX_SAFE_INTEGER, "seq {seq} is past 2^53 - 1");
            let event = Event {
                seq,
                from: None,
                v: upcasters.current(body.tag()),
                ts: TimestampMs(now_ms),
                cause: cause.clone(),
                fallback: None,
                body,
            };
            let json = serde_json::to_string(&event)?.into_boxed_str();
            Ok(Arc::new(Published { event, json }))
        })
        .collect()
}
