//! Connection audit records (design 5.8.6). The gateway sends them into a
//! channel and never waits on it; the host's assembly writes them into the
//! `audit` table and `host.log` (T037). The executable path, signing
//! identifier and Team ID of a peer are T038's.

use std::io;
use std::time::Duration;

use cupper_protocol::error::RpcError;
use cupper_protocol::ids::{ConnId, PrincipalId};
use cupper_protocol::methods::handshake::{ClientIdentity, Principal};
use cupper_sys::{Refusal, RefusalKind};

use super::Verdict;

#[derive(Debug, Clone, PartialEq)]
pub struct AuditRecord {
    /// Milliseconds since the Unix epoch.
    pub at_ms: u64,
    pub conn: ConnId,
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
}
