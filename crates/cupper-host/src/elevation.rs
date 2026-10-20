//! CLI elevation (design 5.8.4, security #8): what decides `auth/elevate`.
//!
//! - [`Requesters`] examines the process behind the asking connection: an
//!   adapter's descendant, or one that cannot be ruled out, is refused
//!   (`forbidden`, `lineage`) without a dialog. The host's is
//!   [`ProcessRequesters`](crate::requester::ProcessRequesters);
//!   [`FailClosed`] refuses everyone.
//! - [`ElevationCore`], pure: one request pending at a time; two denials
//!   within 10 minutes silence requests for 10 minutes (`rate_limited`); a
//!   request without a decision expires after 2 minutes; a grant holds for
//!   its connection, at most 15 minutes, and ends when the connection closes.

use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use cupper_protocol::ids::{ConnId, ElevationId};
use cupper_protocol::methods::admin::{AuthElevationGetResult, SelfReported, VerifiedRequester};
use cupper_protocol::rpc::Scope;
use cupper_sys::AuditToken;
use futures::future::BoxFuture;

use crate::clock::{MonoTime, Now};
use crate::durations::{
    Durations, ELEVATION_DENIALS, ELEVATION_GRANT, ELEVATION_PENDING, ELEVATION_QUIET,
};
use crate::pure::{Core, TimerRequest};

/// What the host finds out about the process behind a connection that asks
/// for elevation (design 5.8.4).
pub trait Requesters: Send + Sync + fmt::Debug {
    /// Examines the peer that `token` names: the connection's pinned token.
    fn examine(&self, token: AuditToken) -> BoxFuture<'static, Examined>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum Examined {
    /// None of the lineage signals matched; the dialog shows `verified`.
    Clear(VerifiedRequester),
    /// An adapter's descendant, or it cannot be ruled out; `signals` go into
    /// the audit record (`same_session`, `same_group`, `ancestor`, ...).
    Descendant { signals: Vec<String> },
}

/// Every requester counts as a possible descendant: what the index's own
/// cases run on, where nothing may be examined.
#[derive(Debug)]
pub struct FailClosed;

impl Requesters for FailClosed {
    fn examine(&self, _: AuditToken) -> BoxFuture<'static, Examined> {
        let signals = vec!["unchecked".to_owned()];
        Box::pin(futures::future::ready(Examined::Descendant { signals }))
    }
}

/// How long the self-reported fields may be, in characters.
pub const SELF_REPORTED_MAX: usize = 200;

/// `text` cut to [`SELF_REPORTED_MAX`] characters.
pub fn truncated(text: &str) -> String {
    text.chars().take(SELF_REPORTED_MAX).collect()
}

#[derive(Debug)]
pub struct ElevationCore {
    pending_for: Duration,
    grant_for: Duration,
    window: Duration,
    quiet_for: Duration,
    pending: Option<Pending>,
    /// Denials within the last window.
    denials: Vec<MonoTime>,
    quiet_until: Option<MonoTime>,
    /// Each granted connection and when its grant ends.
    grants: HashMap<ConnId, MonoTime>,
}

/// The request waiting for the user.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    pub id: ElevationId,
    pub conn: ConnId,
    /// What `auth/elevation_get` returns.
    pub detail: AuthElevationGetResult,
    deadline: MonoTime,
}

/// `auth/elevate` from `conn`, once its requester was examined.
#[derive(Debug, Clone, PartialEq)]
pub struct Ask {
    pub id: ElevationId,
    pub conn: ConnId,
    pub scopes: Vec<Scope>,
    pub examined: Examined,
    pub said: SelfReported,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    Ask(Ask),
    /// The user's decision: the scopes to grant, or `None` to deny.
    Respond {
        id: ElevationId,
        grant: Option<Vec<Scope>>,
    },
    Closed(ConnId),
    Expired(ElevationTimer),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ElevationTimer {
    Pending(ElevationId),
    Grant(ConnId),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// The ask's reply, at once: `forbidden` (`lineage`) or `rate_limited`.
    Refused {
        conn: ConnId,
        refusal: Refusal,
    },
    /// A request waits for the user (`elevation.requested`); so does its reply.
    Requested(Pending),
    /// The pending request is decided (`elevation.resolved`); its reply.
    Resolved {
        id: ElevationId,
        conn: ConnId,
        outcome: Outcome,
    },
    /// The connection's grant ended.
    Revoked(ConnId),
    /// The reply to `Respond`: `Err` when the request is no longer pending.
    Responded(Result<(), NotPending>),
    Timer(TimerRequest<ElevationTimer>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Refusal {
    Lineage { signals: Vec<String> },
    RateLimited { retry_after: Duration },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// What the user granted of what was asked for, until `ends_in`.
    Granted {
        scopes: Vec<Scope>,
        ends_in: Duration,
    },
    Denied,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the elevation request is no longer pending")]
pub struct NotPending;

impl ElevationCore {
    pub fn new(durations: &Durations) -> Self {
        Self {
            pending_for: durations.get(ELEVATION_PENDING),
            grant_for: durations.get(ELEVATION_GRANT),
            window: durations.get(ELEVATION_DENIALS),
            quiet_for: durations.get(ELEVATION_QUIET),
            pending: None,
            denials: Vec::new(),
            quiet_until: None,
            grants: HashMap::new(),
        }
    }

    pub fn pending(&self, id: &ElevationId) -> Option<&Pending> {
        self.pending.as_ref().filter(|p| p.id == *id)
    }

    fn ask(&mut self, ask: Ask, now: MonoTime, out: &mut Vec<Action>) {
        let Ask {
            id,
            conn,
            scopes,
            examined,
            said,
        } = ask;
        let verified = match examined {
            Examined::Clear(verified) => verified,
            Examined::Descendant { signals } => {
                let refusal = Refusal::Lineage { signals };
                return out.push(Action::Refused { conn, refusal });
            }
        };
        let busy = self.pending.as_ref().map(|p| p.deadline);
        let quiet = self.quiet_until.filter(|until| *until > now);
        if let Some(until) = busy.or(quiet) {
            let retry_after = until.saturating_duration_since(now);
            let refusal = Refusal::RateLimited { retry_after };
            return out.push(Action::Refused { conn, refusal });
        }
        let deadline = now + self.pending_for;
        let self_reported = said;
        let detail = AuthElevationGetResult {
            verified,
            self_reported,
            scopes,
        };
        self.pending = Some(Pending {
            id: id.clone(),
            conn,
            detail,
            deadline,
        });
        out.push(set(ElevationTimer::Pending(id), deadline));
        out.extend(self.pending.clone().map(Action::Requested));
    }

    fn respond(
        &mut self,
        id: ElevationId,
        grant: Option<Vec<Scope>>,
        now: MonoTime,
        out: &mut Vec<Action>,
    ) {
        let Some(pending) = self.pending.take_if(|p| p.id == id) else {
            return out.push(Action::Responded(Err(NotPending)));
        };
        out.push(cancel(ElevationTimer::Pending(id.clone())));
        out.push(Action::Responded(Ok(())));
        // Only what was asked for, and only what the user ticked.
        let asked = &pending.detail.scopes;
        let scopes: Vec<Scope> = (grant.unwrap_or_default().into_iter())
            .filter(|scope| asked.contains(scope))
            .collect();
        let conn = pending.conn;
        if scopes.is_empty() {
            self.denials
                .retain(|at| now.saturating_duration_since(*at) < self.window);
            self.denials.push(now);
            if self.denials.len() >= 2 {
                self.denials.clear();
                self.quiet_until = Some(now + self.quiet_for);
            }
            let outcome = Outcome::Denied;
            return out.push(Action::Resolved { id, conn, outcome });
        }
        let ends = now + self.grant_for;
        self.grants.insert(conn.clone(), ends);
        out.push(set(ElevationTimer::Grant(conn.clone()), ends));
        let outcome = Outcome::Granted {
            scopes,
            ends_in: self.grant_for,
        };
        out.push(Action::Resolved { id, conn, outcome });
    }

    /// Ends `conn`'s grant, and its pending request as expired.
    fn closed(&mut self, conn: ConnId, out: &mut Vec<Action>) {
        if self.grants.remove(&conn).is_some() {
            out.push(cancel(ElevationTimer::Grant(conn.clone())));
            out.push(Action::Revoked(conn.clone()));
        }
        if let Some(pending) = self.pending.take_if(|p| p.conn == conn) {
            out.push(cancel(ElevationTimer::Pending(pending.id.clone())));
            let (id, outcome) = (pending.id, Outcome::Expired);
            out.push(Action::Resolved { id, conn, outcome });
        }
    }
}

fn set(key: ElevationTimer, at: MonoTime) -> Action {
    Action::Timer(TimerRequest::Set { key, at })
}

fn cancel(key: ElevationTimer) -> Action {
    Action::Timer(TimerRequest::Cancel(key))
}

impl Core for ElevationCore {
    type Input = Input;
    type Action = Action;
    type TimerKey = ElevationTimer;

    fn step(&mut self, input: Input, now: Now) -> Vec<Action> {
        let mut out = Vec::new();
        match input {
            Input::Ask(ask) => self.ask(ask, now.mono, &mut out),
            Input::Respond { id, grant } => self.respond(id, grant, now.mono, &mut out),
            Input::Closed(conn) => self.closed(conn, &mut out),
            Input::Expired(ElevationTimer::Pending(id)) => {
                if let Some(pending) = self.pending.take_if(|p| p.id == id) {
                    let (conn, outcome) = (pending.conn, Outcome::Expired);
                    out.push(Action::Resolved { id, conn, outcome });
                }
            }
            Input::Expired(ElevationTimer::Grant(conn)) => {
                if self.grants.remove(&conn).is_some() {
                    out.push(Action::Revoked(conn));
                }
            }
        }
        out
    }

    fn timer_request(action: &Action) -> Option<TimerRequest<ElevationTimer>> {
        match action {
            Action::Timer(request) => Some(request.clone()),
            _ => None,
        }
    }

    fn timer_expired(key: ElevationTimer) -> Input {
        Input::Expired(key)
    }
}

#[cfg(test)]
mod tests;
