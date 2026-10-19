//! IT-migrations (design 5.4.6, 5.9.8, test #11): `host.db` migrations with
//! test scripts after the real ones (plan P-4).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use cupper_host::clock::ManualClock;
use cupper_host::exit::HostExit;
use cupper_host::failpoint;
use cupper_host::store::meta::{self, MetaKey};
use cupper_host::store::{
    self, Database, LAYOUT_VERSION, MIGRATIONS, MigrateError, Migrated, Migration,
};
use rusqlite::Connection;

/// Set in the child process that crashes between two scripts: its data
/// directory.
const CHILD_DIR: &str = "CUPPER_TEST_MIGRATIONS_DIR";

/// Test scripts, run after the real ones. Each records that it ran.
const RECORD_A: Migration = Migration {
    name: "test_record_a",
    sql: "CREATE TABLE ran (script TEXT NOT NULL); INSERT INTO ran VALUES ('a');",
};
const RECORD_B: Migration = Migration {
    name: "test_record_b",
    sql: "INSERT INTO ran VALUES ('b');",
};
/// Fails after it has changed something, which must roll back.
const FAILING: Migration = Migration {
    name: "test_failing",
    sql: "CREATE TABLE ran (script TEXT NOT NULL); INSERT INTO missing VALUES (1);",
};

/// The real scripts, then `tests`.
fn scripts(tests: &[Migration]) -> Vec<Migration> {
    MIGRATIONS.iter().chain(tests).copied().collect()
}

/// The schema version after the real scripts.
fn real() -> u32 {
    u32::try_from(MIGRATIONS.len()).unwrap()
}

/// A data directory of its own, removed on drop.
struct DataDir(PathBuf);

impl DataDir {
    fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("it-migrations-{name}"));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn open(&self) -> Connection {
        store::open_writer(&self.0.join("host.db"), Database::Host).unwrap()
    }

    fn backups(&self) -> PathBuf {
        self.0.join("backups")
    }

    fn migrate(
        &self,
        scripts: &[Migration],
        clock: &ManualClock,
    ) -> Result<Migrated, MigrateError> {
        store::migrate(&mut self.open(), scripts, &self.backups(), clock)
    }

    /// Every file in `backups/`.
    fn backup_files(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(self.backups()) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }
}

impl Drop for DataDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn get(conn: &Connection, key: MetaKey) -> Option<String> {
    meta::get(conn, key).unwrap()
}

fn version(conn: &Connection) -> u32 {
    get(conn, MetaKey::SchemaVersion).unwrap().parse().unwrap()
}

/// What the test scripts recorded, in order; empty without their table.
fn ran(conn: &Connection) -> Vec<String> {
    let Ok(mut select) = conn.prepare("SELECT script FROM ran ORDER BY rowid") else {
        return Vec::new();
    };
    select
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

#[test]
fn it_migrations_new_database_migrates_to_the_latest_version() {
    let dir = DataDir::new("new");
    let clock = ManualClock::new();
    let all = scripts(&[RECORD_A]);
    let to = real() + 1;
    let fresh = Migrated {
        from: 0,
        to,
        backup: None,
    };
    assert_eq!(dir.migrate(&all, &clock).unwrap(), fresh);

    let conn = dir.open();
    assert_eq!(version(&conn), to);
    assert_eq!(ran(&conn), ["a"]);
    let host_id = get(&conn, MetaKey::HostId).unwrap();
    let uuid = uuid::Uuid::try_parse(host_id.strip_prefix("h_").unwrap()).unwrap();
    assert_eq!(host_id, format!("h_{}", uuid.simple()));
    assert_eq!(uuid.get_version(), Some(uuid::Version::Random));
    let initial = [
        (
            MetaKey::CreatedAt,
            ManualClock::DEFAULT_START_WALL_MS.to_string(),
        ),
        (MetaKey::LayoutVersion, LAYOUT_VERSION.to_string()),
        (MetaKey::DataEpoch, "0".to_owned()),
    ];
    for (key, value) in initial {
        assert_eq!(get(&conn, key), Some(value), "{key:?}");
    }
    // Every start writes these; T037 does.
    for key in [
        MetaKey::LastStartAt,
        MetaKey::LastCleanExitAt,
        MetaKey::BootSessionUuid,
    ] {
        assert_eq!(get(&conn, key), None, "{key:?}");
    }
    assert_eq!(dir.backup_files(), [] as [String; 0]);

    // Up to date: nothing runs again, nothing is backed up.
    let current = Migrated {
        from: to,
        to,
        backup: None,
    };
    assert_eq!(dir.migrate(&all, &clock).unwrap(), current);
    assert_eq!(ran(&dir.open()), ["a"]);
    assert_eq!(get(&dir.open(), MetaKey::HostId), Some(host_id));
    assert_eq!(dir.backup_files(), [] as [String; 0]);
}

#[test]
fn it_migrations_older_database_is_backed_up_once_before_migrating() {
    let dir = DataDir::new("backup");
    let clock = ManualClock::new();
    dir.migrate(MIGRATIONS, &clock).unwrap();
    dir.open()
        .execute("INSERT INTO settings VALUES ('theme', '\"dark\"', 1)", [])
        .unwrap();

    // The failing script rolls back and names the backup of the older version.
    clock.advance(Duration::from_secs(60));
    let err = dir.migrate(&scripts(&[FAILING]), &clock).unwrap_err();
    let MigrateError::Failed {
        name: "test_failing",
        backup: Some(backup),
        ..
    } = err
    else {
        panic!("{err:?}");
    };
    let backup_name = format!("host-v{}-20260101T000100Z.db", real());
    assert_eq!(backup, dir.backups().join(&backup_name));
    let conn = dir.open();
    assert_eq!(version(&conn), real());
    assert!(conn.prepare("SELECT * FROM ran").is_err(), "rolled back");
    let mode = fs::metadata(dir.backups()).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o700);

    // The backup is the older version, with its data.
    let copy = Connection::open(&backup).unwrap();
    assert_eq!(version(&copy), real());
    let theme: String = copy
        .query_row(
            "SELECT value FROM settings WHERE key = 'theme'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(theme, "\"dark\"");

    // The next attempt finds that backup and makes no other.
    clock.advance(Duration::from_secs(60));
    let migrated = Migrated {
        from: real(),
        to: real() + 1,
        backup: Some(backup),
    };
    assert_eq!(
        dir.migrate(&scripts(&[RECORD_A]), &clock).unwrap(),
        migrated
    );
    assert_eq!(dir.backup_files(), [backup_name]);
    assert_eq!(ran(&dir.open()), ["a"]);
}

#[test]
fn it_migrations_failure_in_a_new_database_has_no_backup() {
    let dir = DataDir::new("new-failing");
    let err = dir
        .migrate(&scripts(&[FAILING]), &ManualClock::new())
        .unwrap_err();
    assert!(
        matches!(
            err,
            MigrateError::Failed {
                name: "test_failing",
                backup: None,
                ..
            }
        ),
        "{err:?}"
    );
    // The real scripts committed before it.
    assert_eq!(version(&dir.open()), real());
    assert_eq!(dir.backup_files(), [] as [String; 0]);
}

#[test]
fn it_migrations_newer_database_is_too_new() {
    let dir = DataDir::new("too-new");
    let clock = ManualClock::new();
    dir.migrate(&scripts(&[RECORD_A]), &clock).unwrap();
    let err = dir.migrate(MIGRATIONS, &clock).unwrap_err();
    assert!(
        matches!(err, MigrateError::TooNew { found, supported }
            if found == real() + 1 && supported == real()),
        "{err:?}"
    );
    assert_eq!(version(&dir.open()), real() + 1);
    assert_eq!(dir.backup_files(), [] as [String; 0]);
}

#[test]
fn it_migrations_crash_between_scripts_resumes_with_the_next_script() {
    let dir = DataDir::new("crash");
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "crash_child", "--ignored", "--nocapture"])
        .env(CHILD_DIR, &dir.0)
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(HostExit::Failpoint.code().into()),
        "{stderr}"
    );
    assert!(
        stderr.contains("failpoint `store.migration_committed` fired"),
        "{stderr}"
    );

    // The child died after the first test script had committed.
    let conn = dir.open();
    let check: String = conn
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .unwrap();
    assert_eq!(check, "ok");
    assert_eq!(version(&conn), real() + 1);
    assert_eq!(ran(&conn), ["a"]);
    drop(conn);

    // The next start goes on with the script after it and runs none twice.
    let migrated = dir
        .migrate(&scripts(&[RECORD_A, RECORD_B]), &ManualClock::new())
        .unwrap();
    assert_eq!((migrated.from, migrated.to), (real() + 1, real() + 2));
    assert_eq!(ran(&dir.open()), ["a", "b"]);
}

#[test]
#[ignore = "the child process of it_migrations_crash_between_scripts_resumes_with_the_next_script"]
fn crash_child() {
    let Some(dir) = std::env::var_os(CHILD_DIR).map(PathBuf::from) else {
        return;
    };
    // The first hit after each script: crash after the first test script.
    let plan = serde_json::json!([{
        "name": failpoint::STORE_MIGRATION_COMMITTED.name(),
        "hit": MIGRATIONS.len() + 1,
    }]);
    failpoint::arm(&serde_json::from_value(plan).unwrap()).unwrap();
    let mut conn = store::open_writer(&dir.join("host.db"), Database::Host).unwrap();
    let all = scripts(&[RECORD_A, RECORD_B]);
    let result = store::migrate(&mut conn, &all, &dir.join("backups"), &ManualClock::new());
    panic!("the failpoint did not fire: {result:?}");
}
