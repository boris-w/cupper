//! The connections that finished `initialize` (design 5.2.13): `host/status`
//! lists them, and the orderly exit sends each `host/shutdown`. The index
//! (T034) builds `clients.updated` from the same table.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cupper_protocol::annotations::TimestampMs;
use cupper_protocol::domain::PrincipalKind;
use cupper_protocol::ids::ConnId;
use cupper_protocol::index::{ClientAuth, ClientInfo};

use super::dispatch::Opened;
use super::outbox::Outbox;

#[derive(Debug, Default)]
pub(super) struct Clients(Mutex<HashMap<ConnId, Client>>);

#[derive(Debug)]
struct Client {
    opened: Opened,
    /// When `initialize` succeeded, in UTC ms.
    since_ms: u64,
    outbox: Arc<Outbox>,
}

impl Clients {
    pub(super) fn add(&self, opened: Opened, since_ms: u64, outbox: Arc<Outbox>) {
        let conn = opened.conn.clone();
        let client = Client {
            opened,
            since_ms,
            outbox,
        };
        self.lock().insert(conn, client);
    }

    pub(super) fn remove(&self, conn: &ConnId) {
        self.lock().remove(conn);
    }

    /// Oldest first.
    pub(super) fn infos(&self) -> Vec<ClientInfo> {
        let mut infos: Vec<ClientInfo> = self.lock().values().map(Client::info).collect();
        infos.sort_by(|a, b| (a.since, a.conn_id.as_str()).cmp(&(b.since, b.conn_id.as_str())));
        infos
    }

    pub(super) fn outboxes(&self) -> Vec<Arc<Outbox>> {
        self.lock()
            .values()
            .map(|client| Arc::clone(&client.outbox))
            .collect()
    }

    /// Every critical section leaves the map consistent.
    fn lock(&self) -> MutexGuard<'_, HashMap<ConnId, Client>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Client {
    fn info(&self) -> ClientInfo {
        let Opened {
            conn,
            principal,
            client,
            weak_auth,
            token: _,
            granted,
        } = &self.opened;
        // How the principal was authenticated (design 5.8.1, 5.8.2).
        let auth = match principal.kind {
            PrincipalKind::DesktopUi if !weak_auth => ClientAuth::Verified,
            PrincipalKind::DesktopUi | PrincipalKind::TestHarness => ClientAuth::Weak,
            _ => ClientAuth::Peer,
        };
        ClientInfo {
            conn_id: conn.clone(),
            principal_kind: principal.kind.clone(),
            role: principal.role.clone(),
            client_name: client.name.clone(),
            client_version: client.version.clone(),
            since: TimestampMs(self.since_ms),
            auth,
            elevated: granted.get(),
        }
    }
}
