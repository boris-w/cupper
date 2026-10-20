//! `SessionActor` (design 5.2.3, 5.2.6, 5.2.16): the adapter that runs a
//! session's [`SessionCore`] on its mailbox, the agent's events, the clock
//! and the store.
//!
//! Every input is a step of the core. A step's events commit as one batch,
//! with the rows they change and, for a command, the ledger's record of its
//! result (design 5.2.3, rule 3); only then does the reply go out, and the
//! step's driver commands and attaches. An attach runs in a task of its own
//! and comes back as an input; so does a snapshot, which is `skipped` until
//! T061 brings git snapshots (plan 1.3). The index hears of the requests as
//! they appear and resolve; the audit channel of answers from the CLI and the
//! test harness (design 5.8.6).

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::mem;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cupper_protocol::annotations::TimestampMs;
use cupper_protocol::canonical;
use cupper_protocol::capabilities::{ConfigOptionId, SessionCapabilities as Capabilities};
use cupper_protocol::domain::{PrincipalKind, ResolvedBy, Tier};
use cupper_protocol::envelope::Cause;
use cupper_protocol::error::{ErrorCode, ErrorReason, INVALID_PARAMS, RpcError};
use cupper_protocol::event::EventBody;
use cupper_protocol::ids::{CommandId, PrincipalId, QueueId, SessionId, TurnId};
use cupper_protocol::ids::{Digest, RequestId};
use cupper_protocol::index::{PendingKind, SessionStatus, SessionSummary};
use cupper_protocol::interaction::InteractionOutcome;
use cupper_protocol::interaction::InteractionRequest;
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
    ConfirmationReason, ConfirmationRequired, SessionConfigured, SnapshotPhase, SnapshotRecorded,
    SnapshotStatus, TurnStatus,
};
use cupper_protocol::view::SessionView;
use rusqlite::OptionalExtension;
use serde_json::json;
use tokio::sync::{mpsc, oneshot};

use super::core::{Action, Input, Key, SessionCore};
use super::rows::Rows;
use super::{IndexNotes, internal, mode_tier, not_found, parse, tier_of};
use crate::clock::Clock;
use crate::driver::api::{
    AgentError, AgentPort, Attach, AttachInfo, AttachTarget, DriverCommand, DriverEvent,
    ProjectConfigPolicy,
};
use crate::gateway::{AuditEvent, AuditRecord, Call};
use crate::ledger::{Outcome, Ticket};
use crate::orchestrator::interaction::{self, Respond};
use crate::orchestrator::tier_guard;
use crate::orchestrator::turn::{self, Disposition, SendMode, SendRequest, SteerFallback};
use crate::policy;
use crate::pure::{Alarm, Core, TimerTable};
use crate::quiescence::Participant;
use crate::store::streams::{Batch, StreamHandle, StreamView, StreamWriter};
use crate::store::{Durability, Readers, Writer};

/// A call and where its reply goes.
pub(super) type Mail = (Call, oneshot::Sender<Outcome>);

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
    pub audit: mpsc::UnboundedSender<AuditRecord>,
    pub index: Arc<dyn IndexNotes>,
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
}

/// A command on its way: its ledger ticket, its reply, and what the audit
/// records once it succeeds.
struct Command {
    ticket: Option<Ticket>,
    cause: Option<Cause>,
    reply: oneshot::Sender<Outcome>,
    audit: Option<AuditRecord>,
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
        first: SendRequest,
        _participant: Participant,
    ) {
        let (events, mut agent) = mpsc::unbounded_channel();
        let (attached, mut attaches) = mpsc::unbounded_channel();
        let alarm = Alarm::new(Arc::clone(&parts.clock));
        let mut actor = Self {
            parts,
            timers: TimerTable::new(),
            alarm,
            commands: HashMap::new(),
            inbox,
            events,
            attached,
        };
        let first = Input::Machine(turn::Input::Send(first));
        actor.input(first, None, Step::default()).await;
        // A branch's handler runs once the other branches are dropped.
        loop {
            tokio::select! {
                mail = actor.inbox.recv() => match mail {
                    Some((call, reply)) => actor.call(call, reply).await,
                    None => break,
                },
                Some(event) = agent.recv() => actor.agent(event).await,
                Some(result) = attaches.recv() => actor.attach_done(result).await,
                () = actor.alarm.rung() => actor.expired().await,
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
        let command = Command {
            ticket,
            cause,
            reply,
            audit,
        };
        self.input(input, Some(command), step).await;
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
            reply,
            set_by,
            audit: records,
        } = step;
        let mut answer = None;
        let mut audit = None;
        let mut completion = None;
        let mut cause = None;
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
            self.parts.index.pending(&self.parts.id, count, oldest);
        }
        for command in driver {
            self.parts.agent.send(command);
        }
        for target in attach {
            self.attach(target);
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
        // The tier is checked before the turn may send its prompt.
        let mode = result
            .as_ref()
            .ok()
            .and_then(|info| mode_tier(&info.capabilities.config));
        if let Some((mode, tier)) = mode {
            let during_turn = false;
            let mode = tier_guard::Input::Mode {
                mode,
                tier,
                during_turn,
            };
            self.input(Input::Tier(mode), None, Step::default()).await;
        }
        let mut step = Step::default();
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
        }
        let input = Input::Machine(turn::Input::Attached(result));
        self.input(input, None, step).await;
    }

    /// An event of the agent. A change of its options rewrites the
    /// configuration, and the guard checks the permission mode.
    async fn agent(&mut self, event: DriverEvent) {
        let DriverEvent::ConfigChanged(options) = event else {
            return self.input(Input::Agent(event), None, Step::default()).await;
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
    /// configuration says so, and the audit records the entry.
    fn unconfirmed(&mut self, reason: Option<ConfirmationReason>, step: &mut Step) {
        let required = reason.clone().map(|reason| ConfirmationRequired {
            reason,
            config_digest: config_digest(&self.parts.configured),
        });
        self.parts.configured.confirmation_required = required;
        let configured = self.parts.configured.clone();
        step.events.push(EventBody::SessionConfigured(configured));
        if let Some(reason) = reason {
            let mode = self
                .parts
                .configured
                .config
                .get(ConfigOptionId::PermissionMode.as_str());
            let event = AuditEvent::ConfigUnconfirmed {
                session_id: self.parts.id.clone(),
                reason,
                mode: mode.cloned(),
            };
            let at_ms = self.parts.clock.now_wall_ms();
            step.audit.push(AuditRecord {
                at_ms,
                conn: None,
                event,
            });
        }
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
            pending_kind: pending.1,
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

/// The session's pending requests, and the class of the oldest.
fn pending(view: &SessionView) -> (usize, Option<PendingKind>) {
    let mut pending: Vec<_> = view.requests.values().filter(|r| r.is_pending()).collect();
    pending.sort_by_key(|r| r.requested.created_at.0);
    let kind = pending.first().map(|r| match r.requested.request {
        InteractionRequest::Question(_) | InteractionRequest::AuthUrl(_) => PendingKind::Input,
        _ => PendingKind::Approval,
    });
    (pending.len(), kind)
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
