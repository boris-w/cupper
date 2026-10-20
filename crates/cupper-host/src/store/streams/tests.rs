//! The registry and the writer on a real `host.db`.

use std::path::Path;
use std::time::Duration;

use cupper_protocol::event::EventBody;
use cupper_protocol::methods::stream::HistoryCursor;
use cupper_protocol::view::{SessionView, View};
use proptest::prelude::*;
use serde_json::json;
use tokio::runtime::Runtime;

use super::state::tests::Collect;
use super::*;
use crate::clock::ManualClock;
use crate::quiescence::Quiescence;
use crate::store::history;
use crate::store::testutil::{TempDir, body, end, item, project, turn};
use crate::store::{Database, Durability, MIGRATIONS, WRITER, WriterConfig, migrate, open_writer};

pub struct Fixture {
    _dir: Option<TempDir>,
    pub clock: ManualClock,
    pub writer: Writer,
    pub readers: Readers,
    pub runtime: Runtime,
}

impl Fixture {
    pub fn new(name: &str) -> Self {
        let dir = TempDir::new(name);
        let fixture = Self::at(dir.path());
        Self {
            _dir: Some(dir),
            ..fixture
        }
    }

    /// On the data directory `dir`, which outlives the fixture.
    pub fn at(dir: &Path) -> Self {
        let (host_db, clock) = (dir.join("host.db"), ManualClock::new());
        let mut conn = open_writer(&host_db, Database::Host).unwrap();
        migrate(&mut conn, MIGRATIONS, &dir.join("backups"), &clock).unwrap();
        let quiescence = Quiescence::new();
        quiescence.register(WRITER);
        let linger = Duration::ZERO;
        let (writer, _) = Writer::spawn(conn, WriterConfig { linger }, &quiescence).unwrap();
        let readers = Readers::open(&host_db).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        Self {
            _dir: None,
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

/// Session events in the order the host writes them (design 5.3.11, T014's
/// rulings): a turn ends only once its items are done and its requests
/// answered; outside a turn there are only notices.
#[derive(Default)]
struct Script {
    bodies: Vec<EventBody>,
    turn: u32,
    running: bool,
    open: Vec<String>,
    pending: Vec<String>,
}

impl Script {
    fn step(&mut self, code: u8, text: &str) {
        let (id, n) = (self.bodies.len(), if self.running { self.turn } else { 0 });
        let done = |id: &str| {
            json!({"type": "item.completed", "data": {"item_id": id,
            "status": "completed", "ended_at": 2, "duration_ms": 1}})
        };
        let answer = |id: &str| {
            json!({"type": "interaction.resolved", "data": {"request_id": id,
            "status": "resolved"}})
        };
        let raw = match code {
            0 if self.running => {
                let open = self.open.drain(..).map(|id| body(done(&id)));
                let pending = self.pending.drain(..).map(|id| body(answer(&id)));
                self.bodies.extend(open.chain(pending));
                self.running = false;
                return self.bodies.push(end(self.turn));
            }
            0 => {
                (self.turn, self.running) = (self.turn + 1, true);
                return self.bodies.push(turn(self.turn));
            }
            1 => {
                let item_id = format!("i_{id}");
                self.bodies.push(item(&item_id, n));
                return self.open.extend((n > 0).then_some(item_id));
            }
            2 if !self.open.is_empty() => json!({"type": "item.delta", "data": {
                "item_id": self.open[id % self.open.len()], "field": "text", "text": text}}),
            3 if !self.open.is_empty() => done(&self.open.remove(id % self.open.len())),
            4 if self.running => {
                self.pending.push(format!("r_{id}"));
                json!({"type": "interaction.requested", "data": {"request_id": format!("r_{id}"),
                    "turn_id": format!("t_{n}"), "created_at": 1, "digest": format!("d_{}", "0".repeat(64)),
                    "actionable_by": ["desktop_ui"], "request": {"type": "auth_url",
                    "url": "https://example.com", "message": text}}})
            }
            5 if !self.pending.is_empty() => answer(&self.pending.remove(0)),
            _ => return,
        };
        self.bodies.push(body(raw));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// PROP-stream-replay (design 5.9.8), on the real log: the stream in
    /// memory, the checkpoint plus the events after it once retention pruned
    /// what it may, and the host's fold of the whole log are one view; and a
    /// snapshot plus every history page give back the view of the whole log.
    #[test]
    fn prop_stream_replay_checkpoints_pruning_and_history_give_back_the_log(
        turns in prop::collection::vec(prop::collection::vec((1u8..6, "\\PC{0,60}"), 0..8), 0..30),
        last in prop::collection::vec((0u8..6, "\\PC{0,60}"), 0..10),
        batches in prop::collection::vec(1usize..5, 1..20),
        keep in 0u64..30,
        page_bytes in 1usize..3_000,
    ) {
        // Up to 30 turns, each followed by a notice; then anything, so that
        // a turn may still run. Turns leave the window of 20.
        let mut script = Script::default();
        for steps in &turns {
            let steps = steps.iter().map(|(code, text)| (*code, text.as_str()));
            for (code, text) in [(0, "")].into_iter().chain(steps).chain([(0, ""), (1, "")]) {
                script.step(code, text);
            }
        }
        for (code, text) in &last {
            script.step(*code, text);
        }
        let fx = Fixture::new("replay");
        let retention = Retention { events: keep, age: Duration::ZERO };
        let handle = fx.open(&fx.streams(retention), &session());
        let mut writer = handle.writer().unwrap();
        let (mut rest, mut sizes, mut checkpoint) = (&script.bodies[..], batches.iter().cycle(), 0);
        while !rest.is_empty() {
            let size = sizes.next().copied().unwrap_or(1).min(rest.len());
            let (now, later) = rest.split_at(size);
            let appended = fx.append(&mut writer, now.to_vec());
            prop_assert!(appended.rejected.is_empty());
            if now.iter().any(|body| matches!(body, EventBody::TurnCompleted(_))) {
                checkpoint = appended.head;
            }
            rest = later;
        }

        let mut folded = StreamView::empty(&session());
        for body in &script.bodies {
            folded.fold(body, &mut |_, _| {}).unwrap();
        }
        let live = handle.read(|view, head| (view.clone(), head));
        prop_assert_eq!(&live, &(folded, script.bodies.len() as u64));
        prop_assert_eq!(&fx.view(&fx.streams(retention), &session()), &live);
        let at = fx.sql::<Option<u64>>("SELECT max(at_seq) FROM checkpoints").unwrap_or(0);
        prop_assert_eq!(at, checkpoint, "at the end of the last batch that ended a turn");
        if at > keep.max(4) {
            // Only the latest batch is young: retention did prune.
            prop_assert!(fx.sql::<u64>("SELECT min(seq) FROM events") > 1);
        }

        let (mut client, mut snapshot) = (SessionView::default(), Vec::new());
        let _subscription = handle.subscribe(Arc::new(Collect::default()), None, |s| snapshot = s.ops);
        let first = |view: &SessionView| view.turns.first().map(|t| t.started.turn_no);
        let mut cursor = snapshot.iter().try_for_each(|op| client.apply(op)).ok()
            .and(first(&client)).map(|turn_no| HistoryCursor { turn_no, op_index: None });
        while let Some(at) = cursor {
            let session = "s_1".parse().unwrap();
            let read = fx.readers.read(move |conn| Ok(history::page(conn, &session, at, page_bytes)));
            let page = fx.runtime.block_on(read).unwrap().unwrap();
            for op in &page.ops {
                client.apply(op).unwrap();
            }
            cursor = page.next;
        }
        let mut replay = SessionView::default();
        for body in &script.bodies {
            replay.apply(body).unwrap();
        }
        prop_assert_eq!(client, replay);
    }
}

/// Set in the child that dies between steps 2 and 3: its data directory.
#[cfg(feature = "test-harness")]
const CRASH_DIR: &str = "CUPPER_TEST_STREAMS_CRASH_DIR";

#[cfg(feature = "test-harness")]
fn crash_script() -> [Vec<EventBody>; 2] {
    let turn_end = vec![item("i_2", 1), end(1), item("i_3", 0)];
    [vec![turn(1), item("i_1", 1)], turn_end]
}

/// A host that dies once the turn-end batch has committed, before step 3,
/// loads that batch on its next start: the events, the turn snapshot and the
/// checkpoint (design 5.2.3).
#[cfg(feature = "test-harness")]
#[test]
fn prop_stream_replay_crash_between_commit_and_publish_loads_the_batch() {
    let dir = TempDir::new("crash");
    let child = concat!(module_path!(), "::crash_child")
        .split_once("::")
        .unwrap()
        .1;
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", child, "--ignored", "--nocapture"])
        .env(CRASH_DIR, dir.path())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    let failpoint = crate::exit::HostExit::Failpoint.code().into();
    assert_eq!(out.status.code(), Some(failpoint), "{stderr}");
    assert!(stderr.contains("`store.before_publish` fired"), "{stderr}");

    let fx = Fixture::at(dir.path());
    let mut folded = StreamView::empty(&session());
    for body in crash_script().concat() {
        folded.fold(&body, &mut |_, _| {}).unwrap();
    }
    assert_eq!(
        fx.view(&fx.streams(Retention::DESIGN), &session()),
        (folded, 5)
    );
    assert_eq!(fx.sql::<u64>("SELECT at_seq FROM checkpoints"), 5);
    assert_eq!(
        fx.sql::<String>("SELECT turn_id FROM turn_snapshots"),
        "t_1"
    );
    assert_eq!(fx.sql::<String>("PRAGMA integrity_check"), "ok");
}

#[cfg(feature = "test-harness")]
#[test]
#[ignore = "the child process of prop_stream_replay_crash_between_commit_and_publish_loads_the_batch"]
fn crash_child() {
    let Some(dir) = std::env::var_os(CRASH_DIR) else {
        return;
    };
    let fx = Fixture::at(Path::new(&dir));
    let handle = fx.open(&fx.streams(Retention::DESIGN), &session());
    let mut writer = handle.writer().unwrap();
    let [first, turn_end] = crash_script();
    fx.append(&mut writer, first);
    let plan = serde_json::from_str(r#"[{"name": "store.before_publish"}]"#).unwrap();
    crate::failpoint::arm(&plan).unwrap();
    fx.append(&mut writer, turn_end);
    unreachable!("the failpoint exits");
}

#[test]
fn store_streams_checkpoint_on_demand_stands_at_the_head() {
    let fx = Fixture::new("checkpoint-now");
    let streams = fx.streams(Retention::DESIGN);
    let mut writer = fx.open(&streams, &StreamId::Index).writer().unwrap();
    let at = || fx.sql::<Option<u64>>("SELECT max(at_seq) FROM checkpoints WHERE stream = 'index'");
    fx.runtime.block_on(writer.checkpoint()).unwrap();
    assert_eq!(at(), None, "nothing to checkpoint at seq 0");
    fx.append(&mut writer, vec![project(1), project(2)]);
    fx.runtime.block_on(writer.checkpoint()).unwrap();
    assert_eq!(at(), Some(2));
    drop(writer);
    // A new registry loads the checkpoint, not the events.
    let again = fx.open(&fx.streams(Retention::DESIGN), &StreamId::Index);
    let projects = again.read(|view, head| match view {
        StreamView::Index(index) => (index.projects.len(), head),
        StreamView::Session(_) => unreachable!(),
    });
    assert_eq!(projects, (2, 2));
}
