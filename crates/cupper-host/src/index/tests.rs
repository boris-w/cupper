//! The index actor on a real `host.db`: the reconciliation at start and the
//! checkpoint of the orderly exit. IT-project-add-trust runs projects and
//! trust through the host.

use std::time::Duration;

use cupper_protocol::ids::{ProjectId, SessionId};
use cupper_protocol::index::{SessionStatus, TrustState};

use super::*;
use crate::clock::ManualClock;
use crate::store::streams::StreamsConfig;
use crate::store::testutil::TempDir;
use crate::store::{Database, MIGRATIONS, WRITER, Writer, WriterConfig, migrate, open_writer};

struct Fx {
    dir: TempDir,
    clock: ManualClock,
    db: Writer,
    readers: Readers,
    quiescence: Quiescence,
}

impl Fx {
    fn new(name: &str) -> Self {
        let (dir, clock) = (TempDir::new(name), ManualClock::new());
        let mut conn = open_writer(&dir.host_db(), Database::Host).unwrap();
        migrate(&mut conn, MIGRATIONS, &dir.path().join("backups"), &clock).unwrap();
        let quiescence = Quiescence::new();
        quiescence.register(WRITER);
        quiescence.register(INDEX);
        let config = WriterConfig {
            linger: Duration::ZERO,
        };
        let (db, _) = Writer::spawn(conn, config, &quiescence).unwrap();
        let readers = Readers::open(&dir.host_db()).unwrap();
        Self {
            dir,
            clock,
            db,
            readers,
            quiescence,
        }
    }

    fn streams(&self) -> Streams {
        let clock: Arc<dyn Clock> = Arc::new(self.clock.clone());
        let config = StreamsConfig::new(1, &Durations::default());
        Streams::new(self.db.clone(), self.readers.clone(), clock, config)
    }

    /// A start of the index, as after a restart: a registry of its own.
    async fn start(&self) -> IndexHandle {
        let clock: Arc<dyn Clock> = Arc::new(self.clock.clone());
        let deps = IndexDeps {
            streams: self.streams(),
            readers: self.readers.clone(),
            ids: Arc::new(IdGen::seeded(Arc::clone(&clock), 1)),
            clock,
            durations: Durations::default(),
            git: None,
            quiescence: self.quiescence.clone(),
            host: host(),
        };
        IndexHandle::start(deps).await.unwrap()
    }

    /// The index view, as a new registry loads it from the log.
    async fn view(&self) -> (IndexView, u64) {
        let handle = self.streams().open(&StreamId::Index).await.unwrap();
        handle.read(|view, head| match view {
            StreamView::Index(view) => (view.clone(), head),
            StreamView::Session(_) => unreachable!(),
        })
    }

    async fn sql(&self, query: &'static str) -> Vec<String> {
        let read = self.readers.read(move |conn| {
            let mut select = conn.prepare(query)?;
            let rows = select.query_map([], |row| row.get(0))?;
            rows.collect::<rusqlite::Result<Vec<String>>>()
        });
        read.await.unwrap()
    }
}

fn host() -> HostUpdated {
    HostUpdated {
        version: "0.0.0".into(),
        weak_auth: false,
        pending_upgrade: false,
        power_assertion: false,
        last_exit_abnormal: true,
    }
}

#[tokio::test]
async fn index_start_reconciles_with_the_tables_and_checkpoints_on_stop() {
    let fx = Fx::new("index-reconcile");
    let (kept, gone) = (ProjectId::from_uuid(1), ProjectId::from_uuid(2));
    let session = SessionId::from_uuid(3);
    // The log knows a project the table no longer has.
    let mut writer = fx
        .streams()
        .open(&StreamId::Index)
        .await
        .unwrap()
        .writer()
        .unwrap();
    let mut upserted = Project {
        id: gone.clone(),
        name: "gone".into(),
        path: "/work/gone".into(),
        git_common_dir: None,
        added_at: 1,
        position: 0,
        trust: Trust::added(&crate::trust::scan(fx.dir.path(), None).unwrap(), 1),
    };
    let body = EventBody::ProjectUpserted(upserted.upserted());
    writer
        .append(Batch {
            bodies: vec![body],
            ..Batch::default()
        })
        .await
        .unwrap();
    drop(writer);
    upserted.id = kept.clone();
    let (kept_row, session_row) = (kept.to_string(), session.to_string());
    let rows = fx.db.submit(Durability::Normal, move |conn| {
        conn.execute(
            "INSERT INTO projects (project_id, path, name, position, added_at) VALUES (?1, '/work/kept', 'kept', 0, 1)",
            [&kept_row],
        )?;
        conn.execute(
            "INSERT INTO sessions (session_id, project_id, agent, workspace_kind, workspace_path, state,
                 last_turn_end_seq, last_read_seq, created_at, updated_at)
             VALUES (?1, ?2, 'claude', 'local', '/work/kept', 'idle', 3, 1, 1, 2)",
            [&session_row, &kept_row],
        )
    });
    rows.await.unwrap();

    let index = fx.start().await;
    let (view, head) = fx.view().await;
    assert_eq!(view.projects.keys().collect::<Vec<_>>(), [&kept]);
    assert_eq!(view.projects[&kept].trust_state, TrustState::Untrusted);
    assert_eq!(view.sessions[&session].status, SessionStatus::Unread);
    assert_eq!(view.host, Some(host()));
    // Nothing differs any more: the next start writes nothing.
    let again = fx.start().await;
    assert_eq!(fx.view().await.1, head);

    Part::stop(&again, false).await;
    let at = fx
        .sql("SELECT CAST(at_seq AS TEXT) FROM checkpoints WHERE stream = 'index'")
        .await;
    assert_eq!(at, [head.to_string()]);
    drop(index);
}
