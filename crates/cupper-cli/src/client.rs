//! The connection to the host (design 5.3.1, 5.3.2): the check of the host,
//! the handshake, and requests matched to their responses by `id`.
//!
//! A reader task owns the read side. It reads through `recv_closing_fds`,
//! compares the host's token after every read and routes each message: a
//! response to the request waiting for it, a notification to
//! [`Client::next`]. The result of [`Client::subscribe`] goes to
//! [`Client::next`] too, in its place among the notifications, so that the
//! stream mirror (T049) tells the new subscription's data from what was
//! still on its way for the old one.

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cupper_protocol::domain::ClientKind;
use cupper_protocol::error::RpcError;
use cupper_protocol::frame::{self, FrameDecoder, FrameType, MAX_FRAME_BYTES};
use cupper_protocol::ids::CommandId;
use cupper_protocol::methods::handshake::{
    Auth, ClientIdentity, Feature, HostHello, HostHelloParams, Initialize, InitializeParams,
    InitializeResult, PROTOCOL, VersionRange,
};
use cupper_protocol::methods::stream::{
    StreamEvents, StreamEventsParams, StreamReset, StreamResetParams, StreamSnapshot,
    StreamSnapshotParams, StreamSubscribe, StreamSubscribeParams, StreamSubscribeResult,
};
use cupper_protocol::rpc::{self, Message, Method, Notification, RawMessage, RpcId};
use cupper_protocol::sync::StreamId;
use cupper_sys::{AuditToken, Refusal, recv_closing_fds};
use serde_json::Value;
use tokio::io::{AsyncWriteExt, Interest};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::{Mutex as AsyncMutex, mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::verify::{HostCheck, VerifyError, still_pinned};

/// A message from the host other than the response to a [`Client::request`].
#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Hello(HostHelloParams),
    /// The response to a [`Client::subscribe`].
    Subscribed {
        stream: StreamId,
        result: Result<StreamSubscribeResult, RpcError>,
    },
    Events(StreamEventsParams),
    Snapshot(StreamSnapshotParams),
    Reset(StreamResetParams),
    /// Any other notification, such as `host/shutdown`.
    Other {
        method: String,
        params: Value,
    },
}

/// Why the reader stopped.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CloseReason {
    #[error("the host closed the connection")]
    Eof,
    #[error("the host passed {0} file descriptors")]
    PassedFds(usize),
    #[error("another process now speaks for the host")]
    TokenChanged,
    #[error("the host could no longer be verified: {0}")]
    Refused(Refusal),
    #[error("the host broke the framing: {0}")]
    Frame(String),
    #[error("the host sent a message outside the protocol")]
    Malformed,
    #[error("reading the socket failed: {0}")]
    Io(io::ErrorKind),
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Verify(#[from] VerifyError),
    #[error(transparent)]
    Rpc(#[from] RpcError),
    #[error(transparent)]
    Encode(#[from] frame::EncodeError),
    #[error("a result that does not match its method: {0}")]
    Result(#[from] serde_json::Error),
    #[error("connection closed: {0:?}")]
    Closed(Option<CloseReason>),
    #[error("the host's first message was not host/hello")]
    NoHello,
    #[error("no protocol version in common with the host's {}..={}", .0.min, .0.max)]
    NoCommonVersion(VersionRange),
    #[error("the host chose protocol version {0}")]
    WrongVersion(u32),
}

/// How the CLI authenticates (design 5.8.1). T039 adds `harness`, keyed
/// from `--harness-key-fd`: `ClientAuth::credentials` gets the `nonce`
/// and the host's pinned token its proof binds to, and the handshake then
/// checks `host_proof`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ClientAuth {
    /// The read-only `cli` principal.
    #[default]
    Peer,
}

impl ClientAuth {
    fn credentials(&self, _hello: &HostHelloParams, _host: &AuditToken) -> Auth {
        match self {
            Self::Peer => Auth::Peer,
        }
    }
}

/// What the client says in `initialize`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub auth: ClientAuth,
    pub features: Vec<Feature>,
    pub coalesce_ms: Option<u32>,
}

impl Default for Options {
    /// `peer`, accepting range events, which the stream mirror (T049) handles.
    fn default() -> Self {
        Self {
            auth: ClientAuth::Peer,
            features: vec![Feature::StreamCoalesce],
            coalesce_ms: None,
        }
    }
}

enum Route {
    Response(oneshot::Sender<Result<Value, RpcError>>),
    Subscribed(StreamId),
}

#[derive(Default)]
struct Routes {
    waiting: HashMap<RpcId, Route>,
    /// Set when the reader stops; nothing waits after that.
    closed: Option<CloseReason>,
}

type Shared = Arc<Mutex<Routes>>;

fn lock(routes: &Shared) -> MutexGuard<'_, Routes> {
    routes.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct Client {
    writer: AsyncMutex<OwnedWriteHalf>,
    routes: Shared,
    next_id: AtomicI64,
    incoming: mpsc::UnboundedReceiver<Incoming>,
    reader: JoinHandle<()>,
    init: Option<InitializeResult>,
}

impl Drop for Client {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

impl Client {
    /// Connects to the host listening on `socket` (see
    /// [`socket_path`](crate::build_kind::socket_path)).
    pub async fn connect(
        socket: &Path,
        check: &HostCheck,
        options: Options,
    ) -> Result<Self, ClientError> {
        Self::handshake(UnixStream::connect(socket).await?, check, options).await
    }

    /// Checks the host at the other end of `stream`, then reads `host/hello`
    /// and sends `initialize`. A host that fails the check gets nothing.
    pub async fn handshake(
        stream: UnixStream,
        check: &HostCheck,
        options: Options,
    ) -> Result<Self, ClientError> {
        let host = check.verify(&stream)?;
        let mut client = Self::start(stream, host);
        let hello = match client.next().await {
            Some(Incoming::Hello(hello)) => hello,
            Some(_) => return Err(ClientError::NoHello),
            None => return Err(client.closed()),
        };
        let version = PROTOCOL
            .negotiate(&hello.protocol)
            .ok_or(ClientError::NoCommonVersion(hello.protocol))?;
        let params = InitializeParams {
            protocol: PROTOCOL,
            auth: options.auth.credentials(&hello, &host),
            nonce: hello.nonce,
            client: ClientIdentity {
                kind: ClientKind::Cli,
                name: "cupper".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            role: None,
            features: options.features,
            coalesce_ms: options.coalesce_ms,
        };
        let init = client.request::<Initialize>(params).await?;
        if init.protocol != version {
            return Err(ClientError::WrongVersion(init.protocol));
        }
        check.accept(&init.host)?;
        client.init = Some(init);
        Ok(client)
    }

    fn start(stream: UnixStream, host: AuditToken) -> Self {
        let (read, write) = stream.into_split();
        let routes = Shared::default();
        let (tx, incoming) = mpsc::unbounded_channel();
        let reader = tokio::spawn(read_loop(read, host, Arc::clone(&routes), tx));
        Self {
            writer: AsyncMutex::new(write),
            routes,
            next_id: AtomicI64::new(1),
            incoming,
            reader,
            init: None,
        }
    }

    /// The result of `initialize`.
    pub fn initialized(&self) -> &InitializeResult {
        self.init
            .as_ref()
            .expect("clients are handed out after the handshake")
    }

    pub async fn request<M: Method>(&self, params: M::Params) -> Result<M::Result, ClientError> {
        let (tx, rx) = oneshot::channel();
        self.send::<M>(params, Route::Response(tx)).await?;
        let result = rx.await.map_err(|_| self.closed())??;
        Ok(serde_json::from_value(result)?)
    }

    /// Sends `stream/subscribe`; the response comes from [`Client::next`]
    /// as [`Incoming::Subscribed`].
    pub async fn subscribe(&self, params: StreamSubscribeParams) -> Result<(), ClientError> {
        let stream = params.stream.clone();
        self.send::<StreamSubscribe>(params, Route::Subscribed(stream))
            .await
    }

    /// The next message, in the order the host sent it; `None` once the
    /// connection is closed.
    pub async fn next(&mut self) -> Option<Incoming> {
        self.incoming.recv().await
    }

    pub fn close_reason(&self) -> Option<CloseReason> {
        lock(&self.routes).closed.clone()
    }

    fn closed(&self) -> ClientError {
        ClientError::Closed(self.close_reason())
    }

    async fn send<M: Method>(&self, params: M::Params, route: Route) -> Result<(), ClientError> {
        let id = RpcId::Number(self.next_id.fetch_add(1, Ordering::Relaxed));
        let bytes = frame::encode_json(&rpc::request::<M>(id.clone(), params))?;
        {
            let mut routes = lock(&self.routes);
            if let Some(reason) = &routes.closed {
                return Err(ClientError::Closed(Some(reason.clone())));
            }
            routes.waiting.insert(id.clone(), route);
        }
        if let Err(err) = self.writer.lock().await.write_all(&bytes).await {
            lock(&self.routes).waiting.remove(&id);
            return Err(err.into());
        }
        Ok(())
    }
}

/// A new `command_id` (design 5.3.1): a UUIDv7 made on the client.
#[expect(
    clippy::disallowed_methods,
    reason = "the CLI is a process of its own and numbers its commands by real time; the host's IdGen does not reach it"
)]
pub fn command_id() -> CommandId {
    let id = uuid::Uuid::now_v7().to_string();
    id.parse().expect("a UUID is a valid command ID")
}

async fn read_loop(
    read: OwnedReadHalf,
    host: AuditToken,
    routes: Shared,
    tx: mpsc::UnboundedSender<Incoming>,
) {
    let reason = read_frames(read.as_ref(), &host, &routes, &tx).await;
    let mut routes = lock(&routes);
    routes.closed = Some(reason);
    // Dropping the senders fails the requests still waiting.
    routes.waiting.clear();
}

async fn read_frames(
    socket: &UnixStream,
    host: &AuditToken,
    routes: &Shared,
    tx: &mpsc::UnboundedSender<Incoming>,
) -> CloseReason {
    let mut decoder = FrameDecoder::new();
    let mut buf = vec![0; MAX_FRAME_BYTES];
    loop {
        let read = match socket.readable().await {
            Ok(()) => socket.try_io(Interest::READABLE, || recv_closing_fds(socket, &mut buf)),
            Err(err) => Err(err),
        };
        let received = match read {
            Ok(received) => received,
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => continue,
            Err(err) => return CloseReason::Io(err.kind()),
        };
        if received.closed_fds > 0 {
            return CloseReason::PassedFds(received.closed_fds);
        }
        if received.len == 0 {
            return CloseReason::Eof;
        }
        match still_pinned(host, socket) {
            Ok(()) => {}
            Err(VerifyError::Refused(refusal)) => return CloseReason::Refused(refusal),
            Err(_) => return CloseReason::TokenChanged,
        }
        decoder.push(&buf[..received.len]);
        loop {
            let frame = match decoder.next_frame() {
                Ok(Some(frame)) => frame,
                Ok(None) => break,
                Err(err) => return CloseReason::Frame(err.to_string()),
            };
            // Blob chunks only follow `blob/get`, which the CLI does not call.
            if frame.frame_type != FrameType::Json {
                continue;
            }
            if let Err(reason) = route(&frame.payload, routes, tx) {
                return reason;
            }
        }
    }
}

fn route(
    payload: &[u8],
    routes: &Shared,
    tx: &mpsc::UnboundedSender<Incoming>,
) -> Result<(), CloseReason> {
    let raw = serde_json::from_slice::<RawMessage>(payload);
    let message = raw.ok().and_then(|raw| raw.classify().ok());
    let incoming = match message.ok_or(CloseReason::Malformed)? {
        Message::Response { id, result } => match lock(routes).waiting.remove(&id) {
            Some(Route::Response(waiter)) => {
                // The request may have been given up on.
                let _ = waiter.send(result);
                return Ok(());
            }
            Some(Route::Subscribed(stream)) => Incoming::Subscribed {
                stream,
                result: match result {
                    Ok(value) => {
                        Ok(serde_json::from_value(value).map_err(|_| CloseReason::Malformed)?)
                    }
                    Err(err) => Err(err),
                },
            },
            None => return Ok(()),
        },
        Message::Notification { method, params } => {
            notification(method, params).map_err(|_| CloseReason::Malformed)?
        }
        // The host never sends requests.
        Message::Request { .. } => return Err(CloseReason::Malformed),
    };
    // The client may be gone.
    let _ = tx.send(incoming);
    Ok(())
}

fn notification(method: String, params: Value) -> serde_json::Result<Incoming> {
    Ok(match method.as_str() {
        HostHello::NAME => Incoming::Hello(serde_json::from_value(params)?),
        StreamEvents::NAME => Incoming::Events(serde_json::from_value(params)?),
        StreamSnapshot::NAME => Incoming::Snapshot(serde_json::from_value(params)?),
        StreamReset::NAME => Incoming::Reset(serde_json::from_value(params)?),
        _ => Incoming::Other { method, params },
    })
}

#[cfg(test)]
mod tests {
    use cupper_protocol::limits::V1;
    use cupper_protocol::methods::Empty;
    use cupper_protocol::methods::handshake::HostStatus;
    use cupper_sys::{Cdhash, own_cdhash};
    use serde_json::json;
    use tokio::io::AsyncReadExt;

    use super::*;

    /// The host's end of a socket pair: both ends are this test process,
    /// which the client then checks as its host.
    struct FakeHost {
        stream: UnixStream,
        decoder: FrameDecoder,
    }

    impl FakeHost {
        async fn send(&mut self, message: Value) {
            let bytes = frame::encode_json(&message).unwrap();
            self.stream.write_all(&bytes).await.unwrap();
        }

        async fn notify(&mut self, method: &str, params: Value) {
            self.send(json!({"jsonrpc": "2.0", "method": method, "params": params}))
                .await;
        }

        async fn reply(&mut self, request: &Value, result: Value) {
            let id = &request["id"];
            self.send(json!({"jsonrpc": "2.0", "id": id, "result": result}))
                .await;
        }

        /// The next message, or `None` at end of file.
        async fn recv(&mut self) -> Option<Value> {
            loop {
                if let Some(frame) = self.decoder.next_frame().unwrap() {
                    return Some(serde_json::from_slice(&frame.payload).unwrap());
                }
                let mut buf = [0; 4096];
                match self.stream.read(&mut buf).await.unwrap() {
                    0 => return None,
                    n => self.decoder.push(&buf[..n]),
                }
            }
        }

        async fn hello(&mut self, min: u32, max: u32) {
            let params = json!({"nonce": "a".repeat(64), "protocol": {"min": min, "max": max},
                "host_version": "0.1.0"});
            self.notify("host/hello", params).await;
        }

        /// Answers `initialize` choosing `protocol`, and returns the request.
        async fn initialize(&mut self, protocol: u32) -> Value {
            let request = self.recv().await.unwrap();
            let result = json!({"protocol": protocol, "host": {"host_id": "h_1", "version": "0.1.0",
                "weak_auth": true, "data_epoch": 3}, "principal": {"id": "pr_cli", "kind": "cli",
                "scopes": ["read"]}, "features": ["stream.coalesce"], "limits": V1});
            self.reply(&request, result).await;
            request
        }
    }

    fn pair() -> (UnixStream, FakeHost) {
        let (client, host) = UnixStream::pair().unwrap();
        let decoder = FrameDecoder::new();
        (
            client,
            FakeHost {
                stream: host,
                decoder,
            },
        )
    }

    fn this_process() -> HostCheck {
        HostCheck::Cdhash(own_cdhash().unwrap())
    }

    #[tokio::test]
    async fn unit_cli_client_handshake_correlates_responses_and_keeps_wire_order() {
        let (stream, mut host) = pair();
        let script = tokio::spawn(async move {
            host.hello(1, 2).await;
            let init = host.initialize(1).await;
            // Two requests, answered in reverse order with their own ids.
            let (first, second) = (host.recv().await.unwrap(), host.recv().await.unwrap());
            for request in [&second, &first] {
                let status = json!({"version": "0.1.0", "pending_upgrade": false,
                    "running_turns": request["id"], "pending_interactions": 0, "clients": []});
                host.reply(request, status).await;
            }
            // Events of an old subscription arrive before the new one's result.
            let subscribe = host.recv().await.unwrap();
            host.notify("stream/events", json!({"stream": "index", "events": []}))
                .await;
            let result = json!({"stream": "index", "mode": "replay", "at_seq": 7, "head_seq": 9,
                "epoch": 3});
            host.reply(&subscribe, result).await;
            host.notify(
                "host/shutdown",
                json!({"reason": "idle", "restart_expected": false}),
            )
            .await;
            // The host never sends requests.
            let request = json!({"jsonrpc": "2.0", "id": 1, "method": "ping", "params": {}});
            host.send(request).await;
            (init, host)
        });
        let mut client = Client::handshake(stream, &this_process(), Options::default())
            .await
            .unwrap();
        assert_eq!(client.initialized().host.data_epoch, 3);

        let (a, b) = tokio::join!(
            client.request::<HostStatus>(Empty {}),
            client.request::<HostStatus>(Empty {})
        );
        assert_eq!(a.unwrap().running_turns + 1, b.unwrap().running_turns);

        let subscribe = StreamSubscribeParams {
            stream: StreamId::Index,
            after_seq: Some(7),
            epoch: Some(3),
        };
        client.subscribe(subscribe).await.unwrap();
        let got = (
            client.next().await,
            client.next().await,
            client.next().await,
        );
        let (
            Some(Incoming::Events(events)),
            Some(Incoming::Subscribed { stream, result }),
            Some(other),
        ) = got
        else {
            panic!("{got:?}")
        };
        assert_eq!((events.stream, stream), (StreamId::Index, StreamId::Index));
        assert_eq!(result.unwrap().at_seq, 7);
        assert!(matches!(other, Incoming::Other { method, .. } if method == "host/shutdown"));
        let (init, _host) = script.await.unwrap();
        assert_eq!(init["method"], "initialize");
        let expected = json!({"protocol": {"min": 1, "max": 1}, "nonce": "a".repeat(64),
            "client": {"kind": "cli", "name": "cupper", "version": env!("CARGO_PKG_VERSION")},
            "auth": {"type": "peer"}, "features": ["stream.coalesce"]});
        assert_eq!(init["params"], expected);

        assert_eq!(client.next().await, None);
        assert_eq!(client.close_reason(), Some(CloseReason::Malformed));
        let err = client.request::<HostStatus>(Empty {}).await.unwrap_err();
        assert!(
            matches!(err, ClientError::Closed(Some(CloseReason::Malformed))),
            "{err}"
        );
    }

    #[tokio::test]
    async fn unit_cli_client_handshake_refusals() {
        // What the host does instead of a proper handshake, and what the client says.
        type Expect = fn(&ClientError) -> bool;
        let cases: [(&str, Expect); 4] = [
            ("close", |e| {
                matches!(e, ClientError::Closed(Some(CloseReason::Eof)))
            }),
            ("no hello", |e| matches!(e, ClientError::NoHello)),
            (
                "version 2..=3",
                |e| matches!(e, ClientError::NoCommonVersion(r) if r.min == 2),
            ),
            ("chooses 2", |e| matches!(e, ClientError::WrongVersion(2))),
        ];
        for (case, expected) in cases {
            let (stream, mut host) = pair();
            let script = tokio::spawn(async move {
                match case {
                    "close" => return None,
                    "no hello" => host.notify("host/shutdown", json!({})).await,
                    "version 2..=3" => host.hello(2, 3).await,
                    _ => {
                        host.hello(1, 2).await;
                        host.initialize(2).await;
                    }
                }
                // Nothing more from the client.
                host.recv().await
            });
            let err = Client::handshake(stream, &this_process(), Options::default())
                .await
                .err()
                .unwrap();
            assert!(expected(&err), "{case}: {err}");
            assert_eq!(script.await.unwrap(), None, "{case}");
        }
    }

    #[tokio::test]
    async fn unit_cli_host_verify_cdhash_mismatch_refuses_the_connection() {
        let (stream, mut host) = pair();
        host.hello(1, 1).await;
        let check = HostCheck::Cdhash(Cdhash([7; 20]));
        let err = Client::handshake(stream, &check, Options::default())
            .await
            .err()
            .unwrap();
        assert!(
            matches!(err, ClientError::Verify(VerifyError::CdhashMismatch { .. })),
            "{err}"
        );
        assert_eq!(host.recv().await, None, "the client sent nothing");
    }

    #[test]
    fn unit_cli_client_command_ids_are_uuid_v7() {
        let (a, b) = (command_id(), command_id());
        let uuid = uuid::Uuid::parse_str(a.as_str()).unwrap();
        assert_eq!(uuid.get_version(), Some(uuid::Version::SortRand));
        assert_ne!(a, b);
    }
}
