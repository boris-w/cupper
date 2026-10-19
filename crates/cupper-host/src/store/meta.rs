//! The `meta` table of `host.db` (design 5.4.2): text values under fixed
//! keys. Numbers are stored in decimal.

use rusqlite::{Connection, OptionalExtension};

/// A key of `meta`, with who writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaKey {
    /// The number of the last migration script run, written in its
    /// transaction ([`migrate`](super::migrate())).
    SchemaVersion,
    /// [`LAYOUT_VERSION`](super::LAYOUT_VERSION) of the files beside the
    /// databases. Set when the database is created.
    LayoutVersion,
    /// `h_` + UUIDv4 (design 5.4.4). Set when the database is created.
    HostId,
    /// When the database was created.
    CreatedAt,
    /// When the host last started. Startup writes it on every start.
    LastStartAt,
    /// When the host last exited in order: the last write of that exit.
    LastCleanExitAt,
    /// Starts at 0; startup bumps it as [`epoch`](super::epoch) decides.
    DataEpoch,
    /// The boot the host last started on. Startup writes it on every start.
    BootSessionUuid,
}

impl MetaKey {
    pub const fn name(self) -> &'static str {
        match self {
            Self::SchemaVersion => "schema_version",
            Self::LayoutVersion => "layout_version",
            Self::HostId => "host_id",
            Self::CreatedAt => "created_at",
            Self::LastStartAt => "last_start_at",
            Self::LastCleanExitAt => "last_clean_exit_at",
            Self::DataEpoch => "data_epoch",
            Self::BootSessionUuid => "boot_session_uuid",
        }
    }
}

pub fn get(conn: &Connection, key: MetaKey) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM meta WHERE key = ?1",
        [key.name()],
        |row| row.get(0),
    )
    .optional()
}

pub fn set(conn: &Connection, key: MetaKey, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        [key.name(), value],
    )
    .map(drop)
}
