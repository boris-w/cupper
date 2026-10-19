//! The registry and the writer on a real `host.db`.

use std::time::Duration;

use cupper_protocol::event::EventBody;
use tokio::runtime::Runtime;

use super::state::tests::Collect;
use super::*;
use crate::clock::ManualClock;
use crate::quiescence::Quiescence;
use crate::store::testutil::{TempDir, end, item, project, turn};
use crate::store::{Database, Durability, MIGRATIONS, WRITER, WriterConfig, migrate, open_writer};

pub struct Fixture {
    _dir: TempDir,
    pub clock: ManualClock,
    pub writer: Writer,
    pub readers: Readers,
    pub runtime: Runtime,
}

impl Fixture {
    pub fn new(name: &str) -> Self {
        let dir = TempDir::new(name);
        let clock = ManualClock::new();
        let mut conn = open_writer(&dir.host_db(), Database::Host).unwrap();
        migrate(&mut conn, MIGRATIONS, &dir.path().join("backups"), &clock).unwrap();
        let quiescence = Quiescence::new();
        quiescence.register(WRITER);
        let linger = Duration::ZERO;
        let (writer, _) = Writer::spawn(conn, WriterConfig { linger }, &quiescence).unwrap();
        let readers = Readers::open(&dir.host_db()).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let _dir = dir;
        Self {
            _dir,
            clock,
            writer,
            readers,
            runtime,
        }
    }

    /// A registry of its own on the fixture's log, as after a restart.
    pub fn streams(&self, retention: Retention) -> Streams {
        let (epoch, idle) = (1, Duration::from_secs(600));
        let config = StreamsConfig {
            epoch,
            idle,
            retention,
        };
        let clock = Arc::new(self.clock.clone());
        Streams::new(self.writer.clone(), self.readers.clone(), clock, config)
    }

    pub fn open(&self, streams: &Streams, id: &StreamId) -> StreamHandle {
        self.runtime.block_on(streams.open(id)).unwrap()
    }

    /// Appends `bodies` a second after the last append.
    pub fn append(&self, writer: &mut StreamWriter, bodies: Vec<EventBody>) -> Appended {
        self.clock.advance(Duration::from_secs(1));
        let batch = Batch {
            bodies,
            ..Batch::default()
        };
        self.runtime.block_on(writer.append(batch)).unwrap()
    }

    pub fn view(&self, streams: &Streams, id: &StreamId) -> (StreamView, u64) {
        self.open(streams, id)
            .read(|view, head| (view.clone(), head))
    }

    pub fn sql<T: rusqlite::types::FromSql + Send + 'static>(&self, query: &'static str) -> T {
        let read = self
            .readers
            .read(move |conn| conn.query_row(query, [], |row| row.get(0)));
        self.runtime.block_on(read).unwrap()
    }
}

fn session() -> StreamId {
    "session/s_1".parse().unwrap()
}

#[test]
fn store_streams_append_commits_the_turn_end_whole_and_reloads() {
    let fx = Fixture::new("append");
    let streams = fx.streams(Retention::DESIGN);
    let handle = fx.open(&streams, &session());
    let mut writer = handle.writer().unwrap();
    assert!(matches!(handle.writer(), Err(StreamError::WriterTaken)));
    let sink = Arc::new(Collect {
        budget: 9,
        ..Collect::default()
    });
    let _subscription = handle.subscribe(sink.clone(), None, drop);

    fx.append(&mut writer, vec![turn(1), item("i_1", 1)]);
    let appended = fx.append(&mut writer, vec![end(1), item("i_2", 0)]);
    assert_eq!((appended.head, appended.rejected), (4, Vec::new()));
    assert_eq!(*sink.seqs.lock().unwrap(), [1, 2, 3, 4]);
    let checkpoint = "SELECT at_seq FROM checkpoints WHERE stream = 'session/s_1'";
    assert_eq!(fx.sql::<u64>(checkpoint), 4, "at the end of the batch");
    assert_eq!(
        fx.sql::<String>("SELECT turn_id FROM turn_snapshots"),
        "t_1"
    );

    // A batch whose own rows fail commits nothing; its seqs are given out again.
    let failing = Batch {
        bodies: vec![turn(2)],
        with: Some(Box::new(|conn| {
            conn.execute_batch("INSERT INTO missing VALUES (1)")
        })),
        ..Batch::default()
    };
    let failed = fx.runtime.block_on(writer.append(failing));
    assert!(matches!(failed, Err(StreamError::Write(WriteError::Op(_)))));
    assert_eq!(fx.append(&mut writer, vec![turn(2)]).head, 5);
    assert_eq!(
        fx.view(&fx.streams(Retention::DESIGN), &session()),
        handle.read(|v, h| (v.clone(), h))
    );
}

#[test]
fn store_streams_idle_streams_leave_memory_after_the_idle_time() {
    let fx = Fixture::new("idle");
    let streams = fx.streams(Retention::DESIGN);
    let loaded = |id: &StreamId| streams.registry.lock().loaded.contains_key(id);
    let handle = fx.open(&streams, &StreamId::Index);
    let mut writer = handle.writer().unwrap();
    fx.append(&mut writer, vec![project(1)]);
    drop(handle);
    fx.clock.advance(Duration::from_secs(3600));
    drop(fx.open(&streams, &session()));
    assert!(loaded(&StreamId::Index), "its writer keeps it");

    drop(writer);
    fx.clock.advance(Duration::from_secs(599));
    drop(fx.open(&streams, &session()));
    assert!(loaded(&StreamId::Index));
    fx.clock.advance(Duration::from_secs(1));
    drop(fx.open(&streams, &session()));
    assert!(!loaded(&StreamId::Index) && loaded(&session()));
    assert_eq!(streams.registry.lock().evictions, 1);
    assert_eq!(fx.view(&streams, &StreamId::Index).1, 1, "loaded again");
}

#[test]
fn store_streams_checkpoints_of_another_view_version_give_way() {
    let fx = Fixture::new("discard");
    let streams = fx.streams(Retention::DESIGN);
    let (mut session_writer, mut index_writer) = (
        fx.open(&streams, &session()).writer().unwrap(),
        fx.open(&streams, &StreamId::Index).writer().unwrap(),
    );
    for n in 1..=22 {
        fx.append(
            &mut session_writer,
            vec![turn(n), item(&format!("i_{n}"), n), end(n)],
        );
    }
    fx.append(&mut session_writer, vec![item("i_tail", 0)]);
    // The index checkpoints every 1,000 events.
    fx.append(&mut index_writer, (1..1000).map(project).collect());
    assert_eq!(fx.sql::<u64>("SELECT count(*) FROM checkpoints"), 1);
    fx.append(&mut index_writer, vec![project(1000), project(1001)]);
    assert_eq!(
        fx.sql::<u64>("SELECT max(at_seq) FROM checkpoints WHERE stream = 'index'"),
        1001
    );

    let expected = [session(), StreamId::Index].map(|id| fx.view(&streams, &id));
    let discard = "UPDATE checkpoints SET view_v = 0";
    fx.runtime
        .block_on(
            fx.writer
                .submit(Durability::Normal, |conn| conn.execute_batch(discard)),
        )
        .unwrap();
    let rebuilt =
        [session(), StreamId::Index].map(|id| fx.view(&fx.streams(Retention::DESIGN), &id));
    assert_eq!(
        rebuilt, expected,
        "the latest 20 turns and the tail; the whole index log"
    );
}
