//! Helpers shared by the store's unit tests.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use cupper_protocol::annotations::TimestampMs;
use cupper_protocol::envelope::Event;
use cupper_protocol::event::{EventBody, Strictness};
use rusqlite::types::Value;
use rusqlite::{Connection, Row};
use serde_json::json;

use super::streams::Published;
use super::{Dir, RawFrame, Writer, WriterConfig, WriterThread};
use crate::clock::ManualClock;
use crate::quiescence::Quiescence;

/// A fresh directory under the system temp dir, removed on drop.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(name: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "cupper-store-test-{}-{unique}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn host_db(&self) -> PathBuf {
        self.0.join("host.db")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The value of `PRAGMA <name>`, as text.
pub fn pragma(conn: &Connection, name: &str) -> String {
    let value = conn
        .query_row(&format!("PRAGMA {name}"), [], |row| row.get(0))
        .expect("read a pragma");
    match value {
        Value::Integer(n) => n.to_string(),
        Value::Text(text) => text,
        other => panic!("PRAGMA {name} returned {other:?}"),
    }
}

pub fn pragmas(conn: &Connection, names: &[&str]) -> Vec<String> {
    names.iter().map(|name| pragma(conn, name)).collect()
}

/// The SQL block under `### <heading>` in design 5.4.2.
pub fn design_sql(heading: &str) -> String {
    let doc = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/design/04-data-model.md");
    let doc = std::fs::read_to_string(doc).expect("read design 5.4");
    let (_, section) = doc
        .split_once(&format!("\n### {heading}\n"))
        .expect("the heading");
    let (_, block) = section.split_once("```sql\n").expect("an SQL block");
    block
        .split_once("```")
        .expect("the end of the block")
        .0
        .to_owned()
}

/// Asserts that `ours` has the same tables, columns, foreign keys and
/// indexes as `design`, which has at least `tables` tables.
pub fn assert_same_schema(ours: &Connection, design: &Connection, tables: usize) {
    let (ours, design) = (schema_of(ours), schema_of(design));
    let in_design = design.iter().filter(|line| line.starts_with("table "));
    assert!(in_design.count() >= tables, "{design:#?}");
    let missing: Vec<_> = design.iter().filter(|line| !ours.contains(line)).collect();
    let extra: Vec<_> = ours.iter().filter(|line| !design.contains(line)).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "missing: {missing:#?}\nnot in the design: {extra:#?}"
    );
}

/// Every table, column, foreign key and index of `conn`, one line each.
fn schema_of(conn: &Connection) -> Vec<String> {
    const FACTS: &str = r#"
        SELECT 'table ' || name || ' wr=' || wr || ' strict=' || strict
          FROM pragma_table_list WHERE schema = 'main' AND name NOT LIKE 'sqlite_%'
        UNION ALL
        SELECT 'column ' || t.name || '.' || c.cid || ' ' || c.name || ' ' || c.type
               || ' notnull=' || c."notnull" || ' default=' || ifnull(c.dflt_value, '-')
               || ' pk=' || c.pk || ' hidden=' || c.hidden
          FROM sqlite_schema t, pragma_table_xinfo(t.name) c WHERE t.type = 'table'
        UNION ALL
        SELECT 'fk ' || t.name || '.' || f.id || '.' || f.seq || ' ' || f."from" || ' -> '
               || f."table" || '(' || ifnull(f."to", '') || ') ' || f.on_update
               || ' ' || f.on_delete || ' ' || f."match"
          FROM sqlite_schema t, pragma_foreign_key_list(t.name) f WHERE t.type = 'table'
        UNION ALL
        SELECT 'index ' || t.name || '.' || i.name || ' unique=' || i."unique"
               || ' origin=' || i.origin || ' partial=' || i.partial
          FROM sqlite_schema t, pragma_index_list(t.name) i WHERE t.type = 'table'
        UNION ALL
        SELECT 'key ' || i.name || '.' || k.seqno || ' ' || ifnull(k.name, '-')
               || ' desc=' || k.desc || ' coll=' || k.coll || ' key=' || k.key
          FROM sqlite_schema t, pragma_index_list(t.name) i, pragma_index_xinfo(i.name) k
         WHERE t.type = 'table'
        UNION ALL
        SELECT 'sql ' || name || ' ' || sql FROM sqlite_schema
         WHERE type = 'index' AND sql IS NOT NULL
    "#;
    let mut select = conn.prepare(FACTS).expect("prepare the schema query");
    select
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query the schema")
        .map(|line| {
            let line = line.expect("read a schema line");
            line.split_whitespace().collect::<Vec<_>>().join(" ")
        })
        .collect()
}

/// `host.db` in memory, migrated.
pub fn host_db() -> Connection {
    let mut conn = Connection::open_in_memory().expect("open in memory");
    let clock = ManualClock::new();
    super::migrate(&mut conn, super::MIGRATIONS, Path::new("backups"), &clock).expect("migrate");
    conn
}

pub fn body(raw: serde_json::Value) -> EventBody {
    EventBody::decode(&raw, Strictness::Strict)
        .unwrap_or_else(|e| panic!("{raw}: {e}"))
        .value
}

/// An event at `seq`, written at `ts`, as an append publishes it.
pub fn published(seq: u64, ts: u64, body: EventBody) -> Arc<Published> {
    let (from, cause, fallback) = (None, None, None);
    let (v, ts) = (1, TimestampMs(ts));
    let event = Event {
        seq,
        from,
        v,
        ts,
        cause,
        fallback,
        body,
    };
    let json = serde_json::to_string(&event)
        .expect("serialize")
        .into_boxed_str();
    Arc::new(Published { event, json })
}

/// `project.upserted` of `p_<n>`.
pub fn project(n: u64) -> EventBody {
    body(
        json!({"type": "project.upserted", "data": {"project_id": format!("p_{n}"),
        "name": "p", "path": "/w", "is_git": false, "added_at": n, "position": n,
        "trust_state": "trusted"}}),
    )
}

/// `turn.started` of `t_<n>`.
pub fn turn(n: u32) -> EventBody {
    body(
        json!({"type": "turn.started", "data": {"turn_id": format!("t_{n}"),
        "turn_no": n, "origin": "user", "started_at": n}}),
    )
}

/// `turn.completed` of `t_<n>`.
pub fn end(n: u32) -> EventBody {
    body(
        json!({"type": "turn.completed", "data": {"turn_id": format!("t_{n}"),
        "status": "completed", "ended_at": n, "duration_ms": 1, "final_item_ids": []}}),
    )
}

/// An agent message in turn `t_<n>`; a notice outside any turn for 0.
pub fn item(id: &str, n: u32) -> EventBody {
    let mut data = json!({"item_id": id, "started_at": 1, "kind": "agent_message"});
    if n == 0 {
        data = json!({"item_id": id, "started_at": 1, "kind": "notice", "code": "c", "params": {}});
    } else {
        data["turn_id"] = json!(format!("t_{n}"));
    }
    body(json!({"type": "item.started", "data": data}))
}

/// `raw.db` in `dir`, at this host's schema.
pub fn raw_db(dir: &TempDir) -> Connection {
    let mut conn =
        super::open_writer(&dir.path().join("raw.db"), super::Database::Raw).expect("open raw.db");
    super::prepare_raw(&mut conn, &super::RAW_SCHEMA).expect("prepare raw.db");
    conn
}

/// A frame of instance `a_1`, received at `ts`.
pub fn raw_frame(seq: u64, ts: u64, frame: &str) -> RawFrame {
    RawFrame {
        instance: "a_1".into(),
        seq,
        ts,
        dir: Dir::Recv,
        agent_session_id: Some("s-1".to_owned()),
        frame: frame.to_owned(),
    }
}

/// The frames in `raw.db` in `dir`, by instance and seq.
pub fn raw_rows(dir: &TempDir) -> Vec<RawFrame> {
    let sql = "SELECT instance_id, seq, ts, dir, agent_session_id, frame FROM raw_frames
               ORDER BY instance_id, seq";
    query(&dir.path().join("raw.db"), sql, |row| {
        let dir = Dir::ALL
            .into_iter()
            .find(|dir| dir.as_str() == row.get_ref_unwrap(3).as_str().unwrap_or_default());
        Ok(RawFrame {
            instance: row.get::<_, String>(0)?.into(),
            seq: row.get(1)?,
            ts: row.get(2)?,
            dir: dir.expect("a known dir"),
            agent_session_id: row.get(4)?,
            frame: row.get(5)?,
        })
    })
}

/// `host.db` in `dir`, migrated, and a writer thread on it.
pub fn host_writer(dir: &TempDir, clock: &ManualClock) -> (Writer, WriterThread) {
    let mut conn = super::open_writer(&dir.host_db(), super::Database::Host).expect("open host.db");
    let backups = dir.path().join("backups");
    super::migrate(&mut conn, super::MIGRATIONS, &backups, clock).expect("migrate");
    let quiescence = Quiescence::new();
    quiescence.register(super::WRITER);
    let config = WriterConfig {
        linger: Duration::ZERO,
    };
    Writer::spawn(conn, config, &quiescence).expect("spawn the writer")
}

/// Every row of `sql` on the database at `db`, read with `row`.
pub fn query<T>(db: &Path, sql: &str, row: impl FnMut(&Row<'_>) -> rusqlite::Result<T>) -> Vec<T> {
    let conn = super::open_reader(db).expect("open a reader");
    let mut select = conn.prepare(sql).expect("prepare");
    let rows = select.query_map([], row).expect("query");
    rows.collect::<Result<_, _>>().expect("read")
}

pub fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a runtime")
}
