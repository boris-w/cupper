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
use std::sync::Arc;

use cupper_protocol::error::{ErrorCode, INVALID_PARAMS, RpcError};
use cupper_protocol::ids::{CommandId, ConnId};
use cupper_protocol::limits::V1;
use cupper_protocol::methods::admin::SettingsUpdate;
use cupper_protocol::methods::git::GitRevert;
use cupper_protocol::methods::handshake::{ClientIdentity, Principal};
use cupper_protocol::methods::session::SessionSetConfig;
use cupper_protocol::methods::stream::{StreamSubscribeParams, StreamUnsubscribeParams};
use cupper_protocol::rpc::{Idempotency, Method, RpcId};
use futures::future::BoxFuture;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::audit::Auditor;
use super::outbox::Outbox;
use super::routes::{Entry, Routes, Target};
use super::subscribe::Subscriptions;
use super::{describe, history, wire};
use crate::ledger::{Begin, Outcome, Repeat, Ticket};
use crate::policy::{self, Confirmable, Facts};

#[cfg(test)]
mod tests;

/// A call's reply, once its executor has one.
pub type Reply = BoxFuture<'static, Outcome>;

/// A request or client notification on its way to its executor.
#[derive(Debug)]
pub struct Call {
    pub conn: ConnId,
    pub principal: Principal,
    pub method: &'static str,
    /// Valid for the method; `{}` when the client left them out.
    pub params: Value,
    /// A command's first run, for the executor to end (see [`crate::ledger`]).
    pub ticket: Option<Ticket>,
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
#[derive(Debug, Clone, PartialEq)]
pub struct Opened {
    pub conn: ConnId,
    pub principal: Principal,
    pub client: ClientIdentity,
    /// The host runs on degraded authentication (ADR-05).
    pub weak_auth: bool,
}

/// An initialized connection's side of dispatching.
#[derive(Debug)]
pub(super) struct Dispatch {
    conn: ConnId,
    principal: Principal,
    outbox: Arc<Outbox>,
    inflight: Arc<Semaphore>,
    subscriptions: Subscriptions,
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
        outbox: Arc<Outbox>,
        auditor: Auditor,
    ) -> Self {
        let inflight = Arc::new(Semaphore::new(V1.max_inflight_requests as usize));
        let subscriptions = Subscriptions::start(Arc::clone(&outbox), auditor);
        Self {
            conn,
            principal,
            outbox,
            inflight,
            subscriptions,
        }
    }

    /// Answers `method` now, or queues the call and answers in a task.
    pub(super) fn request(&self, routes: &Routes, id: RpcId, method: &str, params: Value) {
        match self.answer(routes, &id, method, params) {
            Answer::Now(outcome) => self.outbox.push_frame(frame(&id, outcome)),
            Answer::Later(reply, place) => {
                let outbox = Arc::clone(&self.outbox);
                tokio::spawn(async move {
                    let outcome = reply.await;
                    // Free the place first: the client may send its next
                    // request as soon as it reads this reply.
                    drop(place);
                    outbox.push_frame(frame(&id, outcome));
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
        let params = or_empty(params);
        if self.admit(name, entry, &params).is_err() {
            return;
        }
        if let Target::Port(port) = &entry.target {
            tokio::spawn(port.deliver(self.call(name, params, None)));
        }
    }

    fn answer(&self, routes: &Routes, id: &RpcId, method: &str, params: Value) -> Answer {
        let entry = routes.request_entry(method);
        let Some((name, entry)) = entry.filter(|(_, entry)| entry.target.serves()) else {
            return Answer::Now(Err(unsupported(method)));
        };
        let params = or_empty(params);
        if let Err(err) = self.admit(name, entry, &params) {
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
        let reply = match &entry.target {
            Target::Port(port) => port.deliver(self.call(name, params, ticket)),
            Target::Ping => return Answer::Now(Ok(json!({}))),
            Target::Describe(sessions) => describe::describe(sessions.as_ref(), &params),
            Target::History(source) => Box::pin(history::page(source.clone(), params)),
            Target::Subscribe(source) => {
                let params: StreamSubscribeParams = typed(&params).expect("validated");
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
            Target::Blobs => unreachable!("not served yet"),
        };
        Answer::Later(reply, place)
    }

    /// The method level, the parameters, and the rules that read only them
    /// (design 5.2.5).
    fn admit(&self, name: &str, entry: &Entry, params: &Value) -> Result<(), RpcError> {
        let principal = &self.principal;
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

    fn call(&self, method: &'static str, params: Value, ticket: Option<Ticket>) -> Call {
        Call {
            conn: self.conn.clone(),
            principal: self.principal.clone(),
            method,
            params,
            ticket,
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
