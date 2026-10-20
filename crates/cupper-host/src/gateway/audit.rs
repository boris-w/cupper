//! Audit records (design 5.8.6). The gateway, the sessions and the index
//! send them into one channel and never wait on it; the host's audit writer
//! (`supervisor::audit`) puts each into the `audit` table and `host.log` in
//! the form [`AuditEvent::formal`] gives it.

use std::io;
use std::sync::Arc;
use std::time::Duration;

use cupper_protocol::domain::Tier;
use cupper_protocol::error::RpcError;
use cupper_protocol::ids::{
    ConnId, Digest, ElevationId, PrincipalId, ProjectId, RequestId, SessionId,
};
use cupper_protocol::index::ElevationDecision;
use cupper_protocol::interaction::InteractionOutcome;
use cupper_protocol::methods::handshake::{ClientIdentity, Principal};
use cupper_protocol::rpc::Scope;
use cupper_protocol::session::ConfirmationReason;
use cupper_protocol::sync::{ResyncReason, StreamId, SubscribeMode};
use cupper_sys::{Refusal, RefusalKind};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::Verdict;
use crate::clock::Clock;

#[derive(Debug, Clone, PartialEq)]
pub struct AuditRecord {
    /// Milliseconds since the Unix epoch.
    pub at_ms: u64,
    /// `None` for what a session records of its own (`config_unconfirmed`).
    pub conn: Option<ConnId>,
    pub event: AuditEvent,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AuditEvent {
    /// `conn.accepted`: a peer of this user, its pinned token and the verdict
    /// on its signature, with the executable it runs and who signed that,
    /// when they could be read.
    Accepted {
        pid: i32,
        pid_version: u32,
        verdict: Verdict,
        path: Option<String>,
        identifier: Option<String>,
        team_id: Option<String>,
    },
    /// `conn.rejected`: closed before `host/hello`.
    Rejected(Rejection),
    /// `conn.initialized`.
    Initialized {
        principal: Principal,
        client: ClientIdentity,
        weak_auth: bool,
    },
    /// `conn.closed`.
    Closed {
        reason: CloseReason,
        /// Since the connection was accepted.
        after: Duration,
        /// Who the connection was, once initialized.
        principal: Option<PrincipalId>,
    },
    /// `stream.subscribed`: how a subscription started (Q16). It goes to
    /// `host.log` only, not to the `audit` table (design 5.3.11, rulings
    /// before T033).
    Subscribed {
        stream: StreamId,
        mode: SubscribeMode,
        reason: Option<ResyncReason>,
        /// How far the cursor was behind the head, when it came with the
        /// host's epoch and was not ahead.
        lag: Option<u64>,
    },
    /// `interaction.responded`: an answer the CLI or the test harness gave
    /// (design 5.8.6); the session actor records it.
    InteractionResponded {
        session_id: SessionId,
        request_id: RequestId,
        outcome: InteractionOutcome,
        principal: PrincipalId,
    },
    /// `config.unconfirmed`: a session's tier is above what its setter may
    /// set, or `full_access` without a native confirmation (design 5.2.6).
    ConfigUnconfirmed {
        reason: ConfirmationReason,
        config: ConfigState,
    },
    /// `config.confirmed`: the session left `config_unconfirmed`, which it
    /// was in for `reason`.
    ConfigConfirmed {
        reason: ConfirmationReason,
        config: ConfigState,
    },
    /// `project.trust.*` (design 5.2.6).
    ProjectTrust {
        project_id: ProjectId,
        change: TrustChange,
    },
    /// `elevation.*`: a step of a CLI's request for scopes on this
    /// connection, with the scopes asked for, or those granted (design 5.8.6).
    Elevation {
        step: ElevationStep,
        scopes: Vec<Scope>,
    },
}

/// A session's configuration as `config.*` records it.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigState {
    pub session_id: SessionId,
    /// The permission mode the agent reported, and its tier.
    pub mode: Option<String>,
    pub tier: Option<Tier>,
    /// Who set the tier.
    pub set_by: PrincipalId,
}

/// What became of a project's trust.
#[derive(Debug, Clone, PartialEq)]
pub enum TrustChange {
    /// Confirmed by `by` (`project/trust`).
    Trusted { by: PrincipalId, digest: Digest },
    /// Trusted, but the configuration changed since: now it scans as this.
    Stale { digest: Digest },
    /// The scan failed: what it said.
    ScanFailed { error: String },
}

/// What became of an elevation request (design 5.8.4).
#[derive(Debug, Clone, PartialEq)]
pub enum ElevationStep {
    Requested(ElevationId),
    /// Granted, denied or expired.
    Resolved(ElevationId, ElevationDecision),
    /// Refused without a dialog, with the lineage signals that matched.
    Lineage(Vec<String>),
    RateLimited,
}

/// One connection's way into the audit channel.
#[derive(Debug, Clone)]
pub(super) struct Auditor {
    conn: ConnId,
    records: mpsc::UnboundedSender<AuditRecord>,
    clock: Arc<dyn Clock>,
}

impl Auditor {
    pub fn new(
        conn: ConnId,
        records: mpsc::UnboundedSender<AuditRecord>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            conn,
            records,
            clock,
        }
    }

    /// The connection's clock.
    pub fn clock(&self) -> Arc<dyn Clock> {
        Arc::clone(&self.clock)
    }

    pub fn record(&self, event: AuditEvent) {
        let at_ms = self.clock.now_wall_ms();
        let conn = Some(self.conn.clone());
        // Nobody listening is the receiver's choice.
        let _ = self.records.send(AuditRecord { at_ms, conn, event });
    }
}

/// Why a peer was turned away on accept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    /// The peer runs as another user.
    OtherUser { uid: u32 },
    /// Its user or audit token could not be read; it has exited, for one.
    Unidentified(Refusal),
}

/// Why a connection ended.
#[derive(Debug, Clone, PartialEq)]
pub enum CloseReason {
    PeerClosed,
    ReadFailed(io::ErrorKind),
    /// The peer passed fds; they were closed unused (design 5.8.5).
    FdPassed {
        fds: usize,
    },
    /// Another process or program did I/O on the peer's end (design 5.8.1).
    TokenMismatch,
    /// The token could not be read after a read: the peer exited, for one.
    TokenUnreadable(RefusalKind),
    /// A frame broke the framing rules (design 5.3.1).
    FrameError,
    /// The first message was not `initialize`.
    NotInitialized,
    /// No `initialize` within the handshake deadline (design 5.3.11).
    HandshakeTimeout,
    /// A second `initialize`.
    AlreadyInitialized,
    /// `initialize` failed with this error, which the peer was sent.
    HandshakeFailed(RpcError),
    /// The host is exiting (design 5.2.13).
    HostExiting,
}

/// An audit record in the form of design 5.8.6: the `audit` table's
/// `event`, `principal`, `peer_json` and `detail_json`.
#[derive(Debug, Clone, PartialEq)]
pub struct Formal {
    pub event: String,
    pub principal: Option<PrincipalId>,
    pub peer: Option<Value>,
    pub detail: Value,
}

impl AuditEvent {
    /// Whether the record goes into the `audit` table; `stream.subscribed`
    /// goes to `host.log` only (rulings before T033).
    pub fn kept(&self) -> bool {
        !matches!(self, Self::Subscribed { .. })
    }

    /// The record's event name and contents (design 5.8.6). `host.*`,
    /// `git.unsafe_repo_config` and `privileged.executed` come with their
    /// producers (T163).
    pub fn formal(&self) -> Formal {
        let (event, principal, detail) = match self {
            Self::Accepted {
                pid,
                verdict,
                path,
                identifier,
                team_id,
                ..
            } => {
                let (verdict, error) = match verdict {
                    Verdict::DesktopUi => ("desktop_ui", None),
                    Verdict::Other(refusal) => ("other", refusal.map(|r| code(&r.kind))),
                };
                let peer = json!({"pid": pid, "path": path, "identifier": identifier,
                    "team_id": team_id});
                return Formal {
                    event: "conn.accepted".into(),
                    principal: None,
                    peer: Some(peer),
                    detail: json!({"verdict": verdict, "error": error}),
                };
            }
            Self::Rejected(Rejection::OtherUser { uid }) => (
                "conn.rejected",
                None,
                json!({"error": "other_user", "uid": uid}),
            ),
            Self::Rejected(Rejection::Unidentified(refusal)) => {
                let detail = json!({"error": code(&refusal.kind), "call": refusal.call});
                ("conn.rejected", None, detail)
            }
            Self::Initialized {
                principal,
                client,
                weak_auth,
            } => {
                let detail = json!({"kind": principal.kind, "role": principal.role,
                    "client_kind": client.kind, "client_name": client.name,
                    "client_version": client.version, "weak_auth": weak_auth});
                ("conn.initialized", Some(&principal.id), detail)
            }
            Self::Closed {
                reason,
                after,
                principal,
            } => {
                let mut detail = json!({"reason": code(reason),
                    "after_ms": u64::try_from(after.as_millis()).unwrap_or(u64::MAX)});
                match reason {
                    CloseReason::ReadFailed(kind) => detail["error"] = json!(code(kind)),
                    CloseReason::TokenUnreadable(kind) => detail["error"] = json!(code(kind)),
                    CloseReason::FdPassed { fds } => detail["fds"] = json!(fds),
                    CloseReason::HandshakeFailed(err) => detail["error"] = json!(err),
                    _ => {}
                }
                ("conn.closed", principal.as_ref(), detail)
            }
            Self::Subscribed {
                stream,
                mode,
                reason,
                lag,
            } => {
                let detail = json!({"stream": stream.to_string(), "mode": mode,
                    "reason": reason, "lag": lag});
                ("stream.subscribed", None, detail)
            }
            Self::InteractionResponded {
                session_id,
                request_id,
                outcome,
                principal,
            } => {
                let detail = json!({"session_id": session_id, "request_id": request_id,
                    "outcome": outcome});
                ("interaction.responded", Some(principal), detail)
            }
            Self::ConfigUnconfirmed { reason, config } => (
                "config.unconfirmed",
                Some(&config.set_by),
                configured(reason, config),
            ),
            Self::ConfigConfirmed { reason, config } => (
                "config.confirmed",
                Some(&config.set_by),
                configured(reason, config),
            ),
            Self::Elevation { step, scopes } => {
                let (event, mut detail) = match step {
                    ElevationStep::Requested(id) => ("requested", json!({"elevation_id": id})),
                    ElevationStep::Resolved(id, decision) => {
                        (decision.as_str(), json!({"elevation_id": id}))
                    }
                    ElevationStep::Lineage(signals) => {
                        ("lineage_refused", json!({"signals": signals}))
                    }
                    ElevationStep::RateLimited => ("rate_limited", json!({})),
                };
                detail["scopes"] = json!(scopes);
                return Formal {
                    event: format!("elevation.{event}"),
                    principal: None,
                    peer: None,
                    detail,
                };
            }
            Self::ProjectTrust { project_id, change } => {
                let (event, by, mut detail) = match change {
                    TrustChange::Trusted { by, digest } => {
                        ("trusted", Some(by), json!({"digest": short(digest)}))
                    }
                    TrustChange::Stale { digest } => {
                        ("stale", None, json!({"digest": short(digest)}))
                    }
                    TrustChange::ScanFailed { error } => {
                        ("scan_failed", None, json!({"error": error}))
                    }
                };
                detail["project_id"] = json!(project_id);
                return Formal {
                    event: format!("project.trust.{event}"),
                    principal: by.cloned(),
                    peer: None,
                    detail,
                };
            }
        };
        Formal {
            event: event.into(),
            principal: principal.cloned(),
            peer: None,
            detail,
        }
    }
}

fn configured(reason: &ConfirmationReason, config: &ConfigState) -> Value {
    json!({"session_id": config.session_id, "reason": reason, "mode": config.mode,
        "tier": config.tier})
}

/// The first 16 hex digits of a digest (design 5.8.6).
fn short(digest: &Digest) -> String {
    let hex = digest.as_str().trim_start_matches("d_");
    hex.chars().take(16).collect()
}

/// A variant's name in snake case: `HostExiting` is `host_exiting`.
fn code(value: &impl std::fmt::Debug) -> String {
    let debug = format!("{value:?}");
    let name = debug.split(['(', ' ', '{']).next().unwrap_or_default();
    let mut code = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            code.push('_');
        }
        code.push(c.to_ascii_lowercase());
    }
    code
}

#[cfg(test)]
mod tests {
    use cupper_protocol::domain::{ClientKind, PrincipalKind};

    use super::*;

    /// The event name, principal and detail of `event`.
    fn formal(event: AuditEvent) -> (String, Option<String>, Value) {
        assert!(event.kept());
        let formal = event.formal();
        assert_eq!(formal.peer, None);
        let principal = formal.principal.map(|p| p.to_string());
        (formal.event, principal, formal.detail)
    }

    fn parsed(json: &str) -> Value {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn unit_audit_format_names_and_holds_each_event() {
        let id = |id: &str| id.parse::<PrincipalId>().unwrap();
        let config = ConfigState {
            session_id: SessionId::from_uuid(1),
            mode: Some("bypass".into()),
            tier: Some(Tier::FullAccess),
            set_by: id("pr_test"),
        };
        let reason = ConfirmationReason::TierAboveSetter;
        let principal = Principal {
            id: id("pr_cli"),
            kind: PrincipalKind::Cli,
            role: None,
            scopes: vec![Scope::Read],
        };
        let client = ClientIdentity {
            kind: ClientKind::Cli,
            name: "cupper".into(),
            version: "1.0".into(),
        };
        let after = Duration::from_millis(1_500);
        let closed = |reason| AuditEvent::Closed {
            reason,
            after,
            principal: Some(id("pr_cli")),
        };
        let (elevation, project) = (ElevationId::from_uuid(3), ProjectId::from_uuid(4));
        let elevated = |step| AuditEvent::Elevation {
            step,
            scopes: vec![Scope::Write],
        };
        let trust = |change| AuditEvent::ProjectTrust {
            project_id: project.clone(),
            change,
        };
        let digest: Digest = format!("d_0123456789abcdef{}", "0".repeat(48))
            .parse()
            .unwrap();
        let refusal = Refusal {
            kind: RefusalKind::PeerExited,
            call: "getsockopt(LOCAL_PEERTOKEN)",
            code: 57,
        };
        let (s, e, p) = (
            r#""s_00000000000000000000000000000001""#,
            r#""e_00000000000000000000000000000003""#,
            r#""p_00000000000000000000000000000004""#,
        );
        let configured = format!(
            r#"{{"mode":"bypass","reason":"tier_above_setter","session_id":{s},"tier":"full_access"}}"#
        );
        let cases = [
            (
                AuditEvent::Rejected(Rejection::OtherUser { uid: 502 }),
                ("conn.rejected", None, r#"{"error":"other_user","uid":502}"#.into()),
            ),
            (
                AuditEvent::Rejected(Rejection::Unidentified(refusal)),
                ("conn.rejected", None, r#"{"call":"getsockopt(LOCAL_PEERTOKEN)","error":"peer_exited"}"#.into()),
            ),
            (
                AuditEvent::Initialized { principal, client, weak_auth: true },
                ("conn.initialized", Some("pr_cli"), r#"{"client_kind":"cli","client_name":"cupper","client_version":"1.0","kind":"cli","role":null,"weak_auth":true}"#.into()),
            ),
            (
                closed(CloseReason::HostExiting),
                ("conn.closed", Some("pr_cli"), r#"{"after_ms":1500,"reason":"host_exiting"}"#.into()),
            ),
            (
                AuditEvent::InteractionResponded {
                    session_id: config.session_id.clone(),
                    request_id: RequestId::from_uuid(2),
                    outcome: InteractionOutcome::Dismissed,
                    principal: id("pr_test"),
                },
                ("interaction.responded", Some("pr_test"), format!(r#"{{"outcome":{{"type":"dismissed"}},"request_id":"r_00000000000000000000000000000002","session_id":{s}}}"#)),
            ),
            (
                AuditEvent::ConfigUnconfirmed { reason: reason.clone(), config: config.clone() },
                ("config.unconfirmed", Some("pr_test"), configured.clone()),
            ),
            (
                AuditEvent::ConfigConfirmed { reason, config },
                ("config.confirmed", Some("pr_test"), configured),
            ),
            (
                elevated(ElevationStep::Requested(elevation.clone())),
                ("elevation.requested", None, format!(r#"{{"elevation_id":{e},"scopes":["write"]}}"#)),
            ),
            (
                elevated(ElevationStep::Resolved(elevation, ElevationDecision::Granted)),
                ("elevation.granted", None, format!(r#"{{"elevation_id":{e},"scopes":["write"]}}"#)),
            ),
            (
                elevated(ElevationStep::Lineage(vec!["sandboxed".into()])),
                ("elevation.lineage_refused", None, r#"{"scopes":["write"],"signals":["sandboxed"]}"#.into()),
            ),
            (
                elevated(ElevationStep::RateLimited),
                ("elevation.rate_limited", None, r#"{"scopes":["write"]}"#.into()),
            ),
            (
                trust(TrustChange::Trusted { by: id("pr_desktop"), digest: digest.clone() }),
                ("project.trust.trusted", Some("pr_desktop"), format!(r#"{{"digest":"0123456789abcdef","project_id":{p}}}"#)),
            ),
            (
                trust(TrustChange::Stale { digest }),
                ("project.trust.stale", None, format!(r#"{{"digest":"0123456789abcdef","project_id":{p}}}"#)),
            ),
            (
                trust(TrustChange::ScanFailed { error: "link_cycle".into() }),
                ("project.trust.scan_failed", None, format!(r#"{{"error":"link_cycle","project_id":{p}}}"#)),
            ),
        ];
        for (event, (name, principal, detail)) in cases {
            let expected = (
                name.to_owned(),
                principal.map(str::to_owned),
                parsed(&detail),
            );
            assert_eq!(formal(event), expected);
        }
    }

    #[test]
    fn unit_audit_format_accepted_carries_the_peer_and_subscribed_stays_out() {
        let accepted = AuditEvent::Accepted {
            pid: 7,
            pid_version: 1,
            verdict: Verdict::Other(None),
            path: Some("/usr/local/bin/cupper".into()),
            identifier: Some("dev.cupper.cli".into()),
            team_id: None,
        };
        let formal = accepted.formal();
        let peer = r#"{"identifier":"dev.cupper.cli","path":"/usr/local/bin/cupper","pid":7,"team_id":null}"#;
        assert_eq!(formal.event, "conn.accepted");
        assert_eq!(formal.peer, Some(parsed(peer)));
        assert_eq!(formal.detail, parsed(r#"{"error":null,"verdict":"other"}"#));
        let subscribed = AuditEvent::Subscribed {
            stream: StreamId::Index,
            mode: SubscribeMode::Snapshot,
            reason: None,
            lag: None,
        };
        assert!(!subscribed.kept());
        assert_eq!(subscribed.formal().event, "stream.subscribed");
    }
}
