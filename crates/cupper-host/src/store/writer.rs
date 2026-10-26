//! The store writer (design 5.2.3, 5.2.8): the one thread that writes to
//! `host.db`.
//!
//! Every writer in the host submits its operations here and awaits a
//! [`Receipt`]. The thread commits them in groups:
//!
//! - It takes what is queued, up to [`MAX_BATCH`] operations, and commits when
//!   the batch is full or the queue is empty. When the queue runs empty it
//!   first waits once, up to [`WriterConfig::linger`], for more. That wait only
//!   shapes throughput, so it is the one wait in the host that does not go
//!   through `Clock` (design 5.2.16).
//! - A batch is one transaction, and each operation runs in its own savepoint.
//!   An operation that fails rolls back its savepoint alone and gets its own
//!   error, [`WriteError::Op`]; the rest of the batch commits.
//! - If the transaction itself fails (`BEGIN`, a savepoint statement, `COMMIT`,
//!   or SQLite ending the transaction after an I/O error), the batch is lost:
//!   nothing in it is committed, and every operation in it that did not fail
//!   on its own gets [`WriteError::Batch`].
//! - A batch commits with the highest [`Durability`] among its operations.
//! - Receipts are answered once the batch is settled.
//!
//! An operation must not end the transaction or release savepoints it did not
//! create.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::task::{Context, Poll};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use std::vec;

use rusqlite::Connection;
use tokio::sync::oneshot;

use super::connection::{Durability, set_durability};
use crate::durations::{Durations, STORE_BATCH_LINGER};
use crate::quiescence::{ActorKind, Hold, Participant, Quiescence};
use crate::startup::payload::Payload;

/// The most operations one batch commits.
pub const MAX_BATCH: usize = 256;

/// The writer thread's kind in the quiescence barrier. The host registers it
/// before [`Writer::spawn`].
pub const WRITER: ActorKind = ActorKind::new("store_writer");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriterConfig {
    /// How long the writer waits, once, for more operations after the queue
    /// runs empty. Zero commits at once.
    pub linger: Duration,
}

impl WriterConfig {
    /// For the host that `payload` starts. On `SystemClock` the writer lingers
    /// for `store_batch_linger`; on `ManualClock` (`test_config.clock: manual`)
    /// it never waits, so tests do not run on real time, but batching stays on
    /// (design review D12). The clock kind comes from the startup payload
    /// rather than from `Clock`: the linger is not a `Clock` timer, and a
    /// "virtual time?" query on `Clock` would invite code paths that differ
    /// between tests and production.
    pub fn for_startup(payload: &Payload, durations: &Durations) -> Self {
        #[cfg(feature = "test-harness")]
        if payload.test_config.clock == crate::startup::test_config::ClockKind::Manual {
            return Self {
                linger: Duration::ZERO,
            };
        }
        #[cfg(not(feature = "test-harness"))]
        let _ = payload;
        Self {
            linger: durations.get(STORE_BATCH_LINGER),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// The operation failed: its savepoint was rolled back, the rest of its
    /// batch was not affected.
    #[error("the write operation failed")]
    Op(#[source] rusqlite::Error),
    /// The batch's transaction failed: nothing in it was committed.
    #[error("the write batch was not committed")]
    Batch(#[source] Arc<rusqlite::Error>),
    /// The writer thread has stopped; the operation never ran.
    #[error("the store writer has stopped")]
    Stopped,
}

/// The answer to one submitted operation: its value once its batch committed,
/// or why it was not committed.
#[derive(Debug)]
#[must_use = "a receipt is the only way to learn whether the operation was committed"]
pub struct Receipt<T>(oneshot::Receiver<Result<T, WriteError>>);

impl<T> Receipt<T> {
    /// Blocks until the batch is settled. Panics on an async worker thread;
    /// there, await the receipt instead.
    pub fn wait(self) -> Result<T, WriteError> {
        self.0.blocking_recv().unwrap_or(Err(WriteError::Stopped))
    }
}

impl<T> Future for Receipt<T> {
    type Output = Result<T, WriteError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.get_mut().0)
            .poll(cx)
            .map(|answer| answer.unwrap_or(Err(WriteError::Stopped)))
    }
}

/// Submits operations to the writer thread. Clones share the thread.
#[derive(Debug, Clone)]
pub struct Writer {
    queue: mpsc::Sender<Message>,
    /// A submitted operation holds the barrier until its receipt is answered.
    quiescence: Quiescence,
}

impl Writer {
    /// Starts the writer thread on `conn`, an [`open_writer`] connection, and
    /// enters it into the quiescence barrier as [`WRITER`] for as long as it
    /// runs.
    ///
    /// [`open_writer`]: super::open_writer
    pub fn spawn(
        conn: Connection,
        config: WriterConfig,
        quiescence: &Quiescence,
    ) -> std::io::Result<(Self, WriterThread)> {
        let participant = quiescence.enter(WRITER);
        let (queue, inbox) = mpsc::channel();
        let batcher = Batcher {
            conn,
            linger: config.linger,
            durability: None,
        };
        // Dropped as the thread ends, a panic's unwinding included.
        let (alive, ended) = oneshot::channel::<()>();
        let thread = thread::Builder::new()
            .name("store-writer".to_owned())
            .spawn(move || {
                let _alive = alive;
                batcher.run(&inbox, participant);
            })?;
        let writer = Self {
            queue: queue.clone(),
            quiescence: quiescence.clone(),
        };
        let thread = WriterThread {
            queue,
            thread,
            ended,
        };
        Ok((writer, thread))
    }

    /// Queues `op` to run in its own savepoint in the next batch, committed
    /// with at least `durability`.
    pub fn submit<T, F>(&self, durability: Durability, op: F) -> Receipt<T>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> rusqlite::Result<T> + Send + 'static,
    {
        let (reply, receipt) = oneshot::channel();
        let hold = self.quiescence.hold();
        let job = Job {
            durability,
            op: Box::new(Submitted { op, reply, hold }),
        };
        // If the thread has stopped, the job comes back and is dropped with
        // its reply, and the receipt reads `Stopped`.
        let _ = self.queue.send(Message::Job(job));
        Receipt(receipt)
    }
}

/// The running writer thread.
#[derive(Debug)]
pub struct WriterThread {
    queue: mpsc::Sender<Message>,
    thread: JoinHandle<()>,
    ended: oneshot::Receiver<()>,
}

impl WriterThread {
    /// Completes once the thread has ended without [`stop`](Self::stop): an
    /// operation panicked. The host cannot go on without its writer and exits
    /// (design 5.2.13).
    pub async fn ended(&mut self) {
        let _ = (&mut self.ended).await;
    }

    /// Commits what was submitted before this call, then ends the thread;
    /// operations submitted later get [`WriteError::Stopped`]. Blocks until the
    /// thread has ended; `Err` carries the panic of a thread that died.
    pub fn stop(self) -> thread::Result<()> {
        let _ = self.queue.send(Message::Stop);
        self.thread.join()
    }
}

enum Message {
    Job(Job),
    Stop,
}

struct Job {
    durability: Durability,
    op: Box<dyn Operation>,
}

/// A submitted operation, its result type erased.
trait Operation: Send {
    /// Runs the operation inside its savepoint.
    fn run(self: Box<Self>, conn: &Connection) -> Ran;

    /// Answers without running: the batch was lost before the operation's turn.
    fn refuse(self: Box<Self>, err: Arc<rusqlite::Error>);
}

/// An operation that has run, waiting for its batch to settle.
struct Ran {
    succeeded: bool,
    /// Answers the receipt, given whether the batch committed.
    settle: Box<dyn FnOnce(Result<(), Arc<rusqlite::Error>>) + Send>,
}

struct Submitted<F, T> {
    op: F,
    reply: oneshot::Sender<Result<T, WriteError>>,
    /// Let go once the receipt is answered.
    hold: Hold,
}

impl<F, T> Operation for Submitted<F, T>
where
    T: Send + 'static,
    F: FnOnce(&Connection) -> rusqlite::Result<T> + Send + 'static,
{
    fn run(self: Box<Self>, conn: &Connection) -> Ran {
        let Self { op, reply, hold } = *self;
        let result = op(conn);
        Ran {
            succeeded: result.is_ok(),
            settle: Box::new(move |batch| {
                let answer = match result {
                    Ok(value) => batch.map(|()| value).map_err(WriteError::Batch),
                    Err(err) => Err(WriteError::Op(err)),
                };
                // The submitter may no longer be waiting.
                let _ = reply.send(answer);
                drop(hold);
            }),
        }
    }

    fn refuse(self: Box<Self>, err: Arc<rusqlite::Error>) {
        let _ = self.reply.send(Err(WriteError::Batch(err)));
    }
}

/// The writer thread's state.
struct Batcher {
    conn: Connection,
    linger: Duration,
    /// What the connection is set to; `None` until the first batch, or after a
    /// failed switch.
    durability: Option<Durability>,
}

impl Batcher {
    fn run(mut self, inbox: &Receiver<Message>, participant: Participant) {
        // Idle while it waits for work; each operation holds the barrier.
        while let Ok(Message::Job(first)) = participant.idle_blocking(|| inbox.recv()) {
            let mut batch = vec![first];
            let stop = self.collect(inbox, &mut batch);
            self.flush(batch);
            if stop {
                break;
            }
        }
    }

    /// Moves queued operations into `batch` until it is full or the queue is
    /// empty, lingering once. Returns whether a stop request came.
    fn collect(&self, inbox: &Receiver<Message>, batch: &mut Vec<Job>) -> bool {
        let mut lingered = self.linger.is_zero();
        while batch.len() < MAX_BATCH {
            let message = match inbox.try_recv() {
                Ok(message) => message,
                Err(TryRecvError::Empty) if !lingered => {
                    lingered = true;
                    let Ok(message) = inbox.recv_timeout(self.linger) else {
                        return false;
                    };
                    message
                }
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => return false,
            };
            match message {
                Message::Job(job) => batch.push(job),
                Message::Stop => return true,
            }
        }
        false
    }

    /// Commits `batch` as one transaction and answers every receipt.
    fn flush(&mut self, batch: Vec<Job>) {
        let durability = batch.iter().map(|job| job.durability).max();
        let mut queued = batch.into_iter();
        let mut ran = Vec::with_capacity(queued.len());
        let settled = self.commit(durability.unwrap_or_default(), &mut queued, &mut ran);
        if settled.is_ok() {
            crate::failpoint!(STORE_AFTER_COMMIT);
        }
        if settled.is_err() && !self.conn.is_autocommit() {
            // If even this fails, the next `BEGIN` fails and reports it.
            let _ = self.conn.execute_batch("ROLLBACK");
        }
        for done in ran {
            (done.settle)(settled.clone());
        }
        if let Err(err) = settled {
            for job in queued {
                job.op.refuse(err.clone());
            }
        }
    }

    fn commit(
        &mut self,
        durability: Durability,
        queued: &mut vec::IntoIter<Job>,
        ran: &mut Vec<Ran>,
    ) -> Result<(), Arc<rusqlite::Error>> {
        if self.durability != Some(durability) {
            self.durability = None;
            set_durability(&self.conn, durability)?;
            self.durability = Some(durability);
        }
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        for job in queued.by_ref() {
            if let Err(err) = self.conn.execute_batch("SAVEPOINT store_op") {
                let err = Arc::new(err);
                job.op.refuse(err.clone());
                return Err(err);
            }
            let done = job.op.run(&self.conn);
            let end = if done.succeeded {
                "RELEASE store_op"
            } else {
                "ROLLBACK TO store_op; RELEASE store_op"
            };
            ran.push(done);
            // Fails if the transaction is gone, e.g. after SQLite rolled it
            // back on an I/O error.
            self.conn.execute_batch(end)?;
        }
        crate::failpoint!(STORE_BEFORE_COMMIT);
        Ok(self.conn.execute_batch("COMMIT")?)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use rusqlite::ErrorCode;

    use super::*;
    use crate::store::testutil::{TempDir, pragmas};
    use crate::store::{Database, open_reader, open_writer};

    struct Fixture {
        dir: TempDir,
        quiescence: Quiescence,
        writer: Writer,
        thread: WriterThread,
    }

    fn start(name: &str, linger: Duration) -> Fixture {
        let dir = TempDir::new(name);
        let conn = open_writer(&dir.host_db(), Database::Host).unwrap();
        conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY)")
            .unwrap();
        let quiescence = Quiescence::new();
        quiescence.register(WRITER);
        let (writer, thread) = Writer::spawn(conn, WriterConfig { linger }, &quiescence).unwrap();
        Fixture {
            dir,
            quiescence,
            writer,
            thread,
        }
    }

    fn insert(writer: &Writer, id: i64) -> Receipt<i64> {
        writer.submit(Durability::Normal, move |conn| {
            conn.execute("INSERT INTO t (id) VALUES (?1)", [id])?;
            Ok(id)
        })
    }

    /// Ends the transaction from inside, as SQLite does after some I/O
    /// errors: the whole batch is lost.
    fn lose_batch(writer: &Writer) -> Receipt<()> {
        writer.submit(Durability::Normal, |conn| conn.execute_batch("ROLLBACK"))
    }

    /// Keeps the writer inside an operation, alone in its batch, until the
    /// returned sender is dropped. Operations submitted meanwhile queue up.
    fn hold(writer: &Writer) -> (Receipt<()>, mpsc::Sender<()>) {
        let (entered, inside) = mpsc::channel();
        let (release, released) = mpsc::channel::<()>();
        let receipt = writer.submit(Durability::Normal, move |_| {
            entered.send(()).unwrap();
            let _ = released.recv();
            Ok(())
        });
        inside.recv().unwrap();
        (receipt, release)
    }

    fn committed(db: &Path) -> Vec<i64> {
        let conn = open_reader(db).unwrap();
        let mut select = conn.prepare("SELECT id FROM t ORDER BY id").unwrap();
        select
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    fn assert_op_error(answer: Result<impl std::fmt::Debug, WriteError>, code: ErrorCode) {
        match answer {
            Err(WriteError::Op(err)) => assert_eq!(err.sqlite_error_code(), Some(code), "{err}"),
            other => panic!("expected an operation error, got {other:?}"),
        }
    }

    fn assert_lost(answer: Result<impl std::fmt::Debug, WriteError>) {
        assert!(matches!(answer, Err(WriteError::Batch(_))), "{answer:?}");
    }

    #[test]
    fn unit_writer_savepoint_failed_op_rolls_back_alone_in_its_batch() {
        let fx = start("savepoint", Duration::ZERO);
        let (held, release) = hold(&fx.writer);
        // One batch: the writer is busy with `hold` while these queue up.
        let first = insert(&fx.writer, 1);
        let failing = fx.writer.submit(Durability::Normal, |conn| {
            conn.execute("INSERT INTO t (id) VALUES (2)", [])?;
            conn.execute("INSERT INTO t (id) VALUES (1)", [])
        });
        let third = insert(&fx.writer, 3);
        drop(release);

        held.wait().unwrap();
        assert_eq!(first.wait().unwrap(), 1);
        assert_op_error(failing.wait(), ErrorCode::ConstraintViolation);
        assert_eq!(third.wait().unwrap(), 3);
        // The failing operation's own first insert went with its savepoint.
        assert_eq!(committed(&fx.dir.host_db()), [1, 3]);
        fx.thread.stop().unwrap();
    }

    #[test]
    fn unit_writer_savepoint_batch_closes_at_max_batch() {
        let fx = start("max-batch", Duration::ZERO);
        let (held, release) = hold(&fx.writer);
        // The last operation of the first full batch loses that batch; the
        // operations after it form the next batch and commit.
        let mut receipts: Vec<Receipt<()>> = (0..MAX_BATCH - 1)
            .map(|id| {
                let id = i64::try_from(id).unwrap();
                fx.writer.submit(Durability::Normal, move |conn| {
                    conn.execute("INSERT INTO t (id) VALUES (?1)", [id])
                        .map(drop)
                })
            })
            .collect();
        receipts.push(lose_batch(&fx.writer));
        let next: Vec<_> = (1000..1044).map(|id| insert(&fx.writer, id)).collect();
        drop(release);

        held.wait().unwrap();
        for answer in receipts.into_iter().map(Receipt::wait) {
            assert_lost(answer);
        }
        let next: Vec<i64> = next.into_iter().map(|r| r.wait().unwrap()).collect();
        assert_eq!(next, (1000..1044).collect::<Vec<_>>());
        assert_eq!(committed(&fx.dir.host_db()), next);
        fx.thread.stop().unwrap();
    }

    #[test]
    fn unit_writer_savepoint_lost_batch_fails_ops_that_never_ran() {
        let fx = start("lost", Duration::ZERO);
        let (held, release) = hold(&fx.writer);
        let before = insert(&fx.writer, 1);
        let failing = fx.writer.submit(Durability::Normal, |conn| {
            conn.execute("INSERT INTO t (id) VALUES (1)", []).map(drop)
        });
        let lose = lose_batch(&fx.writer);
        let after = insert(&fx.writer, 2);
        drop(release);

        held.wait().unwrap();
        assert_lost(before.wait());
        // An operation that failed on its own keeps its own error.
        assert_op_error(failing.wait(), ErrorCode::ConstraintViolation);
        assert_lost(lose.wait());
        assert_lost(after.wait());
        assert!(committed(&fx.dir.host_db()).is_empty());
        // The writer goes on with the next batch.
        assert_eq!(insert(&fx.writer, 3).wait().unwrap(), 3);
        assert_eq!(committed(&fx.dir.host_db()), [3]);
        fx.thread.stop().unwrap();
    }

    #[test]
    fn unit_writer_savepoint_empty_queue_flushes_without_lingering() {
        let fx = start("no-linger", Duration::ZERO);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        // Nothing else is queued, so the batch commits at once.
        assert_eq!(runtime.block_on(insert(&fx.writer, 1)).unwrap(), 1);
        assert_lost(lose_batch(&fx.writer).wait());
        assert_eq!(committed(&fx.dir.host_db()), [1]);
        fx.thread.stop().unwrap();
    }

    #[test]
    fn unit_writer_savepoint_empty_queue_lingers_for_more() {
        let noop = || Job {
            durability: Durability::Normal,
            op: Box::new(Submitted {
                op: |_: &Connection| Ok(()),
                reply: oneshot::channel().0,
                hold: Quiescence::new().hold(),
            }),
        };
        let batcher = Batcher {
            conn: Connection::open_in_memory().unwrap(),
            linger: Duration::from_secs(3600),
            durability: None,
        };
        let (queue, inbox) = mpsc::channel();
        let (go, ready) = mpsc::channel();
        let late = thread::spawn(move || {
            ready.recv().unwrap();
            queue.send(Message::Job(noop())).unwrap();
        });
        let mut batch = vec![noop()];
        go.send(()).unwrap();
        // The queue is almost always still empty here, and the late job
        // arrives while the batcher lingers. If it is quicker, the batcher
        // lingers after taking it until the closed queue ends the wait.
        assert!(!batcher.collect(&inbox, &mut batch));
        assert_eq!(batch.len(), 2);
        late.join().unwrap();
    }

    #[test]
    fn unit_writer_savepoint_stop_request_ends_the_linger() {
        let fx = start("linger-stop", Duration::from_secs(3600));
        let lone = insert(&fx.writer, 1);
        fx.thread.stop().unwrap();
        assert_eq!(lone.wait().unwrap(), 1);
        assert_eq!(committed(&fx.dir.host_db()), [1]);
    }

    #[test]
    fn store_durability_full_applies_to_its_whole_batch() {
        let fx = start("durability", Duration::ZERO);
        let read = |durability| {
            fx.writer.submit(durability, |conn| {
                Ok(pragmas(
                    conn,
                    &["synchronous", "fullfsync", "checkpoint_fullfsync"],
                ))
            })
        };
        let (held, release) = hold(&fx.writer);
        let normal = read(Durability::Normal);
        let full = read(Durability::Full);
        drop(release);
        held.wait().unwrap();
        // synchronous: 1 is NORMAL, 2 is FULL.
        assert_eq!(normal.wait().unwrap(), ["2", "1", "1"]);
        assert_eq!(full.wait().unwrap(), ["2", "1", "1"]);
        // The next batch is back to normal.
        assert_eq!(read(Durability::Normal).wait().unwrap(), ["1", "0", "1"]);
        fx.thread.stop().unwrap();
    }

    #[test]
    fn store_writer_thread_is_a_barrier_participant_until_it_stops() {
        let fx = start("stop", Duration::ZERO);
        assert_eq!(fx.quiescence.running(WRITER), 1);
        let queued = insert(&fx.writer, 1);
        fx.thread.stop().unwrap();
        assert_eq!(queued.wait().unwrap(), 1);
        assert_eq!(fx.quiescence.running(WRITER), 0);
        assert!(matches!(
            insert(&fx.writer, 2).wait(),
            Err(WriteError::Stopped)
        ));
        assert_eq!(committed(&fx.dir.host_db()), [1]);
    }

    #[tokio::test]
    async fn store_writer_thread_ends_when_an_operation_panics() {
        let mut fx = start("panic", Duration::ZERO);
        let receipt = fx
            .writer
            .submit(Durability::Normal, |_| -> rusqlite::Result<()> {
                panic!("a broken operation")
            });
        fx.thread.ended().await;
        assert!(matches!(receipt.await, Err(WriteError::Stopped)));
        assert!(fx.thread.stop().is_err());
        assert_eq!(fx.quiescence.running(WRITER), 0);
    }

    #[test]
    fn store_writer_config_lingers_only_on_the_system_clock() {
        let payload = Payload::default();
        let durations = Durations::default();
        assert_eq!(
            WriterConfig::for_startup(&payload, &durations).linger,
            Duration::from_millis(2)
        );
        #[cfg(feature = "test-harness")]
        {
            let mut manual = Payload::default();
            manual.test_config.clock = crate::startup::test_config::ClockKind::Manual;
            assert_eq!(
                WriterConfig::for_startup(&manual, &durations).linger,
                Duration::ZERO
            );
        }
    }
}
