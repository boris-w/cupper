//! Opening connections, and the two durability levels (design 5.2.8, 5.4.2).
//!
//! Every connection runs in WAL mode with `busy_timeout=5000`. The writer
//! connection commits with `synchronous=NORMAL` by default: a process crash
//! loses nothing committed, a power loss may lose the latest transactions but
//! never corrupts the database. The batch that records `pending_external`
//! commits with `synchronous=FULL` and `fullfsync`, because a plain `fsync` on
//! macOS does not flush the drive's cache. `host.db` also checkpoints with
//! `checkpoint_fullfsync`; `raw.db` does not.

use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

/// How long a connection waits for a lock another connection holds.
pub const BUSY_TIMEOUT: Duration = Duration::from_millis(5000);

/// The two databases of the data directory (design 5.4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Database {
    /// `host.db`: the event log and every table but the raw frames.
    Host,
    /// `raw.db`: raw frames, kept for diagnostics and recomputation only.
    Raw,
}

/// How hard a commit pushes its data to the disk.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Durability {
    /// `synchronous=NORMAL`: survives a process crash; a power loss may lose it.
    #[default]
    Normal,
    /// `synchronous=FULL` with `fullfsync`: on the disk itself once committed.
    /// For the batch that records `pending_external`, before git runs.
    Full,
}

#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    #[error("the database stays in journal mode `{0}` instead of WAL")]
    NotWal(String),
}

/// Opens (creating it if needed) the read-write connection to `db` at `path`,
/// set to [`Durability::Normal`]. Blocking I/O: call it from startup or a
/// blocking thread, never from an async worker.
pub fn open_writer(path: &Path, db: Database) -> Result<Connection, OpenError> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(BUSY_TIMEOUT)?;
    let mode: String =
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(OpenError::NotWal(mode));
    }
    set_durability(&conn, Durability::Normal)?;
    conn.pragma_update(None, "checkpoint_fullfsync", db == Database::Host)?;
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(conn)
}

/// Opens a read-only connection to the database at `path`, which an
/// [`open_writer`] connection has already put in WAL mode. Blocking I/O.
pub fn open_reader(path: &Path) -> Result<Connection, OpenError> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(path, flags)?;
    conn.busy_timeout(BUSY_TIMEOUT)?;
    Ok(conn)
}

/// Sets the durability of the next commits. SQLite refuses to change
/// `synchronous` inside a transaction, so the writer calls this between
/// batches.
pub(super) fn set_durability(conn: &Connection, durability: Durability) -> rusqlite::Result<()> {
    let (synchronous, fullfsync) = match durability {
        Durability::Normal => ("NORMAL", false),
        Durability::Full => ("FULL", true),
    };
    conn.pragma_update(None, "synchronous", synchronous)?;
    conn.pragma_update(None, "fullfsync", fullfsync)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::testutil::{TempDir, pragmas};

    const WRITER_PRAGMAS: &[&str] = &[
        "journal_mode",
        "synchronous",
        "fullfsync",
        "checkpoint_fullfsync",
        "foreign_keys",
        "busy_timeout",
    ];

    #[test]
    fn store_pragmas_take_effect_when_a_writer_opens() {
        let dir = TempDir::new("pragmas");
        let host = open_writer(&dir.host_db(), Database::Host).unwrap();
        // synchronous: 1 is NORMAL, 2 is FULL.
        assert_eq!(
            pragmas(&host, WRITER_PRAGMAS),
            ["wal", "1", "0", "1", "1", "5000"]
        );

        let raw = open_writer(&dir.path().join("raw.db"), Database::Raw).unwrap();
        assert_eq!(
            pragmas(&raw, WRITER_PRAGMAS),
            ["wal", "1", "0", "0", "1", "5000"]
        );

        set_durability(&host, Durability::Full).unwrap();
        assert_eq!(pragmas(&host, &["synchronous", "fullfsync"]), ["2", "1"]);
        set_durability(&host, Durability::Normal).unwrap();
        assert_eq!(pragmas(&host, &["synchronous", "fullfsync"]), ["1", "0"]);
    }

    #[test]
    fn store_pragmas_readers_are_read_only_on_the_wal() {
        let dir = TempDir::new("reader");
        let writer = open_writer(&dir.host_db(), Database::Host).unwrap();
        writer.execute_batch("CREATE TABLE t (v INTEGER)").unwrap();

        let reader = open_reader(&dir.host_db()).unwrap();
        assert_eq!(
            pragmas(&reader, &["journal_mode", "busy_timeout"]),
            ["wal", "5000"]
        );
        let err = reader.execute("INSERT INTO t VALUES (1)", []).unwrap_err();
        assert_eq!(
            err.sqlite_error_code(),
            Some(rusqlite::ErrorCode::ReadOnly),
            "{err}"
        );
    }
}
