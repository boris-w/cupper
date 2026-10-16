//! SQLite storage: event log, view checkpoints, sessions, adapter processes,
//! command results and blobs. One connection behind a mutex; every call is short.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, StoreError>;

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS events (
    stream TEXT NOT NULL, seq INTEGER NOT NULL, json TEXT NOT NULL,
    PRIMARY KEY (stream, seq)
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS checkpoints (stream TEXT PRIMARY KEY, at_seq INTEGER NOT NULL, view TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS sessions (
    session_id TEXT PRIMARY KEY, agent TEXT NOT NULL, cwd TEXT NOT NULL,
    agent_session_id TEXT, running INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS adapters (pid INTEGER PRIMARY KEY, session_id TEXT NOT NULL, start_us INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS commands (
    principal TEXT NOT NULL, command_id TEXT NOT NULL, method TEXT NOT NULL, params_hash TEXT NOT NULL,
    result TEXT NOT NULL, created_at INTEGER NOT NULL,
    PRIMARY KEY (principal, command_id)
);
CREATE TABLE IF NOT EXISTS blobs (hash TEXT PRIMARY KEY, bytes BLOB NOT NULL);
"#;

#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub session_id: String,
    pub agent: String,
    pub cwd: String,
    pub agent_session_id: Option<String>,
    pub running: bool,
}

#[derive(Debug, Clone)]
pub struct AdapterRecord {
    pub pid: i32,
    pub session_id: String,
    pub start_us: u64,
}

#[derive(Debug)]
pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    fn with<T>(&self, f: impl FnOnce(&mut Connection) -> rusqlite::Result<T>) -> Result<T> {
        let mut c = self.conn.lock().expect("store mutex poisoned");
        Ok(f(&mut c)?)
    }

    /// The host id, generated on first run.
    pub fn host_id(&self, generate: impl FnOnce() -> String) -> Result<String> {
        self.with(|c| {
            if let Some(v) = c.query_row("SELECT value FROM meta WHERE key='host_id'", [], |r| r.get(0)).optional()? {
                return Ok(v);
            }
            let id = generate();
            c.execute("INSERT INTO meta(key, value) VALUES ('host_id', ?1)", [&id])?;
            Ok(id)
        })
    }

    // ---- events ----

    /// Append consecutive events of one stream in a single transaction.
    pub fn append_events(&self, stream: &str, events: &[(u64, String)]) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            {
                let mut st = tx.prepare_cached("INSERT INTO events(stream, seq, json) VALUES (?1, ?2, ?3)")?;
                for (seq, json) in events {
                    st.execute(params![stream, *seq as i64, json])?;
                }
            }
            tx.commit()
        })
    }

    pub fn read_events(&self, stream: &str, after: u64, up_to: u64, limit: usize) -> Result<Vec<(u64, String)>> {
        self.with(|c| {
            let mut st = c.prepare_cached(
                "SELECT seq, json FROM events WHERE stream=?1 AND seq>?2 AND seq<=?3 ORDER BY seq LIMIT ?4",
            )?;
            let rows = st.query_map(params![stream, after as i64, up_to as i64, limit as i64], |r| {
                Ok((r.get::<_, i64>(0)? as u64, r.get::<_, String>(1)?))
            })?;
            rows.collect()
        })
    }

    /// (first, last) seq still in the log for `stream`.
    pub fn event_bounds(&self, stream: &str) -> Result<Option<(u64, u64)>> {
        self.with(|c| {
            c.query_row("SELECT MIN(seq), MAX(seq) FROM events WHERE stream=?1", [stream], |r| {
                Ok(match (r.get::<_, Option<i64>>(0)?, r.get::<_, Option<i64>>(1)?) {
                    (Some(a), Some(b)) => Some((a as u64, b as u64)),
                    _ => None,
                })
            })
        })
    }

    pub fn prune_events(&self, stream: &str, up_to: u64) -> Result<()> {
        self.with(|c| c.execute("DELETE FROM events WHERE stream=?1 AND seq<=?2", params![stream, up_to as i64]).map(|_| ()))
    }

    pub fn save_checkpoint(&self, stream: &str, at_seq: u64, view_json: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO checkpoints(stream, at_seq, view) VALUES (?1, ?2, ?3)
                 ON CONFLICT(stream) DO UPDATE SET at_seq=excluded.at_seq, view=excluded.view",
                params![stream, at_seq as i64, view_json],
            )
            .map(|_| ())
        })
    }

    pub fn load_checkpoint(&self, stream: &str) -> Result<Option<(u64, String)>> {
        self.with(|c| {
            c.query_row("SELECT at_seq, view FROM checkpoints WHERE stream=?1", [stream], |r| {
                Ok((r.get::<_, i64>(0)? as u64, r.get(1)?))
            })
            .optional()
        })
    }

    // ---- sessions ----

    pub fn insert_session(&self, s: &SessionRecord, now: i64) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO sessions(session_id, agent, cwd, agent_session_id, running, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
                params![s.session_id, s.agent, s.cwd, s.agent_session_id, s.running, now],
            )
            .map(|_| ())
        })
    }

    pub fn set_agent_session_id(&self, session_id: &str, agent_session_id: &str) -> Result<()> {
        self.with(|c| {
            c.execute("UPDATE sessions SET agent_session_id=?2 WHERE session_id=?1", params![session_id, agent_session_id])
                .map(|_| ())
        })
    }

    pub fn set_running(&self, session_id: &str, running: bool) -> Result<()> {
        self.with(|c| c.execute("UPDATE sessions SET running=?2 WHERE session_id=?1", params![session_id, running]).map(|_| ()))
    }

    pub fn session(&self, session_id: &str) -> Result<Option<SessionRecord>> {
        Ok(self.sessions()?.into_iter().find(|s| s.session_id == session_id))
    }

    pub fn sessions(&self) -> Result<Vec<SessionRecord>> {
        self.with(|c| {
            let mut st = c.prepare_cached("SELECT session_id, agent, cwd, agent_session_id, running FROM sessions ORDER BY created_at")?;
            let rows = st.query_map([], |r| {
                Ok(SessionRecord {
                    session_id: r.get(0)?,
                    agent: r.get(1)?,
                    cwd: r.get(2)?,
                    agent_session_id: r.get(3)?,
                    running: r.get(4)?,
                })
            })?;
            rows.collect()
        })
    }

    // ---- adapter processes ----

    pub fn record_adapter(&self, a: &AdapterRecord) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT OR REPLACE INTO adapters(pid, session_id, start_us) VALUES (?1, ?2, ?3)",
                params![a.pid, a.session_id, a.start_us as i64],
            )
            .map(|_| ())
        })
    }

    pub fn remove_adapter(&self, pid: i32) -> Result<()> {
        self.with(|c| c.execute("DELETE FROM adapters WHERE pid=?1", [pid]).map(|_| ()))
    }

    pub fn adapters(&self) -> Result<Vec<AdapterRecord>> {
        self.with(|c| {
            let mut st = c.prepare_cached("SELECT pid, session_id, start_us FROM adapters")?;
            let rows = st.query_map([], |r| {
                Ok(AdapterRecord { pid: r.get(0)?, session_id: r.get(1)?, start_us: r.get::<_, i64>(2)? as u64 })
            })?;
            rows.collect()
        })
    }

    // ---- commands ----

    /// Stored (params_hash, result_json) for a command id.
    pub fn command(&self, principal: &str, command_id: &str) -> Result<Option<(String, String)>> {
        self.with(|c| {
            c.query_row(
                "SELECT params_hash, result FROM commands WHERE principal=?1 AND command_id=?2",
                [principal, command_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
        })
    }

    pub fn save_command(&self, principal: &str, command_id: &str, method: &str, hash: &str, result: &str, now: i64) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT OR IGNORE INTO commands(principal, command_id, method, params_hash, result, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
                params![principal, command_id, method, hash, result, now],
            )
            .map(|_| ())
        })
    }

    pub fn prune_commands(&self, older_than_ms: i64) -> Result<()> {
        self.with(|c| c.execute("DELETE FROM commands WHERE created_at < ?1", [older_than_ms]).map(|_| ()))
    }

    // ---- blobs ----

    /// Store bytes content-addressed; returns the hex sha-256.
    pub fn put_blob(&self, bytes: &[u8]) -> Result<String> {
        let hash = hex::encode(Sha256::digest(bytes));
        self.with(|c| c.execute("INSERT OR IGNORE INTO blobs(hash, bytes) VALUES (?1, ?2)", params![hash, bytes]).map(|_| ()))?;
        Ok(hash)
    }

    pub fn blob(&self, hash: &str) -> Result<Option<Vec<u8>>> {
        self.with(|c| c.query_row("SELECT bytes FROM blobs WHERE hash=?1", [hash], |r| r.get(0)).optional())
    }
}
