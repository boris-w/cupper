//! Subscriptions (design 5.3.3): the `stream/subscribe` and
//! `stream/unsubscribe` of one connection, applied in arrival order by a
//! task of the connection's own.
//!
//! - The read loop starts loading the stream as the request arrives, so
//!   loads run side by side; the task takes the requests in order, so a
//!   subscribe followed by an unsubscribe of the stream ends unsubscribed.
//! - Under the stream lock, `start` queues the response, then the snapshot
//!   or the replay; live events follow (T026).
//! - A connection has at most `limits.max_subscriptions`. Subscribing to a
//!   stream again replaces its subscription; one the stream reset no longer
//!   counts.
//! - When the connection closes, its `Dispatch` and with it the task's
//!   channel go, and the task drops every subscription.
//! - A session stream without events (head 0) is `not_found`: a session's
//!   first events are committed with its creation (design 5.3.11, rulings
//!   before T033).
//! - Each subscription is recorded as `stream.subscribed` (Q16).

use std::collections::HashMap;
use std::sync::Arc;

use cupper_protocol::error::{ErrorCode, RpcError};
use cupper_protocol::limits::V1;
use cupper_protocol::methods::Empty;
use cupper_protocol::methods::stream::{StreamSubscribeParams, StreamSubscribeResult};
use cupper_protocol::rpc::RpcId;
use cupper_protocol::sync::{StreamId, SubscribeMode};
use tokio::sync::{OwnedSemaphorePermit, mpsc};
use tokio::task::JoinHandle;

use super::audit::{AuditEvent, Auditor};
use super::dispatch::internal;
use super::outbox::{Item, Outbox};
use super::routes::Source;
use super::wire;
use crate::store::Readers;
use crate::store::streams::{StreamError, StreamHandle, Subscription};

#[cfg(test)]
mod tests;

/// A connection's subscriptions: where its read loop sends them.
#[derive(Debug)]
pub(super) struct Subscriptions {
    ops: mpsc::UnboundedSender<Op>,
}

#[derive(Debug)]
enum Op {
    Subscribe {
        id: RpcId,
        params: StreamSubscribeParams,
        opening: JoinHandle<Result<StreamHandle, StreamError>>,
        readers: Readers,
        /// The request's place in flight.
        place: OwnedSemaphorePermit,
    },
    Unsubscribe {
        id: RpcId,
        stream: StreamId,
        place: OwnedSemaphorePermit,
    },
}

impl Subscriptions {
    /// Spawns the task; call within a tokio runtime.
    pub fn start(outbox: Arc<Outbox>, auditor: Auditor) -> Self {
        let (ops, queued) = mpsc::unbounded_channel();
        let live = HashMap::new();
        let table = Table {
            outbox,
            auditor,
            live,
        };
        tokio::spawn(table.run(queued));
        Self { ops }
    }

    pub fn subscribe(
        &self,
        id: RpcId,
        source: &Source,
        params: StreamSubscribeParams,
        place: OwnedSemaphorePermit,
    ) {
        let (streams, stream) = (source.streams.clone(), params.stream.clone());
        let opening = tokio::spawn(async move { streams.open(&stream).await });
        let readers = source.readers.clone();
        // The task outlives this sender.
        let _ = self.ops.send(Op::Subscribe {
            id,
            params,
            opening,
            readers,
            place,
        });
    }

    pub fn unsubscribe(&self, id: RpcId, stream: StreamId, place: OwnedSemaphorePermit) {
        let _ = self.ops.send(Op::Unsubscribe { id, stream, place });
    }
}

/// The task's side: the live subscriptions, by stream.
struct Table {
    outbox: Arc<Outbox>,
    auditor: Auditor,
    live: HashMap<StreamId, Live>,
}

struct Live {
    feed: u64,
    _subscription: Subscription,
}

impl Table {
    async fn run(mut self, mut ops: mpsc::UnboundedReceiver<Op>) {
        while let Some(op) = ops.recv().await {
            match op {
                Op::Subscribe {
                    id,
                    params,
                    opening,
                    readers,
                    place,
                } => {
                    let opened = opening.await;
                    // What is left runs without waiting, up to the response.
                    drop(place);
                    let subscribed = match opened {
                        Ok(Ok(handle)) => self.subscribe(&id, &params, &handle, readers),
                        Ok(Err(err)) => Err(internal(err)),
                        Err(err) => Err(internal(err)),
                    };
                    if let Err(err) = subscribed {
                        self.outbox.push_frame(wire::error(Some(&id), &err));
                    }
                }
                Op::Unsubscribe { id, stream, place } => {
                    self.end(&stream);
                    drop(place);
                    self.outbox.push_frame(wire::response(&id, &Empty {}));
                }
            }
        }
    }

    fn subscribe(
        &mut self,
        id: &RpcId,
        params: &StreamSubscribeParams,
        handle: &StreamHandle,
        readers: Readers,
    ) -> Result<(), RpcError> {
        let stream = &params.stream;
        if matches!(stream, StreamId::Session(_)) && handle.read(|_, head| head) == 0 {
            let err = RpcError::app(ErrorCode::NotFound, format!("{stream} has no events"));
            return Err(err);
        }
        self.end(stream);
        let outbox = &self.outbox;
        self.live.retain(|_, live| outbox.feed_live(live.feed));
        if self.live.len() >= V1.max_subscriptions as usize {
            let err = RpcError::app(ErrorCode::RateLimited, "too many subscriptions");
            return Err(err);
        }
        let feed = Arc::new(outbox.open_feed());
        let (resume, feed_id) = (params.resume(), feed.id);
        let mut subscribed = None;
        let subscription = handle.subscribe(feed, resume, |start| {
            let result = StreamSubscribeResult {
                stream: stream.clone(),
                mode: start.mode.clone(),
                reason: start.reason.clone(),
                at_seq: start.at_seq,
                head_seq: start.head_seq,
                epoch: start.epoch,
            };
            outbox.push_frame(wire::response(id, &result));
            let (stream, feed) = (stream.clone(), feed_id);
            outbox.push(match start.mode {
                SubscribeMode::Replay => Item::Replay {
                    stream,
                    feed,
                    after: start.at_seq,
                    up_to: start.head_seq,
                    readers,
                },
                _ => Item::Snapshot {
                    stream,
                    feed,
                    at_seq: start.at_seq,
                    epoch: start.epoch,
                    ops: start.ops,
                },
            });
            let cursor = resume.filter(|cursor| cursor.epoch == start.epoch);
            subscribed = Some(AuditEvent::Subscribed {
                stream: result.stream,
                mode: result.mode,
                reason: result.reason,
                lag: cursor.and_then(|cursor| start.head_seq.checked_sub(cursor.after_seq)),
            });
        });
        let live = Live {
            feed: feed_id,
            _subscription: subscription,
        };
        self.live.insert(stream.clone(), live);
        self.auditor
            .record(subscribed.expect("start runs within subscribe"));
        Ok(())
    }

    /// Ends the subscription to `stream`, if there is one: nothing more of
    /// it is sent.
    fn end(&mut self, stream: &StreamId) {
        if let Some(live) = self.live.remove(stream) {
            self.outbox.end_feed(stream, live.feed, None);
        }
    }
}
