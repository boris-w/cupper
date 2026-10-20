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
//!   `AdapterExited` with the latest stderr. An instance that ends unasked
//!   is a failure: what is left of its process group and of the groups
//!   sampling found is killed, and the connection gets 2 s more. The next
//!   start waits out the backoff ([`lifecycle::Restarts`]); attaches that
//!   wait start it when the backoff is over, and five failures within five
//!   minutes make the agent `crashed`.
//! - **A stop** follows 5.6.8: `session/close` for every session, stdin
//!   closed, 3 s each; then SIGTERM to the adapter's group and those of all
//!   its descendants, found before any signal, and SIGKILL 2 s later.
//! - **Every 30 s** it samples the processes ([`lifecycle::sample`]): their
//!   memory for [`AgentRuntime::resources`], and the descendants' groups,
//!   kept in the `adapters` row for the cleanup after a crash of the host.
//! - Steering (T140) and probing (T009) are not served yet; their commands
//!   are dropped.

pub mod content;
pub mod launch;
pub mod lifecycle;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::fs;
use std::future;
use std::mem;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::sync::Arc;
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    CancelNotification, CloseSessionRequest, CreateElicitationResponse, ElicitationMode,
    ElicitationScope, Meta, NewSessionRequest, PromptRequest, PromptResponse,
    RequestPermissionResponse, ResumeSessionRequest, SessionConfigOption, SessionConfigOptionValue,
    SetSessionConfigOptionRequest,
};
use agent_client_protocol::{Error, RequestCancellation, Responder};
use futures::FutureExt;
use futures::future::BoxFuture;
use serde_json::json;
use tokio::sync::{mpsc, oneshot, watch};

use self::launch::{Context, Instance, Launch};
use self::lifecycle::{Resources, Restarts};
use super::acp::{AcpConnection, FromAgent};
use super::api::{
    AgentError, AgentErrorClass, AgentErrorForm, AgentPort, AgentSessionId, AskAnswer, AskId,
    AskRequest, Attach, AttachInfo, AttachTarget, DriverCommand, DriverEvent, ProjectConfigPolicy,
};
use super::claude::{self, ClaudeSession, capabilities};
use crate::clock::IdGen;
use crate::durations::{ADAPTER_CLOSE_WAIT, ADAPTER_TERM_WAIT, Durations, RESOURCE_SAMPLE};
use crate::pure::{Alarm, TimerRequest, TimerTable};
use crate::quiescence::{ActorKind, Participant, Quiescence};
use crate::store::BlobStore;
use crate::supervisor::{Activity, Part};
use cupper_protocol::ids::AdapterInstanceId;
use cupper_sys::{ProcessGroup, Signal};

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
    pub durations: Durations,
    /// `test_config.rss_samples`: the memory samples report.
    pub rss: Option<u64>,
}

/// The runtime of one agent. Clones share it.
#[derive(Debug, Clone)]
pub struct AgentRuntime {
    mail: mpsc::UnboundedSender<Mail>,
    resources: watch::Receiver<Option<Resources>>,
}

#[derive(Debug)]
enum Mail {
    Attach(Box<Pending>),
    Command(DriverCommand),
    Withdrawn(AskId),
    Stop(oneshot::Sender<()>),
    ClearCrashed,
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
    /// `session/set_config_option`, while attaching or for `SetConfig`.
    Configured(String, Result<Vec<SessionConfigOption>, Error>),
    Prompted(String, super::api::PromptTag, Result<PromptResponse, Error>),
    /// A stop's `session/close` was answered.
    SessionClosed,
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
}

/// An attach applying its configuration, option by option.
#[derive(Debug)]
struct Opening {
    reply: oneshot::Sender<Result<AttachInfo, AgentError>>,
    todo: Vec<(String, String)>,
    options: Vec<SessionConfigOption>,
}

/// What the actor's timer is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Due {
    /// The backoff is over: start an instance for the waiting attaches.
    Start,
    Sample,
    /// A stop: the sessions had their time to close.
    Stdin,
    /// A stop: the adapter had its time to exit after its stdin closed.
    Term,
    /// SIGKILL what is left, and stop waiting for the connection.
    Kill,
}

/// A stop on its way: the groups to signal, and the sessions to close.
#[derive(Debug)]
struct Stopping {
    done: oneshot::Sender<()>,
    groups: Vec<ProcessGroup>,
    closing: usize,
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
    stopping: Option<Stopping>,
    timers: TimerTable<Due>,
    alarm: Alarm,
    restarts: Restarts,
    /// The descendants' groups sampling found, still alive.
    recorded: Vec<ProcessGroup>,
    resources: watch::Sender<Option<Resources>>,
    _participant: Participant,
}

/// The instance, its connection while its stdin is open, and its wire.
fn link<'a>(
    instance: &'a Option<Instance>,
    wire: &'a Option<(mpsc::UnboundedSender<Wire>, mpsc::UnboundedReceiver<Wire>)>,
) -> Option<(
    &'a Instance,
    &'a AcpConnection,
    &'a mpsc::UnboundedSender<Wire>,
)> {
    let (Some(instance), Some((wire, _))) = (instance, wire) else {
        return None;
    };
    Some((instance, instance.conn.as_ref()?, wire))
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
        let (resources, sampled) = watch::channel(None);
        let (alarm, restarts) = (
            Alarm::new(deps.cx.clock.clone()),
            Restarts::new(&deps.durations),
        );
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
            timers: TimerTable::new(),
            alarm,
            restarts,
            recorded: Vec::new(),
            resources,
            _participant: participant,
        };
        tokio::spawn(actor.run(inbox));
        let resources = sampled;
        Self { mail, resources }
    }

    /// The latest sample of the adapter's processes; `None` while none runs.
    pub fn resources(&self) -> watch::Receiver<Option<Resources>> {
        self.resources.clone()
    }

    /// Forgets the failures, so that a `crashed` agent starts again.
    pub fn clear_crashed(&self) {
        let _ = self.mail.send(Mail::ClearCrashed);
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

    /// Closes the adapter in the order of 5.6.8, forced or not: at most
    /// 3 + 3 + 2 s and the time SIGKILL takes.
    fn stop(&self, _force: bool) -> BoxFuture<'static, ()> {
        let (done, stopped) = oneshot::channel();
        let _ = self.mail.send(Mail::Stop(done));
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
                () = self.alarm.rung() => self.due(),
            }
            self.alarm.follow(&self.timers);
        }
    }

    fn now(&self) -> crate::clock::MonoTime {
        self.deps.cx.clock.now_mono()
    }

    fn set(&mut self, key: Due, after: Duration) {
        self.set_at(key, self.now() + after);
    }

    fn set_at(&mut self, key: Due, at: crate::clock::MonoTime) {
        self.timers.apply(TimerRequest::Set { key, at });
    }

    fn due(&mut self) {
        while let Some(due) = self.timers.pop_due(self.now()) {
            match due {
                Due::Start => self.ensure_started(),
                Due::Sample => self.sample(),
                Due::Stdin => self.close_stdin(),
                Due::Term => {
                    let (Some(instance), Some(stopping)) = (&self.instance, &mut self.stopping)
                    else {
                        continue;
                    };
                    let groups =
                        lifecycle::union(instance.group.with_descendants(), &self.recorded);
                    stopping.groups = lifecycle::union(groups, &stopping.groups);
                    lifecycle::signal(&stopping.groups, Signal::Term);
                    self.set(Due::Kill, self.deps.durations.get(ADAPTER_TERM_WAIT));
                }
                Due::Kill => {
                    if let Some(instance) = &self.instance {
                        let stopped = self.stopping.as_ref().map(|s| s.groups.as_slice());
                        let groups = lifecycle::union(vec![instance.group], &self.recorded);
                        let groups = lifecycle::union(groups, stopped.unwrap_or_default());
                        lifecycle::signal(&groups, Signal::Kill);
                    }
                    self.closed = true;
                    self.end();
                }
            }
        }
    }

    /// Samples the instance's processes, and samples again in 30 s.
    fn sample(&mut self) {
        let Some(instance) = &self.instance else {
            return;
        };
        let cli = (self.deps.launch.cli.as_ref()).and_then(|(cli, _)| fs::canonicalize(cli).ok());
        let (resources, groups) = lifecycle::sample(&instance.group, cli.as_deref(), self.deps.rss);
        let groups = lifecycle::union(groups, &self.recorded);
        self.recorded = groups.into_iter().filter(ProcessGroup::is_alive).collect();
        launch::record_groups(&self.deps.cx.writer, &instance.id, &self.recorded);
        let sessions = self.sessions.len();
        if resources.claude_processes > sessions {
            tracing::warn!(
                ?resources,
                sessions,
                "claude processes beyond sessions (Q43)"
            );
        }
        self.resources.send_replace(Some(resources));
        self.set(Due::Sample, self.deps.durations.get(RESOURCE_SAMPLE));
    }

    /// A stop's first step: `session/close` for every session.
    fn begin_stop(&mut self) {
        let Some((_, conn, wire)) = link(&self.instance, &self.wire) else {
            return;
        };
        let Some(stopping) = &mut self.stopping else {
            return;
        };
        for session in self.sessions.keys() {
            let wire = wire.clone();
            let request = CloseSessionRequest::new(session.clone());
            let sent = conn.request(request, move |_| drop(wire.send(Wire::SessionClosed)));
            stopping.closing += usize::from(sent.is_ok());
        }
        match stopping.closing {
            0 => self.close_stdin(),
            _ => self.set(Due::Stdin, self.deps.durations.get(ADAPTER_CLOSE_WAIT)),
        }
    }

    /// A stop's second step: the adapter's stdin closes.
    fn close_stdin(&mut self) {
        let Some(instance) = &mut self.instance else {
            return;
        };
        if instance.conn.take().is_none() {
            return;
        }
        self.timers.apply(TimerRequest::Cancel(Due::Stdin));
        self.set(Due::Term, self.deps.durations.get(ADAPTER_CLOSE_WAIT));
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
            Mail::Stop(done) => {
                // Attaches still waiting hear that the adapter went away.
                self.waiting.clear();
                self.timers.apply(TimerRequest::Cancel(Due::Start));
                if self.wire.is_none() {
                    let _ = done.send(());
                    return;
                }
                // Before any signal, while the adapter still runs (5.6.8).
                let found = self.instance.as_ref().map(|i| i.group.with_descendants());
                let groups = lifecycle::union(found.unwrap_or_default(), &self.recorded);
                self.stopping = Some(Stopping {
                    done,
                    groups,
                    closing: 0,
                });
                self.begin_stop();
            }
            Mail::ClearCrashed => {
                self.restarts.clear();
                self.ensure_started();
            }
        }
    }

    /// Starts an instance for the waiting attaches unless one runs or is
    /// starting, or the backoff is not over: then when it is.
    fn ensure_started(&mut self) {
        let busy = self.instance.is_some() || self.wire.is_some() || self.stopping.is_some();
        if busy || self.waiting.is_empty() {
            return;
        }
        let Some(at) = self.restarts.next_start() else {
            let crashed = error(AgentErrorClass::Crashed, "5 failures within 5 minutes");
            for pending in mem::take(&mut self.waiting) {
                let _ = pending.reply.send(Err(crashed.clone()));
            }
            return;
        };
        if at > self.now() {
            return self.set_at(Due::Start, at);
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
                self.set(Due::Sample, self.deps.durations.get(RESOURCE_SAMPLE));
                if self.stopping.is_some() {
                    return self.begin_stop();
                }
                for pending in mem::take(&mut self.waiting) {
                    self.attach(pending);
                }
            }
            Err(err) => {
                self.wire = None;
                self.restarts.failed(self.now());
                for pending in mem::take(&mut self.waiting) {
                    let _ = pending.reply.send(Err(err.clone()));
                }
                if let Some(stopping) = self.stopping.take() {
                    let _ = stopping.done.send(());
                }
            }
        }
    }

    fn attach(&mut self, mut pending: Pending) {
        let (Some(instance), Some((wire, _))) = (&self.instance, &self.wire) else {
            self.waiting.push(pending);
            return self.ensure_started();
        };
        // Closing: the attach hears that the adapter went away.
        let Some(conn) = &instance.conn else {
            return;
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
            AttachTarget::New => conn.request(
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
            AttachTarget::Resume(AgentSessionId(id)) => conn.request(
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
                if let Some(instance) = self.instance.as_ref().filter(|_| self.stopping.is_none()) {
                    let groups = lifecycle::union(vec![instance.group], &self.recorded);
                    lifecycle::signal(&groups, Signal::Kill);
                    self.set(Due::Kill, self.deps.durations.get(ADAPTER_TERM_WAIT));
                }
                self.exit = Some(status);
                self.end();
            }
            Wire::SessionClosed => {
                let Some(stopping) = &mut self.stopping else {
                    return;
                };
                stopping.closing = stopping.closing.saturating_sub(1);
                if stopping.closing == 0 {
                    self.close_stdin();
                }
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
                    // `SetConfig`: the session hears all the options. A
                    // refusal leaves them as they were.
                    if let (Some(route), Ok(options)) = (self.sessions.get_mut(&session), result) {
                        let _ = route.events.send(route.claude.set_config(&options));
                    }
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
        let Some((instance, conn, wire)) = link(&self.instance, &self.wire) else {
            return;
        };
        let Some(opening) = self.opening.get_mut(&session) else {
            return;
        };
        if let Some((option, value)) = opening.todo.pop() {
            let wire = wire.clone();
            let value = SessionConfigOptionValue::value_id(value);
            let request = SetSessionConfigOptionRequest::new(session.clone(), option, value);
            let sent = conn.request(request, move |result: Result<_, Error>| {
                let result = result.map(|r| r.config_options);
                let _ = wire.send(Wire::Configured(session, result));
            });
            drop(sent);
            return;
        }
        let opening = self.opening.remove(&session).expect("just found");
        let config = capabilities::config_options(&opening.options);
        if let Some(route) = self.sessions.get_mut(&session) {
            route.claude.set_config(&opening.options);
        }
        let versions = self.deps.launch.versions.clone();
        let caps = capabilities::capabilities(&instance.init, versions, config, Vec::new());
        let info = AttachInfo {
            session: AgentSessionId(session),
            instance: instance.id.clone(),
            capabilities: caps,
        };
        let _ = opening.reply.send(Ok(info));
    }

    async fn command(&mut self, command: DriverCommand) {
        let Some((_, conn, wire)) = link(&self.instance, &self.wire) else {
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
                let sent = conn.request(request, move |result| {
                    let _ = wire.send(Wire::Prompted(id, tag, result));
                });
                drop(sent);
            }
            DriverCommand::Cancel { session } => {
                drop(conn.notify(CancelNotification::new(session.0)));
            }
            DriverCommand::Close { session } => {
                let AgentSessionId(id) = session;
                self.sessions.remove(&id);
                let sent = conn.request(CloseSessionRequest::new(id), drop);
                drop(sent);
            }
            DriverCommand::Answer { ask, answer } => self.answer(ask, answer),
            DriverCommand::SetConfig {
                session,
                option,
                value,
            } => {
                let AgentSessionId(id) = session;
                let Some(route) = self.sessions.get(&id) else {
                    return;
                };
                // cupper's ID of the option, as the agent names it.
                let named = route
                    .claude
                    .config()
                    .iter()
                    .find(|o| o.id.as_str() == option);
                let option = named.map_or(option, |o| o.agent_option_id.clone());
                let (wire, value) = (wire.clone(), SessionConfigOptionValue::value_id(value));
                let request = SetSessionConfigOptionRequest::new(id.clone(), option, value);
                let sent = conn.request(request, move |result: Result<_, Error>| {
                    let result = result.map(|r| r.config_options);
                    let _ = wire.send(Wire::Configured(id, result));
                });
                drop(sent);
            }
            DriverCommand::Attach(_) | DriverCommand::Steer { .. } | DriverCommand::Probe => {}
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
                    let _ = route.events.send(event);
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
            // The login state is T009's.
            FromAgent::AuthStatus(_) => {}
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

    /// Ends the instance once it has exited and its connection has closed.
    fn end(&mut self) {
        let (Some(status), true) = (self.exit, self.closed) else {
            return;
        };
        let Some(Instance { id, tail, .. }) = self.instance.take() else {
            return;
        };
        self.wire = None;
        self.asks.clear();
        // Their attaches hear that the adapter went away.
        self.opening.clear();
        for due in [Due::Sample, Due::Stdin, Due::Term, Due::Kill] {
            self.timers.apply(TimerRequest::Cancel(due));
        }
        (self.recorded, self.exit) = (Vec::new(), None);
        self.resources.send_replace(None);
        let (instance, stderr) = (id, tail.map(|tail| tail.text()).unwrap_or_default());
        let exited = DriverEvent::AdapterExited {
            instance,
            status,
            stderr,
        };
        self.broadcast(exited);
        self.sessions.clear();
        match self.stopping.take() {
            Some(stopping) => drop(stopping.done.send(())),
            None => {
                self.restarts.failed(self.now());
                self.ensure_started();
            }
        }
    }
}
