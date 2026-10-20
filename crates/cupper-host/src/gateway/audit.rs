//! Connection audit records (design 5.8.6). The gateway sends them into a
//! channel and never waits on it; the host's assembly writes them into the
//! `audit` table and `host.log` (T037). The executable path, signing
//! identifier and Team ID of a peer are T038's.

use std::io;
use std::sync::Arc;
use std::time::Duration;

use cupper_protocol::error::RpcError;
use cupper_protocol::ids::{ConnId, ElevationId, PrincipalId, RequestId, SessionId};
use cupper_protocol::index::ElevationDecision;
use cupper_protocol::interaction::InteractionOutcome;
use cupper_protocol::methods::handshake::{ClientIdentity, Principal};
use cupper_protocol::rpc::Scope;
use cupper_protocol::session::ConfirmationReason;
use cupper_protocol::sync::{ResyncReason, StreamId, SubscribeMode};
use cupper_sys::{Refusal, RefusalKind};
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
    /// on its signature.
    Accepted {
        pid: i32,
        pid_version: u32,
        verdict: Verdict,
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
    /// `session.config_unconfirmed`: a session's tier is above what its
    /// setter may set, or `full_access` without a native confirmation
    /// (design 5.2.6).
    ConfigUnconfirmed {
        session_id: SessionId,
        reason: ConfirmationReason,
        /// The permission mode the agent reported.
        mode: Option<String>,
    },
    /// `elevation.*`: a step of a CLI's request for scopes on this
    /// connection, with the scopes asked for, or those granted (design 5.8.6).
    Elevation {
        step: ElevationStep,
        scopes: Vec<Scope>,
    },
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
