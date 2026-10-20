//! `AgentRuntime` (design 5.6.1, 5.6.2, ADR-09): one actor per agent. It
//! starts the agent's adapter when a session first attaches, turns the
//! sessions' commands into ACP requests, and passes what the agent sends,
//! through the extraction layer, to the session it is for.
//!
//! - **One mailbox for the wire** (K12): the replies to requests and what
//!   the agent sends of its own reach the actor in one channel per instance,
//!   in the order they were on the wire.
//! - **The runtime numbers** prompts by `PromptTag` (the sessions' turn
//!   machines) and requests by `AskId` (here), and keeps each request's ACP
//!   responder until the session answers. A request the agent takes back
//!   (`$/cancel_request`, K5) goes to the session as `AskWithdrawn`; the
//!   broker's `Cancelled` then lets the responder go.
//! - **An instance ends** once its process has exited and its connection
//!   has closed, so nothing it wrote is lost: every session hears
//!   `AdapterExited`, and the next attach starts another. On the exit the
//!   rest of its process group is killed, which lets the pipes close.
//!   Backing off between restarts, descendants in groups of their own, the
//!   order of closing (5.6.8) and resource sampling are T008's.
//! - **Command lists** (design 5.6.2): Claude pushes its whole list when a
//!   session opens and at every turn's start. An attach answers with the
//!   agent's latest list without waiting for the session's own; after that a
//!   session hears `CommandsChanged` only when its list differs from the one
//!   it has. Attaches and changed lists refresh the capability cache.
//! - **Probing** (design 5.6.7): a session in an empty directory of its own,
//!   its options and its command list (waited for a while), then closed.
//! - The login state the agent reports, and a prompt failing on it, go into
//!   the agent's status for the index (`status`).
//! - Steering (T140) and `SetConfig` (T044) are not served yet; their
//!   commands are dropped.

pub mod agents;
pub mod cache;
pub mod content;
pub mod launch;
pub mod status;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::fs;
use std::future;
use std::mem;
use std::path::PathBuf;
use std::pin::pin;
use std::process::ExitStatus;
use std::sync::Arc;

use agent_client_protocol::schema::v1::{
    CancelNotification, CloseSessionRequest, CreateElicitationResponse, ElicitationMode,
    ElicitationScope, Meta, NewSessionRequest, PromptRequest, PromptResponse,
    RequestPermissionResponse, ResumeSessionRequest, SessionConfigOption, SessionConfigOptionValue,
    SetSessionConfigOptionRequest,
};
use agent_client_protocol::{Error, RequestCancellation, Responder};
use futures::FutureExt;
use futures::future::{BoxFuture, Either, select};
use serde_json::json;
use tokio::sync::{mpsc, oneshot};

use cupper_protocol::capabilities::{SessionCapabilities, SlashCommand};
use cupper_protocol::domain::AgentKind;
use sha2::{Digest, Sha256};

use cupper_protocol::index::{AgentStatus, AuthState, AuthStatus};

use self::launch::{Context, Instance, Launch};
use self::status::{AgentNotes, Running};
use super::acp::FromAgent;
use super::api::{
    AgentError, AgentErrorClass, AgentErrorForm, AgentPort, AgentSessionId, AskAnswer, AskId,
    AskRequest, Attach, AttachInfo, AttachTarget, DriverCommand, DriverEvent, ProjectConfigPolicy,
};
use super::claude::{self, ClaudeSession, capabilities};
use crate::clock::IdGen;
use crate::durations::{Durations, PROBE_COMMANDS};
use crate::quiescence::{ActorKind, Participant, Quiescence};
use crate::store::BlobStore;
use crate::supervisor::{Activity, Part};
use cupper_protocol::ids::AdapterInstanceId;
use cupper_sys::Signal;

/// The runtimes in the quiescence barrier.
pub const AGENT_RUNTIME: ActorKind = ActorKind::new("agent_runtime");

/// What a runtime needs of the host.
#[derive(Clone)]
pub struct RuntimeDeps {
    pub launch: Launch,
    pub cx: Context,
    pub ids: Arc<IdGen>,
    /// Images and long text of prompts.
    pub blobs: BlobStore,
    pub quiescence: Quiescence,
    /// `<data dir>/probe/<agent>/`, made empty for every probe (design 5.2.6).
    pub probe_dir: PathBuf,
    pub durations: Durations,
    pub notes: Arc<dyn AgentNotes>,
}

/// The runtime of one agent. Clones share it.
#[derive(Debug, Clone)]
pub struct AgentRuntime {
    mail: mpsc::UnboundedSender<Mail>,
}

#[derive(Debug)]
enum Mail {
    Attach(Box<Pending>),
    Command(DriverCommand),
    Withdrawn(AskId),
    Stop(oneshot::Sender<()>, bool),
    Probe(oneshot::Sender<Result<SessionCapabilities, AgentError>>),
    Status(oneshot::Sender<AgentStatus>),
    /// `agents/refresh` found the CLI again (design 5.6.6).
    Relaunch(Box<Launch>),
}

/// An attach on its way.
#[derive(Debug)]
struct Pending {
    attach: Attach,
    events: mpsc::UnboundedSender<DriverEvent>,
    reply: oneshot::Sender<Result<AttachInfo, AgentError>>,
}

/// What reaches the actor about one instance, in the order of its wire;
/// `Started` comes before `Exited`.
#[derive(Debug)]
enum Wire {
    Started(Box<Result<Instance, AgentError>>),
    Exited(ExitStatus),
    Agent(Box<FromAgent>),
    /// `session/new` or `session/resume`: the session and its options.
    Opened(
        Box<Pending>,
        Result<(String, Vec<SessionConfigOption>), Error>,
    ),
    /// `session/set_config_option` while attaching.
    Configured(String, Result<Vec<SessionConfigOption>, Error>),
    Prompted(String, super::api::PromptTag, Result<PromptResponse, Error>),
}

impl From<FromAgent> for Wire {
    fn from(message: FromAgent) -> Self {
        Self::Agent(Box::new(message))
    }
}

/// A session the agent knows: where its events go, and its extraction.
#[derive(Debug)]
struct Route {
    events: mpsc::UnboundedSender<DriverEvent>,
    claude: ClaudeSession,
    cwd: PathBuf,
    /// The digest of the command list the session has, once it attached.
    commands: Option<[u8; 32]>,
}

/// An attach applying its configuration, option by option.
#[derive(Debug)]
struct Opening {
    reply: oneshot::Sender<Result<AttachInfo, AgentError>>,
    todo: Vec<(String, String)>,
    options: Vec<SessionConfigOption>,
}

/// A request the session has not answered.
#[derive(Debug)]
enum Asked {
    Permission(Responder<RequestPermissionResponse>),
    Form(Responder<CreateElicitationResponse>),
}

struct Actor {
    deps: RuntimeDeps,
    /// Weak: the actor stops once every handle is dropped.
    mail: mpsc::WeakUnboundedSender<Mail>,
    instance: Option<Instance>,
    /// The wire of the instance that runs or is starting.
    wire: Option<(mpsc::UnboundedSender<Wire>, mpsc::UnboundedReceiver<Wire>)>,
    /// Of the instance's end: its process's status, its connection closed.
    exit: Option<ExitStatus>,
    closed: bool,
    /// Attaches that wait for the instance to start.
    waiting: Vec<Pending>,
    sessions: HashMap<String, Route>,
    opening: HashMap<String, Opening>,
    /// With the session asked and, to drop, the end of the withdrawal watch.
    asks: HashMap<AskId, (String, Asked, oneshot::Sender<()>)>,
    last_ask: u64,
    /// A stop waiting for the instance's end, and whether it is forced.
    stopping: Option<(oneshot::Sender<()>, bool)>,
    /// The agent's latest command list, from any session.
    commands: Vec<SlashCommand>,
    /// What the capability cache holds.
    cached: Option<SessionCapabilities>,
    auth: AuthStatus,
    _participant: Participant,
}

fn digest(commands: &[SlashCommand]) -> [u8; 32] {
    let json = serde_json::to_vec(commands).expect("commands serialize");
    Sha256::digest(json).into()
}

fn error(class: AgentErrorClass, message: impl ToString) -> AgentError {
    let (form, message) = (AgentErrorForm::Text, message.to_string());
    AgentError {
        class,
        form,
        message,
    }
}

impl AgentRuntime {
    /// Starts the runtime's actor; the adapter starts on the first attach.
    pub fn start(deps: RuntimeDeps) -> Self {
        let (mail, inbox) = mpsc::unbounded_channel();
        let participant = deps.quiescence.enter(AGENT_RUNTIME);
        let versions = &deps.launch.versions;
        drop(cache::invalidate(
            &deps.cx.writer,
            &AgentKind::Claude,
            versions,
        ));
        let actor = Actor {
            deps,
            mail: mail.downgrade(),
            instance: None,
            wire: None,
            exit: None,
            closed: false,
            waiting: Vec::new(),
            sessions: HashMap::new(),
            opening: HashMap::new(),
            asks: HashMap::new(),
            last_ask: 0,
            stopping: None,
            commands: Vec::new(),
            cached: None,
            auth: AuthStatus {
                state: AuthState::Unchecked,
                kind: None,
                plan: None,
            },
            _participant: participant,
        };
        tokio::spawn(actor.run(inbox));
        Self { mail }
    }
}

impl AgentRuntime {
    /// The agent's capabilities from a session of its own (design 5.6.7).
    pub fn probe(&self) -> BoxFuture<'static, Result<SessionCapabilities, AgentError>> {
        let (reply, answer) = oneshot::channel();
        let _ = self.mail.send(Mail::Probe(reply));
        let gone = || {
            Err(error(
                AgentErrorClass::ProcessExited,
                "the runtime went away",
            ))
        };
        Box::pin(answer.map(move |reply| reply.unwrap_or_else(|_| gone())))
    }

    /// The agent's status now; `None` once the runtime is gone.
    pub fn status(&self) -> BoxFuture<'static, Option<AgentStatus>> {
        let (reply, answer) = oneshot::channel();
        let _ = self.mail.send(Mail::Status(reply));
        Box::pin(answer.map(Result::ok))
    }

    /// Starts the next instance with `launch`.
    pub fn relaunch(&self, launch: Launch) {
        let _ = self.mail.send(Mail::Relaunch(Box::new(launch)));
    }
}

impl AgentPort for AgentRuntime {
    fn attach(
        &self,
        attach: Attach,
        events: mpsc::UnboundedSender<DriverEvent>,
    ) -> BoxFuture<'static, Result<AttachInfo, AgentError>> {
        let (reply, answer) = oneshot::channel();
        let pending = Pending {
            attach,
            events,
            reply,
        };
        let _ = self.mail.send(Mail::Attach(Box::new(pending)));
        let gone = || {
            Err(error(
                AgentErrorClass::ProcessExited,
                "the adapter went away",
            ))
        };
        Box::pin(answer.map(move |reply| reply.unwrap_or_else(|_| gone())))
    }

    fn send(&self, command: DriverCommand) {
        let _ = self.mail.send(Mail::Command(command));
    }
}

impl Part for AgentRuntime {
    /// Turns and requests are the sessions' to count.
    fn activity(&self) -> Activity {
        Activity::default()
    }

    /// Terminates the adapter's process group (kills it when `force`d), then
    /// waits for its end (the full order of 5.6.8 is T008's).
    fn stop(&self, force: bool) -> BoxFuture<'static, ()> {
        let (done, stopped) = oneshot::channel();
        let _ = self.mail.send(Mail::Stop(done, force));
        Box::pin(stopped.map(drop))
    }
}

impl Actor {
    async fn run(mut self, mut inbox: mpsc::UnboundedReceiver<Mail>) {
        loop {
            let wire = async {
                match &mut self.wire {
                    Some((_, rx)) => rx.recv().await,
                    None => future::pending().await,
                }
            };
            // A branch's handler runs once the other branch is dropped.
            tokio::select! {
                mail = inbox.recv() => match mail {
                    Some(mail) => self.mail(mail).await,
                    None => break,
                },
                Some(wire) = wire => self.wire(wire),
            }
        }
    }

    async fn mail(&mut self, mail: Mail) {
        match mail {
            Mail::Attach(pending) => self.attach(*pending),
            Mail::Command(command) => self.command(command).await,
            Mail::Withdrawn(ask) => {
                // Still asked: tell its session, which answers `Cancelled`.
                if let Some((session, ..)) = self.asks.get(&ask) {
                    self.to_session(session, DriverEvent::AskWithdrawn(ask));
                }
            }
            Mail::Stop(done, force) => {
                if self.wire.is_none() {
                    let _ = done.send(());
                    return;
                }
                self.stopping = Some((done, force));
                self.terminate();
            }
            Mail::Probe(reply) => self.probe(reply),
            Mail::Status(reply) => {
                let _ = reply.send(self.status());
            }
            Mail::Relaunch(launch) => {
                self.deps.launch = *launch;
                self.notify();
            }
        }
    }

    fn status(&self) -> AgentStatus {
        let running = match (&self.instance, &self.wire) {
            (Some(_), _) => Running::Yes,
            (None, Some(_)) => Running::Starting,
            (None, None) => Running::No,
        };
        status::status(&self.deps.launch, running, &self.auth)
    }

    fn notify(&self) {
        self.deps.notes.agent(self.status());
    }

    /// Opens a session in a fresh probe directory, which counts as trusted;
    /// answers with its capabilities once its command list came, or after a
    /// while without one, then closes it.
    fn probe(&mut self, reply: oneshot::Sender<Result<SessionCapabilities, AgentError>>) {
        let dir = self.deps.probe_dir.clone();
        if let Err(err) = fresh_dir(&dir) {
            let _ = reply.send(Err(error(AgentErrorClass::Other, err)));
            return;
        }
        let (events, mut heard) = mpsc::unbounded_channel();
        let (attached, info) = oneshot::channel();
        let attach = Attach {
            target: AttachTarget::New,
            cwd: dir,
            config: Default::default(),
            project_config: ProjectConfigPolicy::Load,
        };
        let (mail, clock) = (self.mail.clone(), Arc::clone(&self.deps.cx.clock));
        let writer = self.deps.cx.writer.clone();
        let wait = self.deps.durations.get(PROBE_COMMANDS);
        self.attach(Pending {
            attach,
            events,
            reply: attached,
        });
        tokio::spawn(async move {
            let info = match info.await {
                Ok(Ok(info)) => info,
                Ok(Err(err)) => return drop(reply.send(Err(err))),
                Err(_) => return drop(reply.send(Err(error(AgentErrorClass::Other, "gone")))),
            };
            let mut caps = info.capabilities;
            let commands = async {
                while let Some(event) = heard.recv().await {
                    if let DriverEvent::CommandsChanged(commands) = event {
                        return Some(commands);
                    }
                }
                None
            };
            if caps.commands.is_empty() {
                let timer = clock.timer_at(clock.now_mono() + wait);
                if let Either::Left((Some(commands), _)) = select(pin!(commands), timer).await {
                    caps.commands = commands;
                }
            }
            let close = DriverCommand::Close {
                session: info.session,
            };
            mail.upgrade().map(|mail| mail.send(Mail::Command(close)));
            // In the cache before the answer, for `agents/capabilities`.
            let _ = cache::write(&writer, &caps.agent, clock.now_wall_ms(), &caps).await;
            let _ = reply.send(Ok(caps));
        });
    }

    /// Starts an instance unless one runs or is starting.
    fn ensure_started(&mut self) {
        if self.instance.is_some() || self.wire.is_some() {
            return;
        }
        let (wire, rx) = mpsc::unbounded_channel();
        (self.wire, self.exit, self.closed) = (Some((wire.clone(), rx)), None, false);
        let id = AdapterInstanceId::from_uuid(self.deps.ids.new_id().as_u128());
        let (launch, cx) = (self.deps.launch.clone(), self.deps.cx.clone());
        tokio::spawn(async move {
            let (exited, exit) = oneshot::channel();
            let exited = move |status| {
                let _ = exited.send(status);
            };
            let started = launch::start(&launch, &cx, id.clone(), wire.clone(), exited).await;
            let _ = wire.send(Wire::Started(Box::new(started)));
            if let Ok(status) = exit.await {
                // Recorded before the sessions hear of it.
                let _ = launch::ended(&cx.writer, cx.clock.now_wall_ms(), &id, status).await;
                let _ = wire.send(Wire::Exited(status));
            }
        });
    }

    fn started(&mut self, started: Result<Instance, AgentError>) {
        match started {
            Ok(instance) => {
                self.instance = Some(instance);
                self.notify();
                if self.stopping.is_some() {
                    return self.terminate();
                }
                for pending in mem::take(&mut self.waiting) {
                    self.attach(pending);
                }
            }
            Err(err) => {
                self.wire = None;
                for pending in mem::take(&mut self.waiting) {
                    let _ = pending.reply.send(Err(err.clone()));
                }
                if let Some((done, _)) = self.stopping.take() {
                    let _ = done.send(());
                }
            }
        }
    }

    fn attach(&mut self, mut pending: Pending) {
        let (Some(instance), Some((wire, _))) = (&self.instance, &self.wire) else {
            self.waiting.push(pending);
            return self.ensure_started();
        };
        // The agent gets the real path (K13).
        let cwd = fs::canonicalize(&pending.attach.cwd).unwrap_or(pending.attach.cwd);
        pending.attach.cwd = cwd.clone();
        // Untrusted: Claude loads the user's settings only (design 5.2.6).
        let meta = (pending.attach.project_config == ProjectConfigPolicy::Ignore).then(|| {
            let options = json!({"settingSources": ["user"]});
            Meta::from_iter([("claudeCode".to_owned(), json!({"options": options}))])
        });
        let wire = wire.clone();
        let sent = match pending.attach.target.clone() {
            AttachTarget::New => instance.conn.request(
                NewSessionRequest::new(cwd).meta(meta),
                move |result: Result<_, Error>| {
                    let result = result.map(|r| {
                        (
                            r.session_id.to_string(),
                            r.config_options.unwrap_or_default(),
                        )
                    });
                    let _ = wire.send(Wire::Opened(Box::new(pending), result));
                },
            ),
            AttachTarget::Resume(AgentSessionId(id)) => instance.conn.request(
                ResumeSessionRequest::new(id.clone(), cwd).meta(meta),
                move |result: Result<_, Error>| {
                    let result = result.map(|r| (id, r.config_options.unwrap_or_default()));
                    let _ = wire.send(Wire::Opened(Box::new(pending), result));
                },
            ),
        };
        // The connection is gone; its exit follows and fails the attach.
        drop(sent);
    }

    fn wire(&mut self, wire: Wire) {
        match wire {
            Wire::Started(started) => self.started(*started),
            Wire::Exited(status) => {
                if let Some(instance) = &self.instance {
                    let _ = instance.group.signal(Signal::Kill);
                }
                self.exit = Some(status);
                self.end();
            }
            Wire::Agent(message) => self.incoming(*message),
            Wire::Opened(pending, result) => match result {
                Ok((session, options)) => self.opened(*pending, session, options),
                Err(err) => {
                    let _ = pending.reply.send(Err(claude::agent_error(&err)));
                }
            },
            Wire::Configured(session, result) => {
                let Some(opening) = self.opening.get_mut(&session) else {
                    return;
                };
                match result {
                    Ok(options) => {
                        opening.options = options;
                        self.configure(session);
                    }
                    Err(err) => {
                        if let Some(opening) = self.opening.remove(&session) {
                            let _ = opening.reply.send(Err(claude::agent_error(&err)));
                        }
                    }
                }
            }
            Wire::Prompted(session, tag, result) => {
                let event = match result {
                    Ok(response) => {
                        let (stop_reason, usage) = claude::prompt_done(&response);
                        DriverEvent::PromptDone {
                            tag,
                            stop_reason,
                            usage,
                        }
                    }
                    Err(err) => {
                        let error = claude::agent_error(&err);
                        if error.class == AgentErrorClass::NotLoggedIn {
                            self.auth.state = AuthState::LoggedOut;
                            self.notify();
                        }
                        DriverEvent::PromptFailed { tag, error }
                    }
                };
                self.to_session(&session, event);
            }
        }
    }

    /// A session opened: its route, then its options as attached.
    fn opened(&mut self, pending: Pending, session: String, options: Vec<SessionConfigOption>) {
        let config = capabilities::config_options(&options);
        let agent_option = |id: &str| config.iter().find(|o| o.id.as_str() == id);
        // Only the options whose value differs.
        let todo = (pending.attach.config.iter())
            .filter_map(|(id, value)| {
                let option = agent_option(id).filter(|option| option.current != *value)?;
                Some((option.agent_option_id.clone(), value.clone()))
            })
            .collect();
        let cwd = pending.attach.cwd.clone();
        let claude = ClaudeSession::new(cwd.clone(), config);
        let events = pending.events;
        self.sessions.insert(
            session.clone(),
            Route {
                events,
                claude,
                cwd,
                commands: None,
            },
        );
        let reply = pending.reply;
        let opening = Opening {
            reply,
            todo,
            options,
        };
        self.opening.insert(session.clone(), opening);
        self.configure(session);
    }

    /// Applies the next option of an opening session, or answers its attach.
    fn configure(&mut self, session: String) {
        let (Some(instance), Some((wire, _))) = (&self.instance, &self.wire) else {
            return;
        };
        let Some(opening) = self.opening.get_mut(&session) else {
            return;
        };
        if let Some((option, value)) = opening.todo.pop() {
            let wire = wire.clone();
            let value = SessionConfigOptionValue::value_id(value);
            let request = SetSessionConfigOptionRequest::new(session.clone(), option, value);
            let sent = instance
                .conn
                .request(request, move |result: Result<_, Error>| {
                    let result = result.map(|r| r.config_options);
                    let _ = wire.send(Wire::Configured(session, result));
                });
            drop(sent);
            return;
        }
        let opening = self.opening.remove(&session).expect("just found");
        let config = capabilities::config_options(&opening.options);
        let commands = self.commands.clone();
        if let Some(route) = self.sessions.get_mut(&session) {
            route.claude.set_config(&opening.options);
            route.commands = Some(digest(&commands));
        }
        let versions = self.deps.launch.versions.clone();
        let caps = capabilities::capabilities(&instance.init, versions, config, commands);
        let info = AttachInfo {
            session: AgentSessionId(session),
            instance: instance.id.clone(),
            capabilities: caps.clone(),
        };
        let _ = opening.reply.send(Ok(info));
        self.cache(caps);
    }

    /// Refreshes the capability cache.
    fn cache(&mut self, caps: SessionCapabilities) {
        let at = self.deps.cx.clock.now_wall_ms();
        drop(cache::write(&self.deps.cx.writer, &caps.agent, at, &caps));
        self.cached = Some(caps);
    }

    /// A session's command list: the agent's latest, and the session's if it
    /// has attached and has another; the cache's when it differs.
    fn commands(&mut self, session: &str, commands: Vec<SlashCommand>) {
        let sum = digest(&commands);
        let opening = self.opening.contains_key(session);
        if let Some(route) = self.sessions.get_mut(session)
            && !opening
            && route.commands != Some(sum)
        {
            route.commands = Some(sum);
            let _ = route
                .events
                .send(DriverEvent::CommandsChanged(commands.clone()));
        }
        if let Some(mut caps) = self.cached.clone()
            && caps.commands != commands
        {
            caps.commands.clone_from(&commands);
            self.cache(caps);
        }
        self.commands = commands;
    }

    async fn command(&mut self, command: DriverCommand) {
        let (Some(instance), Some((wire, _))) = (&self.instance, &self.wire) else {
            return;
        };
        match command {
            DriverCommand::Prompt {
                session,
                tag,
                content,
            } => {
                let AgentSessionId(id) = session;
                let Some(route) = self.sessions.get(&id) else {
                    return;
                };
                let blocks = content::blocks(&content, &route.cwd, &self.deps.blobs).await;
                let blocks = match blocks {
                    Ok(blocks) => blocks,
                    Err(message) => {
                        let error = error(AgentErrorClass::Other, message);
                        return self.to_session(&id, DriverEvent::PromptFailed { tag, error });
                    }
                };
                let wire = wire.clone();
                let request = PromptRequest::new(id.clone(), blocks);
                let sent = instance.conn.request(request, move |result| {
                    let _ = wire.send(Wire::Prompted(id, tag, result));
                });
                drop(sent);
            }
            DriverCommand::Cancel { session } => {
                drop(instance.conn.notify(CancelNotification::new(session.0)));
            }
            DriverCommand::Close { session } => {
                let AgentSessionId(id) = session;
                self.sessions.remove(&id);
                let sent = instance.conn.request(CloseSessionRequest::new(id), drop);
                drop(sent);
            }
            DriverCommand::Answer { ask, answer } => self.answer(ask, answer),
            DriverCommand::Attach(_)
            | DriverCommand::Steer { .. }
            | DriverCommand::SetConfig { .. } => {}
        }
    }

    fn answer(&mut self, ask: AskId, answer: AskAnswer) {
        // The agent hears every answer, including after a withdrawal.
        let _ = match self.asks.remove(&ask) {
            Some((_, Asked::Permission(responder), _)) => {
                responder.respond(content::permission(answer))
            }
            Some((_, Asked::Form(responder), _)) => responder.respond(content::elicitation(answer)),
            None => Ok(()),
        };
    }

    fn incoming(&mut self, message: FromAgent) {
        match message {
            FromAgent::Update(notification) => {
                let session = notification.session_id.to_string();
                let Some(route) = self.sessions.get_mut(&session) else {
                    return;
                };
                for event in route.claude.update(&notification.update) {
                    match event {
                        DriverEvent::CommandsChanged(commands) => self.commands(&session, commands),
                        event => self.to_session(&session, event),
                    }
                }
            }
            FromAgent::Permission(request, responder) => {
                let session = request.session_id.to_string();
                let Some(route) = self.sessions.get(&session) else {
                    return drop(responder.respond_with_error(Error::invalid_params()));
                };
                let (tool_call_id, request) = route.claude.permission(&request);
                let cancellation = responder.cancellation();
                let asked = (Asked::Permission(responder), Some(tool_call_id), request);
                self.ask(session, cancellation, asked);
            }
            FromAgent::Elicitation(request, responder) => {
                let session = match &request.mode {
                    ElicitationMode::Form(form) => match &form.scope {
                        ElicitationScope::Session(scope) => Some(scope.session_id.to_string()),
                        _ => None,
                    },
                    _ => None,
                };
                let asked = claude::ask::elicitation(&request);
                let (Some(session), Some((tool_call_id, request))) = (session, asked) else {
                    let decline = CreateElicitationResponse::new(
                        agent_client_protocol::schema::v1::ElicitationAction::Decline,
                    );
                    return drop(responder.respond(decline));
                };
                let cancellation = responder.cancellation();
                self.ask(
                    session,
                    cancellation,
                    (Asked::Form(responder), tool_call_id, request),
                );
            }
            FromAgent::Closed(_) => {
                self.closed = true;
                self.end();
            }
            FromAgent::AuthStatus(update) => {
                self.auth = status::auth(&update);
                self.notify();
            }
        }
    }

    /// Asks `session` under a new `AskId`, watching for the agent to take
    /// the request back until it is answered.
    fn ask(
        &mut self,
        session: String,
        cancellation: RequestCancellation,
        (asked, tool_call_id, request): (Asked, Option<String>, AskRequest),
    ) {
        self.last_ask += 1;
        let ask = AskId(self.last_ask);
        let (done, answered) = oneshot::channel::<()>();
        let mail = self.mail.clone();
        tokio::spawn(async move {
            tokio::select! {
                () = cancellation.cancelled() => {
                    mail.upgrade().map(|mail| mail.send(Mail::Withdrawn(ask)));
                }
                _ = answered => {}
            }
        });
        let event = DriverEvent::Ask {
            ask,
            tool_call_id,
            request,
        };
        self.to_session(&session, event);
        self.asks.insert(ask, (session, asked, done));
    }

    fn to_session(&self, session: &str, event: DriverEvent) {
        if let Some(route) = self.sessions.get(session) {
            let _ = route.events.send(event);
        }
    }

    /// Every session of this agent.
    fn broadcast(&self, event: DriverEvent) {
        for route in self.sessions.values() {
            let _ = route.events.send(event.clone());
        }
    }

    /// Terminates the instance's process group for a stop.
    fn terminate(&self) {
        let (Some(instance), Some((_, force))) = (&self.instance, &self.stopping) else {
            return;
        };
        let signal = if *force { Signal::Kill } else { Signal::Term };
        let _ = instance.group.signal(signal);
    }

    /// Ends the instance once it has exited and its connection has closed.
    fn end(&mut self) {
        let (Some(status), true) = (self.exit, self.closed) else {
            return;
        };
        let Some(Instance { id: instance, .. }) = self.instance.take() else {
            return;
        };
        self.wire = None;
        self.asks.clear();
        // Their attaches hear that the adapter went away.
        self.opening.clear();
        self.broadcast(DriverEvent::AdapterExited { instance, status });
        self.sessions.clear();
        self.notify();
        if let Some((done, _)) = self.stopping.take() {
            let _ = done.send(());
        } else if !self.waiting.is_empty() {
            self.ensure_started();
        }
    }
}

/// Empties `dir` for a probe: removed, then made again, the user's only
/// (design 5.2.6).
fn fresh_dir(dir: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    match fs::remove_dir_all(dir) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => return Err(err),
        _ => {}
    }
    fs::create_dir_all(dir)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
}
