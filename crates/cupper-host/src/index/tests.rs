//! The index actor on a real `host.db`: the reconciliation at start, the
//! checkpoint of the orderly exit, and the notification intents;
//! IT-elevation-grant through the gateway's dispatch. IT-project-add-trust
//! runs projects and trust through the host.

use std::time::Duration;

use cupper_protocol::annotations::{TimestampMs, WirePath};
use cupper_protocol::domain::{AgentKind, ClientKind, FailureCode, PrincipalKind};
use cupper_protocol::ids::{ElevationId, ProjectId, RequestId, SessionId};
use cupper_protocol::index::{
    Elevation, PendingKind, SessionStatus, TitleSource, TrustState, WorkspaceKind,
};
use cupper_protocol::methods::admin::VerifiedRequester;
use cupper_protocol::methods::handshake::{ClientIdentity, Principal};
use cupper_protocol::rpc::Scope;
use cupper_protocol::session::TurnStatus;

use super::*;
use crate::clock::ManualClock;
use crate::elevation::Examined;
use crate::gateway::{AuditEvent, Routes};
use crate::ledger::Ledger;
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
        let audit = mpsc::unbounded_channel().0;
        self.start_with(Arc::new(crate::elevation::FailClosed), audit)
            .await
    }

    async fn start_with(
        &self,
        requesters: Arc<dyn Requesters>,
        audit: mpsc::UnboundedSender<AuditRecord>,
    ) -> IndexHandle {
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
            requesters,
            audit,
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
    // And an elevation request of the last run.
    let asked = EventBody::ElevationRequested(cupper_protocol::index::ElevationRequested {
        elevation_id: ElevationId::from_uuid(4),
        client_name: "cupper".into(),
        scopes: vec![Scope::Write],
        requested_at: TimestampMs(1),
    });
    writer
        .append(Batch {
            bodies: vec![body, asked],
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
    assert!(view.elevations.is_empty());
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
        reply_queued: tokio::sync::oneshot::channel().1,
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

/// IT-orch-trust-gating's other half: a session's rescan reaches the index,
/// which records a trusted project whose configuration changed as `stale`.
#[tokio::test]
async fn it_orch_trust_gating_the_index_records_a_stale_project() {
    let fx = Fx::new("index-stale");
    let id = ProjectId::from_uuid(1);
    let empty = fx.dir.path().join("app");
    std::fs::create_dir(&empty).unwrap();
    let trusted = crate::trust::scan(&empty, None).unwrap().digest.to_string();
    let row = id.to_string();
    let rows = fx.db.submit(Durability::Normal, move |conn| {
        conn.execute(
            "INSERT INTO projects (project_id, path, name, position, added_at, trust_state,
                 trust_digest, trust_manifest, trusted_at, trusted_by)
             VALUES (?1, '/work/app', 'app', 0, 1, 'trusted', ?2, '{}', 1, 'pr_desktop')",
            [&row, &trusted],
        )
    });
    rows.await.unwrap();
    let index = fx.start().await;
    std::fs::write(empty.join(".mcp.json"), "{}").unwrap();
    index.rescanned(&id, crate::trust::scan(&empty, None).unwrap());
    // The checkpoint comes after the rescan in the actor's mailbox.
    Part::stop(&index, false).await;
    assert_eq!(fx.sql("SELECT trust_state FROM projects").await, ["stale"]);
    assert_eq!(
        fx.view().await.0.projects[&id].trust_state,
        TrustState::Stale
    );
}

/// Every requester is clear of the agents (T133 decides it for real).
#[derive(Debug)]
struct Clear;

impl Requesters for Clear {
    fn examine(&self, token: cupper_sys::AuditToken) -> BoxFuture<'static, Examined> {
        let verified = VerifiedRequester {
            executable: WirePath("/usr/local/bin/cupper".into()),
            signing: None,
            pid: token.pid() as u32,
            parent_chain: Vec::new(),
            has_tty: true,
            origin_unverifiable: false,
            bundled_cli: false,
        };
        Box::pin(futures::future::ready(Examined::Clear(verified)))
    }
}

/// Refused for want of a scope.
fn scope_denied(reply: Result<serde_json::Value, serde_json::Value>) -> bool {
    reply.is_err_and(|error| error["data"]["reason"] == "scope")
}

#[tokio::test]
async fn it_elevation_grant_one_connection_until_it_closes_or_fifteen_minutes() {
    use crate::gateway::ElevationStep::*;
    use crate::gateway::testkit::{Peer, TestGateway};
    use crate::policy::testkit::column;
    use cupper_protocol::index::ElevationDecision::Granted;
    use serde_json::json;
    let fx = Fx::new("elevation-grant");
    let (records, mut audit) = mpsc::unbounded_channel();
    let index = fx.start_with(Arc::new(Clear), records).await;
    let (clock, durations): (Arc<dyn Clock>, _) =
        (Arc::new(fx.clock.clone()), Durations::default());
    let reader = crate::store::open_reader(&fx.dir.host_db()).unwrap();
    let ledger = Ledger::load(&reader, fx.db.clone(), Arc::clone(&clock), &durations);
    let mut routes = Routes::new(ledger.unwrap());
    routes.index(Arc::new(index.clone()));
    let (config, unheard) = (TestGateway::config(None), mpsc::unbounded_channel().0);
    let gateway = Gateway::new(config, clock, durations, Routes::default(), unheard);
    index.serve_for(&gateway);
    let (mut cli, mut other) = (Peer::on(1, column("cli")), Peer::on(2, column("cli")));
    let mut control = Peer::on(3, column("UI control"));
    for peer in [&cli, &other, &control] {
        peer.open(&gateway, &routes);
    }
    let app = fx.dir.path().join("app");
    std::fs::create_dir(&app).unwrap();
    let add = |n: u8| json!({"command_id": format!("add-{n}"), "path": app.to_str().unwrap()});
    assert!(scope_denied(cli.call(&routes, "project/add", add(1)).await));

    // One request at a time; the dialog gets the self-reported text cut.
    let ask = json!({"scopes": ["write", "approve"], "reason": "r".repeat(300)});
    let asked = cli.send(&routes, "auth/elevate", ask);
    fx.quiescence.settled().await;
    let first = fx.view().await.0.elevations.into_keys().next().unwrap();
    let write = json!({"scopes": ["write"]});
    let busy = other.call(&routes, "auth/elevate", write.clone()).await;
    assert_eq!(busy.unwrap_err()["data"]["retry_after_ms"], 120_000);
    let get = json!({"elevation_id": first});
    let detail = control.call(&routes, "auth/elevation_get", get).await;
    let detail = detail.unwrap();
    assert_eq!(detail["self_reported"]["reason"], "r".repeat(200));
    assert_eq!(detail["verified"]["pid"], std::process::id());

    // `approve` was asked for but not given: write only, for this connection.
    let respond = |n: u8, id: &ElevationId| {
        json!({"command_id": format!("respond-{n}"), "elevation_id": id,
            "decision": "grant", "scopes": ["write"]})
    };
    let granted = control.call(&routes, "auth/elevation_respond", respond(1, &first));
    assert_eq!(granted.await, Ok(json!({})));
    let expires_at = fx.clock.now_wall_ms() + 15 * 60_000;
    let reply = json!({"granted": ["write"], "expires_at": expires_at});
    assert_eq!(cli.reply(asked).await, Ok(reply));
    assert!(cli.call(&routes, "project/add", add(2)).await.is_ok());
    assert!(scope_denied(
        other.call(&routes, "project/add", add(3)).await
    ));
    fx.quiescence.settled().await;
    let elevated = |view: IndexView| {
        let clients = view.clients.unwrap().clients.into_iter();
        clients
            .map(|c| (c.conn_id.to_string(), c.elevated))
            .collect::<Vec<_>>()
    };
    let expires_at = TimestampMs(expires_at);
    let until = Some(Elevation {
        scopes: vec![Scope::Write],
        expires_at,
    });
    assert_eq!(elevated(fx.view().await.0)[0], ("c_1".into(), until));

    // Fifteen minutes on, the grant has ended.
    fx.clock.advance(Duration::from_secs(15 * 60));
    fx.quiescence.settled().await;
    assert!(scope_denied(cli.call(&routes, "project/add", add(4)).await));
    assert_eq!(elevated(fx.view().await.0)[0], ("c_1".into(), None));

    // Granted again, the grant ends as the connection closes.
    let asked = cli.send(&routes, "auth/elevate", write);
    fx.quiescence.settled().await;
    let second = fx.view().await.0.elevations.into_keys().next().unwrap();
    let granted = control.call(&routes, "auth/elevation_respond", respond(2, &second));
    assert_eq!(granted.await, Ok(json!({})));
    assert!(cli.reply(asked).await.is_ok());
    assert!(cli.opened.granted.get().is_some());
    cli.close(&gateway, &routes);
    fx.quiescence.settled().await;
    assert_eq!(cli.opened.granted.get(), None);
    let view = fx.view().await.0;
    assert!(view.elevations.is_empty());
    let left: Vec<_> = elevated(view).into_iter().map(|(conn, _)| conn).collect();
    assert_eq!(left, ["c_2", "c_3"]);

    let steps = std::iter::from_fn(|| audit.try_recv().ok()).map(|record| match record.event {
        AuditEvent::Elevation { step, scopes } => (record.conn.unwrap().to_string(), step, scopes),
        event => panic!("only elevations: {event:?}"),
    });
    let (both, write) = (vec![Scope::Write, Scope::Approve], vec![Scope::Write]);
    let expected = [
        ("c_1".into(), Requested(first.clone()), both),
        ("c_2".into(), RateLimited, write.clone()),
        ("c_1".into(), Resolved(first, Granted), write.clone()),
        ("c_1".into(), Requested(second.clone()), write.clone()),
        ("c_1".into(), Resolved(second, Granted), write),
    ];
    assert_eq!(steps.collect::<Vec<_>>(), expected);
}
