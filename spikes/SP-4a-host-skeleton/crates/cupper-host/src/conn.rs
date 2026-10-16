//! Socket gateway: accept, per-connection reader (requests) and writer (outbox).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cupper_protocol::coalesce::coalesce;
use cupper_protocol::frame::{BlobChunkHeader, FrameDecoder, FrameType, MAX_FRAME_BYTES};
use cupper_protocol::rpc::{self, ErrorCode, Message, Method, RawMessage, RequestId, RpcError};
use cupper_protocol::sync::StreamId;
use cupper_protocol::text::MAX_MERGED_ESCAPED;
use cupper_protocol::*;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio::net::UnixListener;
use tokio::net::unix::OwnedWriteHalf;

use crate::auth::{self, Principal};
use crate::hub::MAX_SEQ;
use crate::host::Host;
use crate::outbox::{OutItem, Outbox};
use crate::session::SessionMsg;
use crate::sys;
use crate::wire;

/// Features this host offers in `initialize`.
const HOST_FEATURES: &[&str] = &["stream.coalesce", "blob.get"];

pub async fn serve(host: Arc<Host>, listener: UnixListener) {
    let uid = sys::current_uid();
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                match stream.peer_cred() {
                    Ok(c) if c.uid() == uid => {}
                    other => {
                        tracing::warn!(?other, "rejecting peer with a different uid");
                        continue;
                    }
                }
                let id = host.next_conn_id();
                tokio::spawn(connection(host.clone(), stream, id));
            }
            Err(e) => {
                tracing::warn!("accept failed: {e}");
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
}

struct Conn {
    host: Arc<Host>,
    outbox: Arc<Outbox>,
    principal: Mutex<Option<Principal>>,
    coalesce_ms: Arc<AtomicU32>,
}

async fn connection(host: Arc<Host>, stream: tokio::net::UnixStream, id: u64) {
    let (mut rd, wr) = stream.into_split();
    let outbox = Outbox::new(id, host.config.max_queue_bytes);
    let coalesce_ms = Arc::new(AtomicU32::new(0));
    let conn = Arc::new(Conn { host: host.clone(), outbox: outbox.clone(), principal: Mutex::new(None), coalesce_ms: coalesce_ms.clone() });
    let writer = tokio::spawn(write_loop(wr, outbox.clone(), host, coalesce_ms));
    let mut decoder = FrameDecoder::new();
    let mut buf = vec![0u8; MAX_FRAME_BYTES];
    'read: loop {
        let n = match rd.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        decoder.push(&buf[..n]);
        loop {
            match decoder.next_frame() {
                Ok(Some(f)) if f.frame_type == FrameType::Json => conn.on_json(&f.payload),
                Ok(Some(f)) => tracing::debug!(conn = id, frame_type = f.frame_type.to_byte(), "ignoring non-JSON frame"),
                Ok(None) => break,
                Err(e) => {
                    tracing::warn!(conn = id, "closing connection: {e}");
                    break 'read;
                }
            }
        }
    }
    outbox.close();
    writer.abort();
}

fn parse<T: DeserializeOwned>(params: Value) -> Result<T, RpcError> {
    serde_json::from_value(params).map_err(|e| RpcError::new(rpc::INVALID_PARAMS, e.to_string()))
}

fn internal(e: impl std::fmt::Display) -> RpcError {
    RpcError::app(ErrorCode::Internal, e.to_string())
}

impl Conn {
    fn on_json(self: &Arc<Self>, payload: &[u8]) {
        let msg = match serde_json::from_slice::<RawMessage>(payload) {
            Ok(raw) => raw.classify(),
            Err(e) => Err(RpcError::new(rpc::PARSE_ERROR, e.to_string())),
        };
        match msg {
            Ok(Message::Request { id, method, params }) => self.on_request(id, method, params),
            Ok(Message::Notification { method, .. }) => tracing::debug!(method, "ignoring client notification"),
            Ok(Message::Response { .. }) => tracing::debug!("ignoring response: the host sends no requests"),
            Err(e) => self.outbox.push_frame(wire::error(None, &e)),
        }
    }

    fn principal(&self) -> Option<Principal> {
        self.principal.lock().expect("principal mutex poisoned").clone()
    }

    fn on_request(self: &Arc<Self>, id: RequestId, method: String, params: Value) {
        if method == Initialize::NAME {
            let frame = match self.initialize(params) {
                Ok(r) => wire::response(&id, &r),
                Err(e) => wire::error(Some(&id), &e),
            };
            self.outbox.push_frame(frame);
            return;
        }
        let Some(principal) = self.principal() else {
            self.outbox.push_frame(wire::error(Some(&id), &RpcError::app(ErrorCode::NotInitialized, "send initialize first")));
            return;
        };
        if let Err(e) = auth::authorize(&principal, &method) {
            self.outbox.push_frame(wire::error(Some(&id), &e));
            return;
        }
        let this = self.clone();
        tokio::spawn(async move {
            match this.handle(&principal, &id, &method, params).await {
                Ok(Some(v)) => this.outbox.push_frame(wire::response(&id, &v)),
                Ok(None) => {}
                Err(e) => this.outbox.push_frame(wire::error(Some(&id), &e)),
            }
        });
    }

    fn initialize(&self, params: Value) -> Result<InitializeResult, RpcError> {
        if self.principal().is_some() {
            return Err(RpcError::new(rpc::INVALID_REQUEST, "already initialized"));
        }
        let p: InitializeParams = parse(params)?;
        if p.protocol.min > PROTOCOL_VERSION || p.protocol.max < PROTOCOL_VERSION {
            return Err(RpcError::app(ErrorCode::VersionUnsupported, "no common protocol version")
                .with_extra("supported", serde_json::json!({ "min": PROTOCOL_VERSION, "max": PROTOCOL_VERSION })));
        }
        let principal = auth::authenticate(&p.auth, self.host.config.harness_key.as_deref())?;
        let coalesce_ms = p.coalesce_ms.unwrap_or(0).min(1000);
        self.coalesce_ms.store(coalesce_ms, Ordering::Relaxed);
        let info = principal.info();
        tracing::info!(conn = self.outbox.id, client = %p.client.name, principal = %info.id, "client initialized");
        *self.principal.lock().expect("principal mutex poisoned") = Some(principal);
        Ok(InitializeResult {
            protocol: PROTOCOL_VERSION,
            host: HostInfo { host_id: self.host.host_id.clone(), version: env!("CARGO_PKG_VERSION").into() },
            principal: info,
            features: HOST_FEATURES.iter().filter(|f| p.features.iter().any(|c| c == *f)).map(|f| (*f).to_owned()).collect(),
            limits: Limits { max_frame_bytes: MAX_FRAME_BYTES as u32, coalesce_ms, max_queue_bytes: self.host.config.max_queue_bytes as u64 },
        })
    }

    /// `Ok(None)` means the handler queued its response itself.
    async fn handle(&self, principal: &Principal, id: &RequestId, method: &str, params: Value) -> Result<Option<Value>, RpcError> {
        let host = &self.host;
        match method {
            Subscribe::NAME => {
                let p: SubscribeParams = parse(params)?;
                let stream: StreamId = p.stream.parse().map_err(|e| RpcError::new(rpc::INVALID_PARAMS, format!("{e}")))?;
                if let StreamId::Session(sid) = &stream
                    && !host.session_exists(sid)
                {
                    return Err(RpcError::app(ErrorCode::NotFound, "no such session"));
                }
                host.hub.subscribe(&self.outbox, &stream, p.after_seq, |r| wire::response(id, r))?;
                Ok(None)
            }
            BlobGet::NAME => {
                let p: BlobGetParams = parse(params)?;
                let result = self.blob_get(&p)?;
                Ok(Some(serde_json::to_value(result).map_err(internal)?))
            }
            SessionCreate::NAME => {
                let p: SessionCreateParams = parse(params.clone())?;
                let host = host.clone();
                self.command(principal, &p.meta.command_id, method, &params, async move {
                    let session_id = host.create_session(&p.agent, &p.cwd).await?;
                    Ok(SessionCreateResult { session_id, replayed: false })
                })
                .await
            }
            SessionSend::NAME => {
                let p: SessionSendParams = parse(params.clone())?;
                let handle = host.session(&p.session_id)?;
                let cause = Cause { command_id: p.meta.command_id.clone(), principal: principal.id.clone() };
                self.command(principal, &p.meta.command_id, method, &params, async move {
                    handle.call(|reply| SessionMsg::Send { text: p.text, expected_turn_id: p.expected_turn_id, cause, reply }).await
                })
                .await
            }
            SessionInterrupt::NAME => {
                let p: SessionInterruptParams = parse(params.clone())?;
                let handle = host.session(&p.session_id)?;
                let cause = Cause { command_id: p.meta.command_id.clone(), principal: principal.id.clone() };
                self.command(principal, &p.meta.command_id, method, &params, async move {
                    handle.call(|reply| SessionMsg::Interrupt { expected_turn_id: p.expected_turn_id, cause, reply }).await
                })
                .await
            }
            InteractionRespond::NAME => {
                let p: InteractionRespondParams = parse(params.clone())?;
                let handle = host.session(&p.session_id)?;
                let cause = Cause { command_id: p.meta.command_id.clone(), principal: principal.id.clone() };
                let who = principal.clone();
                let request_id = p.request_id.clone();
                self.command(principal, &p.meta.command_id, method, &params, async move {
                    handle
                        .call(|reply| SessionMsg::Respond {
                            request_id: p.request_id,
                            digest: p.request_digest,
                            outcome: p.outcome,
                            principal: who,
                            cause,
                            reply,
                        })
                        .await?;
                    Ok(InteractionRespondResult { request_id, replayed: false })
                })
                .await
            }
            _ => Err(RpcError::new(rpc::METHOD_NOT_FOUND, format!("unknown method {method}"))),
        }
    }

    async fn command<T: serde::Serialize>(
        &self,
        principal: &Principal,
        command_id: &str,
        method: &str,
        params: &Value,
        exec: impl Future<Output = Result<T, RpcError>>,
    ) -> Result<Option<Value>, RpcError> {
        self.host
            .commands
            .run(&principal.id, command_id, method, params, || async { serde_json::to_value(exec.await?).map_err(internal) })
            .await
            .map(Some)
    }

    /// Queue the requested range as `0x02` chunk frames; the response follows them.
    fn blob_get(&self, p: &BlobGetParams) -> Result<BlobGetResult, RpcError> {
        let not_found = || RpcError::app(ErrorCode::NotFound, "no such blob");
        let hash_bytes: [u8; 32] = hex::decode(&p.hash).ok().and_then(|b| b.try_into().ok()).ok_or_else(not_found)?;
        let bytes = self.host.store.blob(&p.hash).map_err(internal)?.ok_or_else(not_found)?;
        let size = bytes.len() as u64;
        let start = p.offset.min(size);
        let end = p.length.map_or(size, |l| start.saturating_add(l).min(size));
        let range = &bytes[start as usize..end as usize];
        let chunks: Vec<&[u8]> = if range.is_empty() { vec![&[][..]] } else { range.chunks(BlobChunkHeader::MAX_DATA).collect() };
        let n = chunks.len();
        let mut offset = start;
        for (i, data) in chunks.into_iter().enumerate() {
            let header = BlobChunkHeader { hash: hash_bytes, offset, last: i + 1 == n };
            self.outbox.push_frame(header.encode_chunk(data).map_err(internal)?);
            offset += data.len() as u64;
        }
        Ok(BlobGetResult { hash: p.hash.clone(), size, offset: start, length: end - start, chunks: n as u32 })
    }
}

async fn write_loop(wr: OwnedWriteHalf, outbox: Arc<Outbox>, host: Arc<Host>, coalesce_ms: Arc<AtomicU32>) {
    let mut wr = BufWriter::with_capacity(MAX_FRAME_BYTES * 2, wr);
    while let Some(mut items) = outbox.drain().await {
        let window = coalesce_ms.load(Ordering::Relaxed);
        if window > 0 {
            tokio::time::sleep(Duration::from_millis(window as u64)).await;
            if let Some(more) = outbox.try_drain() {
                items.extend(more);
            }
        }
        if let Err(e) = write_items(&mut wr, items, &host).await {
            tracing::debug!(conn = outbox.id, "writer stopped: {e}");
            outbox.close();
            return;
        }
    }
}

type Writer = BufWriter<OwnedWriteHalf>;

async fn write_items(wr: &mut Writer, items: Vec<OutItem>, host: &Host) -> std::io::Result<()> {
    let mut run: Vec<(Arc<Event>, Arc<str>)> = Vec::new();
    let mut run_stream: Option<Arc<str>> = None;
    for item in items {
        match item {
            OutItem::Event { stream, event, json } => {
                if run_stream.as_ref() != Some(&stream) {
                    flush_run(wr, run_stream.as_deref(), &mut run).await?;
                    run_stream = Some(stream);
                }
                run.push((event, json));
            }
            OutItem::Frame(f) => {
                flush_run(wr, run_stream.as_deref(), &mut run).await?;
                wr.write_all(&f).await?;
            }
            OutItem::Replay { stream, after, up_to } => {
                flush_run(wr, run_stream.as_deref(), &mut run).await?;
                replay(wr, host, &stream, after, up_to).await?;
            }
        }
    }
    flush_run(wr, run_stream.as_deref(), &mut run).await?;
    wr.flush().await
}

/// Send-time coalescing: merge what queued up, then pack into frames.
async fn flush_run(wr: &mut Writer, stream: Option<&str>, run: &mut Vec<(Arc<Event>, Arc<str>)>) -> std::io::Result<()> {
    let Some(stream) = stream else { return Ok(()) };
    if run.is_empty() {
        return Ok(());
    }
    let events: Vec<Event> = run.iter().map(|(e, _)| (**e).clone()).collect();
    let jsons: Vec<String> = if run.len() == 1 {
        vec![run[0].1.to_string()]
    } else {
        coalesce(events, MAX_MERGED_ESCAPED).iter().map(|e| serde_json::to_string(e).expect("events serialize")).collect()
    };
    run.clear();
    write_events(wr, stream, &jsons).await
}

async fn write_events(wr: &mut Writer, stream: &str, jsons: &[String]) -> std::io::Result<()> {
    let frames = wire::events(stream, jsons).map_err(std::io::Error::other)?;
    for f in frames {
        wr.write_all(&f).await?;
    }
    Ok(())
}

/// Stream logged events `after+1 ..= up_to` to the client in pages.
async fn replay(wr: &mut Writer, host: &Host, stream: &str, mut after: u64, up_to: u64) -> std::io::Result<()> {
    while after < up_to.min(MAX_SEQ) {
        let page = host.store.read_events(stream, after, up_to, 512).map_err(std::io::Error::other)?;
        let Some((last, _)) = page.last() else { break };
        after = *last;
        let events: Vec<Event> = page.iter().filter_map(|(_, j)| serde_json::from_str(j).ok()).collect();
        let jsons: Vec<String> = coalesce(events, MAX_MERGED_ESCAPED).iter().map(|e| serde_json::to_string(e).expect("events serialize")).collect();
        write_events(wr, stream, &jsons).await?;
    }
    Ok(())
}
