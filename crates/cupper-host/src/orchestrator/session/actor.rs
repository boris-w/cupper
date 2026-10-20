//! `SessionActor` (design 5.2.3, 5.2.6, 5.2.16): the adapter that runs a
//! session's [`SessionCore`] on its mailbox, the agent's events, the clock
//! and the store.
//!
//! Every input is a step of the core. A step's events commit as one batch,
//! with the rows they change and, for a command, the ledger's record of its
//! result (design 5.2.3, rule 3); only then does the reply go out, and the
//! step's driver commands and attaches. An attach runs in a task of its own
//! and comes back as an input; so does a snapshot, which is `skipped` until
//! T061 brings git snapshots (plan 1.3). The index hears of each committed
//! step's turns, requests, branch and read-only state; the audit channel of
//! answers from the CLI and the test harness (design 5.8.6).

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::mem;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use cupper_protocol::annotations::TimestampMs;
use cupper_protocol::canonical;
use cupper_protocol::capabilities::{ConfigOptionId, SessionCapabilities as Capabilities};
use cupper_protocol::domain::{PrincipalKind, ResolvedBy, Tier};
use cupper_protocol::envelope::Cause;
use cupper_protocol::error::{ErrorCode, ErrorReason, INVALID_PARAMS, RpcError};
use cupper_protocol::event::EventBody;
use cupper_protocol::ids::{CommandId, PrincipalId, ProjectId, QueueId, SessionId, TurnId};
use cupper_protocol::ids::{Digest, RequestId};
use cupper_protocol::index::{PendingKind, SessionStatus, SessionSummary, TrustState};
use cupper_protocol::interaction::InteractionOutcome;
use cupper_protocol::interaction::InteractionRequest;
use cupper_protocol::item::{NoticeCode, NoticeParams};
use cupper_protocol::methods::handshake::Principal;
use cupper_protocol::methods::interaction::{
    InteractionGet, InteractionGetParams, InteractionGetResult, InteractionRespond,
    InteractionRespondParams,
};
use cupper_protocol::methods::session::{
    self as methods, Disposition as Wire, FallbackReason, SessionCapabilities,
    SessionConfirmConfig, SessionConfirmConfigParams, SessionGet, SessionGetResult,
    SessionInterrupt, SessionInterruptParams, SessionInterruptResult, SessionQueueResume,
    SessionSend, SessionSendParams, SessionSendResult,
};
use cupper_protocol::rpc::Method;
use cupper_protocol::session::{
    ConfirmationReason, ConfirmationRequired, PauseReason, ProjectConfig, SessionConfigured,
    SnapshotPhase, SnapshotRecorded, SnapshotStatus, TurnStatus,
};
use cupper_protocol::view::SessionView;
use rusqlite::OptionalExtension;
use serde_json::json;
use tokio::sync::{mpsc, oneshot};

use super::core::{Action, Input, Key, SessionCore};
use super::rows::Rows;
use super::{IndexNotes, Note, internal, mode_tier, not_found, parse, tier_of};
use crate::clock::Clock;
use crate::driver::api::{
    AgentError, AgentPort, Attach, AttachInfo, AttachTarget, DriverCommand, DriverEvent,
    ProjectConfigPolicy,
};
use crate::durations::Durations;
use crate::gateway::{AuditEvent, AuditRecord, Call, ConfigState, TrustChange};
use crate::git::GitTool;
use crate::index::{ScanContext, git_of};
use crate::ledger::{Outcome, Ticket};
use crate::orchestrator::interaction::{self, Respond};
use crate::orchestrator::notice_with;
use crate::orchestrator::tier_guard;
use crate::orchestrator::turn::{self, Disposition, SendMode, SendRequest, SteerFallback};
use crate::policy;
use crate::pure::{Alarm, Core, TimerTable};
use crate::quiescence::Participant;
use crate::store::streams::{Batch, StreamHandle, StreamView, StreamWriter};
use crate::store::{Durability, Readers, Writer};
use crate::trust::{self, Scan};

/// A call and where its reply goes.
pub(super) enum Mail {
    Call(Box<Call>, oneshot::Sender<Outcome>),
    /// Leave the project's configuration out from now on (design 5.2.6:
    /// "不加载项目配置继续"); the host's own entry until M1b's method.
    IgnoreProjectConfig(oneshot::Sender<()>),
}
/// The host's exit, whether forced, and the reply once the session took it.
pub(super) type Stop = (bool, oneshot::Sender<()>);

/// What woke the actor.
enum Next {
    Mail(Option<Mail>),
    Agent(DriverEvent),
    Attached(Result<AttachInfo, AgentError>),
    Rescanned(Purpose, Rescan),
    Stop(Stop),
    Alarm,
}

/// What the actor starts with: its session's stream, its agent, and what an
/// attach adds to the turn machine's target (design 5.2.6).
pub(super) struct Parts {
    pub id: SessionId,
    pub core: SessionCore,
    pub clock: Arc<dyn Clock>,
    pub stream: StreamHandle,
    pub writer: StreamWriter,
    /// For rows that go without events.
    pub db: Writer,
    pub readers: Readers,
    pub agent: Arc<dyn AgentPort>,
    /// The workspace, as a real path.
    pub cwd: PathBuf,
    /// The option values the session was created with.
    pub config: BTreeMap<String, String>,
    pub project_config: ProjectConfigPolicy,
    /// The last `session.configured`.
    pub configured: SessionConfigured,
    /// The agent's cached capabilities, until an attach brings the session's.
    pub cached: Option<Capabilities>,
    /// Set while a turn runs, for `running_turns`.
    pub running: Arc<AtomicBool>,
    /// The pending requests, for `pending_interactions`.
    pub pending: Arc<AtomicU32>,
    /// The queued messages, for the supervisor's idle exit.
    pub queued: Arc<AtomicU32>,
    pub audit: mpsc::UnboundedSender<AuditRecord>,
    pub index: Arc<dyn IndexNotes>,
    pub project: ProjectId,
    /// Finds the git root of the trust range (`Discovery::git`).
    pub git: Option<GitTool>,
    pub durations: Durations,
}

pub(super) struct SessionActor {
    parts: Parts,
    timers: TimerTable<Key>,
    alarm: Alarm,
    /// The command that queued each message, for its `queue_entries` row.
    commands: HashMap<QueueId, CommandId>,
    inbox: mpsc::UnboundedReceiver<Mail>,
    events: mpsc::UnboundedSender<DriverEvent>,
    attached: mpsc::UnboundedSender<Result<AttachInfo, AgentError>>,
    rescanned: mpsc::UnboundedSender<(Purpose, Rescan)>,
    /// The session would load the project's configuration while trusted.
    load_wanted: bool,
    /// The gate is shut for `project_config_changed`.
    project_gate: bool,
    /// "不加载项目配置继续": the project's configuration stays out.
    ignore_for_good: bool,
    /// A `session/send` whose reply waits for the check before its turn.
    deferred: Option<Command>,
}

/// Why the project's trust range is rescanned (design 5.2.6).
enum Purpose {
    /// Before a message turns into a turn.
    Check,
    Attach(AttachTarget),
    TurnEnd,
    /// While the gate is shut for `project_config_changed`: the project may
    /// be trusted again.
    Reopen,
}

/// What a rescan found.
struct Rescan {
    trusted: bool,
    /// The project was trusted and is `stale` now: the index records it.
    stale: bool,
    /// Why the rescan failed; it then counts as untrusted.
    failure: Option<String>,
    /// The scan itself failed: `project.trust.scan_failed`.
    scan_failed: bool,
    scan: Option<Scan>,
}

/// A command on its way: its ledger ticket, its reply, and what the audit
/// records once it succeeds.
struct Command {
    ticket: Option<Ticket>,
    cause: Option<Cause>,
    reply: oneshot::Sender<Outcome>,
    audit: Option<AuditRecord>,
    /// `session/send`: its reply may come with a later step.
    deferrable: bool,
}

/// The reply a step gives a command.
enum Reply {
    Turn(turn::Reply),
    Interaction(interaction::Reply),
    /// `{}`.
    Done,
}

/// What a step asks of the actor besides its events.
#[derive(Default)]
struct Step {
    events: Vec<EventBody>,
    /// The agent's session an attach brought.
    agent_session: Option<String>,
    driver: Vec<DriverCommand>,
    attach: Vec<AttachTarget>,
    /// The machine asked for the check before a turn.
    check: bool,
    reply: Option<Reply>,
    /// Who set the tier now, and whether full access was just confirmed.
    set_by: Option<(PrincipalId, bool)>,
    audit: Vec<AuditRecord>,
}

impl SessionActor {
    /// Runs the session, starting with its first message, until the mailbox
    /// closes; it counts in the quiescence barrier meanwhile.
    pub(super) async fn run(
        parts: Parts,
        inbox: mpsc::UnboundedReceiver<Mail>,
        mut stops: mpsc::UnboundedReceiver<Stop>,
        first: SendRequest,
        participant: Participant,
    ) {
        let (events, mut agent) = mpsc::unbounded_channel();
        let (attached, mut attaches) = mpsc::unbounded_channel();
        let (rescanned, mut rescans) = mpsc::unbounded_channel();
        let alarm = Alarm::new(Arc::clone(&parts.clock));
        let load_wanted = parts.project_config == ProjectConfigPolicy::Load;
        let mut actor = Self {
            parts,
            timers: TimerTable::new(),
            alarm,
            commands: HashMap::new(),
            inbox,
            events,
            attached,
            rescanned,
            load_wanted,
            project_gate: false,
            ignore_for_good: false,
            deferred: None,
        };
        let first = Input::Machine(turn::Input::Send(first));
        actor.input(first, None, Step::default()).await;
        // Idle until a branch brings work, which runs once the others are
        // dropped.
        loop {
            let next = participant.idle(async {
                tokio::select! {
                    mail = actor.inbox.recv() => Next::Mail(mail),
                    Some(event) = agent.recv() => Next::Agent(event),
                    Some(result) = attaches.recv() => Next::Attached(result),
                    Some((purpose, found)) = rescans.recv() => Next::Rescanned(purpose, found),
                    Some(stop) = stops.recv() => Next::Stop(stop),
                    () = actor.alarm.rung() => Next::Alarm,
                }
            });
            match next.await {
                Next::Mail(Some(Mail::Call(call, reply))) => actor.call(*call, reply).await,
                Next::Mail(Some(Mail::IgnoreProjectConfig(done))) => {
                    actor.ignore_project_config().await;
                    let _ = done.send(());
                }
                Next::Mail(None) => break,
                Next::Agent(event) => actor.agent(event).await,
                Next::Attached(result) => actor.attach_done(result).await,
                Next::Rescanned(purpose, found) => actor.trust(purpose, found).await,
                Next::Stop((force, done)) => {
                    if force {
                        let exit = Input::Machine(turn::Input::HostExit);
                        actor.input(exit, None, Step::default()).await;
                    }
                    let _ = done.send(());
                }
                Next::Alarm => actor.expired().await,
            }
        }
    }

    async fn call(&mut self, call: Call, reply: oneshot::Sender<Outcome>) {
        let (principal, params) = (call.principal, call.params);
        let mut audit = None;
        let mut step = Step::default();
        // An option that moves the tier makes its chooser the setter.
        let mut chosen = None;
        let input = match call.method {
            SessionSend::NAME => parse(&params).and_then(|p| self.send(p, &principal.id)),
            SessionInterrupt::NAME => parse::<SessionInterruptParams>(&params).map(|p| {
                let expected_turn = p.expected_turn_id;
                Input::Machine(turn::Input::Interrupt { expected_turn })
            }),
            SessionQueueResume::NAME => Ok(Input::Machine(turn::Input::QueueResume)),
            SessionConfirmConfig::NAME => {
                step.reply = Some(Reply::Done);
                parse(&params).and_then(|p| self.confirm(p, &principal))
            }
            InteractionRespond::NAME => parse::<InteractionRespondParams>(&params).map(|params| {
                if matches!(
                    principal.kind,
                    PrincipalKind::Cli | PrincipalKind::TestHarness
                ) {
                    let event = AuditEvent::InteractionResponded {
                        session_id: self.parts.id.clone(),
                        request_id: params.request_id.clone(),
                        outcome: params.outcome.clone(),
                        principal: principal.id.clone(),
                    };
                    let (at_ms, conn) = (self.parts.clock.now_wall_ms(), Some(call.conn.clone()));
                    audit = Some(AuditRecord { at_ms, conn, event });
                }
                let resolved_by = ResolvedBy {
                    principal_id: principal.id.clone(),
                    client_kind: call.client.kind,
                    client_name: call.client.name,
                };
                let tier = self.tier();
                let principal = principal.clone();
                chosen = self
                    .option_tier(&params)
                    .map(|tier| (params.request_id.clone(), tier));
                Input::Respond(Box::new(Respond {
                    principal,
                    resolved_by,
                    tier,
                    params,
                }))
            }),
            SessionGet::NAME => {
                let _ = reply.send(self.get().await);
                return;
            }
            SessionCapabilities::NAME => {
                let capabilities = self.capabilities().map(|c| json!(c));
                let _ = reply.send(capabilities.ok_or_else(|| not_found("capabilities")));
                return;
            }
            InteractionGet::NAME => {
                let _ = reply.send(parse(&params).and_then(|p| self.interaction(p)));
                return;
            }
            method => Err(RpcError::app(ErrorCode::Unsupported, method)),
        };
        let cause = parse::<CommandId>(&params["command_id"])
            .ok()
            .map(|command_id| Cause {
                command_id,
                principal: principal.id.clone(),
            });
        let input = match input {
            Ok(input) => input,
            Err(err) => {
                if let Some(ticket) = call.ticket {
                    ticket.reject(err.clone());
                }
                let _ = reply.send(Err(err));
                return;
            }
        };
        let ticket = call.ticket;
        let deferrable = call.method == SessionSend::NAME;
        let reopen = matches!(call.method, SessionSend::NAME | SessionQueueResume::NAME);
        let command = Command {
            ticket,
            cause,
            reply,
            audit,
            deferrable,
        };
        self.input(input, Some(command), step).await;
        if reopen && self.project_gate {
            self.rescan(Purpose::Reopen);
        }
        if let Some((request, tier)) = chosen
            && self.resolved_by(&request).as_ref() == Some(&principal.id)
        {
            let set = tier_guard::Input::Set {
                by: principal,
                tier,
            };
            self.input(Input::Tier(set), None, Step::default()).await;
        }
    }

    /// `session/confirm_config`: the configuration the confirmation saw is
    /// still the session's.
    fn confirm(&self, p: SessionConfirmConfigParams, by: &Principal) -> Result<Input, RpcError> {
        if p.config_digest != config_digest(&self.parts.configured) {
            let err = RpcError::app(ErrorCode::Conflict, "the configuration changed meanwhile");
            return Err(err.with_reason(ErrorReason::DigestMismatch));
        }
        Ok(Input::Tier(tier_guard::Input::Confirmed { by: by.clone() }))
    }

    /// The tier the option chosen in `params` puts the session in, if any.
    fn option_tier(&self, params: &InteractionRespondParams) -> Option<Tier> {
        let InteractionOutcome::Selected(selected) = &params.outcome else {
            return None;
        };
        self.view(|view| {
            let request = &view.requests.get(&params.request_id)?.requested.request;
            let options = match request {
                InteractionRequest::Permission(p) => &p.options,
                InteractionRequest::PlanConfirm(p) => &p.options,
                _ => return None,
            };
            let option = options.iter().find(|o| o.option_id == selected.option_id)?;
            policy::option_tier(&option.effect)
        })
    }

    fn resolved_by(&self, request: &RequestId) -> Option<PrincipalId> {
        self.view(|view| {
            let resolved = view.requests.get(request)?.resolved.as_ref()?;
            Some(resolved.resolved_by.as_ref()?.principal_id.clone())
        })
    }

    /// `session/send`; `reply_to` must name a request of this session that
    /// was answered (design 5.3.4, v1.5).
    fn send(&self, p: SessionSendParams, by: &PrincipalId) -> Result<Input, RpcError> {
        if let Some(id) = &p.reply_to
            && !self.parts.core.answered(id)
        {
            let message = format!("{id} is not an answered request of this session");
            return Err(RpcError::new(INVALID_PARAMS, message));
        }
        Ok(Input::Machine(turn::Input::Send(SendRequest {
            content: p.content,
            mode: match p.mode {
                methods::SendMode::Steer => SendMode::Steer,
                _ => SendMode::Queue,
            },
            by: by.clone(),
            reply_to: p.reply_to,
        })))
    }

    /// The session's tier now, by its permission mode; `None` while unknown.
    fn tier(&self) -> Option<Tier> {
        let config = &self.parts.configured.config;
        let mode = config.get(ConfigOptionId::PermissionMode.as_str())?;
        tier_of(self.capabilities().as_ref(), mode)
    }

    /// `interaction/get`, for a request in the session's view: the pending
    /// ones and those of the latest turns.
    fn interaction(&self, p: InteractionGetParams) -> Outcome {
        let found = self.view(|view| view.requests.get(&p.request_id).cloned());
        let request = found.ok_or_else(|| not_found("request"))?;
        let resolved = request.resolved.as_ref();
        Ok(json!(InteractionGetResult {
            status: request.status().clone(),
            outcome: resolved.and_then(|r| r.outcome.clone()),
            resolved_by: resolved.and_then(|r| r.resolved_by.clone()),
            reason: resolved.and_then(|r| r.reason.clone()),
            requested: request.requested,
        }))
    }

    /// Steps the core with `input` and whatever its actions bring back, one
    /// batch per step; `step` carries what comes with the first.
    async fn input(&mut self, input: Input, mut command: Option<Command>, mut step: Step) {
        let mut inputs = VecDeque::from([input]);
        while let Some(input) = inputs.pop_front() {
            for action in self.parts.core.step(input, self.parts.clock.now()) {
                match action {
                    Action::Event(body) => step.events.push(body),
                    Action::Timer(request) => self.timers.apply(request),
                    Action::Driver(command) => step.driver.push(command),
                    Action::Answer(reply) => step.reply = Some(Reply::Interaction(reply)),
                    Action::Machine(turn::Action::Attach(target)) => step.attach.push(target),
                    Action::Machine(turn::Action::Check) => step.check = true,
                    Action::Machine(turn::Action::Snapshot { turn_id, phase }) => {
                        inputs.push_back(skipped(turn_id, phase))
                    }
                    Action::Machine(turn::Action::Reply(reply)) => {
                        step.reply = Some(Reply::Turn(reply))
                    }
                    // The core keeps the machine's other actions.
                    Action::Machine(_) => {}
                    Action::Tier(tier_guard::Action::Unconfirmed(reason)) => {
                        self.unconfirmed(reason, &mut step)
                    }
                    Action::Tier(tier_guard::Action::SetBy { by, confirmed }) => {
                        step.set_by = Some((by, confirmed))
                    }
                    Action::Tier(_) => {}
                }
            }
            self.commit(mem::take(&mut step), command.take()).await;
        }
        let running = self.parts.core.turn_id().is_some();
        self.parts.running.store(running, Ordering::Relaxed);
        let queued = self.parts.core.queued();
        self.parts.queued.store(queued, Ordering::Relaxed);
        self.alarm.follow(&self.timers);
    }

    /// Commits a step's events with its rows and the command's result, then
    /// replies and carries out the rest.
    async fn commit(&mut self, step: Step, command: Option<Command>) {
        let Step {
            events,
            agent_session,
            driver,
            attach,
            check,
            reply,
            set_by,
            audit: records,
        } = step;
        let mut answer = None;
        let mut audit = None;
        let mut completion = None;
        let mut cause = None;
        // A send's reply comes with the step that decides its turn.
        let command = match (command, &reply) {
            (Some(command), None) if command.deferrable => {
                self.deferred = Some(command);
                None
            }
            (None, Some(Reply::Turn(turn::Reply::Send(_)))) => self.deferred.take(),
            (command, _) => command,
        };
        let turn_ended = events
            .iter()
            .any(|b| matches!(b, EventBody::TurnCompleted(_)));
        if let Some(command) = command {
            let outcome = match reply {
                Some(Reply::Turn(reply)) => self.outcome(reply, command.cause.as_ref()),
                Some(Reply::Interaction(reply)) => reply.map(|r| json!(r)).map_err(RpcError::from),
                Some(Reply::Done) => Ok(json!({})),
                None => Err(internal("the command had no reply")),
            };
            audit = command.audit.filter(|_| outcome.is_ok());
            match (&outcome, command.ticket) {
                (Ok(result), Some(ticket)) => {
                    completion = Some(ticket.complete(Ok(result.clone())))
                }
                (Err(err), Some(ticket)) => ticket.reject(err.clone()),
                (_, None) => {}
            }
            cause = command.cause;
            answer = Some((command.reply, outcome));
        }
        let at = self.parts.clock.now_wall_ms();
        let first_seq = self.parts.writer.head() + 1;
        let instance = self.parts.core.instance();
        let (id, commands) = (&self.parts.id, &self.commands);
        let mut changes = Rows::of(id, first_seq, at, &events, commands, instance);
        if let Some(agent_session) = agent_session {
            changes.agent_session(&self.parts.id, at, agent_session);
        }
        if let Some((by, confirmed)) = set_by {
            changes.set_by(&self.parts.id, at, &by, confirmed);
        }
        self.forget_dequeued(&events);
        let asks = events.iter().any(|b| {
            matches!(
                b,
                EventBody::InteractionRequested(_) | EventBody::InteractionResolved(_)
            )
        });
        let mut notes = notes(first_seq, &events);
        let idle = changes.is_empty() && completion.is_none();
        let record = completion.as_ref().map(|c| c.row());
        let with = move |conn: &rusqlite::Connection| {
            changes.write(conn)?;
            record.map_or(Ok(()), |record| record(conn))
        };
        let committed = if events.is_empty() {
            // Rows without events: a transaction of their own, if any.
            let write = (!idle).then(|| self.parts.db.submit(Durability::Normal, with));
            match write {
                Some(write) => write.await.map_err(|e| e.to_string()),
                None => Ok(()),
            }
        } else {
            let batch = Batch {
                bodies: events,
                cause,
                durability: Durability::Normal,
                with: Some(Box::new(with)),
            };
            let appended = self.parts.writer.append(batch).await;
            let rejected = appended.as_ref().map(|a| a.rejected.as_slice());
            if let Ok(rejected @ [_, ..]) = rejected {
                let bug = format!("session {}: the view rejected {rejected:?}", self.parts.id);
                if cfg!(any(test, feature = "test-harness")) {
                    panic!("{bug}");
                }
                tracing::error!("{bug}");
            }
            appended.map(drop).map_err(|e| e.to_string())
        };
        if let (Ok(()), Some(completion)) = (&committed, completion) {
            completion.committed();
        }
        if let Some((reply, outcome)) = answer {
            let outcome = committed.as_ref().map_err(internal).and(outcome);
            let _ = reply.send(outcome);
        }
        if committed.is_err() {
            return;
        }
        for record in audit.into_iter().chain(records) {
            // Nobody listening is the receiver's choice.
            let _ = self.parts.audit.send(record);
        }
        if asks {
            let (count, oldest) = self.view(pending);
            let count = u32::try_from(count).unwrap_or(u32::MAX);
            self.parts.pending.store(count, Ordering::Relaxed);
            notes.push(Note::Pending { count, oldest });
        }
        if !notes.is_empty() {
            self.parts.index.note(&self.parts.id, at, notes);
        }
        for command in driver {
            self.parts.agent.send(command);
        }
        for target in attach {
            self.rescan(Purpose::Attach(target));
        }
        if turn_ended {
            self.rescan(Purpose::TurnEnd);
        }
        if check {
            self.rescan(Purpose::Check);
        }
    }

    /// The reply to a command, as the wire has it.
    fn outcome(&mut self, reply: turn::Reply, cause: Option<&Cause>) -> Outcome {
        let value = match reply {
            turn::Reply::Send(Ok(sent)) => {
                if let (Some(queue_id), Some(cause)) = (&sent.queue_id, cause) {
                    self.commands
                        .insert(queue_id.clone(), cause.command_id.clone());
                }
                json!(SessionSendResult {
                    disposition: match sent.disposition {
                        Disposition::Started => Wire::Started,
                        Disposition::Queued => Wire::Queued,
                    },
                    turn_id: sent.turn_id,
                    queue_id: sent.queue_id,
                    fallback_reason: sent
                        .fallback
                        .map(|SteerFallback::Unsupported| { FallbackReason::Unsupported }),
                })
            }
            turn::Reply::Send(Err(full)) => {
                let err = RpcError::app(ErrorCode::TooLarge, full.to_string());
                return Err(err.with_reason(ErrorReason::QueueFull));
            }
            turn::Reply::Interrupt(Ok(turn_id)) => json!(SessionInterruptResult { turn_id }),
            turn::Reply::Interrupt(Err(mismatch)) => {
                let err = RpcError::app(ErrorCode::Conflict, mismatch.to_string());
                return Err(err.with_reason(ErrorReason::TurnMismatch));
            }
            turn::Reply::QueueResume => json!({}),
            turn::Reply::QueueEdit(_) => return Err(internal("session/queue_edit is M1b")),
        };
        Ok(value)
    }

    /// Messages that left the queue need no command any more.
    fn forget_dequeued(&mut self, events: &[EventBody]) {
        for body in events {
            if let EventBody::QueueUpdated(update) = body {
                let queued: Vec<&QueueId> = update.entries.iter().map(|e| &e.queue_id).collect();
                self.commands.retain(|id, _| queued.contains(&id));
            }
        }
    }

    /// Rescans the project's trust range in a task of its own (design
    /// 5.2.6); the result comes back to [`trust`](Self::trust).
    fn rescan(&self, purpose: Purpose) {
        let (readers, project) = (self.parts.readers.clone(), self.parts.project.clone());
        let (cwd, back) = (self.parts.cwd.clone(), self.rescanned.clone());
        let cx = ScanContext {
            git: self.parts.git.clone(),
            writable: Vec::new(),
            clock: Arc::clone(&self.parts.clock),
            durations: self.parts.durations.clone(),
        };
        tokio::spawn(async move {
            let found = rescan(readers, project, cwd, cx).await;
            // The actor may have stopped.
            let _ = back.send((purpose, found));
        });
    }

    /// A rescan came back. A session that would load the project's
    /// configuration, whose project is no longer trusted, shuts the gate for
    /// `project_config_changed`, with a notice; the index records a project
    /// that became `stale`. A trusted project again reopens it.
    async fn trust(&mut self, purpose: Purpose, found: Rescan) {
        if let Some(scan) = found.scan.clone().filter(|_| found.stale) {
            self.parts.index.rescanned(&self.parts.project, scan);
        }
        if let Some(error) = found.failure.clone().filter(|_| found.scan_failed) {
            let (project_id, change) = (
                self.parts.project.clone(),
                TrustChange::ScanFailed { error },
            );
            let event = AuditEvent::ProjectTrust { project_id, change };
            let at_ms = self.parts.clock.now_wall_ms();
            // Nobody listening is the receiver's choice.
            let _ = self.parts.audit.send(AuditRecord {
                at_ms,
                conn: None,
                event,
            });
        }
        self.load_wanted |= found.trusted && !self.ignore_for_good;
        let blocked = self.load_wanted && !found.trusted;
        let mut step = Step::default();
        if blocked && !self.project_gate {
            let params = NoticeParams {
                reason: found.failure.clone(),
                ..NoticeParams::default()
            };
            let code = NoticeCode::ProjectConfigChanged;
            let (ids, now) = (self.parts.core.ids(), self.parts.clock.now());
            step.events.extend(notice_with(ids, now, code, params));
        }
        let changed = blocked != self.project_gate;
        self.project_gate = blocked;
        let (reason, shut) = (PauseReason::ProjectConfigChanged, blocked);
        let gate = Input::Machine(turn::Input::Gate { reason, shut });
        match purpose {
            Purpose::Check => {
                let checked = Input::Machine(turn::Input::Checked(!blocked));
                self.input(checked, None, step).await;
            }
            Purpose::Attach(target) => {
                let policy = match found.trusted && self.load_wanted {
                    true => ProjectConfigPolicy::Load,
                    false => ProjectConfigPolicy::Ignore,
                };
                if policy != self.parts.project_config {
                    self.parts.project_config = policy;
                    self.parts.configured.project_config = project_config(policy);
                    let configured = self.parts.configured.clone();
                    step.events.push(EventBody::SessionConfigured(configured));
                }
                match changed {
                    true => self.input(gate, None, step).await,
                    false => self.commit(step, None).await,
                }
                self.attach(target);
            }
            Purpose::TurnEnd | Purpose::Reopen if changed => self.input(gate, None, step).await,
            Purpose::TurnEnd | Purpose::Reopen => self.commit(step, None).await,
        }
    }

    /// "不加载项目配置继续" (design 5.2.6): from now on the session leaves the
    /// project's configuration out; the idle agent session is closed, so the
    /// next turn resumes it with `settingSources: ["user"]`.
    async fn ignore_project_config(&mut self) {
        (self.load_wanted, self.ignore_for_good) = (false, true);
        self.parts.project_config = ProjectConfigPolicy::Ignore;
        let mut step = Step::default();
        if self.parts.configured.project_config != ProjectConfig::Ignored {
            self.parts.configured.project_config = ProjectConfig::Ignored;
            let configured = self.parts.configured.clone();
            step.events.push(EventBody::SessionConfigured(configured));
        }
        self.input(Input::Machine(turn::Input::Detach), None, step)
            .await;
        if mem::take(&mut self.project_gate) {
            let (reason, shut) = (PauseReason::ProjectConfigChanged, false);
            let gate = Input::Machine(turn::Input::Gate { reason, shut });
            self.input(gate, None, Step::default()).await;
        }
    }

    fn attach(&self, target: AttachTarget) {
        let attach = Attach {
            target,
            cwd: self.parts.cwd.clone(),
            config: self.parts.config.clone(),
            project_config: self.parts.project_config,
        };
        let reply = self.parts.agent.attach(attach, self.events.clone());
        let back = self.attached.clone();
        tokio::spawn(async move {
            // The actor may have stopped.
            let _ = back.send(reply.await);
        });
    }

    /// An attach came back: its capabilities and configuration go into the
    /// stream when they changed, the agent's session into the row.
    async fn attach_done(&mut self, result: Result<AttachInfo, AgentError>) {
        let mut step = Step::default();
        let mut mode = None;
        if let Ok(info) = &result {
            let caps = &info.capabilities;
            self.parts
                .core
                .set_stops_commands(caps.turn.interrupt_stops_commands);
            if self.written_capabilities().as_ref() != Some(caps) {
                step.events
                    .push(EventBody::SessionCapabilities(caps.clone()));
            }
            let config = caps.config.iter();
            let config = config
                .map(|o| (o.id.to_string(), o.current.clone()))
                .collect();
            if config != self.parts.configured.config {
                self.parts.configured.config = config;
                let configured = self.parts.configured.clone();
                step.events.push(EventBody::SessionConfigured(configured));
            }
            step.agent_session = Some(info.session.0.clone());
            mode = mode_tier(&caps.config);
        }
        // The tier is checked before the turn may send its prompt.
        if let Some((mode, tier)) = mode {
            let during_turn = false;
            let mode = tier_guard::Input::Mode {
                mode,
                tier,
                during_turn,
            };
            self.input(Input::Tier(mode), None, mem::take(&mut step))
                .await;
        }
        let input = Input::Machine(turn::Input::Attached(result));
        self.input(input, None, step).await;
    }

    /// A command list the session's capabilities do not have yet goes into
    /// the stream with them (design 5.6.2).
    fn commands_changed(&self, event: &DriverEvent) -> Step {
        let mut step = Step::default();
        if let DriverEvent::CommandsChanged(commands) = event
            && let Some(mut caps) = self.written_capabilities()
            && caps.commands != *commands
        {
            caps.commands.clone_from(commands);
            step.events.push(EventBody::SessionCapabilities(caps));
        }
        step
    }

    /// An event of the agent. A change of its options rewrites the
    /// configuration, and the guard checks the permission mode.
    async fn agent(&mut self, event: DriverEvent) {
        let DriverEvent::ConfigChanged(options) = event else {
            let step = self.commands_changed(&event);
            return self.input(Input::Agent(event), None, step).await;
        };
        let mut step = Step::default();
        let config = options
            .iter()
            .map(|o| (o.id.to_string(), o.current.clone()));
        let config: BTreeMap<String, String> = config.collect();
        if config != self.parts.configured.config {
            self.parts.configured.config = config;
            let configured = self.parts.configured.clone();
            step.events.push(EventBody::SessionConfigured(configured));
        }
        match mode_tier(&options) {
            Some((mode, tier)) => {
                let during_turn = true;
                let mode = tier_guard::Input::Mode {
                    mode,
                    tier,
                    during_turn,
                };
                self.input(Input::Tier(mode), None, step).await
            }
            None => self.commit(step, None).await,
        }
    }

    /// The session enters `config_unconfirmed`, or leaves it: the
    /// configuration says so, and the audit records which, with the actual
    /// tier and its setter (the one this step set, when it did).
    fn unconfirmed(&mut self, reason: Option<ConfirmationReason>, step: &mut Step) {
        let required = reason.clone().map(|reason| ConfirmationRequired {
            reason,
            config_digest: config_digest(&self.parts.configured),
        });
        let left = mem::replace(&mut self.parts.configured.confirmation_required, required);
        let configured = self.parts.configured.clone();
        step.events.push(EventBody::SessionConfigured(configured));
        let set_by = step.set_by.as_ref().map(|(by, _)| by);
        let config = ConfigState {
            session_id: self.parts.id.clone(),
            mode: (self.parts.configured.config)
                .get(ConfigOptionId::PermissionMode.as_str())
                .cloned(),
            tier: self.tier(),
            set_by: set_by.unwrap_or(self.parts.core.setter()).clone(),
        };
        let event = match (reason, left) {
            (Some(reason), _) => AuditEvent::ConfigUnconfirmed { reason, config },
            (None, Some(left)) => AuditEvent::ConfigConfirmed {
                reason: left.reason,
                config,
            },
            (None, None) => return,
        };
        let at_ms = self.parts.clock.now_wall_ms();
        step.audit.push(AuditRecord {
            at_ms,
            conn: None,
            event,
        });
    }

    /// Feeds each timer due now to the core, one step each: a step may
    /// cancel the next.
    async fn expired(&mut self) {
        let now = self.parts.clock.now_mono();
        while let Some(key) = self.timers.pop_due(now) {
            self.input(Input::Expired(key), None, Step::default()).await;
        }
        self.alarm.follow(&self.timers);
    }

    fn written_capabilities(&self) -> Option<Capabilities> {
        self.view(|view| view.capabilities.clone())
    }

    fn capabilities(&self) -> Option<Capabilities> {
        self.written_capabilities()
            .or_else(|| self.parts.cached.clone())
    }

    fn view<T>(&self, read: impl FnOnce(&SessionView) -> T) -> T {
        self.parts.stream.read(|view, _| match view {
            StreamView::Session(view) => read(view),
            StreamView::Index(_) => unreachable!("a session's stream"),
        })
    }

    /// `session/get`: the row, and the state its view and turn tell.
    async fn get(&self) -> Outcome {
        let id = self.parts.id.to_string();
        let row = self.parts.readers.read(move |conn| {
            conn.query_row(
                "SELECT project_id, title, title_source, read_only, last_read_seq,
                     last_turn_end_seq, updated_at, archived_at
                 FROM sessions WHERE session_id = ?1",
                [id],
                |r| {
                    let text = |i| r.get::<_, String>(i);
                    let n = |i| r.get::<_, u64>(i);
                    Ok((
                        text(0)?,
                        text(1)?,
                        text(2)?,
                        n(3)? != 0,
                        n(4)?,
                        n(5)?,
                        n(6)?,
                        r.get::<_, Option<u64>>(7)?,
                    ))
                },
            )
            .optional()
        });
        let row = row.await.map_err(internal)?;
        let (project, title, title_source, read_only, read, ended, active, archived) =
            row.ok_or_else(|| not_found("session"))?;
        let (pending, last_failed) = self.view(|view| {
            let last = view.turns.iter().rev().find_map(|t| t.completed.as_ref());
            let failed = last.filter(|t| t.status == TurnStatus::Failed);
            (pending(view), failed.and_then(|t| t.error_code.clone()))
        });
        let unread = ended > read;
        let status = if pending.0 > 0 {
            SessionStatus::NeedsAction
        } else if self.parts.core.turn_id().is_some() {
            SessionStatus::Running
        } else if unread && last_failed.is_some() {
            SessionStatus::Error
        } else if unread {
            SessionStatus::Unread
        } else {
            SessionStatus::Idle
        };
        let configured = &self.parts.configured;
        let summary = SessionSummary {
            session_id: self.parts.id.clone(),
            project_id: project.parse().map_err(internal)?,
            agent: configured.agent.clone(),
            title,
            title_source: title_source.as_str().into(),
            workspace_kind: configured.workspace.kind.clone(),
            branch: configured.workspace.branch.clone(),
            error_code: last_failed.filter(|_| status == SessionStatus::Error),
            status,
            unread,
            pending_interactions: u32::try_from(pending.0).unwrap_or(u32::MAX),
            pending_kind: pending.1.map(|(kind, _)| kind),
            last_activity_at: TimestampMs(active),
            read_only,
        };
        let result = SessionGetResult {
            summary,
            workspace: configured.workspace.clone(),
            archived_at: archived.map(TimestampMs),
        };
        Ok(json!(result))
    }
}

/// The session's pending requests, and the class and ID of the oldest.
fn pending(view: &SessionView) -> (usize, Option<(PendingKind, RequestId)>) {
    let mut pending: Vec<_> = view.requests.values().filter(|r| r.is_pending()).collect();
    pending.sort_by_key(|r| r.requested.created_at.0);
    let oldest = pending.first().map(|r| {
        let kind = match r.requested.request {
            InteractionRequest::Question(_) | InteractionRequest::AuthUrl(_) => PendingKind::Input,
            _ => PendingKind::Approval,
        };
        (kind, r.requested.request_id.clone())
    });
    (pending.len(), oldest)
}

/// What the index hears of `bodies`, committed from seq `first_seq` on.
fn notes(first_seq: u64, bodies: &[EventBody]) -> Vec<Note> {
    let notes = (first_seq..)
        .zip(bodies)
        .filter_map(|(seq, body)| match body {
            EventBody::TurnStarted(_) => Some(Note::TurnStarted),
            EventBody::TurnCompleted(t) => Some(Note::TurnEnded {
                seq,
                status: t.status.clone(),
                error: t.error_code.clone(),
            }),
            EventBody::WorkspaceReady(ready) => Some(Note::Branch(ready.branch.clone())),
            EventBody::SessionConfigured(c) => Some(Note::ReadOnly(c.read_only)),
            _ => None,
        });
    notes.collect()
}

/// The rescan of the project's trust range, which fails closed: a project
/// row, a workspace, a git root or a scan that cannot be had counts as
/// untrusted, with the reason. Configuration may not come from any
/// project's directory (the repo tier, design 5.2.10).
async fn rescan(readers: Readers, project: ProjectId, cwd: PathBuf, mut cx: ScanContext) -> Rescan {
    let fail = |code: &str| Rescan {
        trusted: false,
        stale: false,
        failure: Some(code.into()),
        scan_failed: false,
        scan: None,
    };
    let scan_failed = |code: &str| Rescan {
        scan_failed: true,
        ..fail(code)
    };
    let row = readers.read(move |conn| {
        let trust = conn
            .query_row(
                "SELECT trust_state, trust_digest FROM projects WHERE project_id = ?1",
                [project.to_string()],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
            )
            .optional()?;
        let mut select = conn.prepare("SELECT path FROM projects WHERE removed_at IS NULL")?;
        let roots = select.query_map([], |r| r.get::<_, String>(0))?;
        Ok((trust, roots.collect::<rusqlite::Result<Vec<String>>>()?))
    });
    let Ok((Some((state, digest)), roots)) = row.await else {
        return fail("project_missing");
    };
    cx.writable = roots.into_iter().map(PathBuf::from).collect();
    let Some(path) = std::fs::canonicalize(&cwd).ok().filter(|p| p.is_dir()) else {
        return fail("workspace_missing");
    };
    let root = match git_of(&path, &cx).await {
        Ok(git) => git.map(|(root, _)| root),
        Err(err) => {
            let reason = err.data.as_ref().and_then(|data| data.reason.as_ref());
            return fail(reason.map_or("git_error", |reason| reason.as_str()));
        }
    };
    let scan = tokio::task::spawn_blocking(move || trust::scan(&path, root.as_deref())).await;
    let scan = match scan {
        Ok(Ok(scan)) => scan,
        Ok(Err(err)) => return scan_failed(err.code()),
        Err(_) => return scan_failed("scan_failed"),
    };
    let digest = digest.and_then(|digest| digest.parse().ok());
    let before = TrustState::from(state.as_str());
    let state = trust::state_after(&before, digest.as_ref(), &scan.digest);
    Rescan {
        trusted: state == TrustState::Trusted,
        stale: before == TrustState::Trusted && state == TrustState::Stale,
        failure: None,
        scan_failed: false,
        scan: Some(scan),
    }
}

fn project_config(policy: ProjectConfigPolicy) -> ProjectConfig {
    match policy {
        ProjectConfigPolicy::Load => ProjectConfig::Loaded,
        ProjectConfigPolicy::Ignore => ProjectConfig::Ignored,
    }
}

/// What `session/confirm_config` echoes back: the digest of the options.
fn config_digest(configured: &SessionConfigured) -> Digest {
    canonical::digest(&configured.config).expect("a map of strings has a canonical form")
}

/// The snapshot port until T061: every snapshot is `skipped`, without a
/// reason (design 5.2.6, rulings before T043).
fn skipped(turn_id: TurnId, phase: SnapshotPhase) -> Input {
    Input::Machine(turn::Input::Snapshot(SnapshotRecorded {
        turn_id,
        phase,
        status: SnapshotStatus::Skipped,
        reason: None,
        excluded: Vec::new(),
        stat: None,
    }))
}
