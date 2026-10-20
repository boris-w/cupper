//! The index actor on a real `host.db`, with no git: projects and trust
//! through the port, the reconciliation at start, and the checkpoint of the
//! orderly exit. IT-project-add-trust runs the same through the host.

use std::fs;
use std::time::Duration;

use cupper_protocol::domain::{ClientKind, PrincipalKind};
use cupper_protocol::ids::{CommandId, ProjectId, SessionId};
use cupper_protocol::index::{SessionStatus, TrustState};
use cupper_protocol::methods::handshake::{ClientIdentity, Principal};
use serde_json::{Value, json};

use super::*;
use crate::clock::ManualClock;
use crate::ledger::{Begin, Ledger};
use crate::store::streams::StreamsConfig;
use crate::store::testutil::TempDir;
use crate::store::{Database, MIGRATIONS, WRITER, Writer, WriterConfig, migrate, open_writer};

struct Fx {
    dir: TempDir,
    clock: ManualClock,
    db: Writer,
    readers: Readers,
    ledger: Ledger,
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
        let reader = crate::store::open_reader(&dir.host_db()).unwrap();
        let shared: Arc<dyn Clock> = Arc::new(clock.clone());
        let ledger = Ledger::load(&reader, db.clone(), shared, &Durations::default()).unwrap();
        Self {
            dir,
            clock,
            db,
            readers,
            ledger,
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

    async fn call(&self, index: &IndexHandle, method: &'static str, params: Value) -> Outcome {
        let principal = Principal {
            id: "pr_test".parse().unwrap(),
            kind: PrincipalKind::TestHarness,
            role: None,
            scopes: Vec::new(),
        };
        let ticket = params.get("command_id").map(|id| {
            let id: CommandId = serde_json::from_value(id.clone()).unwrap();
            match self
                .ledger
                .begin(&principal.id, &id, method, &params)
                .unwrap()
            {
                Begin::Run(ticket) => ticket,
                Begin::Repeat(_) => panic!("a new command"),
            }
        });
        let conn = ConnId::from_index(1);
        let client = ClientIdentity {
            kind: ClientKind::Cli,
            name: "acme-test".into(),
            version: "0".into(),
        };
        let call = Call {
            conn,
            principal,
            client,
            method,
            params,
            ticket,
        };
        index.deliver(call).await
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
async fn index_projects_add_trust_and_turn_stale() {
    let fx = Fx::new("index-projects");
    let index = fx.start().await;
    let app = fx.dir.path().join("work/app");
    fs::create_dir_all(app.join(".claude")).unwrap();
    fs::write(
        app.join(".claude/settings.json"),
        r#"{"env": {"ACME": "1"}}"#,
    )
    .unwrap();
    let path = app.to_str().unwrap();

    let added = fx
        .call(
            &index,
            "project/add",
            json!({"command_id": "c-1", "path": path}),
        )
        .await;
    let added = added.unwrap();
    assert_eq!(added["trust"]["state"], "untrusted");
    assert_eq!(added["trust"]["findings"][0]["detail"], "ACME=1");
    let (project, digest) = (
        added["project_id"].clone(),
        added["trust"]["trust_digest"].clone(),
    );
    let again = fx
        .call(
            &index,
            "project/add",
            json!({"command_id": "c-2", "path": path}),
        )
        .await;
    assert_eq!(again.unwrap()["project_id"], project);

    let wrong = json!({"command_id": "c-3", "project_id": project, "trust_digest": format!("d_{}", "0".repeat(64))});
    let err = fx.call(&index, "project/trust", wrong).await.unwrap_err();
    assert_eq!(err.reason(), Some(&ErrorReason::DigestMismatch));
    let right = json!({"command_id": "c-4", "project_id": project, "trust_digest": digest});
    let trusted = fx.call(&index, "project/trust", right).await.unwrap();
    assert_eq!(trusted, json!({"trust_state": "trusted"}));

    fs::write(app.join(".claude/settings.json"), "{}").unwrap();
    let info = fx
        .call(&index, "project/trust_info", json!({"project_id": project}))
        .await;
    let info = info.unwrap();
    assert_eq!(info["state"], "stale");
    let diff = json!([{"path": ".claude/settings.json", "change": "update"}]);
    assert_eq!(info["diff_from_trusted"], diff);

    fs::create_dir_all(fx.dir.path().join("work/empty")).unwrap();
    let empty = json!({"command_id": "c-5", "path": fx.dir.path().join("work/empty")});
    let empty = fx.call(&index, "project/add", empty).await.unwrap();
    assert_eq!(empty["trust"]["state"], "trusted");
    let missing = json!({"command_id": "c-6", "path": fx.dir.path().join("work/none")});
    let err = fx.call(&index, "project/add", missing).await.unwrap_err();
    assert_eq!(err.app_code(), Some(&ErrorCode::NotFound));

    let states = fx.sql("SELECT trust_state || ' ' || coalesce(trusted_by, '-') FROM projects ORDER BY position").await;
    assert_eq!(states, ["stale pr_test", "trusted auto_empty"]);
    let (view, _) = fx.view().await;
    let shown: Vec<_> = view
        .projects
        .values()
        .map(|p| p.trust_state.clone())
        .collect();
    assert_eq!(shown, [TrustState::Stale, TrustState::Trusted]);
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
