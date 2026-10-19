//! Migrations of `host.db` (design 5.4.6, plan P-4).
//!
//! - The scripts are `migrations/NNNN_<name>.sql`, compiled in as
//!   [`MIGRATIONS`]. Script `NNNN` brings the database to `schema_version`
//!   `NNNN`. Until T160, `0001` is the only script and changes in place (P-4).
//! - Each script runs in its own transaction, which also records its number
//!   in `meta.schema_version`. A crash between two scripts leaves the database
//!   at the earlier one; the next start goes on with the next script and runs
//!   none twice (test #11).
//! - [`migrate`] at startup:
//!   1. a database newer than the last script: [`MigrateError::TooNew`];
//!   2. an older one: first a backup, `VACUUM INTO
//!      'backups/host-v<old>-<time>.db'`, unless one of that version is there
//!      already; then the missing scripts in order;
//!   3. a script that fails rolls back: [`MigrateError::Failed`], which names
//!      the backup.
//! - A new database gets its initial `meta` in the first script's transaction,
//!   so no database has a schema version without a host ID.
//!
//! The migration runs on the [`open_writer`](super::open_writer) connection
//! before it goes to the writer thread: the scripts need their own
//! transactions, and `VACUUM INTO` runs outside any.

use std::fs::{self, DirBuilder, File};
use std::io;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use cupper_protocol::ids::HostId;
use rusqlite::{Connection, TransactionBehavior};

use super::meta::{self, MetaKey};
use crate::clock::Clock;
use crate::startup::new_host_uuid;

/// One migration script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Migration {
    /// The file name without `.sql`: `NNNN_<name>`, where `NNNN` is the
    /// script's place in [`MIGRATIONS`], from 1.
    pub name: &'static str,
    /// Statements without transaction control.
    pub sql: &'static str,
}

/// The scripts compiled into the host, in order.
pub const MIGRATIONS: &[Migration] = &[Migration {
    name: "0001_initial",
    sql: include_str!("../../migrations/0001_initial.sql"),
}];

/// The layout of the files beside the databases (blobs, runtime, snapshot
/// object stores), recorded in `meta.layout_version` (design 5.4.6). A change
/// comes with a Rust function next to the scripts that moves the files; none
/// exists yet.
pub const LAYOUT_VERSION: u32 = 1;

/// A migration that succeeded, or had nothing to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migrated {
    /// The schema version found; 0 for a new database.
    pub from: u32,
    /// The schema version now: the number of the last script.
    pub to: u32,
    /// The backup of version `from`; `None` for a new or current database.
    pub backup: Option<PathBuf>,
}

/// Why `host.db` cannot be used. The startup sequence maps these to exit
/// codes (design 5.2.13).
#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    /// The database comes from a newer host; there are no downgrades (NFR-10).
    /// Exit 4, `HostExit::DataTooNew`.
    #[error("host.db has schema version {found}; this host knows up to {supported}")]
    TooNew { found: u32, supported: u32 },
    /// Script `name` failed and rolled back: the database stays at the
    /// version before it. Exit 6, `HostExit::MigrationFailed`; the UI shows
    /// `backup`, which is `None` only for a new database.
    #[error("migration {name} failed")]
    Failed {
        name: &'static str,
        backup: Option<PathBuf>,
        #[source]
        source: rusqlite::Error,
    },
    /// The backup could not be made, so nothing was migrated.
    #[error("cannot back up host.db to {}", path.display())]
    Backup {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("cannot read the schema version of host.db")]
    Unreadable(#[source] rusqlite::Error),
    #[error("meta.schema_version is {0:?}, not a version")]
    BadVersion(Option<String>),
}

/// Brings `host.db`, open on `conn`, to the last of `scripts` (the host
/// passes [`MIGRATIONS`]). `backups` is the data directory's `backups/`,
/// created if missing. Blocking I/O, for startup.
pub fn migrate(
    conn: &mut Connection,
    scripts: &[Migration],
    backups: &Path,
    clock: &dyn Clock,
) -> Result<Migrated, MigrateError> {
    let from = schema_version(conn)?;
    let to = u32::try_from(scripts.len()).expect("fewer than 2^32 scripts");
    if from > to {
        return Err(MigrateError::TooNew {
            found: from,
            supported: to,
        });
    }
    let backup = if from == 0 || from == to {
        None
    } else {
        Some(back_up(conn, from, backups, clock.now_wall_ms())?)
    };
    for (version, script) in (1..).zip(scripts).skip(from as usize) {
        run(conn, script, version, clock).map_err(|source| MigrateError::Failed {
            name: script.name,
            backup: backup.clone(),
            source,
        })?;
        crate::failpoint!(STORE_MIGRATION_COMMITTED);
    }
    Ok(Migrated { from, to, backup })
}

fn schema_version(conn: &Connection) -> Result<u32, MigrateError> {
    let has_meta: bool = conn
        .query_row(
            "SELECT count(*) > 0 FROM sqlite_schema WHERE type = 'table' AND name = 'meta'",
            [],
            |row| row.get(0),
        )
        .map_err(MigrateError::Unreadable)?;
    if !has_meta {
        return Ok(0);
    }
    let text = meta::get(conn, MetaKey::SchemaVersion).map_err(MigrateError::Unreadable)?;
    match text.as_deref().map(str::parse) {
        Some(Ok(version)) => Ok(version),
        _ => Err(MigrateError::BadVersion(text)),
    }
}

fn run(
    conn: &mut Connection,
    script: &Migration,
    version: u32,
    clock: &dyn Clock,
) -> rusqlite::Result<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(script.sql)?;
    if version == 1 {
        let host_id = HostId::from_uuid(new_host_uuid().as_u128());
        meta::set(&tx, MetaKey::HostId, host_id.as_str())?;
        meta::set(&tx, MetaKey::CreatedAt, &clock.now_wall_ms().to_string())?;
        meta::set(&tx, MetaKey::LayoutVersion, &LAYOUT_VERSION.to_string())?;
        meta::set(&tx, MetaKey::DataEpoch, "0")?;
    }
    meta::set(&tx, MetaKey::SchemaVersion, &version.to_string())?;
    tx.commit()
}

/// The backup of the version-`version` database: one an earlier start made,
/// or a new copy. The copy goes to `host.partial` and is renamed once it is on
/// the disk, so a crash midway leaves nothing that passes for a backup.
fn back_up(
    conn: &Connection,
    version: u32,
    dir: &Path,
    now_ms: u64,
) -> Result<PathBuf, MigrateError> {
    let prefix = format!("host-v{version}-");
    let path = dir.join(format!("{prefix}{}.db", utc_stamp(now_ms)));
    let partial = dir.join("host.partial");
    let copy = || -> io::Result<Option<PathBuf>> {
        DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
        if let Some(existing) = existing_backup(dir, &prefix)? {
            return Ok(Some(existing));
        }
        match fs::remove_file(&partial) {
            Err(err) if err.kind() != io::ErrorKind::NotFound => return Err(err),
            _ => {}
        }
        let target = partial
            .to_str()
            .ok_or_else(|| io::Error::other("not UTF-8"))?;
        conn.execute("VACUUM INTO ?1", [target])
            .map_err(io::Error::other)?;
        File::open(&partial)?.sync_all()?;
        fs::rename(&partial, &path)?;
        File::open(dir)?.sync_all()?;
        Ok(None)
    };
    match copy() {
        Ok(existing) => Ok(existing.unwrap_or(path)),
        Err(source) => Err(MigrateError::Backup { path, source }),
    }
}

/// The latest backup in `dir` whose name starts with `prefix`.
fn existing_backup(dir: &Path, prefix: &str) -> io::Result<Option<PathBuf>> {
    let mut latest = None;
    for entry in fs::read_dir(dir)? {
        let name = entry?.file_name().into_string().ok();
        if name
            .as_deref()
            .is_some_and(|name| name.starts_with(prefix) && name.ends_with(".db"))
        {
            latest = latest.max(name);
        }
    }
    Ok(latest.map(|name| dir.join(name)))
}

/// `YYYYMMDDTHHMMSSZ` in UTC, which sorts as it reads.
fn utc_stamp(wall_ms: u64) -> String {
    let secs = wall_ms / 1000;
    let (days, time) = (secs / 86_400, secs % 86_400);
    // `civil_from_days` (Howard Hinnant), for days since 1970-01-01.
    let z = days + 719_468;
    let (era, doe) = (z / 146_097, z % 146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = era * 400 + yoe + u64::from(month <= 2);
    let (hour, minute, second) = (time / 3600, time / 60 % 60, time % 60);
    format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::testutil::{assert_same_schema, design_sql};

    #[test]
    fn store_schema_host_db_after_all_scripts_is_design_5_4_2() {
        let design = Connection::open_in_memory().unwrap();
        design.execute_batch(&design_sql("host.db")).unwrap();
        let migrated = Connection::open_in_memory().unwrap();
        for script in MIGRATIONS {
            migrated.execute_batch(script.sql).unwrap();
        }
        assert_same_schema(&migrated, &design, 16);
    }

    #[test]
    fn store_schema_scripts_are_numbered_in_order_and_all_listed() {
        let mut listed = Vec::new();
        for (place, script) in (1..).zip(MIGRATIONS) {
            let (number, name) = script.name.split_once('_').expect("`NNNN_<name>`");
            assert_eq!(number, format!("{place:04}"), "{}", script.name);
            assert!(!name.is_empty(), "{}", script.name);
            listed.push(format!("{}.sql", script.name));
        }
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let mut files: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        files.sort();
        assert_eq!(files, listed);
    }

    #[test]
    fn store_schema_backup_names_carry_the_utc_time() {
        for (wall_ms, stamp) in [
            (0, "19700101T000000Z"),
            (946_684_799_000, "19991231T235959Z"),
            (951_868_800_000, "20000301T000000Z"),
            (1_709_251_199_999, "20240229T235959Z"),
            (1_767_225_660_000, "20260101T000100Z"),
        ] {
            assert_eq!(utc_stamp(wall_ms), stamp, "{wall_ms}");
        }
    }
}
