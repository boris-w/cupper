//! The index (design 5.2.9, 5.3.5): `IndexActor` is the only writer of the
//! index stream and of the `projects` table (design 5.2.3, rule 1), and
//! [`IndexHandle`] is how the rest of the host reaches it: the gateway's
//! index port, the sessions' [`IndexNotes`], and a [`Part`] of the orderly
//! exit, which writes the stream's checkpoint.
//!
//! - [`IndexHandle::start`] is startup step 7 (design 5.2.13), before the
//!   socket: it loads the stream, reconciles it with `projects` and
//!   `sessions`, and writes `host.updated`.
//! - Projects and trust are in `projects`; the slow part of each call runs in
//!   a task of its own and comes back to the actor as a message (rule 5).
//! - `clients.updated` lists [`Gateway::clients`] on every connection opened
//!   or closed, and on every elevation granted or ended.
//! - Session status, read marks, presence and notification intents are the
//!   [`status`] core's, fed by the sessions' [`IndexNotes`], `session/mark_read`
//!   and `client/presence`. `sessions.last_read_seq` is the one column of
//!   `sessions` the index writes; the session actors never do.
//! - `agents.updated` merges what each agent runtime reports ([`AgentNotes`]).
//! - Elevation (design 5.8.4) is in [`elevations`].
//! - The status core's timers and elevation's share one table and one alarm:
//!   the actor waits in one place (design 5.2.16).

mod budget;
mod elevations;
mod projects;
mod rows;
mod status;

use std::mem;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, Weak};

use cupper_protocol::domain::AgentKind;
use cupper_protocol::envelope::Cause;
use cupper_protocol::error::{ErrorCode, ErrorReason, INVALID_PARAMS, RpcError};
use cupper_protocol::event::EventBody;
use cupper_protocol::ids::{ConnId, IntentId, ProjectId, SessionId};
use cupper_protocol::index::{
    AgentStatus, AgentsUpdated, ClientsUpdated, GitTools, HostUpdated, NotificationIntent,
    RuntimeFailure, RuntimeState, SessionSummary, TrustState,
};
use cupper_protocol::methods::handshake::{ClientPresence, ClientPresenceParams};
use cupper_protocol::methods::project::{
    ProjectAdd, ProjectAddParams, ProjectTrust, ProjectTrustParams,
};
use cupper_protocol::methods::session::{SessionMarkRead, SessionMarkReadParams};
use cupper_protocol::rpc::{Method, Notification};
use cupper_protocol::sync::StreamId;
use cupper_protocol::view::IndexView;
use futures::FutureExt;
use futures::future::BoxFuture;
use serde::de::DeserializeOwned;
use serde_json::json;
use tokio::sync::{mpsc, oneshot};

use self::elevations::{Asking, Elevations};
use self::projects::{Context, Prepared};
use self::rows::{Project, Tables};
use self::status::{Action, Flush, Input, Status};
use crate::clock::{Clock, IdGen};
use crate::driver::runtime::status::AgentNotes;
use crate::durations::Durations;
use crate::elevation::{Ask, ElevationTimer, Requesters};
use crate::gateway::{
    AuditEvent, AuditRecord, Call, Gateway, IndexPort, Opened, Port, Reply, TrustChange,
};
use crate::git::GitTool;
use crate::ledger::{Outcome, Ticket};
use crate::orchestrator::session::{IndexNotes, Note};
use crate::pure::{Alarm, Core, TimerRequest, TimerTable};
use crate::quiescence::{ActorKind, Participant, Quiescence};
use crate::store::streams::{Batch, StreamError, StreamHandle, StreamView, StreamWriter, Streams};
use crate::store::{Durability, ReadError, Readers, Writer};
use crate::supervisor::{Activity, Part};
use crate::trust::{Scan, Trust};

pub use self::budget::{BUDGET, DETAIL_MAX};
pub(crate) use self::projects::{Context as ScanContext, git_of};

/// The index actor's kind in the quiescence barrier.
pub const INDEX: ActorKind = ActorKind::new("index");

pub struct IndexDeps {
    pub streams: Streams,
    pub readers: Readers,
    /// For `sessions.last_read_seq`, written without events.
    pub db: Writer,
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<IdGen>,
    pub durations: Durations,
    /// The git `project/add` asks where a work tree is; without one, every
    /// project is outside git.
    pub git: Option<GitTool>,
    pub quiescence: Quiescence,
    /// `host.updated` of this run.
    pub host: HostUpdated,
    /// What examines a CLI that asks for elevation.
    pub requesters: Arc<dyn Requesters>,
    /// The host's audit channel (design 5.8.6).
    pub audit: mpsc::UnboundedSender<AuditRecord>,
}

#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error(transparent)]
    Stream(#[from] StreamError),
    #[error(transparent)]
    Read(#[from] ReadError),
}

/// The index actor's handle. Clones share the actor.
#[derive(Debug, Clone)]
pub struct IndexHandle {
    mail: mpsc::UnboundedSender<Mail>,
    gateway: Arc<OnceLock<Weak<Gateway>>>,
}

#[derive(Debug)]
enum Mail {
    Call(Box<Call>, oneshot::Sender<Outcome>),
    Prepared(Box<Done>),
    Opened(Box<Opened>),
    Closed(ConnId),
    Examined(Box<(Ask, Asking)>),
    Status(Input),
    Rescanned(ProjectId, Scan),
    Agent(Box<AgentStatus>),
    /// The self-check found the code on disk changed.
    PendingUpgrade,
    /// The runtime installer's state of Claude's adapter runtime.
    Runtime(RuntimeState, Option<RuntimeFailure>),
    Checkpoint(oneshot::Sender<()>),
    /// `test/seed` wrote sessions: reconcile with the tables again.
    #[cfg(feature = "test-harness")]
    Reconcile(oneshot::Sender<()>),
    /// The alarm rang; nobody sends it.
    Rung,
}

/// A timer of the status core or of elevation, in the actor's one table.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Timer {
    Status(Flush),
    Elevation(ElevationTimer),
}

#[derive(Debug)]
struct Done {
    call: Call,
    reply: oneshot::Sender<Outcome>,
    prepared: Result<Prepared, RpcError>,
}

impl IndexHandle {
    /// Startup step 7 (design 5.2.13): loads the index stream, brings it to
    /// what the tables say, and starts the actor. Call within the runtime.
    pub async fn start(deps: IndexDeps) -> Result<Self, IndexError> {
        let handle = deps.streams.open(&StreamId::Index).await?;
        let mut writer = handle.writer()?;
        let mut tables = deps.readers.read(rows::read).await?;
        let mut status = Status::default();
        let mut bodies = reconciled(&handle, &mut tables, &mut status, &deps.host);
        bodies.extend(read(&handle, elevations::left_over));
        writer
            .append(Batch {
                bodies,
                ..Batch::default()
            })
            .await?;
        let (mail, inbox) = mpsc::unbounded_channel();
        let gateway = Arc::default();
        let participant = deps.quiescence.enter(INDEX);
        let actor = IndexActor {
            handle,
            writer,
            tables,
            status,
            timers: TimerTable::new(),
            alarm: Alarm::new(Arc::clone(&deps.clock)),
            elevations: Elevations::new(&deps.durations),
            mail: mail.clone(),
            gateway: Arc::clone(&gateway),
            deps,
            runtime: None,
        };
        tokio::spawn(actor.run(inbox, participant));
        Ok(Self { mail, gateway })
    }

    /// Reconciles with the tables again, after `test/seed` (rulings before
    /// T047).
    #[cfg(feature = "test-harness")]
    pub async fn reconcile(&self) {
        let (done, reconciled) = oneshot::channel();
        let _ = self.mail.send(Mail::Reconcile(done));
        let _ = reconciled.await;
    }

    /// `agents.updated` shows the runtime installer's state for Claude's
    /// adapter, whatever its runtime reports (design 5.2.12).
    pub fn runtime(&self, state: RuntimeState, failure: Option<RuntimeFailure>) {
        let _ = self.mail.send(Mail::Runtime(state, failure));
    }

    /// `host.updated` says an upgrade waits (design 5.2.13).
    pub fn pending_upgrade(&self) {
        let _ = self.mail.send(Mail::PendingUpgrade);
    }

    /// The gateway whose clients `clients.updated` lists; set once it exists.
    pub fn serve_for(&self, gateway: &Arc<Gateway>) {
        let _ = self.gateway.set(Arc::downgrade(gateway));
    }
}

impl Port for IndexHandle {
    fn deliver(&self, call: Call) -> Reply {
        let (reply, answer) = oneshot::channel();
        if let Err(mpsc::error::SendError(Mail::Call(call, _))) =
            self.mail.send(Mail::Call(Box::new(call), reply))
        {
            return futures::future::ready(refuse(call.ticket, stopped())).boxed();
        }
        answer
            .map(|outcome| outcome.unwrap_or_else(|_| Err(stopped())))
            .boxed()
    }
}

impl IndexPort for IndexHandle {
    fn opened(&self, client: Opened) {
        let _ = self.mail.send(Mail::Opened(Box::new(client)));
    }

    fn closed(&self, conn: &ConnId) {
        let _ = self.mail.send(Mail::Closed(conn.clone()));
    }
}

impl IndexNotes for IndexHandle {
    fn created(&self, summary: SessionSummary) {
        let _ = self
            .mail
            .send(Mail::Status(Input::Created(Box::new(summary))));
    }

    fn note(&self, session: &SessionId, at: u64, notes: Vec<Note>) {
        let input = Input::Notes(session.clone(), at, notes);
        let _ = self.mail.send(Mail::Status(input));
    }

    fn rescanned(&self, project: &ProjectId, scan: Scan) {
        let _ = self.mail.send(Mail::Rescanned(project.clone(), scan));
    }
}

impl AgentNotes for IndexHandle {
    fn agent(&self, status: AgentStatus) {
        let _ = self.mail.send(Mail::Agent(Box::new(status)));
    }
}

impl Part for IndexHandle {
    fn activity(&self) -> Activity {
        Activity::default()
    }

    fn stop(&self, _force: bool) -> BoxFuture<'static, ()> {
        let (done, checkpointed) = oneshot::channel();
        let _ = self.mail.send(Mail::Checkpoint(done));
        checkpointed.map(drop).boxed()
    }
}

struct IndexActor {
    handle: StreamHandle,
    writer: StreamWriter,
    tables: Tables,
    status: Status,
    elevations: Elevations,
    timers: TimerTable<Timer>,
    alarm: Alarm,
    /// For the tasks it starts to answer.
    mail: mpsc::UnboundedSender<Mail>,
    gateway: Arc<OnceLock<Weak<Gateway>>>,
    deps: IndexDeps,
    /// What the runtime installer last said of Claude's adapter runtime.
    runtime: Option<(RuntimeState, Option<RuntimeFailure>)>,
}

impl IndexActor {
    async fn run(mut self, mut inbox: mpsc::UnboundedReceiver<Mail>, participant: Participant) {
        loop {
            let next = participant.idle(async {
                tokio::select! {
                    mail = inbox.recv() => mail,
                    () = self.alarm.rung() => Some(Mail::Rung),
                }
            });
            let Some(mail) = next.await else {
                break;
            };
            match mail {
                Mail::Call(call, reply)
                    if [SessionMarkRead::NAME, ClientPresence::NAME].contains(&call.method) =>
                {
                    let _ = reply.send(self.attend(*call).await);
                }
                // Elevation's three methods.
                Mail::Call(call, reply) if call.method.starts_with("auth/") => {
                    self.elevation_call(*call, reply).await
                }
                Mail::Call(call, reply) => self.call(*call, reply),
                Mail::Prepared(done) => {
                    let Done {
                        call,
                        reply,
                        prepared,
                    } = *done;
                    let outcome = match prepared {
                        Ok(prepared) => self.decide(call, prepared).await,
                        Err(err) => {
                            let failed = err.data.as_ref().and_then(|d| d.reason.as_ref());
                            let project = parse::<ProjectId>(&call.params["project_id"]);
                            if let (Some(ErrorReason::ScanFailed), Ok(project)) = (failed, project)
                            {
                                let error = err.message.clone();
                                let change = TrustChange::ScanFailed { error };
                                self.audit_trust(Some(call.conn.clone()), project, change);
                            }
                            refuse(call.ticket, err)
                        }
                    };
                    let _ = reply.send(outcome);
                }
                Mail::Opened(opened) => {
                    self.opened(*opened);
                    self.clients().await
                }
                Mail::Closed(conn) => {
                    let _ = self.step(Input::Closed(conn.clone())).await;
                    self.closed(conn).await;
                    self.clients().await
                }
                Mail::Examined(asked) => {
                    let (ask, asking) = *asked;
                    self.examined(ask, asking).await
                }
                Mail::Rung => self.rung().await,
                // Only a read mark fails, and `attend` is the one to answer.
                Mail::Status(input) => {
                    let _ = self.step(input).await;
                }
                Mail::Rescanned(project, scan) => self.rescanned(project, scan).await,
                Mail::Agent(status) => self.agent(*status).await,
                Mail::Runtime(state, failure) => {
                    self.runtime = Some((state, failure));
                    let claude = |view: &IndexView| {
                        let agents = view.agents.as_ref()?.agents.iter();
                        agents
                            .clone()
                            .find(|a| a.agent == AgentKind::Claude)
                            .cloned()
                    };
                    if let Some(status) = read(&self.handle, claude) {
                        self.agent(status).await;
                    }
                }
                Mail::PendingUpgrade => {
                    self.deps.host.pending_upgrade = true;
                    self.append(EventBody::HostUpdated(self.deps.host.clone()))
                        .await;
                }
                #[cfg(feature = "test-harness")]
                Mail::Reconcile(done) => {
                    if let Ok(mut tables) = self.deps.readers.read(rows::read).await {
                        let (status, host) = (&mut self.status, &self.deps.host);
                        let bodies = reconciled(&self.handle, &mut tables, status, host);
                        let batch = Batch {
                            bodies,
                            ..Batch::default()
                        };
                        if self.writer.append(batch).await.is_ok() {
                            self.tables.workspaces = tables.workspaces;
                        }
                    }
                    let _ = done.send(());
                }
                Mail::Checkpoint(done) => {
                    // What gathered goes out first.
                    let _ = self.step(Input::Flush).await;
                    // A failed checkpoint costs the next start a longer load.
                    let _ = self.writer.checkpoint().await;
                    let _ = done.send(());
                }
            }
        }
    }

    /// Starts the slow part of a project method in a task of its own.
    fn call(&mut self, call: Call, reply: oneshot::Sender<Outcome>) {
        let path = match call.method {
            ProjectAdd::NAME => {
                parse::<ProjectAddParams>(&call.params).map(|p| PathBuf::from(p.path.0))
            }
            _ => self.project_of(&call).map(|project| project.path.clone()),
        };
        let path = match path {
            Ok(path) => path,
            Err(err) => {
                let _ = reply.send(refuse(call.ticket, err));
                return;
            }
        };
        let mut writable: Vec<PathBuf> = self
            .tables
            .projects
            .values()
            .map(|p| p.path.clone())
            .collect();
        writable.extend(self.tables.workspaces.iter().cloned());
        writable.push(path.clone());
        let cx = Context {
            git: self.deps.git.clone(),
            writable,
            clock: Arc::clone(&self.deps.clock),
            durations: self.deps.durations.clone(),
        };
        let (mail, hold) = (self.mail.clone(), self.deps.quiescence.hold());
        tokio::spawn(async move {
            let prepared = projects::prepare(path, cx).await;
            let done = Done {
                call,
                reply,
                prepared,
            };
            let _ = mail.send(Mail::Prepared(Box::new(done)));
            // Let go once the actor has its work.
            drop(hold);
        });
    }

    fn project_of(&self, call: &Call) -> Result<&Project, RpcError> {
        let id = parse::<ProjectId>(&call.params["project_id"])?;
        let project = self.tables.projects.get(&id);
        project.ok_or_else(|| RpcError::app(ErrorCode::NotFound, "no such project"))
    }

    async fn decide(&mut self, mut call: Call, prepared: Prepared) -> Outcome {
        let now = self.deps.clock.now_wall_ms();
        let Prepared { path, git, scan } = prepared;
        let (project, write, result, change) = match call.method {
            ProjectAdd::NAME => {
                let existing = self.tables.projects.values().find(|p| p.path == path);
                let (project, write, change) = match existing {
                    Some(project) => {
                        let trust = project.trust.rescanned(&scan);
                        let changed = trust != project.trust;
                        let change = stale(&project.trust, &trust, &scan);
                        let project = Project {
                            trust,
                            ..project.clone()
                        };
                        (
                            project,
                            changed.then_some(projects::update as Write),
                            change,
                        )
                    }
                    None => {
                        let position = self.tables.projects.values().map(|p| p.position + 1).max();
                        let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
                        let project = Project {
                            id: ProjectId::from_uuid(self.deps.ids.new_id().as_u128()),
                            name: name.unwrap_or_else(|| path.to_string_lossy().into_owned()),
                            path,
                            git_common_dir: git.map(|(_, common)| common),
                            added_at: now,
                            position: position.unwrap_or(0),
                            trust: Trust::added(&scan, now),
                        };
                        (project, Some(projects::insert as Write), None)
                    }
                };
                let result = projects::add_result(&project, &scan);
                (project, write, result, change)
            }
            ProjectTrust::NAME => {
                let params = match parse::<ProjectTrustParams>(&call.params) {
                    Ok(params) => params,
                    Err(err) => return refuse(call.ticket, err),
                };
                let by = call.principal.id.clone();
                let digest = scan.digest.clone();
                let change = TrustChange::Trusted {
                    by: by.clone(),
                    digest,
                };
                let Some(project) = self.tables.projects.get(&params.project_id) else {
                    return refuse(
                        call.ticket,
                        RpcError::app(ErrorCode::NotFound, "no such project"),
                    );
                };
                let trust = match Trust::confirmed(&scan, &params.trust_digest, by, now) {
                    Ok(trust) => trust,
                    Err(mismatch) => {
                        let err = RpcError::app(ErrorCode::Conflict, mismatch.to_string())
                            .with_reason(ErrorReason::DigestMismatch);
                        return refuse(call.ticket, err);
                    }
                };
                let result = projects::trust_result(&trust);
                let project = Project {
                    trust,
                    ..project.clone()
                };
                (
                    project,
                    Some(projects::update as Write),
                    result,
                    Some(change),
                )
            }
            _ => {
                let project = match self.project_of(&call) {
                    Ok(project) => project.clone(),
                    Err(err) => return Err(err),
                };
                let trust = project.trust.rescanned(&scan);
                let result = projects::info_result(&project, &trust, &scan);
                let changed = trust != project.trust;
                let change = stale(&project.trust, &trust, &scan);
                let project = Project { trust, ..project };
                let write = changed.then_some(projects::update as Write);
                (project, write, result, change)
            }
        };
        let id = project.id.clone();
        let outcome = self.save(&mut call, project, write, result).await;
        if let (Ok(_), Some(change)) = (&outcome, change) {
            self.audit_trust(Some(call.conn.clone()), id, change);
        }
        outcome
    }

    /// `project.trust.*` (design 5.8.6), from `conn`'s call or a session's
    /// rescan.
    fn audit_trust(&self, conn: Option<ConnId>, project_id: ProjectId, change: TrustChange) {
        let event = AuditEvent::ProjectTrust { project_id, change };
        let at_ms = self.deps.clock.now_wall_ms();
        // Nobody listening is the receiver's choice.
        let _ = self.deps.audit.send(AuditRecord { at_ms, conn, event });
    }

    /// Records `project`: its row through `write`, with `project.upserted`,
    /// and the command's outcome, in one transaction (design 5.2.3, rule 3).
    async fn save(
        &mut self,
        call: &mut Call,
        project: Project,
        write: Option<Write>,
        result: serde_json::Value,
    ) -> Outcome {
        let completion = call.ticket.take().map(|t| t.complete(Ok(result.clone())));
        let Some(write) = write else {
            if let Some(completion) = completion {
                completion.commit().await?;
            }
            return Ok(result);
        };
        let record = completion.as_ref().map(|completion| completion.row());
        let row = project.clone();
        let cause = call.params["command_id"]
            .as_str()
            .and_then(|id| id.parse().ok());
        let batch = Batch {
            bodies: vec![EventBody::ProjectUpserted(project.upserted())],
            cause: cause.map(|command_id| Cause {
                command_id,
                principal: call.principal.id.clone(),
            }),
            durability: Durability::Normal,
            with: Some(Box::new(move |conn| {
                write(&row, conn)?;
                record.map_or(Ok(()), |record| record(conn))
            })),
        };
        let appended = self.writer.append(batch).await;
        appended.map_err(|err| RpcError::app(ErrorCode::Internal, err.to_string()))?;
        self.tables.projects.insert(project.id.clone(), project);
        if let Some(completion) = completion {
            completion.committed();
        }
        Ok(result)
    }

    async fn clients(&mut self) {
        let Some(gateway) = self.gateway.get().and_then(Weak::upgrade) else {
            return;
        };
        let clients = ClientsUpdated {
            clients: gateway.clients(),
        };
        if read(&self.handle, |view| view.clients.as_ref() != Some(&clients)) {
            self.append(EventBody::ClientsUpdated(clients)).await;
        }
    }

    /// `session/mark_read` and `client/presence`.
    async fn attend(&mut self, call: Call) -> Outcome {
        let input = if call.method == SessionMarkRead::NAME {
            let params = parse::<SessionMarkReadParams>(&call.params)?;
            Input::MarkRead(params.session_id, params.up_to_seq)
        } else {
            let params = parse::<ClientPresenceParams>(&call.params)?;
            Input::Presence {
                conn: call.conn,
                kind: call.principal.kind,
                foreground: params.foreground,
                session: params.session_id,
            }
        };
        self.step(input).await?;
        Ok(json!({}))
    }

    /// Steps the status core and carries out what it decides: the summaries
    /// and intents go out in one batch, after the read mark is stored.
    async fn step(&mut self, input: Input) -> Result<(), RpcError> {
        let actions = self.status.step(input, self.deps.clock.now());
        let (mut bodies, mut stored) = (Vec::new(), Ok(()));
        for action in actions {
            match action {
                Action::Upsert(summary) => bodies.push(EventBody::SessionUpserted(*summary)),
                Action::Intent(intent) => {
                    let id = self.deps.ids.new_id().as_u128();
                    bodies.push(EventBody::NotificationIntent(NotificationIntent {
                        intent_id: IntentId::from_uuid(id),
                        session_id: intent.session,
                        reason: intent.reason,
                        request_id: intent.request,
                    }));
                }
                Action::Read(session, seq) => {
                    let session = session.to_string();
                    let stored_at = self.deps.db.submit(Durability::Normal, move |conn| {
                        conn.execute(
                            "UPDATE sessions SET last_read_seq = ?2
                             WHERE session_id = ?1 AND last_read_seq < ?2",
                            rusqlite::params![session, seq],
                        )
                    });
                    let written = stored_at.await.map(drop);
                    stored =
                        written.map_err(|err| RpcError::app(ErrorCode::Internal, err.to_string()));
                }
                Action::Timer(request) => self.timers.apply(timer(request, Timer::Status)),
            }
        }
        self.alarm.follow(&self.timers);
        if !bodies.is_empty() {
            let batch = Batch {
                bodies,
                ..Batch::default()
            };
            // A lost summary is written again by the next start's
            // reconciliation; T028 logs it.
            let _ = self.writer.append(batch).await;
        }
        stored
    }

    /// Hands each timer due now to its core.
    async fn rung(&mut self) {
        let now = self.deps.clock.now_mono();
        while let Some(key) = self.timers.pop_due(now) {
            match key {
                Timer::Status(key) => {
                    let _ = self.step(Status::timer_expired(key)).await;
                }
                Timer::Elevation(key) => self.expired(key).await,
            }
        }
        self.alarm.follow(&self.timers);
    }

    /// One agent's status into `agents.updated`.
    async fn agent(&mut self, mut status: AgentStatus) {
        if let (AgentKind::Claude, Some((state, failure))) = (&status.agent, &self.runtime) {
            status.adapter.runtime = state.clone();
            status.adapter.failure = failure.clone();
        }
        let agents = read(&self.handle, |view| view.agents.clone());
        let mut agents = agents.unwrap_or_else(|| AgentsUpdated {
            agents: Vec::new(),
            git: GitTools {
                version: None,
                lfs: None,
            },
        });
        match agents.agents.iter_mut().find(|a| a.agent == status.agent) {
            Some(known) if *known == status => return,
            Some(known) => *known = status,
            None => agents.agents.push(status),
        }
        self.append(EventBody::AgentsUpdated(agents)).await;
    }

    /// A session rescanned its project (design 5.2.6): a trusted project
    /// whose configuration changed becomes `stale`.
    async fn rescanned(&mut self, id: ProjectId, scan: Scan) {
        let Some(project) = self.tables.projects.get(&id) else {
            return;
        };
        let trust = project.trust.rescanned(&scan);
        if trust == project.trust {
            return;
        }
        let change = stale(&project.trust, &trust, &scan);
        let project = Project {
            trust,
            ..project.clone()
        };
        let row = project.clone();
        let batch = Batch {
            bodies: vec![EventBody::ProjectUpserted(project.upserted())],
            with: Some(Box::new(move |conn| projects::update(&row, conn))),
            ..Batch::default()
        };
        // Lost, it is found again by the next rescan.
        if self.writer.append(batch).await.is_ok() {
            self.tables.projects.insert(id.clone(), project);
            if let Some(change) = change {
                self.audit_trust(None, id, change);
            }
        }
    }

    async fn append(&mut self, body: EventBody) {
        let batch = Batch {
            bodies: vec![body],
            ..Batch::default()
        };
        // A lost update is written again by the next change; T028 logs it.
        let _ = self.writer.append(batch).await;
    }
}

/// A core's timer request, for the actor's table.
fn timer<K>(request: TimerRequest<K>, timer: fn(K) -> Timer) -> TimerRequest<Timer> {
    match request {
        TimerRequest::Set { key, at } => TimerRequest::Set {
            key: timer(key),
            at,
        },
        TimerRequest::Cancel(key) => TimerRequest::Cancel(timer(key)),
    }
}

/// `project.trust.stale` when a rescan took a trusted project to `stale`.
fn stale(before: &Trust, after: &Trust, scan: &Scan) -> Option<TrustChange> {
    let to_stale = before.state == TrustState::Trusted && after.state == TrustState::Stale;
    to_stale.then(|| TrustChange::Stale {
        digest: scan.digest.clone(),
    })
}

/// A row write of [`projects`].
type Write = fn(&Project, &rusqlite::Connection) -> rusqlite::Result<()>;

/// The events that bring the index stream to `tables`, the sessions' rows
/// going through `status`.
fn reconciled(
    handle: &StreamHandle,
    tables: &mut Tables,
    status: &mut Status,
    host: &HostUpdated,
) -> Vec<EventBody> {
    read(handle, |view| {
        let mut bodies = rows::reconcile(view, tables, host);
        let sessions = status.load(mem::take(&mut tables.sessions), &view.sessions);
        bodies.extend(sessions.into_iter().map(EventBody::SessionUpserted));
        bodies
    })
}

fn read<T>(handle: &StreamHandle, read: impl FnOnce(&IndexView) -> T) -> T {
    handle.read(|view, _| match view {
        StreamView::Index(view) => read(view),
        StreamView::Session(_) => unreachable!("the index stream has the index view"),
    })
}

fn parse<T: DeserializeOwned>(value: &serde_json::Value) -> Result<T, RpcError> {
    T::deserialize(value).map_err(|err| RpcError::new(INVALID_PARAMS, err.to_string()))
}

fn refuse(ticket: Option<Ticket>, err: RpcError) -> Outcome {
    if let Some(ticket) = ticket {
        ticket.reject(err.clone());
    }
    Err(err)
}

fn stopped() -> RpcError {
    RpcError::app(ErrorCode::Internal, "the index stopped")
}

#[cfg(test)]
mod tests;
