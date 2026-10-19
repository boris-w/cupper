//! Content-addressed blobs (design 5.2.8, 5.4.1, 5.4.5): images, long outputs
//! and long diffs, each in a file `blobs/sha256/<first 2>/<hash>` of the data
//! directory, with a `blobs` row.
//!
//! - A put writes a temporary file under `blobs/tmp/`, syncs it, renames it
//!   into place and syncs the directory, so a crash midway leaves nothing at a
//!   blob's path. The same content is stored once.
//! - A get checks the content against its hash.
//! - A blob is reachable while a `blob_refs` row names it ([`add_ref`]).
//!   [`BlobStore::sweep`] removes the unreachable blobs not put again within a
//!   grace period, and any file without a row, which only a crash leaves.
//!   Puts and sweeps take turns, so a sweep never sees a put halfway.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs::{self, DirBuilder, File};
use std::io::{self, Write};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cupper_protocol::domain::BlobRef;
use cupper_protocol::ids::{BlobHash, SessionId};
use cupper_protocol::limits::V1;
use rusqlite::Connection;
use sha2::{Digest, Sha256};

use super::connection::Durability;
use super::writer::{WriteError, Writer};
use crate::clock::Clock;

#[derive(Debug, thiserror::Error)]
pub enum BlobError {
    /// Over `max_blob_bytes` (design 5.3.9).
    #[error("the blob has {size} bytes, more than {max}")]
    TooLarge { size: u64, max: u64 },
    /// The content does not hash to the value the put expected.
    #[error("the content does not match the expected hash")]
    Mismatch,
    #[error("no such blob")]
    NotFound,
    /// The stored file no longer matches its hash.
    #[error("the stored blob does not match its hash")]
    Corrupt,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Write(#[from] WriteError),
}

/// The blob store. Clones share it.
#[derive(Debug, Clone)]
pub struct BlobStore {
    shared: Arc<Shared>,
}

#[derive(Debug)]
struct Shared {
    root: PathBuf,
    writer: Writer,
    clock: Arc<dyn Clock>,
    /// Held by a put or a sweep for its whole run.
    turn: tokio::sync::Mutex<()>,
}

impl BlobStore {
    /// The store under `data_dir/blobs`, writing its rows through `writer`.
    pub fn new(data_dir: &Path, writer: Writer, clock: Arc<dyn Clock>) -> Self {
        let shared = Shared {
            root: data_dir.join("blobs"),
            writer,
            clock,
            turn: tokio::sync::Mutex::new(()),
        };
        Self {
            shared: Arc::new(shared),
        }
    }

    /// Stores `data` unless it is stored already, and marks it used now. With
    /// `expected`, refuses content that hashes to anything else.
    pub async fn put(
        &self,
        data: Vec<u8>,
        media_type: String,
        expected: Option<BlobHash>,
    ) -> Result<BlobRef, BlobError> {
        let (size, max) = (data.len() as u64, u64::from(V1.max_blob_bytes));
        if size > max {
            return Err(BlobError::TooLarge { size, max });
        }
        let _turn = self.shared.turn.lock().await;
        let root = self.shared.root.clone();
        let hash = blocking(move || {
            let hash = hash_of(&data);
            if expected.is_some_and(|expected| expected != hash) {
                return Err(BlobError::Mismatch);
            }
            store(&root, &hash, &data)?;
            Ok(hash)
        })
        .await?;
        let row = (hash.to_string(), size, media_type.clone());
        let now = self.shared.clock.now_wall_ms();
        let upsert = move |conn: &Connection| {
            conn.execute(
                "INSERT INTO blobs (hash, size, media_type, created_at, last_used_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)
                 ON CONFLICT (hash) DO UPDATE SET last_used_at = excluded.last_used_at",
                (row.0, row.1, row.2, now),
            )
        };
        self.shared
            .writer
            .submit(Durability::Normal, upsert)
            .await?;
        Ok(BlobRef {
            hash,
            size,
            media_type,
        })
    }

    /// The content of blob `hash`, checked against the hash.
    pub async fn get(&self, hash: &BlobHash) -> Result<Vec<u8>, BlobError> {
        let path = path_of(&self.shared.root, hash).ok_or(BlobError::NotFound)?;
        let hash = hash.clone();
        blocking(move || read_checked(&path, &hash)).await
    }

    /// Removes every blob that no `blob_refs` row names and that was last put
    /// more than `grace` ago, then every file under `blobs/` without a row.
    /// Returns how many files it removed.
    pub async fn sweep(&self, grace: Duration) -> Result<usize, BlobError> {
        let _turn = self.shared.turn.lock().await;
        let grace_ms = u64::try_from(grace.as_millis()).unwrap_or(u64::MAX);
        let cutoff = self.shared.clock.now_wall_ms().saturating_sub(grace_ms);
        let kept = self.shared.writer.submit(Durability::Normal, move |conn| {
            conn.execute(
                "DELETE FROM blobs
                 WHERE last_used_at < ?1 AND hash NOT IN (SELECT hash FROM blob_refs)",
                [cutoff],
            )?;
            let mut select = conn.prepare("SELECT hash FROM blobs")?;
            let hashes = select.query_map([], |row| row.get(0))?;
            hashes.collect::<rusqlite::Result<HashSet<String>>>()
        });
        let kept = kept.await?;
        let root = self.shared.root.clone();
        blocking(move || Ok(remove_unkept(&root, &kept)?)).await
    }
}

/// Records, in the caller's write operation, that session `owner` uses blob
/// `hash`, which keeps the blob for as long as the row stays. Returns false
/// when no blob has that hash, e.g. because a sweep removed it: the content
/// has to be put again.
pub fn add_ref(conn: &Connection, hash: &BlobHash, owner: &SessionId) -> rusqlite::Result<bool> {
    let known: bool = conn.query_row(
        "SELECT count(*) > 0 FROM blobs WHERE hash = ?1",
        [hash.as_str()],
        |row| row.get(0),
    )?;
    if known {
        conn.execute(
            "INSERT OR IGNORE INTO blob_refs (hash, owner) VALUES (?1, ?2)",
            [hash.as_str(), owner.as_str()],
        )?;
    }
    Ok(known)
}

fn hash_of(data: &[u8]) -> BlobHash {
    BlobHash::from_sha256(&Sha256::digest(data).into())
}

/// Where blob `hash` lives; `None` for anything but 64 lowercase hex digits,
/// so no hash names a path outside `blobs/sha256/`.
fn path_of(root: &Path, hash: &BlobHash) -> Option<PathBuf> {
    let hex = hash.as_str();
    let valid = hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    valid.then(|| root.join("sha256").join(&hex[..2]).join(hex))
}

fn read_checked(path: &Path, hash: &BlobHash) -> Result<Vec<u8>, BlobError> {
    let data = fs::read(path).map_err(|err| match err.kind() {
        io::ErrorKind::NotFound => BlobError::NotFound,
        _ => BlobError::Io(err),
    })?;
    if hash_of(&data) != *hash {
        return Err(BlobError::Corrupt);
    }
    Ok(data)
}

/// Puts `data` at its path, unless the file there is intact already.
fn store(root: &Path, hash: &BlobHash, data: &[u8]) -> io::Result<()> {
    let path = path_of(root, hash).expect("a computed hash is 64 hex digits");
    if read_checked(&path, hash).is_ok() {
        return Ok(());
    }
    let tmp = root.join("tmp");
    create_dir(&tmp)?;
    let partial = tmp.join(hash.as_str());
    let mut file = File::create(&partial)?;
    file.write_all(data)?;
    file.sync_all()?;
    let dir = path.parent().expect("a blob path has a directory");
    create_dir(dir)?;
    fs::rename(&partial, &path)?;
    File::open(dir)?.sync_all()
}

fn create_dir(dir: &Path) -> io::Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(dir)
}

/// Removes everything in `tmp/` and every blob file not in `kept`.
fn remove_unkept(root: &Path, kept: &HashSet<String>) -> io::Result<usize> {
    let mut stale = entries(&root.join("tmp"))?;
    for fan_out in entries(&root.join("sha256"))? {
        let unkept = entries(&fan_out)?.into_iter().filter(|path| {
            let name = path.file_name().and_then(OsStr::to_str);
            !name.is_some_and(|name| kept.contains(name))
        });
        stale.extend(unkept);
    }
    for path in &stale {
        fs::remove_file(path)?;
    }
    Ok(stale.len())
}

/// The entries of `dir`; none if it does not exist.
pub(super) fn entries(dir: &Path) -> io::Result<Vec<PathBuf>> {
    match fs::read_dir(dir) {
        Ok(entries) => entries.map(|entry| Ok(entry?.path())).collect(),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(err) => Err(err),
    }
}

/// Runs `work` on a blocking thread. A panic resumes in the caller.
pub(super) async fn blocking<T, E>(
    work: impl FnOnce() -> Result<T, E> + Send + 'static,
) -> Result<T, E>
where
    T: Send + 'static,
    E: From<io::Error> + Send + 'static,
{
    match tokio::task::spawn_blocking(work).await {
        Ok(result) => result,
        Err(err) if err.is_panic() => std::panic::resume_unwind(err.into_panic()),
        Err(_) => Err(io::Error::other("the runtime is shutting down").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::ManualClock;
    use crate::store::WriterThread;
    use crate::store::testutil::{TempDir, host_writer, query, runtime};

    const HELLO: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    struct Fixture {
        dir: TempDir,
        clock: Arc<ManualClock>,
        blobs: BlobStore,
        _thread: WriterThread,
    }

    fn start(name: &str) -> Fixture {
        let dir = TempDir::new(name);
        let clock = Arc::new(ManualClock::new());
        let (writer, thread) = host_writer(&dir, &clock);
        let blobs = BlobStore::new(dir.path(), writer, clock.clone());
        Fixture {
            dir,
            clock,
            blobs,
            _thread: thread,
        }
    }

    impl Fixture {
        fn put(&self, data: &[u8], expected: Option<&str>) -> Result<BlobRef, BlobError> {
            let expected = expected.map(|hash| hash.parse().unwrap());
            runtime().block_on(
                self.blobs
                    .put(data.to_vec(), "text/plain".to_owned(), expected),
            )
        }

        fn get(&self, hash: &str) -> Result<Vec<u8>, BlobError> {
            runtime().block_on(self.blobs.get(&hash.parse().unwrap()))
        }

        fn file(&self, hash: &str) -> PathBuf {
            path_of(&self.dir.path().join("blobs"), &hash.parse().unwrap()).unwrap()
        }

        /// `(hash, size, created_at, last_used_at)` of every row.
        fn rows(&self) -> Vec<(String, u64, u64, u64)> {
            let sql = "SELECT hash, size, created_at, last_used_at FROM blobs ORDER BY hash";
            query(&self.dir.host_db(), sql, |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
        }
    }

    #[test]
    fn unit_blobstore_put_stores_once_and_get_returns_the_content() {
        let fx = start("blob-put");
        let t0 = fx.clock.now_wall_ms();
        let stored = fx.put(b"hello", None).unwrap();
        assert_eq!((stored.hash.as_str(), stored.size), (HELLO, 5));
        assert_eq!(fs::read(fx.file(HELLO)).unwrap(), b"hello");

        fx.clock.advance(Duration::from_secs(1));
        assert_eq!(fx.put(b"hello", Some(HELLO)).unwrap(), stored);
        let fan_out = fx.file(HELLO).parent().unwrap().to_owned();
        assert_eq!(fs::read_dir(fan_out).unwrap().count(), 1);
        assert_eq!(fx.rows(), [(HELLO.to_owned(), 5, t0, t0 + 1000)]);
        assert_eq!(fx.get(HELLO).unwrap(), b"hello");

        assert!(matches!(fx.get(&"0".repeat(64)), Err(BlobError::NotFound)));
        assert!(matches!(fx.get("2cf24d"), Err(BlobError::NotFound)));
    }

    #[test]
    fn unit_blobstore_get_checks_the_hash_and_a_put_repairs_the_file() {
        let fx = start("blob-corrupt");
        fx.put(b"hello", None).unwrap();
        fs::write(fx.file(HELLO), b"jello").unwrap();
        assert!(matches!(fx.get(HELLO), Err(BlobError::Corrupt)));

        fx.put(b"hello", None).unwrap();
        assert_eq!(fx.get(HELLO).unwrap(), b"hello");
    }

    #[test]
    fn unit_blobstore_put_refuses_content_that_misses_the_expected_hash() {
        let fx = start("blob-mismatch");
        let other = "0".repeat(64);
        assert!(matches!(
            fx.put(b"hello", Some(&other)),
            Err(BlobError::Mismatch)
        ));
        assert!(!fx.file(HELLO).exists());
        assert!(fx.rows().is_empty());
    }

    #[test]
    fn unit_blobstore_crash_midway_leaves_nothing_readable() {
        let fx = start("blob-crash");
        // A put that died while writing leaves only part of a temporary file.
        let tmp = fx.dir.path().join("blobs/tmp");
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join(HELLO), b"hel").unwrap();
        assert!(matches!(fx.get(HELLO), Err(BlobError::NotFound)));

        fx.put(b"hello", None).unwrap();
        assert_eq!(fx.get(HELLO).unwrap(), b"hello");
        assert_eq!(fs::read_dir(tmp).unwrap().count(), 0);
    }

    #[test]
    fn unit_blobstore_size_limit_is_max_blob_bytes() {
        let fx = start("blob-size");
        let max = V1.max_blob_bytes as usize;
        let over = fx.put(&vec![0; max + 1], None);
        assert!(
            matches!(over, Err(BlobError::TooLarge { size, max: limit }) if size == limit + 1),
            "{over:?}"
        );
        assert!(fx.rows().is_empty());
        assert_eq!(fx.put(&vec![0; max], None).unwrap().size, max as u64);
    }
}
