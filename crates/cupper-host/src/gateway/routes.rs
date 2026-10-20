//! The method table (design 5.2.4, 5.3.4): which methods this host serves,
//! and who serves each. A method nobody registered is `unsupported` (plan
//! 1.2); UNIT-unsupported-set holds the set of those to the milestones.
//!
//! Executors register their methods when the host is assembled:
//! [`Routes::add`] for a request, [`Routes::notification`] for a client
//! notification. A method with `x-builds` is registered only in those builds.
//! [`Routes::sessions`] and [`Routes::index`] register the M1a methods of the
//! session and index ports, and [`Routes::streams`] the subscriptions and
//! history pages the gateway serves itself; the tasks that bring more add
//! them.

use std::collections::HashMap;
use std::sync::Arc;

use cupper_protocol::error::RpcError;
use cupper_protocol::ids::ConnId;
use cupper_protocol::methods::admin::PolicyDescribe;
use cupper_protocol::methods::handshake::{ClientPresence, Ping};
use cupper_protocol::methods::interaction::{InteractionGet, InteractionRespond};
use cupper_protocol::methods::project::{ProjectAdd, ProjectTrust, ProjectTrustInfo};
use cupper_protocol::methods::session::{
    SessionCapabilities, SessionConfirmConfig, SessionCreate, SessionGet, SessionInterrupt,
    SessionMarkRead, SessionQueueResume, SessionSend,
};
use cupper_protocol::methods::stream::{
    BlobGet, BlobPut, HistoryPage, StreamSubscribe, StreamUnsubscribe,
};
use cupper_protocol::rpc::{Build, ClientNotification, Idempotency, Method, Scope};
use rusqlite::Connection;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::task::JoinHandle;

use super::dispatch::{IndexPort, Opened, Port, SessionPort, typed};
use crate::clock::Clock;
use crate::durations::Durations;
use crate::ledger::{Ledger, LedgerError};
use crate::store::streams::Streams;
use crate::store::{Readers, Writer};

/// The methods this host serves, and the ledger their commands go through.
/// [`Routes::default`] serves what the gateway answers itself, and no
/// commands.
#[derive(Debug)]
pub struct Routes {
    requests: HashMap<&'static str, Entry>,
    notifications: HashMap<&'static str, Entry>,
    ledger: Option<Ledger>,
    index: Option<Arc<dyn IndexPort>>,
}

#[derive(Debug)]
pub(super) struct Entry {
    pub scope: Option<Scope>,
    pub idempotency: Idempotency,
    /// Parses the params as the method's type, or fails with invalid params.
    pub validate: fn(&Value) -> Result<(), RpcError>,
    pub target: Target,
}

/// Who answers a method.
#[derive(Debug)]
pub(super) enum Target {
    Port(Arc<dyn Port>),
    Ping,
    /// `policy/describe`, with the session port's facts.
    Describe(Arc<dyn SessionPort>),
    Subscribe(Source),
    Unsubscribe,
    History(Source),
    /// `blob/get`, `blob/put`: the gateway serves them itself from T029 on,
    /// and `unsupported` until then.
    Blobs,
}

/// What the gateway serves subscriptions and history pages from.
#[derive(Debug, Clone)]
pub(super) struct Source {
    pub streams: Streams,
    /// Replays and history pages read through these.
    pub readers: Readers,
}

impl Target {
    /// Whether the method is served; until it is, it is `unsupported` like
    /// a method nobody registered.
    pub(super) fn serves(&self) -> bool {
        !matches!(self, Self::Blobs)
    }
}

impl Default for Routes {
    fn default() -> Self {
        let mut routes = Self {
            requests: HashMap::new(),
            notifications: HashMap::new(),
            ledger: None,
            index: None,
        };
        routes.insert::<Ping>(Target::Ping);
        routes.insert::<BlobGet>(Target::Blobs);
        routes.insert::<BlobPut>(Target::Blobs);
        routes
    }
}

impl Routes {
    /// Routes whose commands go through `ledger`.
    pub fn new(ledger: Ledger) -> Self {
        Self {
            ledger: Some(ledger),
            ..Self::default()
        }
    }

    /// The host's routes as it starts (design 5.2.13; the order of the steps
    /// is T037's): the ledger loaded through `conn`, a `host.db` connection,
    /// and writing through `writer`; its sweeper, spawned (call within a tokio
    /// runtime; abort the handle on the way out); the session and index
    /// ports with their methods.
    pub fn start(
        conn: &Connection,
        writer: Writer,
        clock: Arc<dyn Clock>,
        durations: &Durations,
        sessions: Arc<dyn SessionPort>,
        index: Arc<dyn IndexPort>,
    ) -> Result<(Self, JoinHandle<()>), LedgerError> {
        let ledger = Ledger::load(conn, writer, clock, durations)?;
        let sweeper = tokio::spawn(ledger.clone().sweeper());
        let mut routes = Self::new(ledger);
        routes.sessions(sessions);
        routes.index(Arc::clone(&index));
        routes.presence(index);
        Ok((routes, sweeper))
    }

    /// `port` serves the request `M` in the builds of `M::BUILDS`. A command
    /// needs routes with a ledger.
    pub fn add<M: Method>(&mut self, port: Arc<dyn Port>) {
        self.insert::<M>(Target::Port(port));
    }

    /// `port` serves the client notification `N`.
    pub fn notification<N: ClientNotification>(&mut self, port: Arc<dyn Port>) {
        let entry = Entry {
            scope: N::SCOPE,
            idempotency: N::IDEMPOTENCY,
            validate: validate::<N::Params>,
            target: Target::Port(port),
        };
        insert(&mut self.notifications, N::NAME, entry);
    }

    /// The session port (T043) serves the session methods of M1a and the
    /// facts of `policy/describe`, in order per connection (design 5.2.4).
    pub fn sessions(&mut self, port: Arc<dyn SessionPort>) {
        let to = || Arc::clone(&port) as Arc<dyn Port>;
        self.add::<SessionCreate>(to());
        self.add::<SessionSend>(to());
        self.add::<SessionInterrupt>(to());
        self.add::<SessionQueueResume>(to());
        self.add::<SessionConfirmConfig>(to());
        self.add::<SessionGet>(to());
        self.add::<SessionCapabilities>(to());
        self.add::<InteractionRespond>(to());
        self.add::<InteractionGet>(to());
        self.insert::<PolicyDescribe>(Target::Describe(port));
    }

    /// The index port (T034) serves the project methods of M1a, and hears
    /// of every connection that opens and closes.
    pub fn index(&mut self, port: Arc<dyn IndexPort>) {
        let to = || Arc::clone(&port) as Arc<dyn Port>;
        self.add::<ProjectAdd>(to());
        self.add::<ProjectTrust>(to());
        self.add::<ProjectTrustInfo>(to());
        self.index = Some(port);
    }

    /// The index port also serves read marks and presence (T100).
    pub fn presence(&mut self, port: Arc<dyn IndexPort>) {
        self.add::<SessionMarkRead>(Arc::clone(&port) as Arc<dyn Port>);
        self.notification::<ClientPresence>(port);
    }

    /// The gateway serves `stream/subscribe`, `stream/unsubscribe` and
    /// `history/page` from `streams`, reading replays and history pages
    /// through `readers`.
    pub fn streams(&mut self, streams: Streams, readers: Readers) {
        let source = Source { streams, readers };
        self.insert::<StreamSubscribe>(Target::Subscribe(source.clone()));
        self.insert::<StreamUnsubscribe>(Target::Unsubscribe);
        self.insert::<HistoryPage>(Target::History(source));
    }

    pub(super) fn request_entry(&self, method: &str) -> Option<(&'static str, &Entry)> {
        let (name, entry) = self.requests.get_key_value(method)?;
        Some((name, entry))
    }

    pub(super) fn notification_entry(&self, method: &str) -> Option<(&'static str, &Entry)> {
        let (name, entry) = self.notifications.get_key_value(method)?;
        Some((name, entry))
    }

    pub(super) fn ledger(&self) -> Option<&Ledger> {
        self.ledger.as_ref()
    }

    pub(super) fn opened(&self, client: Opened) {
        if let Some(index) = &self.index {
            index.opened(client);
        }
    }

    pub(super) fn closed(&self, conn: &ConnId) {
        if let Some(index) = &self.index {
            index.closed(conn);
        }
    }

    fn insert<M: Method>(&mut self, target: Target) {
        if !in_build(M::BUILDS) {
            return;
        }
        let command = M::IDEMPOTENCY == Idempotency::Command;
        assert!(
            !command || self.ledger.is_some(),
            "{} is a command: route it with a ledger",
            M::NAME
        );
        let entry = Entry {
            scope: M::SCOPE,
            idempotency: M::IDEMPOTENCY,
            validate: validate::<M::Params>,
            target,
        };
        insert(&mut self.requests, M::NAME, entry);
    }
}

fn insert(table: &mut HashMap<&'static str, Entry>, name: &'static str, entry: Entry) {
    assert!(table.insert(name, entry).is_none(), "{name} routed twice");
}

fn validate<P: DeserializeOwned>(params: &Value) -> Result<(), RpcError> {
    typed::<P>(params).map(drop)
}

/// This build has a method of `builds` (`x-builds`; empty: every build).
fn in_build(builds: &[Build]) -> bool {
    builds.is_empty()
        || builds.iter().any(|build| match build {
            Build::TestHarness => cfg!(feature = "test-harness"),
            Build::DevAuth => cfg!(feature = "dev-auth"),
        })
}
