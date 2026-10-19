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
use std::time::Duration;

use cupper_protocol::error::{ErrorCode, ErrorReason, PARSE_ERROR, RpcError};
use cupper_protocol::frame::{Frame, FrameDecoder, FrameType, MAX_FRAME_BYTES};
use cupper_protocol::ids::ConnId;
use cupper_protocol::limits::V1;
use cupper_protocol::methods::handshake::{Feature, HostHello, Initialize};
use cupper_protocol::rpc::{Message, Method, RawMessage, RpcId};
use cupper_sys::{AuditToken, Received};
use futures::future::{self, Either};
use serde_json::Value;
use tokio::io::Interest;
use tokio::net::UnixStream;
use tokio::net::unix::OwnedReadHalf;

use super::audit::{AuditEvent, CloseReason, Rejection};
use super::dispatch::{Dispatch, Opened};
use super::handshake::{self, Nonce, Session};
use super::outbox::{Coalesce, Outbox};
use super::{Gateway, Verdict, send, wire};
use crate::clock::Timer;
use crate::durations::{GATEWAY_CLOSE_GRACE, GATEWAY_HANDSHAKE};

pub(super) async fn run(gw: Arc<Gateway>, stream: UnixStream, id: ConnId) {
    let opened = gw.clock.now_mono();
    let Some((pinned, verdict)) = admit(&gw, &stream, &id) else {
        return;
    };
    let (reader, writer) = stream.into_split();
    let outbox = Arc::new(Outbox::new(V1.max_queue_bytes as usize));
    let clock = Arc::clone(&gw.clock);
    let mut writing = tokio::spawn(send::write_loop(writer, Arc::clone(&outbox), clock));
    let handshake = gw.durations.get(GATEWAY_HANDSHAKE);
    let deadline = gw.clock.timer_at(gw.clock.now_mono() + handshake);
    let nonce = handshake::new_nonce();
    outbox.push_frame(wire::notification::<HostHello>(&handshake::hello(&nonce)));
    let mut conn = Conn {
        gw: &gw,
        id: &id,
        outbox: &outbox,
        pinned,
        verdict,
        phase: Phase::Hello(nonce),
        deadline: Some(deadline),
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
        Phase::Ready(session, _) => {
            gw.routes.closed(&id);
            Some(session.principal.id)
        }
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
    Ready(Session, Box<Dispatch>),
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
    outbox: &'a Arc<Outbox>,
    pinned: AuditToken,
    verdict: Verdict,
    phase: Phase,
    /// Until `initialize`: when the handshake times out.
    deadline: Option<Timer>,
}

impl Conn<'_> {
    async fn read_loop(&mut self, reader: &OwnedReadHalf) -> End {
        let stream: &UnixStream = reader.as_ref();
        let mut buf = vec![0; MAX_FRAME_BYTES];
        let mut decoder = FrameDecoder::new();
        loop {
            let read = read(stream, &mut buf);
            let got = match self.deadline.as_mut() {
                Some(deadline) => match future::select(std::pin::pin!(read), deadline).await {
                    Either::Left((got, _)) => got,
                    Either::Right(_) => return End::now(CloseReason::HandshakeTimeout),
                },
                None => read.await,
            };
            let got = match got {
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
        let ready = matches!(self.phase, Phase::Ready(..));
        if frame.frame_type != FrameType::Json {
            // Blob chunks are T029's; before `initialize` nothing else counts.
            return (!ready).then(|| End::now(CloseReason::NotInitialized));
        }
        let message = serde_json::from_slice::<RawMessage>(&frame.payload)
            .map_err(|err| RpcError::new(PARSE_ERROR, err.to_string()))
            .and_then(RawMessage::classify);
        let (id, method, params) = match message {
            Ok(Message::Request { id, method, params }) => (id, method, params),
            Ok(Message::Notification { method, params }) if ready => {
                if let Phase::Ready(_, dispatch) = &self.phase {
                    dispatch.notification(&self.gw.routes, &method, params);
                }
                return None;
            }
            // The host sends no requests, so it takes no responses.
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
            Phase::Ready(..) => self.on_request(id, &method, params),
        }
    }

    fn initialize(&mut self, id: &RpcId, nonce: &Nonce, params: Value) -> Option<End> {
        let config = &self.gw.config;
        match handshake::initialize(config, nonce, &self.pinned, self.verdict, params) {
            Ok((result, session, client)) => {
                self.deadline = None;
                self.outbox.push_frame(wire::response(id, &result));
                let principal = session.principal.clone();
                let weak_auth = result.host.weak_auth;
                self.gw.routes.opened(Opened {
                    conn: self.id.clone(),
                    principal: principal.clone(),
                    client: client.clone(),
                    weak_auth,
                });
                let event = AuditEvent::Initialized {
                    principal,
                    client,
                    weak_auth,
                };
                self.gw.audit(self.id, event);
                self.outbox.set_coalesce(Coalesce {
                    window: Duration::from_millis(session.coalesce_ms.into()),
                    ranges: session.features.contains(&Feature::StreamCoalesce),
                });
                let outbox = Arc::clone(self.outbox);
                let principal = session.principal.clone();
                let dispatch = Box::new(Dispatch::new(self.id.clone(), principal, outbox));
                self.phase = Phase::Ready(session, dispatch);
                None
            }
            Err(err) => {
                self.outbox.push_frame(wire::error(Some(id), &err));
                Some(End::after_replies(CloseReason::HandshakeFailed(err)))
            }
        }
    }

    fn on_request(&mut self, id: RpcId, method: &str, params: Value) -> Option<End> {
        if method == Initialize::NAME {
            let err = RpcError::app(ErrorCode::Forbidden, "initialize comes once")
                .with_reason(ErrorReason::AlreadyInitialized);
            self.outbox.push_frame(wire::error(Some(&id), &err));
            return Some(End::after_replies(CloseReason::AlreadyInitialized));
        }
        if let Phase::Ready(_, dispatch) = &self.phase {
            dispatch.request(&self.gw.routes, id, method, params);
        }
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
