//! Elevation in the index actor (design 5.8.4): `auth/elevate`, whose reply
//! waits for the user's decision; `auth/elevation_get` and
//! `auth/elevation_respond` from the control connection; the timers of
//! [`ElevationCore`], in the actor's table; the granted scopes each connection reads at admission
//! ([`Granted`](crate::gateway::Granted)), which `clients.updated` lists.

use std::collections::HashMap;

use cupper_protocol::annotations::{DurationMs, TimestampMs};
use cupper_protocol::envelope::Cause;
use cupper_protocol::error::{ErrorCode, ErrorReason, INVALID_PARAMS, RpcError};
use cupper_protocol::event::EventBody;
use cupper_protocol::ids::{ConnId, ElevationId};
use cupper_protocol::index::{Elevation, ElevationDecision, ElevationRequested, ElevationResolved};
use cupper_protocol::methods::admin::{
    AuthElevate, AuthElevateParams, AuthElevationGet, AuthElevationGetParams,
    AuthElevationRespondParams, ElevationAnswer, SelfReported,
};
use cupper_protocol::rpc::{Method, Scope};
use cupper_protocol::view::IndexView;
use serde_json::{Value, json};
use tokio::sync::oneshot;

use super::{IndexActor, Mail, Timer, parse, refuse, timer};
use crate::durations::Durations;
use crate::elevation::{
    Action, Ask, ElevationCore, ElevationTimer, Input, NotPending, Outcome as Decided, Refusal,
    truncated,
};
use crate::gateway::{AuditEvent, AuditRecord, Call, ElevationStep, Opened};
use crate::ledger::Outcome;
use crate::pure::Core;
use crate::store::Durability;
use crate::store::streams::{Batch, StreamError};

#[derive(Debug)]
pub(super) struct Elevations {
    core: ElevationCore,
    /// The initialized connections.
    conns: HashMap<ConnId, Opened>,
    /// The `auth/elevate` replies that wait for the user, and what they
    /// asked for.
    waiting: HashMap<ElevationId, Asking>,
}

pub(super) type Asking = (oneshot::Sender<Outcome>, Vec<Scope>);

/// The `auth/elevation_respond` being answered.
type Command = (Call, oneshot::Sender<Outcome>);

impl Elevations {
    pub(super) fn new(durations: &Durations) -> Self {
        Self {
            core: ElevationCore::new(durations),
            conns: HashMap::new(),
            waiting: HashMap::new(),
        }
    }

    /// Sets what `conn` was granted, unless it closed; whether it was set.
    fn grant(&self, conn: &ConnId, elevation: Option<Elevation>) -> bool {
        let granted = self.conns.get(conn).map(|opened| &opened.granted);
        granted.map(|granted| granted.set(elevation)).is_some()
    }
}

impl IndexActor {
    pub(super) fn opened(&mut self, opened: Opened) {
        self.elevations.conns.insert(opened.conn.clone(), opened);
    }

    /// The connection's grant, and its pending request, end with it.
    pub(super) async fn closed(&mut self, conn: ConnId) {
        let closed = Input::Closed(conn.clone());
        self.elevation(closed, None, None).await;
        self.elevations.conns.remove(&conn);
    }

    pub(super) async fn elevation_call(&mut self, call: Call, reply: oneshot::Sender<Outcome>) {
        match call.method {
            AuthElevate::NAME => self.ask(call, reply),
            AuthElevationGet::NAME => drop(reply.send(self.detail(&call.params))),
            _ => match parse::<AuthElevationRespondParams>(&call.params) {
                Ok(params) => {
                    // Anything but `grant` denies.
                    let grant = params.decision == ElevationAnswer::Grant;
                    let (id, grant) = (params.elevation_id, grant.then_some(params.scopes));
                    let respond = Input::Respond { id, grant };
                    self.elevation(respond, None, Some((call, reply))).await;
                }
                Err(err) => drop(reply.send(refuse(call.ticket, err))),
            },
        }
    }

    /// Examines the requester in a task; the core decides once it is back.
    fn ask(&mut self, call: Call, reply: oneshot::Sender<Outcome>) {
        let params = match asked(&call.params) {
            Ok(params) => params,
            Err(err) => return drop(reply.send(Err(err))),
        };
        // Gone: the connection closed, and nobody reads the reply.
        let Some(opened) = self.elevations.conns.get(&call.conn) else {
            return;
        };
        let examine = self.deps.requesters.examine(opened.token);
        let client_name = truncated(&call.client.name);
        let reason = params.reason.as_deref().map(truncated);
        let said = SelfReported {
            client_name,
            reason,
        };
        let id = ElevationId::from_uuid(self.deps.ids.new_id().as_u128());
        let (conn, scopes) = (call.conn, params.scopes);
        let asking = (reply, scopes.clone());
        let (mail, hold) = (self.mail.clone(), self.deps.quiescence.hold());
        tokio::spawn(async move {
            let examined = examine.await;
            let ask = Ask {
                id,
                conn,
                scopes,
                examined,
                said,
            };
            let _ = mail.send(Mail::Examined(Box::new((ask, asking))));
            // Let go once the actor has its work.
            drop(hold);
        });
    }

    pub(super) async fn examined(&mut self, ask: Ask, asking: Asking) {
        if self.elevations.conns.contains_key(&ask.conn) {
            self.elevation(Input::Ask(ask), Some(asking), None).await;
        }
    }

    /// `auth/elevation_get`: what the dialog shows.
    fn detail(&self, params: &Value) -> Outcome {
        let params: AuthElevationGetParams = parse(params)?;
        let pending = self.elevations.core.pending(&params.elevation_id);
        let gone = || RpcError::app(ErrorCode::NotFound, "no such pending elevation request");
        Ok(json!(pending.ok_or_else(gone)?.detail))
    }

    pub(super) async fn expired(&mut self, key: ElevationTimer) {
        self.elevation(Input::Expired(key), None, None).await;
    }

    /// Steps the core with `input`, `asking` being the ask's reply. Its
    /// events go out in one batch, with `command`'s outcome; the replies
    /// after them.
    async fn elevation(&mut self, input: Input, asking: Option<Asking>, command: Option<Command>) {
        let now = self.deps.clock.now();
        let (mut bodies, mut replies, mut audits) = (Vec::new(), Vec::new(), Vec::new());
        let (mut asking, mut answer, mut clients) = (asking, Ok(()), false);
        let e = &mut self.elevations;
        for action in e.core.step(input, now) {
            match action {
                Action::Timer(request) => self.timers.apply(timer(request, Timer::Elevation)),
                Action::Refused { conn, refusal } => {
                    let (step, err) = refused(refusal);
                    if let Some((reply, scopes)) = asking.take() {
                        audits.push((conn, step, scopes));
                        replies.push((reply, Err(err)));
                    }
                }
                Action::Requested(pending) => {
                    let (id, detail) = (pending.id, pending.detail);
                    let (client_name, scopes) = (detail.self_reported.client_name, detail.scopes);
                    let requested_at = TimestampMs(now.wall_ms);
                    let step = ElevationStep::Requested(id.clone());
                    audits.push((pending.conn, step, scopes.clone()));
                    bodies.push(EventBody::ElevationRequested(ElevationRequested {
                        elevation_id: id.clone(),
                        client_name,
                        scopes,
                        requested_at,
                    }));
                    e.waiting.extend(asking.take().map(|asking| (id, asking)));
                }
                Action::Resolved { id, conn, outcome } => {
                    let Some((reply, asked)) = e.waiting.remove(&id) else {
                        continue;
                    };
                    let (decision, scopes, result) = match outcome {
                        Decided::Granted { scopes, ends_in } => {
                            let expires_at = TimestampMs(now.wall_ms + ends_in.as_millis() as u64);
                            let elevation = Elevation { scopes, expires_at };
                            let result =
                                json!({"granted": elevation.scopes, "expires_at": expires_at});
                            clients |= e.grant(&conn, Some(elevation.clone()));
                            (ElevationDecision::Granted, elevation.scopes, Ok(result))
                        }
                        Decided::Denied => {
                            (ElevationDecision::Denied, asked, Ok(json!({"granted": []})))
                        }
                        Decided::Expired => {
                            let err = RpcError::app(ErrorCode::Timeout, "no decision in time");
                            (ElevationDecision::Expired, asked, Err(err))
                        }
                    };
                    let step = ElevationStep::Resolved(id.clone(), decision.clone());
                    audits.push((conn, step, scopes));
                    let resolved = ElevationResolved {
                        elevation_id: id,
                        decision,
                    };
                    bodies.push(EventBody::ElevationResolved(resolved));
                    replies.push((reply, result));
                }
                Action::Revoked(conn) => clients |= e.grant(&conn, None),
                Action::Responded(result) => answer = result,
            }
        }
        self.alarm.follow(&self.timers);
        for (conn, step, scopes) in audits {
            let (event, conn) = (AuditEvent::Elevation { step, scopes }, Some(conn));
            // Nobody listening is the receiver's choice.
            let _ = (self.deps.audit).send(AuditRecord {
                at_ms: now.wall_ms,
                conn,
                event,
            });
        }
        let (mut cause, mut completion, mut responded) = (None, None, None);
        if let Some((mut call, reply)) = command {
            if let Err(NotPending) = answer {
                let err = RpcError::app(ErrorCode::Expired, NotPending.to_string());
                return drop(reply.send(refuse(call.ticket, err)));
            }
            let principal = call.principal.id.clone();
            let command_id = parse(&call.params["command_id"]).ok();
            cause = command_id.map(|command_id| Cause {
                command_id,
                principal,
            });
            completion = (call.ticket.take()).map(|ticket| ticket.complete(Ok(json!({}))));
            responded = Some(reply);
        }
        let mut written = Ok(json!({}));
        if !bodies.is_empty() {
            let with = completion
                .as_ref()
                .map(|completion| Box::new(completion.row()) as _);
            let durability = Durability::Normal;
            let appended = self
                .writer
                .append(Batch {
                    bodies,
                    cause,
                    durability,
                    with,
                })
                .await;
            let internal = |err: StreamError| RpcError::app(ErrorCode::Internal, err.to_string());
            written = appended.map(|_| json!({})).map_err(internal);
        }
        if let Some(reply) = responded {
            if let (Ok(_), Some(completion)) = (&written, completion) {
                completion.committed();
            }
            let _ = reply.send(written);
        }
        for (reply, outcome) in replies {
            let _ = reply.send(outcome);
        }
        if clients {
            self.clients().await;
        }
    }
}

/// At start: the requests the last run left pending ended with their
/// connections. Not on `test/seed`'s reconciliation, while requests live.
pub(super) fn left_over(view: &IndexView) -> Vec<EventBody> {
    let ids = view.elevations.keys().cloned();
    let expired = |elevation_id| ElevationResolved {
        elevation_id,
        decision: ElevationDecision::Expired,
    };
    ids.map(|id| EventBody::ElevationResolved(expired(id)))
        .collect()
}

/// `auth/elevate`'s params: a CLI asks for `write`, `approve` or both.
fn asked(params: &Value) -> Result<AuthElevateParams, RpcError> {
    let params: AuthElevateParams = parse(params)?;
    let cli = |scope: &Scope| matches!(scope, Scope::Write | Scope::Approve);
    if params.scopes.is_empty() || !params.scopes.iter().all(cli) {
        return Err(RpcError::new(
            INVALID_PARAMS,
            "ask for write, approve or both",
        ));
    }
    Ok(params)
}

/// The audit step and the ask's reply for a refusal.
fn refused(refusal: Refusal) -> (ElevationStep, RpcError) {
    match refusal {
        Refusal::Lineage { signals } => {
            let err = RpcError::app(ErrorCode::Forbidden, "may descend from an agent");
            (
                ElevationStep::Lineage(signals),
                err.with_reason(ErrorReason::Lineage),
            )
        }
        Refusal::RateLimited { retry_after } => {
            let ms = Some(DurationMs(retry_after.as_millis() as u64));
            let err = RpcError::app(ErrorCode::RateLimited, "pending, or silenced after denials");
            (
                ElevationStep::RateLimited,
                err.with_data(|data| data.retry_after_ms = ms),
            )
        }
    }
}
