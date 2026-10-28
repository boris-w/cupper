//! Dispatching (design 5.2.4, 5.2.5): after `initialize`, every request and
//! client notification goes through the policy, a command then through the
//! ledger, and the call to whoever serves the method in [`Routes`].
//!
//! The read loop dispatches each message in arrival order and never waits:
//! a [`Port`] only queues the call, so the commands one connection sends to
//! one session reach its mailbox in order. Their replies are awaited in tasks
//! of their own and come back in any order.
//!
//! A connection has at most `limits.max_inflight_requests` requests in
//! flight, every request counted. One more is `rate_limited` without
//! `retry_after_ms`: only the connection's own replies free a place (design
//! 5.3.11, rulings before T033).

use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use cupper_protocol::error::{ErrorCode, INVALID_PARAMS, RpcError};
use cupper_protocol::frame::BlobChunkHeader;
use cupper_protocol::ids::{CommandId, ConnId, SessionId};
use cupper_protocol::index::Elevation;
use cupper_protocol::limits::V1;
use cupper_protocol::methods::admin::{
    AuthElevationGet, ConfirmAction, PolicyDescribe, PolicyDescribeParams, SettingsUpdate,
};
use cupper_protocol::methods::git::GitRevert;
use cupper_protocol::methods::handshake::{ClientIdentity, Principal};
use cupper_protocol::methods::session::SessionSetConfig;
use cupper_protocol::methods::stream::{
    BlobPut, BlobPutParams, StreamSubscribeParams, StreamUnsubscribeParams,
};
use cupper_protocol::rpc::{Idempotency, Method, RpcId, Scope};
use cupper_protocol::sync::StreamId;
use cupper_sys::AuditToken;
use futures::FutureExt;
use futures::future::BoxFuture;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, oneshot};

use super::audit::{AuditEvent, Auditor};
use super::blobs::{self, Uploads};
use super::outbox::Outbox;
use super::routes::{Entry, Routes, Target};
use super::subscribe::Subscriptions;
use super::{REPLY, describe, history, wire};
use crate::clock::Clock;
use crate::ledger::{Begin, Outcome, Repeat, Ticket};
use crate::policy::{self, Confirmable, Facts};
use crate::quiescence::Participant;

#[cfg(test)]
mod tests;

/// A call's reply, once its executor has one.
pub type Reply = BoxFuture<'static, Outcome>;

/// A request or client notification on its way to its executor.
#[derive(Debug)]
pub struct Call {
    pub conn: ConnId,
    pub principal: Principal,
    /// Who the client said it was in `initialize`.
    pub client: ClientIdentity,
    pub method: &'static str,
    /// Valid for the method; `{}` when the client left them out.
    pub params: Value,
    /// A command's first run, for the executor to end (see [`crate::ledger`]).
    pub ticket: Option<Ticket>,
    /// Resolves once the reply is queued on the connection, or never will
    /// be: what must come after the reply waits on it, as the exit after
    /// `host/exit`'s (design 5.2.13).
    pub reply_queued: oneshot::Receiver<()>,
}

/// Where calls go: a mailbox, or an executor that runs each call in a task.
pub trait Port: Send + Sync + fmt::Debug {
    /// Called by the read loop in arrival order: queue `call` and return at
    /// once. The gateway awaits the reply in a task; a notification's reply
    /// is dropped.
    fn deliver(&self, call: Call) -> Reply;
}

/// The sessions' side (T043): session commands and queries, and what
/// `policy/describe` needs to know of the host's state.
pub trait SessionPort: Port {
    /// The facts describing `command` reads (design 5.3.11, rulings before
    /// T033): `mode_tier`, `option` and `running_turns`.
    fn facts(&self, command: &Confirmable) -> BoxFuture<'static, Result<Facts, RpcError>>;
    /// A client subscribes to session `id`'s stream: a session not in
    /// memory loads, so that what its load writes reaches the subscriber
    /// (design 5.2.13).
    fn wake(&self, id: &SessionId);
}

/// The index's side (T034): projects, read marks and presence, and the
/// connected clients of `clients.updated`.
pub trait IndexPort: Port {
    /// A connection finished `initialize`.
    fn opened(&self, client: Opened);
    /// An initialized connection closed: it leaves `clients`, and the
    /// elevations granted on it end (T038, design 5.2.4).
    fn closed(&self, conn: &ConnId);
}

/// A connection that finished `initialize`.
#[derive(Debug, Clone)]
pub struct Opened {
    pub conn: ConnId,
    pub principal: Principal,
    pub client: ClientIdentity,
    /// The host runs on degraded authentication (ADR-05).
    pub weak_auth: bool,
    /// The peer's token, pinned on accept (design 5.8.1).
    pub token: AuditToken,
    /// What an elevation granted the connection; the index sets it.
    pub granted: Arc<Granted>,
}

/// The scopes an elevation granted one connection, until `expires_at`
/// (design 5.8.4). Read as each call is admitted: a call admitted after a
/// grant ends has the connection's own scopes; one already running finishes.
#[derive(Debug, Default)]
pub struct Granted(Mutex<Option<Elevation>>);

impl Granted {
    pub fn set(&self, elevation: Option<Elevation>) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = elevation;
    }

    pub fn get(&self) -> Option<Elevation> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// `principal` with the granted scopes too, until `expires_at` at
    /// `now_ms`: should the index fail to end a grant, it ends all the same.
    fn widen(&self, principal: &Principal, now_ms: u64) -> Principal {
        let mut principal = principal.clone();
        let current = self.get().filter(|e| e.expires_at.0 > now_ms);
        for scope in current.into_iter().flat_map(|e| e.scopes) {
            if !principal.scopes.contains(&scope) {
                principal.scopes.push(scope);
            }
        }
        principal
    }
}

/// An initialized connection's side of dispatching.
#[derive(Debug)]
pub(super) struct Dispatch {
    conn: ConnId,
    principal: Principal,
    client: ClientIdentity,
    granted: Arc<Granted>,
    /// For when a grant ends.
    clock: Arc<dyn Clock>,
    outbox: Arc<Outbox>,
    inflight: Arc<Semaphore>,
    subscriptions: Subscriptions,
    uploads: Mutex<Uploads>,
    /// For `privileged.executed`.
    auditor: Auditor,
}

/// How a request is answered.
enum Answer {
    Now(Outcome),
    /// By a task, which holds the request's place in flight.
    Later(Reply, OwnedSemaphorePermit),
    /// By the connection's subscriptions, which hold the place.
    Queued,
}

impl Dispatch {
    /// Call within a tokio runtime.
    pub(super) fn new(
        conn: ConnId,
        principal: Principal,
        client: ClientIdentity,
        granted: Arc<Granted>,
        outbox: Arc<Outbox>,
        auditor: Auditor,
        subscriptions: Participant,
    ) -> Self {
        let clock = auditor.clock();
        let inflight = Arc::new(Semaphore::new(V1.max_inflight_requests as usize));
        let subscriptions =
            Subscriptions::start(Arc::clone(&outbox), auditor.clone(), subscriptions);
        Self {
            conn,
            principal,
            client,
            granted,
            clock,
            outbox,
            inflight,
            subscriptions,
            uploads: Mutex::default(),
            auditor,
        }
    }

    /// A `0x02` chunk: into the uploads, from a principal that may put blobs
    /// (design 5.3.1). One that does not parse, or may not, is dropped.
    pub(super) fn chunk(&self, payload: &[u8]) {
        let principal = self
            .granted
            .widen(&self.principal, self.clock.now_wall_ms());
        if let (Ok(()), Ok((header, data))) = (
            policy::method(&principal, BlobPut::NAME, BlobPut::SCOPE.as_ref()),
            BlobChunkHeader::decode(payload),
        ) {
            let mut uploads = self.uploads.lock().unwrap_or_else(PoisonError::into_inner);
            uploads.chunk(&header, data);
        }
    }

    /// Answers `method` now, or queues the call and answers in a task.
    pub(super) fn request(&self, routes: &Routes, id: RpcId, method: &str, params: Value) {
        let (queued, reply_queued) = oneshot::channel();
        match self.answer(routes, &id, method, params, reply_queued) {
            Answer::Now(outcome) => self.outbox.push_frame(frame(&id, outcome)),
            Answer::Later(reply, place) => {
                let outbox = Arc::clone(&self.outbox);
                let participant = routes.quiescence().enter(REPLY);
                tokio::spawn(async move {
                    let outcome = participant.idle(reply).await;
                    // Free the place first: the client may send its next
                    // request as soon as it reads this reply.
                    drop(place);
                    outbox.push_frame(frame(&id, outcome));
                    let _ = queued.send(());
                });
            }
            Answer::Queued => {}
        }
    }

    /// A client notification has no reply: one the host does not serve, or
    /// refuses, is dropped.
    pub(super) fn notification(&self, routes: &Routes, method: &str, params: Value) {
        let Some((name, entry)) = routes.notification_entry(method) else {
            return;
        };
        let (params, principal) = (
            or_empty(params),
            self.granted
                .widen(&self.principal, self.clock.now_wall_ms()),
        );
        if Self::admit(&principal, name, entry, &params).is_err() {
            return;
        }
        if let Target::Port(port) = &entry.target {
            let (hold, reply) = (
                routes.quiescence().hold(),
                port.deliver(self.call(principal, name, params, None, oneshot::channel().1)),
            );
            tokio::spawn(async move {
                let _hold = hold;
                reply.await
            });
        }
    }

    fn answer(
        &self,
        routes: &Routes,
        id: &RpcId,
        method: &str,
        params: Value,
        reply_queued: oneshot::Receiver<()>,
    ) -> Answer {
        let Some((name, entry)) = routes.request_entry(method) else {
            return Answer::Now(Err(unsupported(method)));
        };
        let (params, principal) = (
            or_empty(params),
            self.granted
                .widen(&self.principal, self.clock.now_wall_ms()),
        );
        if let Err(err) = Self::admit(&principal, name, entry, &params) {
            return Answer::Now(Err(err));
        }
        let Ok(place) = Arc::clone(&self.inflight).try_acquire_owned() else {
            let err = RpcError::app(ErrorCode::RateLimited, "too many requests in flight");
            return Answer::Now(Err(err));
        };
        let ticket = match entry.idempotency {
            Idempotency::Command => match self.begin(routes, name, &params) {
                Ok(Begin::Run(ticket)) => Some(ticket),
                Ok(Begin::Repeat(Repeat::Replay(outcome))) => return Answer::Now(outcome),
                Ok(Begin::Repeat(Repeat::Wait(first))) => {
                    return Answer::Later(Box::pin(first.outcome()), place);
                }
                Err(err) => return Answer::Now(Err(err)),
            },
            Idempotency::Natural | Idempotency::None => None,
        };
        let executed = privileged(&principal, name, entry, &params);
        let reply = match &entry.target {
            Target::Port(port) => {
                let call = self.call(principal, name, params, ticket, reply_queued);
                let reply = port.deliver(call);
                match executed {
                    Some(event) => {
                        let auditor = self.auditor.clone();
                        let recorded = reply.inspect(move |outcome| {
                            if outcome.is_ok() {
                                auditor.record(event);
                            }
                        });
                        Box::pin(recorded)
                    }
                    None => reply,
                }
            }
            Target::Ping => return Answer::Now(Ok(json!({}))),
            Target::Describe(sessions) => describe::describe(sessions.as_ref(), &params),
            Target::History(source) => Box::pin(history::page(source.clone(), params)),
            Target::Subscribe(source) => {
                let params: StreamSubscribeParams = typed(&params).expect("validated");
                if let (StreamId::Session(session), Some(sessions)) =
                    (&params.stream, &routes.sessions)
                {
                    sessions.wake(session);
                }
                let id = id.clone();
                self.subscriptions.subscribe(id, source, params, place);
                return Answer::Queued;
            }
            Target::Unsubscribe => {
                let params: StreamUnsubscribeParams = typed(&params).expect("validated");
                let id = id.clone();
                self.subscriptions.unsubscribe(id, params.stream, place);
                return Answer::Queued;
            }
            Target::Blobs(store) if name == BlobPut::NAME => {
                let params: BlobPutParams = typed(&params).expect("validated");
                let mut uploads = self.uploads.lock().unwrap_or_else(PoisonError::into_inner);
                let upload = uploads.take(&params.hash);
                Box::pin(blobs::put(store.clone(), params, upload))
            }
            Target::Blobs(store) => {
                let params = typed(&params).expect("validated");
                Box::pin(blobs::get(store.clone(), Arc::clone(&self.outbox), params))
            }
        };
        Answer::Later(reply, place)
    }

    /// The method level, the parameters, and the rules that read only them
    /// (design 5.2.5), for the principal with what it was granted.
    fn admit(
        principal: &Principal,
        name: &str,
        entry: &Entry,
        params: &Value,
    ) -> Result<(), RpcError> {
        policy::method(principal, name, entry.scope.as_ref())?;
        (entry.validate)(params)?;
        match name {
            SettingsUpdate::NAME => policy::settings_update(principal, &typed(params)?)?,
            SessionSetConfig::NAME => policy::set_config(principal, &typed(params)?)?,
            GitRevert::NAME => policy::git_revert(principal, &typed(params)?)?,
            _ => {}
        }
        Ok(())
    }

    fn begin(&self, routes: &Routes, name: &str, params: &Value) -> Result<Begin, RpcError> {
        let command: CommandId = typed(&params["command_id"])?;
        let ledger = routes.ledger().expect("commands are routed with a ledger");
        Ok(ledger.begin(&self.principal.id, &command, name, params)?)
    }

    fn call(
        &self,
        principal: Principal,
        method: &'static str,
        params: Value,
        ticket: Option<Ticket>,
        reply_queued: oneshot::Receiver<()>,
    ) -> Call {
        Call {
            conn: self.conn.clone(),
            principal,
            client: self.client.clone(),
            method,
            params,
            ticket,
            reply_queued,
        }
    }
}

/// `params` as `T`, or invalid params.
pub(super) fn typed<T: DeserializeOwned>(params: &Value) -> Result<T, RpcError> {
    T::deserialize(params).map_err(|err| RpcError::new(INVALID_PARAMS, err.to_string()))
}

/// A missing `params` reads as `{}`.
fn or_empty(params: Value) -> Value {
    match params {
        Value::Null => json!({}),
        params => params,
    }
}

pub(super) fn internal(err: impl fmt::Display) -> RpcError {
    RpcError::app(ErrorCode::Internal, err.to_string())
}

fn unsupported(method: &str) -> RpcError {
    RpcError::app(ErrorCode::Unsupported, format!("{method} is not served"))
}

fn frame(id: &RpcId, outcome: Outcome) -> Vec<u8> {
    match outcome {
        Ok(result) => wire::response(id, &result),
        Err(err) => wire::error(Some(id), &err),
    }
}

/// `privileged.executed` for a call that takes `privileged` (design 5.8.6):
/// a method of that scope, queries aside, or a security-related setting. Its
/// action and details are `policy/describe`'s, where they need none of the
/// sessions' facts.
fn privileged(
    principal: &Principal,
    name: &'static str,
    entry: &Entry,
    params: &Value,
) -> Option<AuditEvent> {
    let queries = [PolicyDescribe::NAME, AuthElevationGet::NAME];
    let scoped = entry.scope == Some(Scope::Privileged) && !queries.contains(&name);
    if !scoped && name != SettingsUpdate::NAME {
        return None;
    }
    let (method, params) = (name.to_owned(), params.clone());
    let command = Confirmable::parse(&PolicyDescribeParams { method, params });
    let described = command.and_then(|c| policy::describe(&c, &Facts::default()));
    let described = described.ok();
    let security = (described.as_ref()).is_some_and(|d| d.action == ConfirmAction::SecuritySetting);
    (scoped || security).then(|| AuditEvent::Privileged {
        method: name,
        principal: principal.id.clone(),
        action: described.as_ref().map(|d| d.action.clone()),
        details: described.map_or_else(|| json!({}), |d| json!(d.details)),
    })
}
