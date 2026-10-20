//! `SessionManager` (design 5.2.6): the table of session actors, the
//! gateway's [`SessionPort`], and `session/create`. A call for a session
//! without an actor starts one that loads the session from the store first
//! ([`load`](super::load)); its mailbox is in the table from the start, so a
//! session has one actor however many calls come at once.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cupper_protocol::annotations::{TimestampMs, WirePath};
use cupper_protocol::capabilities::{ConfigOptionId, SessionCapabilities};
use cupper_protocol::domain::{AgentKind, Tier};
use cupper_protocol::envelope::Cause;
use cupper_protocol::error::{ErrorCode, RpcError};
use cupper_protocol::event::EventBody;
use cupper_protocol::ids::SessionId;
use cupper_protocol::index::{
    SessionStatus, SessionSummary, TitleSource, TrustState, WorkspaceKind,
};
use cupper_protocol::interaction::{InteractionOutcome, InteractionRequest};
use cupper_protocol::methods::handshake::Principal;
use cupper_protocol::methods::session::{
    SessionCreate, SessionCreateParams, SessionCreateResult, SessionSend, WorkspaceSpec,
};
use cupper_protocol::rpc::Method;
use cupper_protocol::session::{
    ProjectConfig, SessionConfigured, SessionWorkspace, WorkspaceReady,
};
use cupper_protocol::sync::StreamId;
use futures::FutureExt;
use futures::future::{self, BoxFuture};
use rusqlite::OptionalExtension;
use serde_json::json;
use tokio::sync::{mpsc, oneshot};

use super::actor::{Mail, Parts, SessionActor, Start, Stop};
use super::core::{Input, SessionCore};
use super::load::load;
use super::rows::Rows;
use super::{IndexNotes, internal, not_found, parse, privileged, tier_of};
use crate::clock::{Clock, IdGen};
use crate::driver::api::{AgentPort, ProjectConfigPolicy};
use crate::durations::Durations;
use crate::gateway::{AuditRecord, Call, Port, Reply, SessionPort};
use crate::git::GitTool;
use crate::ledger::Outcome;
use crate::orchestrator::queue::preview;
use crate::orchestrator::turn::{self, SendMode, SendRequest};
use crate::policy::{self, Confirmable, Facts};
use crate::quiescence::{ActorKind, Participant, Quiescence};
use crate::store::streams::{Batch, StreamView, Streams};
use crate::store::{Durability, Readers};
use crate::supervisor::watch::Admission;
use crate::supervisor::{Activity, Part};

/// The session actors in the quiescence barrier.
pub const SESSION: ActorKind = ActorKind::new("session");

/// What the sessions need of the host.
#[derive(Debug, Clone)]
pub struct SessionDeps {
    pub streams: Streams,
    pub readers: Readers,
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<IdGen>,
    pub durations: Durations,
    /// The runtime of each agent the host can run.
    pub agents: HashMap<AgentKind, Arc<dyn AgentPort>>,
    pub quiescence: Quiescence,
    /// The host's audit channel, the gateway's too.
    pub audit: mpsc::UnboundedSender<AuditRecord>,
    pub index: Arc<dyn IndexNotes>,
    /// Finds the git root of a project's trust range (`Discovery::git`).
    pub git: Option<GitTool>,
    /// Whether new sessions and turns are accepted (design 5.2.13).
    pub admission: Arc<Admission>,
}

/// The session manager. Clones share it; the host has one.
#[derive(Debug, Clone)]
pub struct SessionManager {
    shared: Arc<Shared>,
}

#[derive(Debug)]
struct Shared {
    deps: SessionDeps,
    /// Held only to look up, add and count.
    sessions: Mutex<HashMap<SessionId, Handle>>,
}

#[derive(Debug)]
struct Handle {
    mail: mpsc::UnboundedSender<Mail>,
    stop: mpsc::UnboundedSender<Stop>,
    running: Arc<AtomicBool>,
    pending: Arc<AtomicU32>,
    queued: Arc<AtomicU32>,
}

/// The actor's side of a [`Handle`]'s counters: a turn runs, the pending
/// requests, the queued messages.
pub(super) type Counters = (Arc<AtomicBool>, Arc<AtomicU32>, Arc<AtomicU32>);

/// An actor's mailboxes.
type Mailboxes = (mpsc::UnboundedReceiver<Mail>, mpsc::UnboundedReceiver<Stop>);

/// The other ends of a [`Handle`], for its actor.
type Ends = (
    mpsc::UnboundedReceiver<Mail>,
    mpsc::UnboundedReceiver<Stop>,
    Counters,
);

impl Handle {
    fn new() -> (Self, Ends) {
        let (mail, inbox) = mpsc::unbounded_channel();
        let (stop, stops) = mpsc::unbounded_channel();
        let counters = Counters::default();
        let (running, pending, queued) = counters.clone();
        let handle = Self {
            mail,
            stop,
            running,
            pending,
            queued,
        };
        (handle, (inbox, stops, counters))
    }
}

impl SessionManager {
    /// Registers the session actors in the quiescence barrier.
    pub fn new(deps: SessionDeps) -> Self {
        deps.quiescence.register(SESSION);
        let sessions = Mutex::default();
        Self {
            shared: Arc::new(Shared { deps, sessions }),
        }
    }

    fn sessions(&self) -> MutexGuard<'_, HashMap<SessionId, Handle>> {
        self.shared
            .sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// `session/create` (design 5.2.6, 5.5.2, local workspaces): the session
    /// row with its first message, `session.configured`, `workspace.ready`
    /// and the command's result in one transaction; then its actor starts
    /// the first turn.
    async fn create(self, call: Call) -> Outcome {
        let Call {
            principal,
            params,
            ticket,
            conn,
            ..
        } = call;
        let accepted = match self.accept(&principal, &params).await {
            Ok(accepted) => accepted,
            Err(err) => {
                if let Some(ticket) = ticket {
                    ticket.reject(err.clone());
                }
                return Err(err);
            }
        };
        let Accepted {
            params,
            agent,
            path,
            project_config,
            config,
            cached,
            confirmed,
        } = accepted;
        let deps = &self.shared.deps;
        // In full access, which only `privileged` may set (design 5.8.6).
        let executed = confirmed.then(|| Confirmable::Create(params.clone()));
        let id = SessionId::from_uuid(deps.ids.new_id().as_u128());
        let stream_id = StreamId::Session(id.clone());
        let stream = deps.streams.open(&stream_id).await.map_err(internal)?;
        let mut writer = stream.writer().map_err(internal)?;
        let workspace = SessionWorkspace {
            kind: WorkspaceKind::Local,
            path: WirePath(path.clone()),
            branch: None,
            base_ref: None,
        };
        let mut current: BTreeMap<String, String> = (cached.iter())
            .flat_map(|caps| caps.config.iter())
            .map(|o| (o.id.to_string(), o.current.clone()))
            .collect();
        current.extend(config.clone());
        let configured = SessionConfigured {
            agent: params.agent.clone(),
            workspace,
            config: current,
            read_only: false,
            project_config: match project_config {
                ProjectConfigPolicy::Load => ProjectConfig::Loaded,
                ProjectConfigPolicy::Ignore => ProjectConfig::Ignored,
            },
            confirmation_required: None,
        };
        let (at, title) = (deps.clock.now_wall_ms(), preview(&params.message));
        let mut row = Rows::created(
            [
                id.to_string(),
                params.project_id.to_string(),
                params.agent.to_string(),
                path.clone(),
                json!(configured.config).to_string(),
                json!(params.message).to_string(),
                title.clone(),
                principal.id.to_string(),
            ],
            at,
        );
        if confirmed {
            row.set_by(&id, at, &principal.id, true);
        }
        let result = json!(SessionCreateResult {
            session_id: id.clone()
        });
        let completion = ticket.map(|ticket| ticket.complete(Ok(result.clone())));
        let record = completion.as_ref().map(|c| c.row());
        let ready = WorkspaceReady {
            path: WirePath(path.clone()),
            branch: None,
        };
        let batch = Batch {
            bodies: vec![
                EventBody::SessionConfigured(configured.clone()),
                EventBody::WorkspaceReady(ready),
            ],
            cause: Some(Cause {
                command_id: params.command_id.clone(),
                principal: principal.id.clone(),
            }),
            durability: Durability::Normal,
            with: Some(Box::new(move |conn| {
                row.write(conn)?;
                record.map_or(Ok(()), |record| record(conn))
            })),
        };
        writer.append(batch).await.map_err(internal)?;
        let facts = Facts {
            mode_tier: Some(Tier::FullAccess),
            project_path: Some(WirePath(path.clone())),
            ..Facts::default()
        };
        let executed = executed.and_then(|c| privileged(c, &facts, &principal.id, &conn, at));
        if let Some(record) = executed {
            // Nobody listening is the receiver's choice.
            let _ = deps.audit.send(record);
        }
        let summary = SessionSummary {
            session_id: id.clone(),
            project_id: params.project_id.clone(),
            agent: params.agent.clone(),
            title_source: if title.is_empty() {
                TitleSource::None
            } else {
                TitleSource::Prompt
            },
            title,
            workspace_kind: WorkspaceKind::Local,
            branch: None,
            status: SessionStatus::Idle,
            unread: false,
            pending_interactions: 0,
            pending_kind: None,
            last_activity_at: TimestampMs(at),
            error_code: None,
            read_only: false,
        };
        deps.index.created(summary);
        let setter = (principal.clone(), confirmed);
        let mut core = SessionCore::new(
            id.clone(),
            Arc::clone(&deps.ids),
            &deps.durations,
            false,
            setter,
        );
        core.require_trust_checks();
        let (handle, (inbox, stops, (running, pending, queued))) = Handle::new();
        let parts = Parts {
            id: id.clone(),
            core,
            clock: Arc::clone(&deps.clock),
            stream,
            writer,
            readers: deps.readers.clone(),
            agent,
            cwd: PathBuf::from(path),
            config,
            project_config,
            configured,
            cached,
            running,
            pending,
            queued,
            audit: deps.audit.clone(),
            index: Arc::clone(&deps.index),
            project: params.project_id.clone(),
            git: deps.git.clone(),
            durations: deps.durations.clone(),
            quiescence: deps.quiescence.clone(),
        };
        let first = SendRequest {
            content: params.message,
            mode: SendMode::Queue,
            by: principal.id,
            reply_to: None,
        };
        let start = Start {
            first: Input::Machine(turn::Input::Send(first)),
            commands: HashMap::new(),
            project_gate: false,
            ignore_for_good: false,
            initial: true,
        };
        let participant = deps.quiescence.enter(SESSION);
        let (actor, mail) = ((parts, start), handle.mail.clone());
        let run = self
            .clone()
            .run(id.clone(), mail, actor, (inbox, stops), participant);
        tokio::spawn(run);
        self.sessions().insert(id, handle);
        if let Some(completion) = completion {
            completion.committed();
        }
        Ok(result)
    }

    /// The checks before a session is created: the workspace, the agent,
    /// the project and its trust, and the tier of the mode asked for.
    async fn accept(
        &self,
        principal: &Principal,
        params: &serde_json::Value,
    ) -> Result<Accepted, RpcError> {
        let params: SessionCreateParams = parse(params)?;
        if !matches!(params.workspace, WorkspaceSpec::Local) {
            let message = "only local workspaces before M1b";
            return Err(RpcError::app(ErrorCode::Unsupported, message));
        }
        let deps = &self.shared.deps;
        let agent = deps.agents.get(&params.agent).cloned();
        let agent = agent.ok_or_else(|| RpcError::app(ErrorCode::Unavailable, "no such agent"))?;
        let project = params.project_id.to_string();
        let project = deps.readers.read(move |conn| {
            conn.query_row(
                "SELECT path, trust_state FROM projects
                 WHERE project_id = ?1 AND removed_at IS NULL",
                [project],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
        });
        let project = project.await.map_err(internal)?;
        let (path, trust) = project.ok_or_else(|| not_found("project"))?;
        let project_config = match policy::attach(&params.agent, &TrustState::from(trust.as_str()))?
        {
            policy::ProjectConfig::Load => ProjectConfigPolicy::Load,
            policy::ProjectConfig::UserOnly => ProjectConfigPolicy::Ignore,
        };
        let cached = cached(&deps.readers, &params.agent).await?;
        let initial = params.config.clone().unwrap_or_default();
        let mut confirmed = false;
        if let Some(mode) = &initial.permission_mode {
            // A mode the mapping does not know is full access (design 5.6.5).
            let tier = tier_of(cached.as_ref(), mode).unwrap_or(Tier::FullAccess);
            policy::tier_change(principal, None, &tier)?;
            // Full access takes the native confirmation (design 5.8.2).
            confirmed = policy::tier_rank(&tier) == policy::tier_rank(&Tier::FullAccess);
        }
        let config = [
            (ConfigOptionId::PermissionMode, initial.permission_mode),
            (ConfigOptionId::Model, initial.model),
            (ConfigOptionId::ReasoningEffort, initial.reasoning_effort),
        ];
        let config = (config.into_iter())
            .filter_map(|(id, value)| Some((id.to_string(), value?)))
            .collect();
        Ok(Accepted {
            params,
            agent,
            path,
            project_config,
            config,
            cached,
            confirmed,
        })
    }

    /// "不加载项目配置继续" (design 5.2.6): session `id` leaves its project's
    /// configuration out from now on and resumes its agent session without
    /// it. The host's own entry; the method on the wire is M1b's.
    pub fn ignore_project_config(
        &self,
        id: &SessionId,
    ) -> BoxFuture<'static, Result<(), RpcError>> {
        let (done, finished) = oneshot::channel();
        if self
            .mailbox(id)
            .send(Mail::IgnoreProjectConfig(done))
            .is_err()
        {
            return Box::pin(future::ready(Err(internal("the session stopped"))));
        }
        let stopped = || Err(internal("the session stopped"));
        Box::pin(finished.map(move |done| done.unwrap_or_else(|_| stopped())))
    }

    /// Loads the sessions whose first message never started a turn: the
    /// host stopped while they prepared (design 5.2.13), and their actors
    /// take the message up again.
    pub async fn reopen(&self) {
        let read = self.shared.deps.readers.read(|conn| {
            let mut select = conn.prepare(
                "SELECT session_id FROM sessions
                 WHERE initial_prompt_json IS NOT NULL AND archived_at IS NULL",
            )?;
            let ids = select.query_map([], |r| r.get::<_, String>(0))?;
            ids.collect::<rusqlite::Result<Vec<_>>>()
        });
        let ids = read.await.unwrap_or_else(|err| {
            tracing::error!("the sessions to reopen could not be read: {err}");
            Vec::new()
        });
        for id in ids.iter().filter_map(|id| id.parse().ok()) {
            self.mailbox(&id);
        }
    }

    /// The mailbox of session `id`'s actor. Without one, an actor starts
    /// that loads the session first; until then its mailbox holds the calls.
    fn mailbox(&self, id: &SessionId) -> mpsc::UnboundedSender<Mail> {
        let mut sessions = self.sessions();
        if let Some(handle) = sessions.get(id) {
            return handle.mail.clone();
        }
        let (handle, ends) = Handle::new();
        let mail = handle.mail.clone();
        sessions.insert(id.clone(), handle);
        drop(sessions);
        let participant = self.shared.deps.quiescence.enter(SESSION);
        let loading = self
            .clone()
            .loading(id.clone(), mail.clone(), ends, participant);
        tokio::spawn(loading);
        mail
    }

    /// Runs session `id`'s actor. Once it left memory idle, its mailbox
    /// leaves the table and the calls in it go to the session's next actor,
    /// which loads it again: the stream's writer is free by then.
    async fn run(
        self,
        id: SessionId,
        mail: mpsc::UnboundedSender<Mail>,
        (parts, start): (Parts, Start),
        (inbox, stops): Mailboxes,
        participant: Participant,
    ) {
        let left = SessionActor::run(parts, inbox, stops, start, participant).await;
        let Some(mut inbox) = left else {
            return;
        };
        self.forget(&id, &mail);
        inbox.close();
        while let Ok(left) = inbox.try_recv() {
            // A mailbox that closed at once answers nobody; its caller sees
            // the session stop.
            let _ = self.mailbox(&id).send(left);
        }
    }

    /// Session `id` has an actor.
    #[cfg(test)]
    pub(super) fn in_memory(&self, id: &SessionId) -> bool {
        self.sessions().contains_key(id)
    }

    /// Once every actor waits and nothing is held (design 5.2.16).
    #[cfg(test)]
    pub(super) async fn settled(&self) {
        self.shared.deps.quiescence.settled().await;
    }

    /// Takes session `id`'s actor out of the table, if `mail` is its mailbox.
    fn forget(&self, id: &SessionId, mail: &mpsc::UnboundedSender<Mail>) {
        let mut sessions = self.sessions();
        if sessions.get(id).is_some_and(|h| h.mail.same_channel(mail)) {
            sessions.remove(id);
        }
    }

    /// Loads session `id`, then runs its actor. When the load fails, the
    /// mailbox leaves the table and every call that came meanwhile gets the
    /// error; the next call tries again.
    async fn loading(
        self,
        id: SessionId,
        mail: mpsc::UnboundedSender<Mail>,
        (mut inbox, stops, counters): Ends,
        participant: Participant,
    ) {
        let err = match load(&self.shared.deps, &id, counters).await {
            Ok(actor) => return self.run(id, mail, actor, (inbox, stops), participant).await,
            Err(err) => err,
        };
        self.forget(&id, &mail);
        inbox.close();
        while let Some(mail) = inbox.recv().await {
            match mail {
                Mail::Call(call, reply) => {
                    if let Some(ticket) = call.ticket {
                        ticket.reject(err.clone());
                    }
                    let _ = reply.send(Err(err.clone()));
                }
                Mail::IgnoreProjectConfig(done) => {
                    let _ = done.send(Err(err.clone()));
                }
            }
        }
    }

    fn running_turns(&self) -> u32 {
        let running = self
            .sessions()
            .values()
            .filter(|h| h.running.load(Ordering::Relaxed))
            .count();
        u32::try_from(running).unwrap_or(u32::MAX)
    }
}

struct Accepted {
    params: SessionCreateParams,
    agent: Arc<dyn AgentPort>,
    path: String,
    project_config: ProjectConfigPolicy,
    config: BTreeMap<String, String>,
    cached: Option<SessionCapabilities>,
    /// Created in full access, which the native confirmation allowed.
    confirmed: bool,
}

/// The agent's capabilities in the host's cache (design 5.6.7), if any.
pub(super) async fn cached(
    readers: &Readers,
    agent: &AgentKind,
) -> Result<Option<SessionCapabilities>, RpcError> {
    let agent = agent.to_string();
    let json = readers.read(move |conn| {
        conn.query_row(
            "SELECT json FROM agent_capabilities WHERE agent = ?1",
            [agent],
            |row| row.get::<_, String>(0),
        )
        .optional()
    });
    let json = json.await.map_err(internal)?;
    Ok(json.and_then(|json| serde_json::from_str(&json).ok()))
}

/// Refuses `call` with `err`.
fn refuse(call: Call, err: RpcError) -> Reply {
    if let Some(ticket) = call.ticket {
        ticket.reject(err.clone());
    }
    Box::pin(future::ready(Err(err)))
}

impl Part for SessionManager {
    fn activity(&self) -> Activity {
        let sum = |count: fn(&Handle) -> &AtomicU32| {
            let sessions = self.sessions();
            sessions
                .values()
                .map(|h| count(h).load(Ordering::Relaxed))
                .sum()
        };
        Activity {
            running_turns: self.running_turns(),
            pending_interactions: sum(|h| &h.pending),
            queued_messages: sum(|h| &h.queued),
        }
    }

    /// Every session takes the exit; when `force`d, their running turns
    /// complete `interrupted` (design 5.2.13).
    fn stop(&self, force: bool) -> BoxFuture<'static, ()> {
        let sessions = self.sessions();
        let stopped = sessions.values().map(|handle| {
            let (done, stopped) = oneshot::channel();
            let _ = handle.stop.send((force, done));
            stopped
        });
        Box::pin(future::join_all(stopped.collect::<Vec<_>>()).map(drop))
    }
}

impl Port for SessionManager {
    /// Queues a session's call in its actor's mailbox; `session/create` runs
    /// in the reply's task. A mailbox that closed as its load failed is
    /// tried once more.
    fn deliver(&self, call: Call) -> Reply {
        let new_turn = [SessionCreate::NAME, SessionSend::NAME].contains(&call.method);
        if let Some(err) = new_turn
            .then(|| self.shared.deps.admission.refusal())
            .flatten()
        {
            return refuse(call, err);
        }
        if call.method == SessionCreate::NAME {
            return Box::pin(self.clone().create(call));
        }
        let Ok(id) = parse::<SessionId>(&call.params["session_id"]) else {
            return refuse(call, not_found("session"));
        };
        let mut call = Box::new(call);
        for _ in 0..2 {
            let (reply, answer) = oneshot::channel();
            match self.mailbox(&id).send(Mail::Call(call, reply)) {
                Ok(()) => {
                    let stopped = || Err(internal("the session stopped before it replied"));
                    return Box::pin(
                        answer.map(move |outcome| outcome.unwrap_or_else(|_| stopped())),
                    );
                }
                Err(mpsc::error::SendError(Mail::Call(back, _))) => call = back,
                Err(_) => unreachable!("a call went in"),
            }
        }
        refuse(*call, internal("the session stopped"))
    }
}

impl SessionPort for SessionManager {
    /// Loads the session unless it is in memory; a load that fails leaves
    /// the subscription as it is.
    fn wake(&self, id: &SessionId) {
        self.mailbox(id);
    }

    fn facts(&self, command: &Confirmable) -> BoxFuture<'static, Result<Facts, RpcError>> {
        let mut facts = Facts {
            running_turns: self.running_turns(),
            ..Facts::default()
        };
        let deps = self.shared.deps.clone();
        match command.clone() {
            Confirmable::Create(params) => Box::pin(async move {
                let cached = cached(&deps.readers, &params.agent).await?;
                let mode = params.config.and_then(|c| c.permission_mode);
                facts.mode_tier = mode.and_then(|mode| tier_of(cached.as_ref(), &mode));
                // The project as the native confirmation names it (T135).
                let index = deps
                    .streams
                    .open(&StreamId::Index)
                    .await
                    .map_err(internal)?;
                facts.project_path = index.read(|view, _| match view {
                    StreamView::Index(view) => {
                        Some(view.projects.get(&params.project_id)?.path.clone())
                    }
                    StreamView::Session(_) => None,
                });
                Ok(facts)
            }),
            // The chosen option, as the session's pending request has it.
            Confirmable::Respond(params) if self.sessions().contains_key(&params.session_id) => {
                Box::pin(async move {
                    let stream = StreamId::Session(params.session_id.clone());
                    let handle = deps.streams.open(&stream).await.map_err(internal)?;
                    let InteractionOutcome::Selected(selected) = &params.outcome else {
                        return Ok(facts);
                    };
                    facts.option = handle.read(|view, _| {
                        let StreamView::Session(view) = view else {
                            return None;
                        };
                        let request = &view.requests.get(&params.request_id)?.requested;
                        let options = match &request.request {
                            InteractionRequest::Permission(p) => &p.options,
                            InteractionRequest::PlanConfirm(p) => &p.options,
                            _ => return None,
                        };
                        let mut options = options.iter();
                        options.find(|o| o.option_id == selected.option_id).cloned()
                    });
                    Ok(facts)
                })
            }
            // The mode asked for, by the session's own mapping (M1b).
            Confirmable::SetConfig(params)
                if params.option == ConfigOptionId::PermissionMode
                    && self.sessions().contains_key(&params.session_id) =>
            {
                Box::pin(async move {
                    let stream = StreamId::Session(params.session_id.clone());
                    let handle = deps.streams.open(&stream).await.map_err(internal)?;
                    let caps = handle.read(|view, _| match view {
                        StreamView::Session(view) => view.capabilities.clone(),
                        StreamView::Index(_) => None,
                    });
                    facts.mode_tier = tier_of(caps.as_ref(), &params.value);
                    Ok(facts)
                })
            }
            _ => Box::pin(future::ready(Ok(facts))),
        }
    }
}
