//! Async socket client: framing, request/response correlation, notifications.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cupper_protocol::frame::{self, BlobChunkHeader, FrameDecoder, FrameType, MAX_FRAME_BYTES};
use cupper_protocol::rpc::{self, Message, Method, RawMessage, RequestId, RpcError};
use cupper_protocol::*;
use serde_json::Value;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::net::unix::OwnedWriteHalf;
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, Error)]
pub enum ClientError {
    #[error(transparent)]
    Rpc(#[from] RpcError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Encode(#[from] frame::EncodeError),
    #[error("connection closed")]
    Closed,
    #[error("timed out")]
    Timeout,
}

impl ClientError {
    /// `error.data.code` of an application error.
    pub fn app_code(&self) -> Option<&rpc::ErrorCode> {
        match self {
            ClientError::Rpc(e) => e.app_code(),
            _ => None,
        }
    }
}

/// Something the host sent that is not a response.
#[derive(Debug, Clone)]
pub enum Incoming {
    Events(StreamEventsParams),
    Snapshot(StreamSnapshotParams),
    Reset(StreamResetParams),
    BlobChunk(BlobChunkHeader, Vec<u8>),
    Other { method: String, params: Value },
}

type Pending = Arc<Mutex<HashMap<RequestId, oneshot::Sender<Result<Value, RpcError>>>>>;

pub struct Client {
    writer: tokio::sync::Mutex<OwnedWriteHalf>,
    pending: Pending,
    next_id: AtomicI64,
    incoming: mpsc::UnboundedReceiver<Incoming>,
    reader: tokio::task::JoinHandle<()>,
    pub init: Option<InitializeResult>,
    /// Every frame size seen, for frame-limit assertions.
    pub max_frame_seen: Arc<std::sync::atomic::AtomicUsize>,
}

impl Drop for Client {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

impl Client {
    /// Connect and complete `initialize`.
    pub async fn connect(socket: &Path, auth: AuthParams, coalesce_ms: Option<u32>) -> Result<Self, ClientError> {
        let stream = UnixStream::connect(socket).await?;
        let mut c = Self::from_stream(stream);
        let init = c
            .request::<Initialize>(InitializeParams {
                protocol: VersionRange { min: PROTOCOL_VERSION, max: PROTOCOL_VERSION },
                client: ClientInfo { kind: ClientKind::Cli, name: "cupper-cli".into(), version: env!("CARGO_PKG_VERSION").into() },
                features: vec!["stream.coalesce".into(), "blob.get".into()],
                auth,
                coalesce_ms,
            })
            .await?;
        c.init = Some(init);
        Ok(c)
    }

    /// Wrap a connected socket without sending anything.
    pub fn from_stream(stream: UnixStream) -> Self {
        let (mut rd, wr) = stream.into_split();
        let pending: Pending = Arc::default();
        let (tx, incoming) = mpsc::unbounded_channel();
        let max_frame_seen = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (p, seen) = (pending.clone(), max_frame_seen.clone());
        let reader = tokio::spawn(async move {
            let mut decoder = FrameDecoder::new();
            let mut buf = vec![0u8; MAX_FRAME_BYTES];
            loop {
                let n = match rd.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                decoder.push(&buf[..n]);
                loop {
                    let f = match decoder.next_frame() {
                        Ok(Some(f)) => f,
                        Ok(None) => break,
                        Err(e) => {
                            let _ = tx.send(Incoming::Other { method: "framing_error".into(), params: e.to_string().into() });
                            p.lock().expect("pending mutex").clear();
                            return;
                        }
                    };
                    seen.fetch_max(f.payload.len() + frame::FRAME_HEADER_BYTES, Ordering::Relaxed);
                    match f.frame_type {
                        FrameType::Json => dispatch(&f.payload, &p, &tx),
                        FrameType::BlobChunk => {
                            if let Ok((h, data)) = BlobChunkHeader::decode(&f.payload) {
                                let _ = tx.send(Incoming::BlobChunk(h, data.to_vec()));
                            }
                        }
                        _ => {}
                    }
                }
            }
            // Fail whatever is still waiting.
            p.lock().expect("pending mutex").clear();
        });
        Self { writer: tokio::sync::Mutex::new(wr), pending, next_id: AtomicI64::new(1), incoming, reader, init: None, max_frame_seen }
    }

    pub async fn request<M: Method>(&self, params: M::Params) -> Result<M::Result, ClientError> {
        let v = self.request_raw(M::NAME, serde_json::to_value(params)?).await?;
        Ok(serde_json::from_value(v)?)
    }

    pub async fn request_raw(&self, method: &str, params: Value) -> Result<Value, ClientError> {
        let id = RequestId::Number(self.next_id.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = oneshot::channel();
        self.pending.lock().expect("pending mutex").insert(id.clone(), tx);
        let msg = rpc::OutRequest { jsonrpc: rpc::JSONRPC_VERSION, id, method, params };
        let bytes = frame::encode_json(&msg)?;
        self.writer.lock().await.write_all(&bytes).await?;
        Ok(rx.await.map_err(|_| ClientError::Closed)??)
    }

    pub async fn next(&mut self) -> Option<Incoming> {
        self.incoming.recv().await
    }

    pub async fn next_timeout(&mut self, d: Duration) -> Result<Incoming, ClientError> {
        match tokio::time::timeout(d, self.incoming.recv()).await {
            Ok(Some(i)) => Ok(i),
            Ok(None) => Err(ClientError::Closed),
            Err(_) => Err(ClientError::Timeout),
        }
    }
}

fn dispatch(payload: &[u8], pending: &Pending, tx: &mpsc::UnboundedSender<Incoming>) {
    let Ok(raw) = serde_json::from_slice::<RawMessage>(payload) else { return };
    match raw.classify() {
        Ok(Message::Response { id, result }) => {
            if let Some(waiter) = pending.lock().expect("pending mutex").remove(&id) {
                let _ = waiter.send(result);
            }
        }
        Ok(Message::Notification { method, params }) => {
            let parsed = match method.as_str() {
                "stream/events" => serde_json::from_value(params.clone()).map(Incoming::Events).ok(),
                "stream/snapshot" => serde_json::from_value(params.clone()).map(Incoming::Snapshot).ok(),
                "stream/reset" => serde_json::from_value(params.clone()).map(Incoming::Reset).ok(),
                _ => None,
            };
            let _ = tx.send(parsed.unwrap_or(Incoming::Other { method, params }));
        }
        // An error without an id (e.g. a parse error report) or a request: nothing to route.
        _ => {}
    }
}
