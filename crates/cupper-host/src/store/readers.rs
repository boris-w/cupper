//! The read pool (design 5.2.3, 5.2.8): [`Readers::SIZE`] read-only
//! connections for catch-up, history pages and queries. WAL lets them read
//! while the writer commits. Each read runs under `spawn_blocking`, so SQLite
//! never blocks an async worker.

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use rusqlite::Connection;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::connection::{OpenError, open_reader};

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    /// The runtime is shutting down and dropped the read before it ran.
    #[error("the read was cancelled by the runtime shutting down")]
    Cancelled,
}

/// The pool. Clones share it.
#[derive(Debug, Clone)]
pub struct Readers {
    shared: Arc<Pool>,
}

#[derive(Debug)]
struct Pool {
    /// One permit per idle connection: a holder always finds one in `idle`.
    permits: Arc<Semaphore>,
    idle: Mutex<Vec<Connection>>,
}

impl Readers {
    pub const SIZE: usize = 4;

    /// Opens the connections to `path`, which the writer has already opened.
    /// Blocking I/O: call it from startup or a blocking thread.
    pub fn open(path: &Path) -> Result<Self, OpenError> {
        let idle = (0..Self::SIZE)
            .map(|_| open_reader(path))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            shared: Arc::new(Pool {
                permits: Arc::new(Semaphore::new(Self::SIZE)),
                idle: Mutex::new(idle),
            }),
        })
    }

    /// Runs `read` on an idle connection on a blocking thread, waiting while
    /// all of them are busy. A panic in `read` resumes in the caller; the
    /// connection goes back to the pool either way. Must be awaited inside the
    /// tokio runtime.
    pub async fn read<T, F>(&self, read: F) -> Result<T, ReadError>
    where
        T: Send + 'static,
        F: FnOnce(&Connection) -> rusqlite::Result<T> + Send + 'static,
    {
        let permit = Arc::clone(&self.shared.permits)
            .acquire_owned()
            .await
            .expect("the pool never closes its semaphore");
        let lease = Lease {
            conn: Some(
                self.shared
                    .lock()
                    .pop()
                    .expect("a permit guarantees an idle connection"),
            ),
            pool: Arc::clone(&self.shared),
            _permit: permit,
        };
        let joined = tokio::task::spawn_blocking(move || read(lease.conn())).await;
        match joined {
            Ok(result) => Ok(result?),
            Err(err) if err.is_panic() => std::panic::resume_unwind(err.into_panic()),
            Err(_) => Err(ReadError::Cancelled),
        }
    }
}

impl Pool {
    /// Every critical section leaves the list consistent, so a poisoned lock
    /// is still usable.
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Connection>> {
        self.idle.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A borrowed connection. Dropping it, also while a panic unwinds, returns
/// the connection to the pool and then releases the permit.
struct Lease {
    conn: Option<Connection>,
    pool: Arc<Pool>,
    _permit: OwnedSemaphorePermit,
}

impl Lease {
    fn conn(&self) -> &Connection {
        self.conn
            .as_ref()
            .expect("a lease holds its connection until dropped")
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            self.pool.lock().push(conn);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use super::*;
    use crate::store::testutil::TempDir;
    use crate::store::{Database, open_writer};

    #[test]
    fn store_readers_lend_each_connection_to_one_read_at_a_time() {
        let dir = TempDir::new("readers");
        let writer = open_writer(&dir.host_db(), Database::Host).unwrap();
        writer
            .execute_batch("CREATE TABLE t (v INTEGER); INSERT INTO t VALUES (7);")
            .unwrap();
        let readers = Readers::open(&dir.host_db()).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();

        // Each read reports that it runs, then waits for its release.
        let (started, starts) = mpsc::channel();
        let read = |id: usize| {
            let (release, released) = mpsc::channel::<()>();
            let (readers, started, handle) =
                (readers.clone(), started.clone(), runtime.handle().clone());
            let reading = thread::spawn(move || {
                handle.block_on(readers.read(move |conn| {
                    started.send(id).unwrap();
                    let _ = released.recv();
                    conn.query_row("SELECT v FROM t", [], |row| row.get::<_, i64>(0))
                }))
            });
            (release, reading)
        };
        let wait = Duration::from_secs(10);

        let mut busy: Vec<_> = (0..Readers::SIZE).map(read).collect();
        for _ in 0..Readers::SIZE {
            starts.recv_timeout(wait).expect("four reads run at once");
        }
        let (fifth_release, fifth) = read(Readers::SIZE);
        assert!(
            starts.recv_timeout(Duration::from_millis(50)).is_err(),
            "a fifth read waits while all connections are busy"
        );
        let (release, first) = busy.remove(0);
        drop(release);
        assert_eq!(first.join().unwrap().unwrap(), 7);
        assert_eq!(starts.recv_timeout(wait).unwrap(), Readers::SIZE);

        drop(fifth_release);
        assert_eq!(fifth.join().unwrap().unwrap(), 7);
        for (release, reading) in busy {
            drop(release);
            assert_eq!(reading.join().unwrap().unwrap(), 7);
        }
    }

    #[test]
    fn store_readers_get_their_connection_back_from_a_panicking_read() {
        let dir = TempDir::new("readers-panic");
        drop(open_writer(&dir.host_db(), Database::Host).unwrap());
        let readers = Readers::open(&dir.host_db()).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();

        for _ in 0..=Readers::SIZE {
            let panicked = runtime.block_on(async {
                tokio::spawn({
                    let readers = readers.clone();
                    async move {
                        readers
                            .read(|_| -> rusqlite::Result<()> { panic!("read failed") })
                            .await
                    }
                })
                .await
            });
            assert!(panicked.unwrap_err().is_panic());
        }
        assert_eq!(readers.shared.lock().len(), Readers::SIZE);
        let err = runtime
            .block_on(readers.read(|conn| conn.execute_batch("SELECT * FROM missing")))
            .unwrap_err();
        assert!(matches!(err, ReadError::Sql(_)), "{err}");
    }
}
