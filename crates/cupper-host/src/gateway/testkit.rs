//! Test helpers for the gateway, for this crate's tests and for test-harness
//! integration tests.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use cupper_protocol::frame::{self, FrameDecoder, FrameType};
use cupper_protocol::ids::HostId;
use cupper_sys::Requirement;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::JoinHandle;

use super::listen::HostLock;
use super::{AuditEvent, AuditRecord, CloseReason, Gateway, GatewayConfig};
use crate::clock::SystemClock;
use crate::durations::Durations;

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
    /// Starts serving; call within a tokio runtime.
    pub fn start(ui_requirement: Option<Requirement>) -> Self {
        let dir = TempDir::new("gateway");
        let lock = HostLock::acquire(dir.path()).expect("take host.lock");
        let (listener, socket) = lock.bind().expect("bind the socket");
        let config = GatewayConfig {
            host_id: HostId::from_uuid(1),
            data_epoch: 1,
            ui_requirement,
        };
        let clock = Arc::new(SystemClock::new());
        let (gateway, audit) = Gateway::new(config, clock, Durations::default());
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
