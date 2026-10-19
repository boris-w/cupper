//! Test helpers for the gateway, for this crate's tests and for test-harness
//! integration tests.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use cupper_protocol::frame::{self, FrameDecoder, FrameType};
use cupper_protocol::ids::HostId;
use cupper_protocol::proof::{self, Bytes32, Channel, Direction};
use cupper_sys::Requirement;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::JoinHandle;

use super::listen::HostLock;
use super::{AuditEvent, AuditRecord, CloseReason, Gateway, GatewayConfig, Routes};
use crate::clock::{Clock, SystemClock};
use crate::durations::Durations;

/// The key of the test gateway's `dev_key` and `harness_key`.
pub const KEY: Bytes32 = [0x5a; 32];

/// A fresh directory under the system temp dir, removed on drop. Its path
/// stays short enough for a socket in `run/` (design 5.2.13).
#[derive(Debug)]
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(name: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("cupper-gw-{}-{unique}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create a temp dir");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A gateway on a socket in a fresh data directory, serving until dropped.
pub struct TestGateway {
    pub socket: PathBuf,
    pub audit: UnboundedReceiver<AuditRecord>,
    serving: JoinHandle<()>,
    _lock: HostLock,
    _dir: TempDir,
}

impl TestGateway {
    /// Starts serving [`config`](Self::config) on the system clock, with
    /// the default routes; call within a tokio runtime.
    pub fn start(ui_requirement: Option<Requirement>) -> Self {
        let clock = Arc::new(SystemClock::new());
        Self::with(Self::config(ui_requirement), clock, Routes::default())
    }

    /// Host 1 at epoch 1, and [`KEY`] for `dev_key`, as started by this
    /// process, and for `harness`.
    pub fn config(ui_requirement: Option<Requirement>) -> GatewayConfig {
        #[cfg(any(feature = "dev-auth", feature = "test-harness"))]
        let key: crate::startup::payload::SecretKey =
            serde_json::from_value(json!(proof::to_hex(&KEY))).expect("a key");
        GatewayConfig {
            host_id: HostId::from_uuid(1),
            data_epoch: 1,
            ui_requirement,
            #[cfg(feature = "dev-auth")]
            dev_key: Some(super::DevKey {
                key: key.clone(),
                launcher: cupper_sys::unique_ids(std::process::id() as i32)
                    .expect("our own ids")
                    .unique_id,
            }),
            #[cfg(feature = "test-harness")]
            harness_key: Some(key),
        }
    }

    pub fn with(config: GatewayConfig, clock: Arc<dyn Clock>, routes: Routes) -> Self {
        let dir = TempDir::new("gateway");
        let lock = HostLock::acquire(dir.path()).expect("take host.lock");
        let (listener, socket) = lock.bind().expect("bind the socket");
        let (gateway, audit) = Gateway::new(config, clock, Durations::default(), routes);
        Self {
            socket,
            audit,
            serving: tokio::spawn(gateway.serve(listener)),
            _lock: lock,
            _dir: dir,
        }
    }

    pub async fn record(&mut self) -> AuditRecord {
        self.audit.recv().await.expect("an audit record")
    }

    /// Why the next connection to close closed.
    pub async fn closed(&mut self) -> CloseReason {
        loop {
            if let AuditEvent::Closed { reason, .. } = self.record().await.event {
                return reason;
            }
        }
    }
}

impl Drop for TestGateway {
    fn drop(&mut self) {
        self.serving.abort();
    }
}

/// A client for tests: JSON values in and out.
pub struct Client {
    stream: UnixStream,
    decoder: FrameDecoder,
    next_id: i64,
}

impl Client {
    pub async fn connect(socket: &Path) -> Self {
        Self {
            stream: UnixStream::connect(socket).await.expect("connect"),
            decoder: FrameDecoder::new(),
            next_id: 0,
        }
    }

    pub fn stream(&self) -> &UnixStream {
        &self.stream
    }

    pub async fn send_frame(&mut self, frame: &[u8]) {
        self.stream.write_all(frame).await.expect("send");
    }

    pub async fn send(&mut self, message: &Value) {
        self.send_frame(&frame::encode_json(message).expect("a frame"))
            .await;
    }

    /// The next message; `None` once the host has closed the connection.
    pub async fn next(&mut self) -> Option<Value> {
        let mut buf = vec![0; 4096];
        loop {
            if let Some(frame) = self.decoder.next_frame().expect("well-formed frames") {
                assert_eq!(frame.frame_type, FrameType::Json);
                return Some(serde_json::from_slice(&frame.payload).expect("JSON"));
            }
            // A connection the host ends may also read as reset.
            let read = self.stream.read(&mut buf).await.unwrap_or(0);
            if read == 0 {
                return None;
            }
            self.decoder.push(&buf[..read]);
        }
    }

    /// The nonce of `host/hello`, which comes first.
    pub async fn hello(&mut self) -> String {
        let hello = self.next().await.expect("host/hello");
        assert_eq!(hello["method"], "host/hello", "{hello}");
        hello["params"]["nonce"]
            .as_str()
            .expect("a nonce")
            .to_owned()
    }

    /// Sends a request, `params` left out when null, and returns its
    /// `result`, or its `error`.
    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value, Value> {
        self.next_id += 1;
        let id = self.next_id;
        let mut request = json!({"jsonrpc": "2.0", "id": id, "method": method});
        if !params.is_null() {
            request["params"] = params;
        }
        self.send(&request).await;
        let response = self.next().await.expect("a response");
        assert_eq!(response["id"], id, "{response}");
        match response.get("error") {
            Some(error) => Err(error.clone()),
            None => Ok(response["result"].clone()),
        }
    }

    /// `host/hello`, then `initialize` with `auth`.
    pub async fn handshake(&mut self, auth: Value) -> Result<Value, Value> {
        let nonce = self.hello().await;
        self.call("initialize", initialize_params(&nonce, auth))
            .await
    }
}

/// `initialize` of a `cli` client offering protocol 1 and no features.
pub fn initialize_params(nonce: &str, auth: Value) -> Value {
    json!({"protocol": {"min": 1, "max": 1}, "nonce": nonce, "auth": auth, "features": [],
        "client": {"kind": "cli", "name": "testkit", "version": "0"}})
}

/// `auth` of `kind` (`dev_key` or `harness`): the proof of `key` by process
/// `pid` for the `nonce` of `host/hello`; and the channel of `host_proof`.
pub fn key_auth(kind: &str, key: &Bytes32, nonce: &str, pid: i32) -> (Value, Channel) {
    let h_nonce = proof::parse_hex32(nonce).expect("a nonce of 64 hex digits");
    let channel = Channel {
        h_nonce,
        c_nonce: [0xc0; 32],
    };
    let proof = channel.prove(key, Direction::ClientToHost, pid);
    let c_nonce = proof::to_hex(&channel.c_nonce);
    let auth = json!({"type": kind, "c_nonce": c_nonce, "proof": proof});
    (auth, channel)
}

/// One connection's dispatch, driven as the read loop drives it, and the
/// replies it queued: requests with any principal, made in the crate.
#[cfg(test)]
pub(crate) struct Peer {
    pub(super) dispatch: super::dispatch::Dispatch,
    outbox: Arc<super::outbox::Outbox>,
    replies: Vec<Value>,
    next_id: i64,
}

#[cfg(test)]
impl Peer {
    pub(crate) fn new(principal: cupper_protocol::methods::handshake::Principal) -> Self {
        let outbox = Arc::new(super::outbox::Outbox::new(
            cupper_protocol::limits::V1.max_queue_bytes as usize,
        ));
        let conn = cupper_protocol::ids::ConnId::from_index(1);
        Self {
            dispatch: super::dispatch::Dispatch::new(conn, principal, Arc::clone(&outbox)),
            outbox,
            replies: Vec::new(),
            next_id: 0,
        }
    }

    /// Sends a request; returns its id.
    pub(crate) fn send(&mut self, routes: &Routes, method: &str, params: Value) -> i64 {
        self.next_id += 1;
        let id = cupper_protocol::rpc::RpcId::Number(self.next_id);
        self.dispatch.request(routes, id, method, params);
        self.next_id
    }

    pub(crate) async fn call(
        &mut self,
        routes: &Routes,
        method: &str,
        params: Value,
    ) -> Result<Value, Value> {
        let id = self.send(routes, method, params);
        self.reply(id).await
    }

    /// The reply to request `id`: its `result`, or its `error`.
    pub(crate) async fn reply(&mut self, id: i64) -> Result<Value, Value> {
        loop {
            if let Some(at) = self.replies.iter().position(|reply| reply["id"] == id) {
                let reply = self.replies.remove(at);
                return match reply.get("error") {
                    Some(error) => Err(error.clone()),
                    None => Ok(reply["result"].clone()),
                };
            }
            let frames = self.outbox.drain().await.expect("the outbox is open");
            self.take(frames);
        }
    }

    /// The ids of the replies queued once the tasks that can run have run.
    pub(crate) async fn answered(&mut self) -> Vec<i64> {
        for _ in 0..16 {
            tokio::task::yield_now().await;
        }
        if let Some(Some(frames)) = futures::FutureExt::now_or_never(self.outbox.drain()) {
            self.take(frames);
        }
        let ids = self.replies.iter().map(|reply| reply["id"].as_i64());
        ids.map(Option::unwrap).collect()
    }

    fn take(&mut self, frames: Vec<Vec<u8>>) {
        let mut decoder = FrameDecoder::new();
        for frame in frames {
            decoder.push(&frame);
            let payload = decoder.next_frame().unwrap().unwrap().payload;
            self.replies.push(serde_json::from_slice(&payload).unwrap());
        }
    }
}
