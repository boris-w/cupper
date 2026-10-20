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
//!   or closed.
//! - Session status, read marks, presence, notification intents and agents
//!   are T100's; [`IndexNotes`] so far takes the pending requests into the
//!   summary.

mod budget;
mod projects;
mod rows;

use std::path::PathBuf;
use std::sync::{Arc, OnceLock, Weak};

use cupper_protocol::envelope::Cause;
use cupper_protocol::error::{ErrorCode, ErrorReason, INVALID_PARAMS, RpcError};
use cupper_protocol::event::EventBody;
use cupper_protocol::ids::{ConnId, ProjectId, SessionId};
use cupper_protocol::index::{ClientsUpdated, HostUpdated, PendingKind};
use cupper_protocol::methods::project::{
    ProjectAdd, ProjectAddParams, ProjectTrust, ProjectTrustParams,
};
use cupper_protocol::rpc::Method;
use cupper_protocol::sync::StreamId;
use cupper_protocol::view::IndexView;
use futures::FutureExt;
use futures::future::BoxFuture;
use serde::de::DeserializeOwned;
use tokio::sync::{mpsc, oneshot};

use self::projects::{Context, Prepared};
use self::rows::{Project, Tables};
use crate::clock::{Clock, IdGen};
use crate::durations::Durations;
use crate::gateway::{Call, Gateway, IndexPort, Opened, Port, Reply};
use crate::git::GitTool;
use crate::ledger::{Outcome, Ticket};
use crate::orchestrator::session::IndexNotes;
use crate::quiescence::{ActorKind, Participant, Quiescence};
use crate::store::streams::{Batch, StreamError, StreamHandle, StreamView, StreamWriter, Streams};
use crate::store::{Durability, ReadError, Readers};
use crate::supervisor::{Activity, Part};
use crate::trust::Trust;

pub use self::budget::{BUDGET, DETAIL_MAX};

/// The index actor's kind in the quiescence barrier.
pub const INDEX: ActorKind = ActorKind::new("index");

pub struct IndexDeps {
    pub streams: Streams,
    pub readers: Readers,
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<IdGen>,
    pub durations: Durations,
    /// The git `project/add` asks where a work tree is; without one, every
    /// project is outside git.
    pub git: Option<GitTool>,
    pub quiescence: Quiescence,
    /// `host.updated` of this run.
    pub host: HostUpdated,
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
    Clients,
    Pending(SessionId, u32, Option<PendingKind>),
    Checkpoint(oneshot::Sender<()>),
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
        let tables = deps.readers.read(rows::read).await?;
        let bodies = read(&handle, |view| rows::reconcile(view, &tables, &deps.host));
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
            mail: mail.clone(),
            gateway: Arc::clone(&gateway),
            deps,
        };
        tokio::spawn(actor.run(inbox, participant));
        Ok(Self { mail, gateway })
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
    fn opened(&self, _: Opened) {
        let _ = self.mail.send(Mail::Clients);
    }

    fn closed(&self, _: &ConnId) {
        let _ = self.mail.send(Mail::Clients);
    }
}

impl IndexNotes for IndexHandle {
    fn pending(&self, session: &SessionId, count: u32, oldest: Option<PendingKind>) {
        let _ = self
            .mail
            .send(Mail::Pending(session.clone(), count, oldest));
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
    /// For the tasks it starts to answer.
    mail: mpsc::UnboundedSender<Mail>,
    gateway: Arc<OnceLock<Weak<Gateway>>>,
    deps: IndexDeps,
}

impl IndexActor {
    async fn run(mut self, mut inbox: mpsc::UnboundedReceiver<Mail>, participant: Participant) {
        let _participant = participant;
        while let Some(mail) = inbox.recv().await {
            match mail {
                Mail::Call(call, reply) => self.call(*call, reply),
                Mail::Prepared(done) => {
                    let Done {
                        call,
                        reply,
                        prepared,
                    } = *done;
                    let outcome = match prepared {
                        Ok(prepared) => self.decide(call, prepared).await,
                        Err(err) => refuse(call.ticket, err),
                    };
                    let _ = reply.send(outcome);
                }
                Mail::Clients => self.clients().await,
                Mail::Pending(session, count, oldest) => self.pending(session, count, oldest).await,
                Mail::Checkpoint(done) => {
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
        let mail = self.mail.clone();
        tokio::spawn(async move {
            let prepared = projects::prepare(path, cx).await;
            let done = Done {
                call,
                reply,
                prepared,
            };
            let _ = mail.send(Mail::Prepared(Box::new(done)));
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
        match call.method {
            ProjectAdd::NAME => {
                let existing = self.tables.projects.values().find(|p| p.path == path);
                let (project, write) = match existing {
                    Some(project) => {
                        let trust = project.trust.rescanned(&scan);
                        let changed = trust != project.trust;
                        let project = Project {
                            trust,
                            ..project.clone()
                        };
                        (project, changed.then_some(projects::update as Write))
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
                        (project, Some(projects::insert as Write))
                    }
                };
                let result = projects::add_result(&project, &scan);
                self.save(&mut call, project, write, result).await
            }
            ProjectTrust::NAME => {
                let params = match parse::<ProjectTrustParams>(&call.params) {
                    Ok(params) => params,
                    Err(err) => return refuse(call.ticket, err),
                };
                let by = call.principal.id.clone();
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
                self.save(&mut call, project, Some(projects::update), result)
                    .await
            }
            _ => {
                let project = match self.project_of(&call) {
                    Ok(project) => project.clone(),
                    Err(err) => return Err(err),
                };
                let trust = project.trust.rescanned(&scan);
                let result = projects::info_result(&trust, &scan);
                let changed = trust != project.trust;
                let project = Project { trust, ..project };
                self.save(
                    &mut call,
                    project,
                    changed.then_some(projects::update as Write),
                    result,
                )
                .await
            }
        }
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

    /// The session's pending requests go into its summary; its status is
    /// T100's.
    async fn pending(&mut self, session: SessionId, count: u32, oldest: Option<PendingKind>) {
        let summary = read(&self.handle, |view| view.sessions.get(&session).cloned());
        let Some(mut summary) = summary else {
            return;
        };
        if (summary.pending_interactions, &summary.pending_kind) != (count, &oldest) {
            summary.pending_interactions = count;
            summary.pending_kind = oldest;
            self.append(EventBody::SessionUpserted(summary)).await;
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

/// A row write of [`projects`].
type Write = fn(&Project, &rusqlite::Connection) -> rusqlite::Result<()>;

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
