//! IT-migrations (design 5.4.6, 5.9.8, test #11): `host.db` migrations with
//! test scripts after the real ones (plan P-4), and the rebuild of `raw.db`.
//!
//! IT-migrations-samples (plan P-4): released scripts never change, and a
//! sample `host.db` of every schema version, `testdata/migrations/v<N>.sql`,
//! made by a host and never edited, migrates to the latest version with its
//! data and a host starts on it.

mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use cupper_cli::client;
use cupper_host::clock::ManualClock;
use cupper_host::exit::HostExit;
use cupper_host::failpoint;
use cupper_host::gateway::testkit::TempDir;
use cupper_host::store::meta::{self, MetaKey};
use cupper_host::store::{
    self, Database, LAYOUT_VERSION, MIGRATIONS, MigrateError, Migrated, Migration, RAW_SCHEMA,
    RawDb, RawSchema,
};
use cupper_protocol::methods::Empty;
use cupper_protocol::methods::admin::SettingsUpdate;
use cupper_protocol::methods::handshake::HostStatus;
use cupper_protocol::methods::session::SessionInterrupt;
use rusqlite::Connection;
use serde_json::json;
use sha2::{Digest, Sha256};

use common::*;

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

#[test]
fn it_migrations_raw_db_is_rebuilt_when_its_schema_changes() {
    let dir = DataDir::new("raw");
    let mut conn = store::open_writer(&dir.0.join("raw.db"), Database::Raw).unwrap();
    let frames = |conn: &Connection| -> i64 {
        conn.query_row("SELECT count(*) FROM raw_frames", [], |row| row.get(0))
            .unwrap()
    };
    let rebuilt = store::prepare_raw(&mut conn, &RAW_SCHEMA).unwrap();
    assert_eq!(rebuilt, RawDb::Rebuilt { from: 0 });
    conn.execute(
        "INSERT INTO raw_frames VALUES ('a_1', 1, 5, 'send', NULL, '{}')",
        [],
    )
    .unwrap();

    // The same version: kept as it is.
    let kept = store::prepare_raw(&mut conn, &RAW_SCHEMA).unwrap();
    assert_eq!(kept, RawDb::Kept);
    assert_eq!(frames(&conn), 1);

    // A changed schema: emptied and rebuilt, no migration.
    let changed = RawSchema {
        version: RAW_SCHEMA.version + 1,
        sql: "CREATE TABLE raw_frames (frame TEXT NOT NULL);",
    };
    let rebuilt = store::prepare_raw(&mut conn, &changed).unwrap();
    let from = i64::from(RAW_SCHEMA.version);
    assert_eq!(rebuilt, RawDb::Rebuilt { from });
    assert_eq!(frames(&conn), 0);
    let schema: Vec<String> = conn
        .prepare("SELECT sql FROM sqlite_schema")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(schema, [changed.sql.trim_end_matches(';')]);
    let user_version: u32 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(user_version, changed.version);
}

/// The released scripts and the sha-256 of each (plan P-4): every script on
/// `main`, since self-use builds come from there. A released script never
/// changes; a schema change is a new script, pinned here in the same change
/// as the sample of the version it makes.
const RELEASED: &[(&str, &str)] = &[(
    "0001_initial",
    "f0aa2e36717ba501927c4cca3d692e12c89e725ba63d597bba6bb3d4833ba215",
)];

#[test]
fn it_migrations_samples_released_scripts_never_change() {
    let hash = |sql: &str| hex(&Sha256::digest(sql));
    let scripts: Vec<_> = MIGRATIONS.iter().map(|m| (m.name, hash(m.sql))).collect();
    let pinned: Vec<_> = RELEASED.iter().map(|(n, h)| (*n, h.to_string())).collect();
    let why = "a released script changed, or a new one is not pinned: change the schema with \
        a new script, pin it in RELEASED, and add its sample (plan P-4)";
    assert_eq!(scripts, pinned, "{why}");
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Set to make the sample of the latest schema version when it is missing.
const WRITE: &str = "CUPPER_WRITE_MIGRATION_SAMPLE";

#[tokio::test]
async fn it_migrations_samples_every_version_migrates_and_a_host_starts() {
    let samples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/migrations");
    for version in 1..=real() {
        let path = samples.join(format!("v{version}.sql"));
        if !path.exists() && version == real() && std::env::var_os(WRITE).is_some() {
            fs::create_dir_all(&samples).unwrap();
            fs::write(&path, make_sample().await).unwrap();
        }
        let sample = fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!("no sample of schema version {version}: run this test with {WRITE}=1")
        });
        check_sample(version, &sample).await;
    }
}

/// A sample of the latest version: a host runs a session with the fake
/// agent (a tool call with an output long enough for a blob, a permission,
/// a message queued, then an interrupt) and a security setting is changed;
/// once it exits, its `host.db` as SQL, with no path of this machine.
async fn make_sample() -> String {
    let dir = TempDir::new("migration-sample");
    let (data_dir, mut host, client) = fake_agent_host(&dir, json!({})).await;
    let script = "tool t1 execute Run; tool_done t1 20000; permission p1 execute";
    let session = create(&client, script).await;
    events(&data_dir, &session, "interaction.requested", 1).await;
    send(&client, &session, "later").await;
    let stop = json!({"command_id": client::command_id(), "session_id": session});
    client
        .request::<SessionInterrupt>(typed(stop))
        .await
        .unwrap();
    let patch = json!({"command_id": client::command_id(), "patch": {"diagnostics.verbose": true}});
    client
        .request::<SettingsUpdate>(typed(patch))
        .await
        .unwrap();
    until(
        &data_dir,
        "host.db",
        "SELECT status FROM turns",
        &["cancelled"],
    )
    .await;
    drop(client);
    terminate(&host.0);
    assert_eq!(exit_code(&mut host.0).await, 0);
    let conn = Connection::open(data_dir.join("host.db")).unwrap();
    for (table, (_, rows)) in tables(&conn) {
        assert!(!rows.is_empty(), "the sample has no rows in {table}");
    }
    let mut sample = format!(
        "-- host.db at schema version {}, made by a host; never edit\n",
        real()
    );
    let schema = "SELECT sql || ';' FROM sqlite_master WHERE sql IS NOT NULL
        AND name NOT LIKE 'sqlite_%' ORDER BY type DESC, name";
    let tables = tables(&conn).into_values().flat_map(|(_, rows)| rows);
    for line in column(&conn, schema).into_iter().chain(tables) {
        sample = sample + &line + "\n";
    }
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let (tmp, canonical) = (dir.path().to_owned(), dir.path().canonicalize().unwrap());
    let places = [canonical, tmp, workspace.canonicalize().unwrap()];
    for (place, to) in places.iter().zip(["/sample", "/sample", "/cupper"]) {
        sample = sample.replace(place.to_str().unwrap(), to);
    }
    assert!(
        !sample.contains(env!("HOME")),
        "no home directory in the sample"
    );
    sample
}

/// The sample of `version` has the schema the scripts up to it make, which
/// catches a released script changed in place even with its pin; migrated
/// to the latest version, it keeps its data; a host starts on it.
async fn check_sample(version: u32, sample: &str) {
    let dir = DataDir::new(&format!("sample-v{version}"));
    let old = dir.open();
    old.execute_batch(sample).unwrap();
    let made = Connection::open_in_memory().unwrap();
    for script in &MIGRATIONS[..version as usize] {
        made.execute_batch(script.sql).unwrap();
    }
    let schema = "SELECT type || ' ' || name || ' ' || sql FROM sqlite_master
        WHERE name NOT LIKE 'sqlite_%' ORDER BY name";
    assert_eq!(
        column(&old, schema),
        column(&made, schema),
        "v{version}'s schema"
    );
    let before = tables(&old);
    drop(old);

    let migrated = dir.migrate(MIGRATIONS, &ManualClock::new()).unwrap();
    assert_eq!((migrated.from, migrated.to), (version, real()));
    let conn = dir.open();
    assert_eq!(column(&conn, "PRAGMA integrity_check"), ["ok"]);
    assert!(column(&conn, "SELECT \"table\" FROM pragma_foreign_key_check").is_empty());
    let after = tables(&conn);
    for (table, (sql, rows)) in before {
        let (sql_now, rows_now) = &after[&table];
        // A table a later script changed keeps its rows; one it did not, every
        // value; `meta` all but the schema version.
        let kept = |rows: &[String]| {
            rows.iter()
                .filter(|r| !r.contains("'schema_version'"))
                .count()
        };
        match (sql == *sql_now, table.as_str()) {
            (true, "meta") => assert_eq!(kept(&rows), kept(rows_now), "meta"),
            (true, _) => assert_eq!(&rows, rows_now, "{table}"),
            (false, _) => assert_eq!(rows.len(), rows_now.len(), "{table}"),
        }
    }
    drop(conn);

    let mut host = Running(spawn_with(&dir.0, fake_agent_config(json!({}))));
    let client = connect(&dir.0).await;
    client.request::<HostStatus>(Empty {}).await.unwrap();
    drop(client);
    terminate(&host.0);
    assert_eq!(exit_code(&mut host.0).await, 0, "a host on v{version}");
}

/// Each table's statement and its rows as `INSERT`s, ordered by every column.
fn tables(conn: &Connection) -> std::collections::BTreeMap<String, (String, Vec<String>)> {
    let names =
        "SELECT name || char(9) || coalesce(sql, '') FROM sqlite_master WHERE type = 'table'";
    let rows = |table: &str| {
        let columns = column(
            conn,
            &format!("SELECT name FROM pragma_table_info('{table}')"),
        );
        let quoted: Vec<String> = columns.iter().map(|c| format!("quote(\"{c}\")")).collect();
        let ordered: Vec<String> = columns.iter().map(|c| format!("\"{c}\"")).collect();
        let (values, order) = (quoted.join(" || ',' || "), ordered.join(", "));
        let select = format!(
            "SELECT 'INSERT INTO \"{table}\" VALUES(' || {values} || ');' FROM \"{table}\" ORDER BY {order}"
        );
        column(conn, &select)
    };
    let named = column(conn, names).into_iter().map(|row| {
        let (name, sql) = row.split_once('\t').unwrap();
        (name.to_owned(), (sql.to_owned(), rows(name)))
    });
    named.collect()
}

/// The first column of `sql`'s rows, as text.
fn column(conn: &Connection, sql: &str) -> Vec<String> {
    let mut select = conn.prepare(sql).unwrap();
    let rows = select.query_map([], |row| row.get::<_, String>(0)).unwrap();
    rows.map(Result::unwrap).collect()
}
