//! The raw frame queue and the writer of `raw.db` (design 5.2.3, 5.2.8).
//!
//! The wire tap queues every line as a [`RawFrame`] on a [`RawSink`]. The
//! queue holds at most [`RAW_QUEUE_CAPACITY`] frames; a frame that finds it
//! full is dropped and counted, so diagnostics never hold up the ACP read
//! path. A thread of its own takes the frames off the queue and commits up to
//! [`MAX_BATCH`] of them per transaction. Being the only writer of `raw.db`, it
//! also runs retention's [`RawWriter::prune`], after the frames queued before
//! it.
//!
//! `raw.db` has a thread of its own rather than a second connection on the
//! [`Writer`](super::Writer)'s thread: pruning it and `VACUUM` can take
//! seconds, and they must not hold up the event log.

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rusqlite::{Connection, TransactionBehavior};
use tokio::sync::oneshot;

use super::writer::{MAX_BATCH, WriteError};
use crate::quiescence::{ActorKind, Participant, Quiescence};

/// Raw frames waiting for the writer (design 5.2.3).
pub const RAW_QUEUE_CAPACITY: usize = 4096;

/// The `raw.db` writer thread's kind in the quiescence barrier. The host
/// registers it before [`RawWriter::spawn`].
pub const RAW_WRITER: ActorKind = ActorKind::new("store_raw_writer");

/// How many of the oldest frames a prune over the size cap deletes at a time.
const PRUNE_STEP: usize = 256;

/// One line as `raw.db` keeps it (design 5.4.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawFrame {
    pub instance: Arc<str>,
    /// Counts the instance's lines in both directions from 1.
    pub seq: u64,
    /// Wall-clock milliseconds.
    pub ts: u64,
    pub dir: Dir,
    /// The first `sessionId` in the line, for exporting one session's frames.
    pub agent_session_id: Option<String>,
    /// The line after structured redaction.
    pub frame: String,
}

/// Which way a line went (`raw_frames.dir`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    /// Host to adapter.
    Send,
    /// Adapter to host.
    Recv,
    /// A line the adapter wrote that is not JSON; the SDK never sees it.
    RecvNonjson,
}

impl Dir {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Send => "send",
            Self::Recv => "recv",
            Self::RecvNonjson => "recv_nonjson",
        }
    }
}

/// How much of `raw.db` a prune keeps (design 5.4.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawRetention {
    /// Frames older than this go.
    pub max_age: Duration,
    /// Beyond this many bytes of pages in use, the oldest frames go. Deleting
    /// rows does not shrink the file until `VACUUM`, so pages on the freelist
    /// do not count.
    pub max_bytes: u64,
}

/// What a prune did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawPruned {
    pub deleted: usize,
    pub vacuumed: bool,
}

enum Message {
    Frame(RawFrame),
    Prune {
        retention: RawRetention,
        now_ms: u64,
        reply: oneshot::Sender<rusqlite::Result<RawPruned>>,
    },
    Stop,
}

/// What the two ends of the queue share.
#[derive(Debug, Default)]
struct Counts {
    /// Frames on the queue. Other messages do not count against the capacity.
    queued: AtomicUsize,
    dropped: AtomicU64,
}

impl Counts {
    fn lose(&self, frames: usize) {
        self.dropped.fetch_add(frames as u64, Ordering::Relaxed);
    }
}

/// The sending side of the raw frame queue, shared by every instance's tap.
#[derive(Debug, Clone)]
pub struct RawSink {
    queue: mpsc::Sender<Message>,
    counts: Arc<Counts>,
}

impl RawSink {
    /// Frames that never reached `raw.db`: the queue was full, the writer had
    /// stopped, or their write failed.
    pub fn dropped(&self) -> u64 {
        self.counts.dropped.load(Ordering::Relaxed)
    }

    /// Queues `frame`, or drops and counts it if the queue is full. Never
    /// blocks.
    pub fn push(&self, frame: RawFrame) {
        if self.counts.queued.fetch_add(1, Ordering::AcqRel) >= RAW_QUEUE_CAPACITY
            || self.queue.send(Message::Frame(frame)).is_err()
        {
            self.counts.queued.fetch_sub(1, Ordering::AcqRel);
            self.counts.lose(1);
        }
    }
}

fn queue() -> (RawSink, Receiver<Message>) {
    let (queue, inbox) = mpsc::channel();
    let counts = Arc::default();
    (RawSink { queue, counts }, inbox)
}

/// Hands retention's work to the `raw.db` writer. Clones share the thread.
#[derive(Debug, Clone)]
pub struct RawWriter {
    sink: RawSink,
}

impl RawWriter {
    /// Starts the writer thread on `conn`, an `open_writer(_, Database::Raw)`
    /// connection that [`prepare_raw`](super::prepare_raw) has brought to the
    /// schema, and enters it into the quiescence barrier as [`RAW_WRITER`] for
    /// as long as it runs.
    pub fn spawn(conn: Connection, quiescence: &Quiescence) -> io::Result<(Self, RawWriterThread)> {
        let (sink, inbox) = queue();
        let thread = start(conn, inbox, &sink, quiescence.enter(RAW_WRITER))?;
        let queue = sink.queue.clone();
        Ok((Self { sink }, RawWriterThread { queue, thread }))
    }

    /// The sending side of the queue, for the taps.
    pub fn sink(&self) -> RawSink {
        self.sink.clone()
    }

    /// Once the frames queued before this call are committed, deletes the
    /// frames `retention` lets go as of `now_ms`, the oldest first. Then, if
    /// at least a quarter of the file is free pages and no frame is waiting,
    /// runs `VACUUM`.
    pub async fn prune(
        &self,
        retention: RawRetention,
        now_ms: u64,
    ) -> Result<RawPruned, WriteError> {
        let (reply, answer) = oneshot::channel();
        let prune = Message::Prune {
            retention,
            now_ms,
            reply,
        };
        // If the thread has stopped, the reply is dropped with the message.
        let _ = self.sink.queue.send(prune);
        answer
            .await
            .map_err(|_| WriteError::Stopped)?
            .map_err(WriteError::Op)
    }
}

fn start(
    conn: Connection,
    inbox: Receiver<Message>,
    sink: &RawSink,
    participant: Participant,
) -> io::Result<JoinHandle<()>> {
    let batcher = RawBatcher {
        conn,
        counts: Arc::clone(&sink.counts),
    };
    thread::Builder::new()
        .name("store-raw-writer".to_owned())
        .spawn(move || batcher.run(&inbox, participant))
}

/// The running `raw.db` writer thread.
#[derive(Debug)]
pub struct RawWriterThread {
    queue: mpsc::Sender<Message>,
    thread: JoinHandle<()>,
}

impl RawWriterThread {
    /// Commits the frames queued before this call, then ends the thread;
    /// frames pushed later are dropped and counted. Blocks until the thread
    /// has ended; `Err` carries the panic of a thread that died.
    pub fn stop(self) -> thread::Result<()> {
        let _ = self.queue.send(Message::Stop);
        self.thread.join()
    }
}

struct RawBatcher {
    conn: Connection,
    counts: Arc<Counts>,
}

impl RawBatcher {
    fn run(mut self, inbox: &Receiver<Message>, participant: Participant) {
        let _participant = participant;
        let mut next = inbox.recv().ok();
        while let Some(message) = next.take() {
            match message {
                Message::Frame(first) => {
                    let (frames, after) = self.collect(inbox, first);
                    self.write(&frames);
                    next = after;
                }
                Message::Prune {
                    retention,
                    now_ms,
                    reply,
                } => {
                    // The caller may no longer be waiting.
                    let _ = reply.send(self.prune(retention, now_ms));
                }
                Message::Stop => {
                    let late = inbox.try_iter().filter(|m| matches!(m, Message::Frame(_)));
                    self.counts.lose(late.count());
                    return;
                }
            }
            if next.is_none() {
                next = inbox.recv().ok();
            }
        }
    }

    /// `first` and the frames queued after it, up to [`MAX_BATCH`]; and the
    /// message that ended the batch, if it is not a frame.
    fn collect(
        &self,
        inbox: &Receiver<Message>,
        first: RawFrame,
    ) -> (Vec<RawFrame>, Option<Message>) {
        let mut frames = vec![first];
        let after = loop {
            if frames.len() == MAX_BATCH {
                break None;
            }
            match inbox.try_recv() {
                Ok(Message::Frame(frame)) => frames.push(frame),
                Ok(other) => break Some(other),
                Err(_) => break None,
            }
        };
        self.counts.queued.fetch_sub(frames.len(), Ordering::AcqRel);
        (frames, after)
    }

    fn write(&mut self, frames: &[RawFrame]) {
        let lost = self.insert(frames).unwrap_or(frames.len());
        self.counts.lose(lost);
    }

    /// Inserts `frames` in one transaction. A frame whose insert fails is
    /// left out; returns how many were. `Err` if the transaction failed, which
    /// loses them all.
    fn insert(&mut self, frames: &[RawFrame]) -> rusqlite::Result<usize> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut failed = 0;
        {
            let mut insert = tx.prepare(
                "INSERT INTO raw_frames (instance_id, seq, ts, dir, agent_session_id, frame)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for f in frames {
                let row = (
                    &*f.instance,
                    f.seq,
                    f.ts,
                    f.dir.as_str(),
                    f.agent_session_id.as_deref(),
                    &f.frame,
                );
                if let Err(err) = insert.execute(row) {
                    // Some errors (a full disk, I/O) end the transaction.
                    if tx.is_autocommit() {
                        return Err(err);
                    }
                    failed += 1;
                }
            }
        }
        tx.commit()?;
        Ok(failed)
    }

    fn prune(&mut self, retention: RawRetention, now_ms: u64) -> rusqlite::Result<RawPruned> {
        let max_age_ms = u64::try_from(retention.max_age.as_millis()).unwrap_or(u64::MAX);
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut deleted = tx.execute(
            "DELETE FROM raw_frames WHERE ts < ?1",
            [now_ms.saturating_sub(max_age_ms)],
        )?;
        while bytes_in_use(&tx)? > retention.max_bytes {
            let oldest = tx.execute(
                "DELETE FROM raw_frames WHERE (instance_id, seq) IN
                   (SELECT instance_id, seq FROM raw_frames ORDER BY ts LIMIT ?1)",
                [PRUNE_STEP],
            )?;
            if oldest == 0 {
                break;
            }
            deleted += oldest;
        }
        tx.commit()?;
        let (pages, free) = (
            pragma(&self.conn, "page_count")?,
            pragma(&self.conn, "freelist_count")?,
        );
        let idle = self.counts.queued.load(Ordering::Acquire) == 0;
        let vacuumed = idle && free > 0 && free * 4 >= pages;
        if vacuumed {
            self.conn.execute_batch("VACUUM")?;
        }
        Ok(RawPruned { deleted, vacuumed })
    }
}

fn pragma(conn: &Connection, name: &str) -> rusqlite::Result<u64> {
    conn.pragma_query_value(None, name, |row| row.get(0))
}

fn bytes_in_use(conn: &Connection) -> rusqlite::Result<u64> {
    let pages = pragma(conn, "page_count")? - pragma(conn, "freelist_count")?;
    Ok(pages * pragma(conn, "page_size")?)
}

/// For tests elsewhere: a sink with no writer behind it, and its queue.
#[cfg(test)]
impl RawSink {
    pub(crate) fn unattended() -> (Self, Unattended) {
        let (sink, inbox) = queue();
        (sink, Unattended(inbox))
    }
}

#[cfg(test)]
pub(crate) struct Unattended(Receiver<Message>);

#[cfg(test)]
impl Unattended {
    /// The frames queued so far.
    pub(crate) fn frames(&self) -> Vec<RawFrame> {
        let frames = self.0.try_iter().filter_map(|message| match message {
            Message::Frame(frame) => Some(frame),
            _ => None,
        });
        frames.collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::testutil::{TempDir, raw_db, raw_frame, raw_rows};

    fn spawn_on(dir: &TempDir, sink: &RawSink, inbox: Receiver<Message>) -> RawWriterThread {
        let quiescence = Quiescence::new();
        quiescence.register(RAW_WRITER);
        let thread = start(raw_db(dir), inbox, sink, quiescence.enter(RAW_WRITER)).unwrap();
        RawWriterThread {
            queue: sink.queue.clone(),
            thread,
        }
    }

    #[test]
    fn unit_raw_queue_full_queue_drops_and_counts_without_blocking() {
        let dir = TempDir::new("raw-queue-full");
        let (sink, inbox) = queue();
        // Nothing drains the queue yet: a blocking push would hang here.
        for seq in 1..=RAW_QUEUE_CAPACITY as u64 + 2 {
            sink.push(raw_frame(seq, 7, "{}"));
        }
        assert_eq!(sink.dropped(), 2);

        let thread = spawn_on(&dir, &sink, inbox);
        thread.stop().unwrap();
        let rows = raw_rows(&dir);
        assert_eq!(rows.len(), RAW_QUEUE_CAPACITY);
        assert_eq!(rows[0], raw_frame(1, 7, "{}"));
        assert!(rows.iter().zip(1..).all(|(row, seq)| row.seq == seq));
        assert_eq!(sink.dropped(), 2);

        // The writer has stopped: what comes now is lost and counted.
        sink.push(raw_frame(9999, 7, "{}"));
        assert_eq!(sink.dropped(), 3);
    }

    #[test]
    fn unit_raw_queue_batches_end_at_max_batch_or_a_control_message() {
        let (sink, inbox) = queue();
        for seq in 1..=300 {
            sink.push(raw_frame(seq, 7, "{}"));
        }
        let (reply, _answer) = oneshot::channel();
        let retention = RawRetention {
            max_age: Duration::ZERO,
            max_bytes: 0,
        };
        let prune = Message::Prune {
            retention,
            now_ms: 0,
            reply,
        };
        sink.queue.send(prune).unwrap();
        let batcher = RawBatcher {
            conn: Connection::open_in_memory().unwrap(),
            counts: Arc::clone(&sink.counts),
        };
        let first = |message| match message {
            Ok(Message::Frame(frame)) => frame,
            _ => panic!("expected a frame"),
        };

        let (frames, after) = batcher.collect(&inbox, first(inbox.recv()));
        assert_eq!((frames.len(), after.is_none()), (MAX_BATCH, true));
        assert_eq!(sink.counts.queued.load(Ordering::Acquire), 300 - MAX_BATCH);
        let (frames, after) = batcher.collect(&inbox, first(inbox.recv()));
        assert_eq!(frames.len(), 300 - MAX_BATCH);
        assert!(matches!(after, Some(Message::Prune { .. })));
        assert_eq!(sink.counts.queued.load(Ordering::Acquire), 0);

        // A frame behind a stop request is lost, and counted.
        sink.queue.send(Message::Stop).unwrap();
        sink.push(raw_frame(301, 7, "{}"));
        let quiescence = Quiescence::new();
        quiescence.register(RAW_WRITER);
        batcher.run(&inbox, quiescence.enter(RAW_WRITER));
        assert_eq!(sink.dropped(), 1);
    }

    #[test]
    fn unit_raw_queue_a_failed_frame_loses_only_itself() {
        let dir = TempDir::new("raw-queue-dup");
        let (sink, inbox) = queue();
        // The third frame repeats the first one's key.
        for seq in [1, 2, 1, 3] {
            sink.push(raw_frame(seq, 7, "{}"));
        }
        spawn_on(&dir, &sink, inbox).stop().unwrap();
        let seqs: Vec<u64> = raw_rows(&dir).iter().map(|row| row.seq).collect();
        assert_eq!(seqs, [1, 2, 3]);
        assert_eq!(sink.dropped(), 1);
    }
}
