//! One connection (design 5.2.4, 5.8.1): admission, the read loop with its
//! pinned token, the handshake, and the writer task.
//!
//! The kernel names the process of the peer's latest I/O, as it is now, not
//! the one that connected (SP-4b). So the token taken on accept is pinned,
//! and after every read that returns data the token must still be the pinned
//! one, or the connection ends: a forked child or an exec'd program cannot
//! borrow a verified connection.

use std::io;
use std::sync::Arc;

use cupper_protocol::error::{ErrorCode, ErrorReason, INVALID_PARAMS, PARSE_ERROR, RpcError};
use cupper_protocol::frame::{Frame, FrameDecoder, FrameType, MAX_FRAME_BYTES};
use cupper_protocol::ids::ConnId;
use cupper_protocol::limits::V1;
use cupper_protocol::methods::Empty;
use cupper_protocol::methods::handshake::{HostHello, Initialize, Ping};
use cupper_protocol::rpc::{Message, Method, RawMessage, RpcId};
use cupper_sys::{AuditToken, Received};
use serde_json::Value;
use tokio::io::{AsyncWriteExt, Interest};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};

use super::audit::{AuditEvent, CloseReason, Rejection};
use super::handshake::{self, Nonce, Session};
use super::outbox::Outbox;
use super::{Gateway, Verdict, wire};
use crate::durations::GATEWAY_CLOSE_GRACE;

pub(super) async fn run(gw: Arc<Gateway>, stream: UnixStream, id: ConnId) {
    let opened = gw.clock.now_mono();
    let Some((pinned, verdict)) = admit(&gw, &stream, &id) else {
        return;
    };
    let (reader, writer) = stream.into_split();
    let outbox = Arc::new(Outbox::new(V1.max_queue_bytes as usize));
    let mut writing = tokio::spawn(write_loop(writer, Arc::clone(&outbox)));
    let nonce = handshake::new_nonce();
    outbox.push_frame(wire::notification::<HostHello>(&handshake::hello(&nonce)));
    let mut conn = Conn {
        gw: &gw,
        id: &id,
        outbox: &outbox,
        pinned,
        verdict,
        phase: Phase::Hello(nonce),
    };
    let end = conn.read_loop(&reader).await;
    if end.flush {
        // Let the writer send the error behind the close, but not wait on a
        // peer that does not read.
        outbox.finish();
        let grace = gw.durations.get(GATEWAY_CLOSE_GRACE);
        let grace = std::pin::pin!(gw.clock.timer_at(gw.clock.now_mono() + grace));
        futures::future::select(&mut writing, grace).await;
    }
    outbox.close();
    writing.abort();
    let principal = match conn.phase {
        Phase::Ready(session) => Some(session.principal.id),
        Phase::Hello(_) => None,
    };
    let after = gw.clock.now_mono().saturating_duration_since(opened);
    let reason = end.reason;
    gw.audit(
        &id,
        AuditEvent::Closed {
            reason,
            after,
            principal,
        },
    );
}

/// Admission (design 5.2.4 steps 1–2): a peer of this user, its token pinned
/// for the connection's life, and the verdict on its signature. A refused
/// peer is closed without a word.
fn admit(gw: &Gateway, stream: &UnixStream, id: &ConnId) -> Option<(AuditToken, Verdict)> {
    let pinned = cupper_sys::peer_uid(stream)
        .map_err(Rejection::Unidentified)
        .and_then(|uid| match uid == cupper_sys::current_uid() {
            true => Ok(()),
            false => Err(Rejection::OtherUser { uid }),
        })
        .and_then(|()| cupper_sys::peer_token(stream).map_err(Rejection::Unidentified));
    let pinned = match pinned {
        Ok(token) => token,
        Err(rejection) => {
            gw.audit(id, AuditEvent::Rejected(rejection));
            return None;
        }
    };
    // Any failure to verify counts as "not the UI" (design 5.8.1).
    let verdict = match gw.config.ui_requirement {
        Some(requirement) => pinned.verify(requirement).map_or_else(
            |refusal| Verdict::Other(Some(refusal)),
            |()| Verdict::DesktopUi,
        ),
        None => Verdict::Other(None),
    };
    let (pid, pid_version) = (pinned.pid(), pinned.pid_version());
    gw.audit(
        id,
        AuditEvent::Accepted {
            pid,
            pid_version,
            verdict,
        },
    );
    Some((pinned, verdict))
}

enum Phase {
    /// `host/hello` went out with this nonce; `initialize` has to come next.
    Hello(Nonce),
    Ready(Session),
}

/// How a connection ends: why, and whether the queued replies go out first.
struct End {
    reason: CloseReason,
    flush: bool,
}

impl End {
    fn now(reason: CloseReason) -> Self {
        Self {
            reason,
            flush: false,
        }
    }

    fn after_replies(reason: CloseReason) -> Self {
        Self {
            reason,
            flush: true,
        }
    }
}

struct Conn<'a> {
    gw: &'a Gateway,
    id: &'a ConnId,
    outbox: &'a Outbox,
    pinned: AuditToken,
    verdict: Verdict,
    phase: Phase,
}

impl Conn<'_> {
    async fn read_loop(&mut self, reader: &OwnedReadHalf) -> End {
        let stream: &UnixStream = reader.as_ref();
        let mut buf = vec![0; MAX_FRAME_BYTES];
        let mut decoder = FrameDecoder::new();
        loop {
            let got = match read(stream, &mut buf).await {
                Ok(got) => got,
                Err(err) => return End::now(CloseReason::ReadFailed(err.kind())),
            };
            if got.closed_fds > 0 {
                let fds = got.closed_fds;
                return End::now(CloseReason::FdPassed { fds });
            }
            if got.len == 0 {
                return End::now(CloseReason::PeerClosed);
            }
            match cupper_sys::peer_token(stream) {
                Ok(token) if token == self.pinned => {}
                Ok(_) => return End::now(CloseReason::TokenMismatch),
                Err(refusal) => return End::now(CloseReason::TokenUnreadable(refusal.kind)),
            }
            decoder.push(&buf[..got.len]);
            loop {
                match decoder.next_frame() {
                    Ok(Some(frame)) => {
                        if let Some(end) = self.on_frame(frame) {
                            return end;
                        }
                    }
                    Ok(None) => break,
                    Err(_) => return End::now(CloseReason::FrameError),
                }
            }
        }
    }

    fn on_frame(&mut self, frame: Frame) -> Option<End> {
        let ready = matches!(self.phase, Phase::Ready(_));
        if frame.frame_type != FrameType::Json {
            // Blob chunks are T033's; before `initialize` nothing else counts.
            return (!ready).then(|| End::now(CloseReason::NotInitialized));
        }
        let message = serde_json::from_slice::<RawMessage>(&frame.payload)
            .map_err(|err| RpcError::new(PARSE_ERROR, err.to_string()))
            .and_then(RawMessage::classify);
        let (id, method, params) = match message {
            Ok(Message::Request { id, method, params }) => (id, method, params),
            // Client notifications are T033's; the host sends no requests.
            Ok(_) if ready => return None,
            Ok(_) => return Some(End::now(CloseReason::NotInitialized)),
            Err(err) => {
                self.outbox.push_frame(wire::error(None, &err));
                return (!ready).then(|| End::after_replies(CloseReason::NotInitialized));
            }
        };
        match &self.phase {
            Phase::Hello(nonce) if method == Initialize::NAME => {
                let nonce = *nonce;
                self.initialize(&id, &nonce, params)
            }
            Phase::Hello(_) => {
                let err = RpcError::app(ErrorCode::NotInitialized, "send initialize first");
                self.outbox.push_frame(wire::error(Some(&id), &err));
                Some(End::after_replies(CloseReason::NotInitialized))
            }
            Phase::Ready(_) => self.on_request(&id, &method, params),
        }
    }

    fn initialize(&mut self, id: &RpcId, nonce: &Nonce, params: Value) -> Option<End> {
        let config = &self.gw.config;
        match handshake::initialize(config, nonce, &self.pinned, self.verdict, params) {
            Ok((result, session, client)) => {
                self.outbox.push_frame(wire::response(id, &result));
                let principal = session.principal.clone();
                let weak_auth = result.host.weak_auth;
                let event = AuditEvent::Initialized {
                    principal,
                    client,
                    weak_auth,
                };
                self.gw.audit(self.id, event);
                self.phase = Phase::Ready(session);
                None
            }
            Err(err) => {
                self.outbox.push_frame(wire::error(Some(id), &err));
                Some(End::after_replies(CloseReason::HandshakeFailed(err)))
            }
        }
    }

    fn on_request(&mut self, id: &RpcId, method: &str, params: Value) -> Option<End> {
        let result = match method {
            Initialize::NAME => {
                let err = RpcError::app(ErrorCode::Forbidden, "initialize comes once")
                    .with_reason(ErrorReason::AlreadyInitialized);
                self.outbox.push_frame(wire::error(Some(id), &err));
                return Some(End::after_replies(CloseReason::AlreadyInitialized));
            }
            // A missing `params` reads as `{}`.
            Ping::NAME => match params {
                Value::Null => Ok(Empty {}),
                params => serde_json::from_value::<Empty>(params)
                    .map_err(|err| RpcError::new(INVALID_PARAMS, err.to_string())),
            },
            // Routing to the executors is T033's; until then nothing serves
            // the other methods.
            _ => Err(RpcError::app(
                ErrorCode::Unsupported,
                format!("{method} is not served"),
            )),
        };
        self.outbox.push_frame(match result {
            Ok(result) => wire::response(id, &result),
            Err(err) => wire::error(Some(id), &err),
        });
        None
    }
}

/// Reads like `read(2)`, closing any fds that came with the data: a plain
/// read would install them in our fd table (design 5.8.5).
async fn read(stream: &UnixStream, buf: &mut [u8]) -> io::Result<Received> {
    loop {
        stream.readable().await?;
        let read = || cupper_sys::recv_closing_fds(stream, &mut *buf);
        match stream.try_io(Interest::READABLE, read) {
            Err(err) if matches!(err.kind(), io::ErrorKind::WouldBlock) => {}
            Err(err) if matches!(err.kind(), io::ErrorKind::Interrupted) => {}
            result => return result,
        }
    }
}

async fn write_loop(mut writer: OwnedWriteHalf, outbox: Arc<Outbox>) {
    while let Some(frames) = outbox.drain().await {
        for frame in frames {
            if writer.write_all(&frame).await.is_err() {
                outbox.close();
                return;
            }
        }
    }
}
