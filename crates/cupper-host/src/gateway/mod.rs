//! The gateway (design 5.2.4): the host socket, the connections on it, and
//! local authentication (design 5.8.1, ADR-05).
//!
//! [`listen`] takes `host.lock` and binds the socket; [`Gateway::serve`]
//! accepts on it. Each connection runs in its own task (`conn`), with a
//! writer task draining its [`Outbox`](outbox::Outbox): admission, then
//! `host/hello`, then exactly one `initialize` (`handshake`), then requests
//! and client notifications, which [`dispatch`] hands to the executors in
//! [`Routes`]. The gateway serves subscriptions (`subscribe`, sent by the
//! writer in `send`) and history pages (`history`) itself; blobs are T029's.
//!
//! On the way out (design 5.2.13) the host stops [`Gateway::serve`], sends
//! [`Gateway::broadcast`] `host/shutdown`, and at last ends every connection
//! with [`Gateway::close_all`].

mod audit;
mod clients;
mod conn;
mod describe;
pub mod dispatch;
mod handshake;
mod history;
pub mod listen;
pub mod outbox;
pub mod routes;
mod send;
mod subscribe;
#[cfg(any(test, feature = "test-harness"))]
pub mod testkit;
#[cfg(test)]
mod tests;
pub mod wire;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use cupper_protocol::ids::{ConnId, HostId};
use cupper_protocol::index::ClientInfo;
use cupper_protocol::rpc::HostNotification;
use cupper_sys::{Refusal, Requirement};
use tokio::net::UnixListener;
use tokio::sync::{mpsc, watch};

use self::audit::Auditor;
pub use self::audit::{AuditEvent, AuditRecord, CloseReason, ElevationStep, Rejection};
pub use self::dispatch::{Call, Granted, IndexPort, Opened, Port, Reply, SessionPort};
pub use self::handshake::Session;
pub use self::routes::Routes;
use crate::clock::Clock;
use crate::durations::{Durations, GATEWAY_ACCEPT_BACKOFF};
use crate::quiescence::{ActorKind, Participant};
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
    clients: clients::Clients,
    /// Set once the host exits: every connection ends.
    closing: watch::Sender<bool>,
    /// How many connection tasks run.
    live: watch::Sender<usize>,
}

impl Gateway {
    /// A gateway serving `routes`, recording into `audit`, the host's audit
    /// channel, which the session actors write into too.
    pub fn new(
        config: GatewayConfig,
        clock: Arc<dyn Clock>,
        durations: Durations,
        routes: Routes,
        audit: mpsc::UnboundedSender<AuditRecord>,
    ) -> Arc<Self> {
        let gateway = Self {
            config,
            clock,
            durations,
            audit,
            connections: AtomicU64::new(0),
            routes,
            clients: clients::Clients::default(),
            closing: watch::Sender::new(false),
            live: watch::Sender::new(0),
        };
        Arc::new(gateway)
    }

    /// Accepts connections on `listener` for as long as the task runs; each
    /// connection gets a task of its own. Aborting the task stops accepting.
    pub async fn serve(self: Arc<Self>, listener: UnixListener) {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let index = self.connections.fetch_add(1, Ordering::Relaxed) + 1;
                    let live = Live::enter(Arc::clone(&self));
                    tokio::spawn(conn::run(live, stream, ConnId::from_index(index)));
                }
                // Out of fds, for one: wait instead of spinning.
                Err(_) => {
                    let backoff = self.durations.get(GATEWAY_ACCEPT_BACKOFF);
                    self.clock.timer_at(self.clock.now_mono() + backoff).await;
                }
            }
        }
    }

    /// The initialized connections, oldest first, as `host/status` and
    /// `clients.updated` list them.
    pub fn clients(&self) -> Vec<ClientInfo> {
        self.clients.infos()
    }

    /// Queues `N` on every initialized connection.
    pub fn broadcast<N: HostNotification>(&self, params: &N::Params) {
        let frame = wire::notification::<N>(params);
        for outbox in self.clients.outboxes() {
            outbox.push_frame(frame.clone());
        }
    }

    /// Ends every connection once what is queued on it has gone out, or its
    /// close grace has passed, and returns when all have ended. Stop
    /// [`serve`](Self::serve) first.
    pub async fn close_all(&self) {
        self.closing.send_replace(true);
        let _ = self.live.subscribe().wait_for(|live| *live == 0).await;
    }

    fn audit(&self, conn: &ConnId, event: AuditEvent) {
        let at_ms = self.clock.now_wall_ms();
        let conn = Some(conn.clone());
        // Nobody listening is the receiver's choice.
        let _ = self.audit.send(AuditRecord { at_ms, conn, event });
    }

    /// The audit channel for what connection `conn` records later.
    fn auditor(&self, conn: &ConnId) -> Auditor {
        let clock = Arc::clone(&self.clock);
        Auditor::new(conn.clone(), self.audit.clone(), clock)
    }
}

/// A running connection task, counted in `Gateway::live`.
/// The kinds of the gateway's tasks in the quiescence barrier.
pub const CONN: ActorKind = ActorKind::new("gateway_conn");
pub const SEND: ActorKind = ActorKind::new("gateway_send");
pub const REPLY: ActorKind = ActorKind::new("gateway_reply");
pub const SUBSCRIPTIONS: ActorKind = ActorKind::new("gateway_subscriptions");
pub const KINDS: [ActorKind; 4] = [CONN, SEND, REPLY, SUBSCRIPTIONS];

/// A running connection task, counted in `Gateway::live`; it waits on its
/// peer as an idle participant.
struct Live(Arc<Gateway>, Participant);

impl Live {
    fn enter(gateway: Arc<Gateway>) -> Self {
        gateway.live.send_modify(|live| *live += 1);
        let participant = gateway.routes.quiescence().enter(CONN);
        Self(gateway, participant)
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.0.live.send_modify(|live| *live -= 1);
    }
}
