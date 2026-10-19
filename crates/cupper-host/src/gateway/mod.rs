//! The gateway (design 5.2.4): the host socket, the connections on it, and
//! local authentication (design 5.8.1, ADR-05).
//!
//! [`listen`] takes `host.lock` and binds the socket; [`Gateway::serve`]
//! accepts on it. Each connection runs in its own task (`conn`), with a
//! writer task draining its [`Outbox`](outbox::Outbox): admission, then
//! `host/hello`, then exactly one `initialize` (`handshake`), then requests
//! and client notifications, which [`dispatch`] hands to the executors in
//! [`Routes`]. Subscriptions and history are T019's, blobs T029's.

mod audit;
mod conn;
mod describe;
pub mod dispatch;
mod handshake;
pub mod listen;
pub mod outbox;
pub mod routes;
#[cfg(any(test, feature = "test-harness"))]
pub mod testkit;
#[cfg(test)]
mod tests;
pub mod wire;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use cupper_protocol::ids::{ConnId, HostId};
use cupper_sys::{Refusal, Requirement};
use tokio::net::UnixListener;
use tokio::sync::mpsc;

pub use self::audit::{AuditEvent, AuditRecord, CloseReason, Rejection};
pub use self::dispatch::{Call, IndexPort, Opened, Port, Reply, SessionPort};
pub use self::handshake::Session;
pub use self::routes::Routes;
use crate::clock::Clock;
use crate::durations::{Durations, GATEWAY_ACCEPT_BACKOFF};
#[cfg(any(feature = "dev-auth", feature = "test-harness"))]
use crate::startup::payload::SecretKey;

/// The verdict on a peer's code signature, taken on accept (design 5.8.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The desktop UI's main process.
    DesktopUi,
    /// Anyone else, with why verification failed when it ran.
    Other(Option<Refusal>),
}

#[derive(Debug, Clone)]
pub struct GatewayConfig {
    pub host_id: HostId,
    /// `meta.data_epoch` of this run (design 5.2.8).
    pub data_epoch: u64,
    /// The code requirement of the desktop UI's main process, built into
    /// the binary (design 5.8.1). Release builds get theirs with the Team
    /// ID (T180); without one, no peer is the UI by signature.
    pub ui_requirement: Option<Requirement>,
    /// The startup payload's `dev_key`, and who may present it.
    #[cfg(feature = "dev-auth")]
    pub dev_key: Option<DevKey>,
    /// The startup payload's `harness_key`.
    #[cfg(feature = "test-harness")]
    pub harness_key: Option<SecretKey>,
}

/// `dev_key` is only accepted from the process that started the host
/// (design 5.8.1).
#[cfg(feature = "dev-auth")]
#[derive(Debug, Clone)]
pub struct DevKey {
    pub key: SecretKey,
    /// Its unique id, from [`launcher`](crate::startup::launcher).
    pub launcher: u64,
}

#[derive(Debug)]
pub struct Gateway {
    config: GatewayConfig,
    clock: Arc<dyn Clock>,
    durations: Durations,
    audit: mpsc::UnboundedSender<AuditRecord>,
    connections: AtomicU64,
    routes: Routes,
}

impl Gateway {
    /// A gateway serving `routes`, and the receiving end of its audit
    /// records.
    pub fn new(
        config: GatewayConfig,
        clock: Arc<dyn Clock>,
        durations: Durations,
        routes: Routes,
    ) -> (Arc<Self>, mpsc::UnboundedReceiver<AuditRecord>) {
        let (audit, records) = mpsc::unbounded_channel();
        let gateway = Self {
            config,
            clock,
            durations,
            audit,
            connections: AtomicU64::new(0),
            routes,
        };
        (Arc::new(gateway), records)
    }

    /// Accepts connections on `listener` for as long as the task runs; each
    /// connection gets a task of its own.
    pub async fn serve(self: Arc<Self>, listener: UnixListener) {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let index = self.connections.fetch_add(1, Ordering::Relaxed) + 1;
                    let gateway = Arc::clone(&self);
                    tokio::spawn(conn::run(gateway, stream, ConnId::from_index(index)));
                }
                // Out of fds, for one: wait instead of spinning.
                Err(_) => {
                    let backoff = self.durations.get(GATEWAY_ACCEPT_BACKOFF);
                    self.clock.timer_at(self.clock.now_mono() + backoff).await;
                }
            }
        }
    }

    fn audit(&self, conn: &ConnId, event: AuditEvent) {
        let at_ms = self.clock.now_wall_ms();
        let conn = conn.clone();
        // Nobody listening is the receiver's choice.
        let _ = self.audit.send(AuditRecord { at_ms, conn, event });
    }
}
