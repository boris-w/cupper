//! One actor per session. It owns the adapter driver, the running turn and the
//! pending interaction requests, and is the only writer of its session stream.
//! Commands and driver events are processed one at a time, which is what makes
//! "first answer wins" a plain check instead of a race.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cupper_protocol::rpc::{ErrorCode, RpcError};
use cupper_protocol::sync::StreamId;
use cupper_protocol::text::{MAX_DELTA_ESCAPED, MAX_LABEL_CHARS, split_escaped, truncate_chars};
use cupper_protocol::{
    BlobRef, Cause, EventBody, InteractionOutcome, InteractionRequest, InteractionRequested, InteractionResolved,
    InteractionStatus, ItemCompleted, ItemDelta, ItemKind, ItemStarted, ItemStatus, PermissionOption, PermissionOptionKind,
    PrincipalKind, SessionInterruptResult, SessionSendResult, SessionStatus, TurnCompleted, TurnStarted, TurnStatus,
};
use tokio::sync::{mpsc, oneshot};

use crate::auth::Principal;
use crate::driver::{Driver, DriverEvent, PermissionAnswer, PermissionAsk, QuestionAnswer, QuestionAsk};
use crate::host::Host;
use crate::normalize::{BlobSink, Normalizer};
use crate::store::{AdapterRecord, SessionRecord};
use crate::util::{canonical_hash, new_id};

type Reply<T> = oneshot::Sender<Result<T, RpcError>>;

pub enum SessionMsg {
    /// Start the adapter and create the agent-side session.
    Start { reply: Reply<()> },
    Send { text: String, expected_turn_id: Option<String>, cause: Cause, reply: Reply<SessionSendResult> },
    Interrupt { expected_turn_id: Option<String>, cause: Cause, reply: Reply<SessionInterruptResult> },
    Respond { request_id: String, digest: String, outcome: InteractionOutcome, principal: Principal, cause: Cause, reply: Reply<()> },
    Shutdown,
}

#[derive(Clone, Debug)]
pub struct SessionHandle {
    tx: mpsc::UnboundedSender<SessionMsg>,
}

impl SessionHandle {
    pub async fn call<T>(&self, make: impl FnOnce(Reply<T>) -> SessionMsg) -> Result<T, RpcError> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(make(tx)).map_err(|_| RpcError::app(ErrorCode::Unavailable, "session actor stopped"))?;
        rx.await.map_err(|_| RpcError::app(ErrorCode::Internal, "session actor dropped the request"))?
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(SessionMsg::Shutdown);
    }
}

enum Pending {
    Permission { digest: String, options: Vec<PermissionOption>, reply: oneshot::Sender<PermissionAnswer> },
    Question { digest: String, reply: oneshot::Sender<QuestionAnswer> },
}

impl Pending {
    fn digest(&self) -> &str {
        match self {
            Pending::Permission { digest, .. } | Pending::Question { digest, .. } => digest,
        }
    }

    fn cancel(self) {
        match self {
            Pending::Permission { reply, .. } => drop(reply.send(PermissionAnswer::Cancelled)),
            Pending::Question { reply, .. } => drop(reply.send(QuestionAnswer::Declined)),
        }
    }
}

struct Turn {
    id: String,
    normalizer: Normalizer,
    cause: Option<Cause>,
}

struct StoreBlobs<'a>(&'a crate::store::Store);

impl BlobSink for StoreBlobs<'_> {
    fn put(&mut self, bytes: &[u8]) -> BlobRef {
        match self.0.put_blob(bytes) {
            Ok(hash) => BlobRef { hash, size: bytes.len() as u64, media_type: Some("text/plain".into()) },
            Err(e) => {
                tracing::error!("storing blob failed: {e}");
                BlobRef { hash: String::new(), size: 0, media_type: None }
            }
        }
    }
}

/// Principal kinds that may answer interaction requests.
const ACTIONABLE_BY: &[PrincipalKind] = &[PrincipalKind::DesktopUi, PrincipalKind::TestHarness];

pub struct SessionActor {
    host: Arc<Host>,
    rec: SessionRecord,
    stream: StreamId,
    driver: Option<Driver>,
    events_tx: mpsc::UnboundedSender<DriverEvent>,
    events_rx: mpsc::UnboundedReceiver<DriverEvent>,
    rx: mpsc::UnboundedReceiver<SessionMsg>,
    turn: Option<Turn>,
    pending: HashMap<String, Pending>,
    base_status: SessionStatus,
    window: Duration,
}

impl SessionActor {
    pub fn spawn(host: Arc<Host>, rec: SessionRecord, status: SessionStatus) -> SessionHandle {
        let (tx, rx) = mpsc::unbounded_channel();
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        let window = host.config.write_coalesce;
        let actor = SessionActor {
            stream: StreamId::Session(rec.session_id.clone()),
            host,
            rec,
            driver: None,
            events_tx,
            events_rx,
            rx,
            turn: None,
            pending: HashMap::new(),
            base_status: status,
            window,
        };
        tokio::spawn(actor.run());
        SessionHandle { tx }
    }

    async fn run(mut self) {
        loop {
            let deadline = self.turn.as_ref().and_then(|t| t.normalizer.deadline());
            tokio::select! {
                msg = self.rx.recv() => match msg {
                    Some(SessionMsg::Shutdown) | None => break,
                    Some(m) => self.on_msg(m).await,
                },
                Some(ev) = self.events_rx.recv() => self.on_driver(ev),
                _ = sleep_until(deadline), if deadline.is_some() => {
                    let bodies = self.turn.as_mut().map(|t| t.normalizer.flush()).unwrap_or_default();
                    self.log(bodies);
                }
            }
        }
        if let Some(d) = self.driver.take() {
            d.shutdown();
        }
    }

    fn cause(&self) -> Option<Cause> {
        self.turn.as_ref().and_then(|t| t.cause.clone())
    }

    fn log(&self, bodies: Vec<EventBody>) {
        self.log_with(bodies, self.cause().as_ref());
    }

    fn log_with(&self, bodies: Vec<EventBody>, cause: Option<&Cause>) {
        if let Err(e) = self.host.hub.append(&self.stream, bodies, cause) {
            tracing::error!(session = %self.rec.session_id, "appending events failed: {e}");
        }
    }

    fn status(&self) -> SessionStatus {
        if !self.pending.is_empty() {
            SessionStatus::AwaitingInput
        } else if self.turn.is_some() {
            SessionStatus::Running
        } else {
            self.base_status.clone()
        }
    }

    fn publish(&self) {
        self.host.publish_session(&self.rec, self.status(), self.pending.len() as u32);
    }

    async fn on_msg(&mut self, msg: SessionMsg) {
        match msg {
            SessionMsg::Start { reply } => {
                let r = self.ensure_driver().await;
                self.base_status = if r.is_ok() { SessionStatus::Idle } else { SessionStatus::Error };
                self.publish();
                let _ = reply.send(r);
            }
            SessionMsg::Send { text, expected_turn_id, cause, reply } => {
                let _ = reply.send(self.send(text, expected_turn_id, cause).await);
            }
            SessionMsg::Interrupt { expected_turn_id, cause, reply } => {
                let _ = reply.send(self.interrupt(expected_turn_id, cause));
            }
            SessionMsg::Respond { request_id, digest, outcome, principal, cause, reply } => {
                let _ = reply.send(self.respond(request_id, digest, outcome, &principal, cause));
            }
            SessionMsg::Shutdown => {}
        }
    }

    async fn ensure_driver(&mut self) -> Result<(), RpcError> {
        if self.driver.is_some() {
            return Ok(());
        }
        let unavailable = |reason: &str, e: &dyn std::fmt::Display| {
            RpcError::app(ErrorCode::Unavailable, format!("adapter: {e}")).with_reason(reason).with_extra("retryable", true.into())
        };
        let spec = self.host.config.agents.get(&self.rec.agent).ok_or_else(|| {
            RpcError::app(ErrorCode::NotFound, format!("unknown agent {}", self.rec.agent)).with_reason("unknown_agent")
        })?;
        let log = self.host.config.data_dir.join("logs").join(format!("adapter-{}.log", self.rec.session_id));
        let store = self.host.store.clone();
        let session_id = self.rec.session_id.clone();
        let record = |pid, start_us| {
            if let Err(e) = store.record_adapter(&AdapterRecord { pid, session_id, start_us }) {
                tracing::error!("recording adapter failed: {e}");
            }
        };
        let driver = Driver::start(spec, &log, self.events_tx.clone(), record).await.map_err(|e| unavailable("adapter_crashed", &e))?;
        let cwd = PathBuf::from(&self.rec.cwd);
        let ready = match self.rec.agent_session_id.clone() {
            None => match driver.new_session(&cwd).await {
                Ok(agent_sid) => {
                    if let Err(e) = store.set_agent_session_id(&self.rec.session_id, &agent_sid) {
                        tracing::error!("saving agent session id failed: {e}");
                    }
                    self.rec.agent_session_id = Some(agent_sid);
                    Ok(())
                }
                Err(e) => Err(unavailable("adapter_crashed", &e)),
            },
            Some(_) if !driver.caps.resume => {
                Err(RpcError::app(ErrorCode::Unsupported, "agent cannot resume sessions").with_extra("capability", "session.resume".into()))
            }
            Some(agent_sid) => {
                tracing::info!(session = %self.rec.session_id, "resuming agent session");
                driver.resume(&agent_sid, &cwd).await.map_err(|e| unavailable("adapter_crashed", &e))
            }
        };
        match ready {
            Ok(()) => {
                self.driver = Some(driver);
                Ok(())
            }
            Err(e) => {
                driver.shutdown();
                Err(e)
            }
        }
    }

    async fn send(&mut self, text: String, expected_turn_id: Option<String>, cause: Cause) -> Result<SessionSendResult, RpcError> {
        if let Some(t) = &self.turn {
            return Err(match expected_turn_id {
                Some(_) => RpcError::app(ErrorCode::Unsupported, "steering is not implemented").with_extra("capability", "session.steer".into()),
                None => RpcError::app(ErrorCode::Conflict, "a turn is running").with_reason("turn_running").with_extra("turn_id", t.id.clone().into()),
            });
        }
        if expected_turn_id.is_some() {
            return Err(RpcError::app(ErrorCode::Conflict, "no turn is running").with_reason("turn_mismatch"));
        }
        self.ensure_driver().await?;
        let turn_id = new_id("t");
        let item_id = new_id("i");
        let mut bodies = vec![
            EventBody::TurnStarted(TurnStarted { turn_id: turn_id.clone() }),
            EventBody::ItemStarted(ItemStarted { item_id: item_id.clone(), turn_id: turn_id.clone(), kind: ItemKind::UserMessage, title: None, tool_kind: None }),
        ];
        bodies.extend(split_escaped(&text, MAX_DELTA_ESCAPED).into_iter().map(|p| EventBody::ItemDelta(ItemDelta { item_id: item_id.clone(), text: p.to_owned() })));
        bodies.push(EventBody::ItemCompleted(ItemCompleted { item_id, status: ItemStatus::Completed }));
        self.log_with(bodies, Some(&cause));
        let driver = self.driver.as_ref().expect("ensured above");
        let agent_sid = self.rec.agent_session_id.clone().expect("set by ensure_driver");
        self.turn = Some(Turn { id: turn_id.clone(), normalizer: Normalizer::new(turn_id.clone(), self.window), cause: Some(cause) });
        if let Err(e) = self.host.store.set_running(&self.rec.session_id, true) {
            tracing::error!("marking session running failed: {e}");
        }
        if let Err(e) = driver.prompt(&agent_sid, &text) {
            self.finish_turn(TurnStatus::Failed, Some(format!("driver: {e}")));
        } else {
            self.publish();
        }
        Ok(SessionSendResult { turn_id, replayed: false })
    }

    fn interrupt(&mut self, expected_turn_id: Option<String>, cause: Cause) -> Result<SessionInterruptResult, RpcError> {
        let Some(turn) = &self.turn else {
            return Ok(SessionInterruptResult { turn_id: None, replayed: false });
        };
        if expected_turn_id.as_ref().is_some_and(|t| *t != turn.id) {
            return Err(RpcError::app(ErrorCode::Conflict, "turn mismatch").with_reason("turn_mismatch"));
        }
        let turn_id = turn.id.clone();
        if let (Some(d), Some(sid)) = (&self.driver, &self.rec.agent_session_id) {
            let _ = d.cancel(sid);
        }
        self.cancel_pending(InteractionStatus::Cancelled, Some(&cause));
        self.publish();
        Ok(SessionInterruptResult { turn_id: Some(turn_id), replayed: false })
    }

    fn cancel_pending(&mut self, status: InteractionStatus, cause: Option<&Cause>) {
        let mut ids: Vec<String> = self.pending.keys().cloned().collect();
        ids.sort();
        let mut bodies = Vec::new();
        for id in ids {
            if let Some(p) = self.pending.remove(&id) {
                p.cancel();
                bodies.push(EventBody::InteractionResolved(InteractionResolved { request_id: id, status: status.clone(), outcome: None, resolved_by: None }));
            }
        }
        self.log_with(bodies, cause);
    }

    fn respond(&mut self, request_id: String, digest: String, outcome: InteractionOutcome, principal: &Principal, cause: Cause) -> Result<(), RpcError> {
        let Some(pending) = self.pending.get(&request_id) else {
            // Not pending: already answered, expired, cancelled, or unknown.
            let known = self.host.hub.with_view(&self.stream, |v, _| match v {
                cupper_protocol::view::StreamView::Session(s) => s.interaction(&request_id).map(|i| (i.status.clone(), i.resolved_by.clone())),
                _ => None,
            })?;
            return Err(match known {
                Some((InteractionStatus::Expired, _)) => RpcError::app(ErrorCode::Expired, "interaction expired"),
                Some((status, resolved_by)) => RpcError::app(ErrorCode::Conflict, "interaction already handled")
                    .with_reason("already_resolved")
                    .with_extra("status", status.as_str().into())
                    .with_extra("resolved_by", resolved_by.map_or(serde_json::Value::Null, Into::into)),
                None => RpcError::app(ErrorCode::NotFound, "no such interaction"),
            });
        };
        if pending.digest() != digest {
            return Err(RpcError::app(ErrorCode::Conflict, "request digest does not match").with_reason("digest_mismatch"));
        }
        if !ACTIONABLE_BY.contains(&principal.kind) {
            return Err(RpcError::app(ErrorCode::Forbidden, "principal may not answer interactions"));
        }
        match (pending, &outcome) {
            (Pending::Permission { options, .. }, InteractionOutcome::Selected { option_id }) => {
                let opt = options.iter().find(|o| o.option_id == *option_id).ok_or_else(|| {
                    RpcError::new(cupper_protocol::rpc::INVALID_PARAMS, format!("unknown option {option_id}"))
                })?;
                if opt.kind == PermissionOptionKind::AllowAlways && !principal.may_allow_always() {
                    return Err(RpcError::app(ErrorCode::Forbidden, "only the desktop UI may choose allow_always").with_reason("allow_always"));
                }
            }
            (Pending::Question { .. }, InteractionOutcome::Answer { .. }) | (_, InteractionOutcome::Dismissed) => {}
            _ => return Err(RpcError::new(cupper_protocol::rpc::INVALID_PARAMS, "outcome does not fit this request")),
        }
        match (self.pending.remove(&request_id).expect("checked"), &outcome) {
            (Pending::Permission { reply, .. }, InteractionOutcome::Selected { option_id }) => drop(reply.send(PermissionAnswer::Selected(option_id.clone()))),
            (Pending::Question { reply, .. }, InteractionOutcome::Answer { text }) => drop(reply.send(QuestionAnswer::Text(text.clone()))),
            (p, _) => p.cancel(),
        }
        self.log_with(
            vec![EventBody::InteractionResolved(InteractionResolved {
                request_id,
                status: InteractionStatus::Resolved,
                outcome: Some(outcome),
                resolved_by: Some(principal.id.clone()),
            })],
            Some(&cause),
        );
        self.publish();
        Ok(())
    }

    fn on_driver(&mut self, ev: DriverEvent) {
        match ev {
            DriverEvent::Update(u) => match self.turn.as_mut() {
                Some(t) => {
                    let bodies = t.normalizer.on_update(u, Instant::now(), &mut StoreBlobs(&self.host.store));
                    self.log(bodies);
                }
                None => {
                    // Out-of-turn output (e.g. command lists after session/new) is only noted.
                    let kind = match &u {
                        crate::driver::NormUpdate::Other { kind } => kind.clone(),
                        other => format!("out_of_turn:{}", variant_name(other)),
                    };
                    self.log(vec![EventBody::AgentNotice(cupper_protocol::AgentNotice { kind, summary: None })]);
                }
            },
            DriverEvent::Permission(ask, reply) => self.on_permission(ask, reply),
            DriverEvent::Question(ask, reply) => self.on_question(ask, reply),
            DriverEvent::PromptDone { stop_reason } => {
                let status = if stop_reason == "cancelled" { TurnStatus::Cancelled } else { TurnStatus::Completed };
                self.finish_turn(status, Some(stop_reason));
            }
            DriverEvent::PromptFailed { message } => self.finish_turn(TurnStatus::Failed, Some(truncate_chars(&message, MAX_LABEL_CHARS))),
            DriverEvent::Exited(pid) => {
                if let Err(e) = self.host.store.remove_adapter(pid) {
                    tracing::error!("removing adapter record failed: {e}");
                }
                if self.driver.as_ref().is_some_and(|d| d.pid == pid) {
                    self.driver = None;
                    if self.turn.is_some() {
                        self.finish_turn(TurnStatus::Failed, Some("adapter_exited".into()));
                    }
                }
            }
        }
    }

    fn register(&mut self, request: InteractionRequest, item_id: Option<String>, pending: impl FnOnce(String) -> Pending) {
        let Some(turn) = self.turn.as_mut() else {
            // The protocol only allows interactions inside a turn; refuse anything else.
            pending(String::new()).cancel();
            return;
        };
        let request_id = new_id("r");
        let digest = format!(
            "d_{}",
            &canonical_hash(&serde_json::json!({
                "session_id": self.rec.session_id, "request_id": request_id, "request": request,
            }))[..32]
        );
        let mut bodies = turn.normalizer.before_other_event();
        bodies.push(EventBody::InteractionRequested(InteractionRequested {
            request_id: request_id.clone(),
            turn_id: turn.id.clone(),
            item_id,
            digest: digest.clone(),
            actionable_by: ACTIONABLE_BY.to_vec(),
            request,
        }));
        self.log(bodies);
        self.pending.insert(request_id, pending(digest));
        self.publish();
    }

    fn on_permission(&mut self, ask: PermissionAsk, reply: oneshot::Sender<PermissionAnswer>) {
        let options: Vec<PermissionOption> = ask
            .options
            .into_iter()
            .map(|(id, name, kind)| PermissionOption { option_id: id, name: truncate_chars(&name, MAX_LABEL_CHARS), kind })
            .collect();
        let item_id = self.turn.as_ref().and_then(|t| t.normalizer.tool_item(&ask.tool_call_id));
        let request = InteractionRequest::Permission { title: truncate_chars(&ask.title, MAX_LABEL_CHARS), options: options.clone(), detail: None };
        self.register(request, item_id, |digest| Pending::Permission { digest, options, reply });
    }

    fn on_question(&mut self, ask: QuestionAsk, reply: oneshot::Sender<QuestionAnswer>) {
        let request = InteractionRequest::Question {
            prompt: truncate_chars(&ask.prompt, MAX_LABEL_CHARS * 8),
            choices: ask.choices.iter().map(|c| truncate_chars(c, MAX_LABEL_CHARS)).collect(),
            free_text: ask.choices.is_empty(),
        };
        self.register(request, None, |digest| Pending::Question { digest, reply });
    }

    fn finish_turn(&mut self, status: TurnStatus, stop_reason: Option<String>) {
        let Some(mut turn) = self.turn.take() else { return };
        let cause = turn.cause.clone();
        let mut bodies = turn.normalizer.finish(status != TurnStatus::Completed);
        self.log_with(std::mem::take(&mut bodies), cause.as_ref());
        self.cancel_pending(InteractionStatus::Cancelled, cause.as_ref());
        self.log_with(vec![EventBody::TurnCompleted(TurnCompleted { turn_id: turn.id, status, stop_reason })], cause.as_ref());
        if let Err(e) = self.host.store.set_running(&self.rec.session_id, false) {
            tracing::error!("clearing running flag failed: {e}");
        }
        self.base_status = SessionStatus::Idle;
        self.publish();
        for stream in [&self.stream, &StreamId::Index] {
            if let Err(e) = self.host.hub.checkpoint_and_prune(stream) {
                tracing::error!("checkpoint failed: {e}");
            }
        }
    }
}

fn variant_name(u: &crate::driver::NormUpdate) -> &'static str {
    use crate::driver::NormUpdate::*;
    match u {
        AgentText { .. } => "agent_text",
        Thought { .. } => "thought",
        ToolCall { .. } => "tool_call",
        ToolUpdate { .. } => "tool_update",
        Plan { .. } => "plan",
        Other { .. } => "other",
    }
}

async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(d) => tokio::time::sleep_until(d.into()).await,
        None => std::future::pending().await,
    }
}
