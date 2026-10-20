//! The index actor on a real `host.db`: the reconciliation at start, the
//! checkpoint of the orderly exit, and the notification intents.
//! IT-project-add-trust runs projects and trust through the host.

use std::time::Duration;

use cupper_protocol::annotations::TimestampMs;
use cupper_protocol::domain::{AgentKind, ClientKind, FailureCode, PrincipalKind};
use cupper_protocol::ids::{ProjectId, RequestId, SessionId};
use cupper_protocol::index::{PendingKind, SessionStatus, TitleSource, TrustState, WorkspaceKind};
use cupper_protocol::methods::handshake::{ClientIdentity, Principal};
use cupper_protocol::rpc::Scope;
use cupper_protocol::session::TurnStatus;

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
            db: self.db.clone(),
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

    /// Lets what was sent settle, and the gathered changes go out.
    async fn flush(&self) {
        self.quiescence.settled().await;
        self.clock.advance(status::COALESCE);
        self.quiescence.settled().await;
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

fn summary(session: &SessionId) -> SessionSummary {
    SessionSummary {
        session_id: session.clone(),
        project_id: ProjectId::from_uuid(9),
        agent: AgentKind::Claude,
        title: "Fix the build".into(),
        title_source: TitleSource::Prompt,
        workspace_kind: WorkspaceKind::Local,
        branch: None,
        status: SessionStatus::Idle,
        unread: false,
        pending_interactions: 0,
        pending_kind: None,
        last_activity_at: TimestampMs(1),
        error_code: None,
        read_only: false,
    }
}

/// `client/presence` from connection `conn`, as the gateway delivers it.
async fn presence(index: &IndexHandle, conn: u64, kind: PrincipalKind, seen: (bool, &SessionId)) {
    let call = Call {
        conn: ConnId::from_index(conn),
        principal: Principal {
            id: "pr_test".parse().unwrap(),
            kind,
            role: None,
            scopes: vec![Scope::Read],
        },
        client: ClientIdentity {
            kind: ClientKind::Desktop,
            name: "Cupper".into(),
            version: "0.0.0".into(),
        },
        method: ClientPresence::NAME,
        params: json!({"foreground": seen.0, "session_id": seen.1}),
        ticket: None,
    };
    index.deliver(call).await.unwrap();
}

fn ended(status: TurnStatus) -> Note {
    let error = (status == TurnStatus::Failed).then_some(FailureCode::ProcessExited);
    Note::TurnEnded {
        seq: 3,
        status,
        error,
    }
}

fn pending(count: u32) -> Note {
    let oldest = (count > 0).then(|| (PendingKind::Approval, RequestId::from_uuid(7)));
    Note::Pending { count, oldest }
}

#[tokio::test]
async fn unit_index_intent_only_when_no_desktop_ui_in_front_watches() {
    let fx = Fx::new("index-intent");
    let index = fx.start().await;
    let [watched, claimed, alone] = [1, 2, 3].map(SessionId::from_uuid);
    for session in [&watched, &claimed, &alone] {
        index.created(summary(session));
    }
    // The desktop UI shows `watched`; a CLI says it shows `claimed`.
    presence(&index, 1, PrincipalKind::DesktopUi, (true, &watched)).await;
    presence(&index, 2, PrincipalKind::Cli, (true, &claimed)).await;
    for session in [&watched, &claimed] {
        index.note(
            session,
            5,
            vec![Note::TurnStarted, ended(TurnStatus::Completed)],
        );
        index.note(session, 6, vec![pending(1)]);
    }
    // Answered within the 100 ms; a cancelled turn calls for nothing.
    index.note(&alone, 5, vec![pending(1)]);
    index.note(&alone, 6, vec![pending(0), ended(TurnStatus::Cancelled)]);
    fx.flush().await;
    // The desktop UI goes to the background, comes back, and closes.
    presence(&index, 1, PrincipalKind::DesktopUi, (false, &watched)).await;
    index.note(&watched, 7, vec![ended(TurnStatus::Failed)]);
    fx.flush().await;
    presence(&index, 1, PrincipalKind::DesktopUi, (true, &watched)).await;
    index.closed(&ConnId::from_index(1));
    index.note(&watched, 8, vec![pending(0), ended(TurnStatus::Completed)]);
    fx.flush().await;

    let intents = fx
        .sql(
            "SELECT json_extract(json, '$.data.session_id') || ' ' ||
                    json_extract(json, '$.data.reason') || ' ' ||
                    coalesce(json_extract(json, '$.data.request_id'), '-')
             FROM events WHERE stream = 'index' AND type = 'notification.intent' ORDER BY seq",
        )
        .await;
    let request = RequestId::from_uuid(7);
    let expected = [
        format!("{claimed} turn_completed -"),
        format!("{claimed} needs_action {request}"),
        format!("{watched} error -"),
        format!("{watched} turn_completed -"),
    ];
    assert_eq!(intents, expected);
    let (view, _) = fx.view().await;
    let status = |id| view.sessions[id].status.clone();
    assert_eq!(status(&claimed), SessionStatus::NeedsAction);
    assert_eq!(status(&alone), SessionStatus::Unread);
}
