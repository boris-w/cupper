//! A connection's send queue (design 5.2.3, 5.2.4).
//!
//! Producers lock, push and leave; they never wait for the socket. The
//! connection's writer task (`send`) takes everything queued at once and
//! writes it. Frames such as responses are never dropped. Live events count
//! against the connection's budget (`limits.max_queue_bytes`, design 5.3.9):
//! a push that would exceed it is refused with [`Push::Full`], and the
//! stream then resets the subscription for this connection only (design
//! 5.3.3).
//!
//! Each subscription is a [`Feed`] of its own: its live events, its snapshot
//! and its replay from the log queue under the feed's id, and ending the
//! feed drops whatever of it is still queued. The writer encodes live events
//! only when it sends them, so that it can coalesce them, and it pages
//! snapshots and reads replays itself, outside the stream lock.

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use cupper_protocol::event::EventBody;
use cupper_protocol::methods::stream::{ResetReason, StreamReset, StreamResetParams};
use cupper_protocol::sync::StreamId;
use tokio::sync::Notify;

use super::wire;
use crate::store::Readers;
pub use crate::store::streams::Push;
use crate::store::streams::{Published, Sink};

#[derive(Debug)]
pub struct Outbox {
    max_live_bytes: usize,
    state: Mutex<State>,
    notify: Notify,
}

#[derive(Debug, Default)]
struct State {
    items: VecDeque<Item>,
    /// Bytes of the live events in `items`.
    live_bytes: usize,
    phase: Phase,
    /// The feeds that have not ended.
    feeds: HashSet<u64>,
    next_feed: u64,
    coalesce: Coalesce,
}

#[derive(Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Open,
    /// Takes nothing new; the writer sends what is queued, then stops.
    Finishing,
    Closed,
}

/// How the writer sends live events, as `initialize` settled it (design
/// 5.3.2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Coalesce {
    /// `coalesce_ms`: having taken a live event, the writer waits this long
    /// for more before it sends.
    pub window: Duration,
    /// `stream.coalesce`: the client takes range events with `from`.
    pub ranges: bool,
}

#[derive(Debug)]
pub enum Item {
    Frame(Vec<u8>),
    Live {
        stream: StreamId,
        feed: u64,
        event: Arc<Published>,
    },
    /// A snapshot at `at_seq`; the writer sends it as `stream/snapshot` pages.
    Snapshot {
        stream: StreamId,
        feed: u64,
        at_seq: u64,
        epoch: u64,
        ops: Vec<EventBody>,
    },
    /// The events `after + 1..=up_to`, which the writer reads from the log.
    Replay {
        stream: StreamId,
        feed: u64,
        after: u64,
        up_to: u64,
        readers: Readers,
    },
}

impl Item {
    /// The feed of a subscription's item.
    pub fn feed(&self) -> Option<u64> {
        match self {
            Self::Frame(_) => None,
            Self::Live { feed, .. } | Self::Snapshot { feed, .. } | Self::Replay { feed, .. } => {
                Some(*feed)
            }
        }
    }
}

/// One subscription's way into the outbox: the [`Sink`] its stream holds.
#[derive(Debug)]
pub struct Feed {
    outbox: Arc<Outbox>,
    pub id: u64,
}

impl Sink for Feed {
    fn push(&self, stream: &StreamId, event: &Arc<Published>) -> Push {
        self.outbox.push_live(stream, self.id, event)
    }

    fn reset(&self, stream: &StreamId) {
        let reason = Some(ResetReason::SlowConsumer);
        self.outbox.end_feed(stream, self.id, reason);
    }
}

impl Outbox {
    pub fn new(max_live_bytes: usize) -> Self {
        Self {
            max_live_bytes,
            state: Mutex::default(),
            notify: Notify::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Queues a frame unless the connection is closing.
    pub fn push_frame(&self, frame: Vec<u8>) {
        self.push(Item::Frame(frame));
    }

    /// Queues `item` unless the connection is closing or its feed ended.
    /// Live events go through [`push_live`](Self::push_live) instead.
    pub fn push(&self, item: Item) {
        let mut state = self.lock();
        let ended = item.feed().is_some_and(|feed| !state.feeds.contains(&feed));
        if state.phase == Phase::Open && !ended {
            state.items.push_back(item);
            self.notify.notify_one();
        }
    }

    /// A new feed; it lives until [`end_feed`](Self::end_feed).
    pub fn open_feed(self: &Arc<Self>) -> Feed {
        let mut state = self.lock();
        let id = state.next_feed;
        state.next_feed += 1;
        state.feeds.insert(id);
        Feed {
            outbox: Arc::clone(self),
            id,
        }
    }

    pub fn feed_live(&self, feed: u64) -> bool {
        self.lock().feeds.contains(&feed)
    }

    /// Queues a live event of `feed` within the budget.
    pub fn push_live(&self, stream: &StreamId, feed: u64, event: &Arc<Published>) -> Push {
        let mut state = self.lock();
        if state.phase != Phase::Open || !state.feeds.contains(&feed) {
            return Push::Closed;
        }
        let bytes = event.json.len();
        if state.live_bytes + bytes > self.max_live_bytes {
            return Push::Full;
        }
        state.live_bytes += bytes;
        state.items.push_back(Item::Live {
            stream: stream.clone(),
            feed,
            event: Arc::clone(event),
        });
        self.notify.notify_one();
        Push::Queued
    }

    /// Ends `feed`: what it has queued is dropped, and with a `reset`
    /// reason `stream/reset` is queued after everything else. Returns
    /// whether the feed was still live; ending it again does nothing.
    pub fn end_feed(&self, stream: &StreamId, feed: u64, reset: Option<ResetReason>) -> bool {
        let mut state = self.lock();
        if !state.feeds.remove(&feed) {
            return false;
        }
        let mut freed = 0;
        state.items.retain(|item| match item {
            Item::Live {
                feed: of, event, ..
            } if *of == feed => {
                freed += event.json.len();
                false
            }
            item => item.feed() != Some(feed),
        });
        state.live_bytes -= freed;
        if let Some(reason) = reset
            && state.phase == Phase::Open
        {
            let stream = stream.clone();
            let params = StreamResetParams { stream, reason };
            let frame = wire::notification::<StreamReset>(&params);
            state.items.push_back(Item::Frame(frame));
            self.notify.notify_one();
        }
        true
    }

    pub fn set_coalesce(&self, coalesce: Coalesce) {
        self.lock().coalesce = coalesce;
    }

    pub fn coalesce(&self) -> Coalesce {
        self.lock().coalesce
    }

    /// Waits for items and takes all of them; `None` once there are no more
    /// to send.
    pub async fn drain(&self) -> Option<Vec<Item>> {
        loop {
            {
                let mut state = self.lock();
                if state.phase == Phase::Closed {
                    return None;
                }
                if !state.items.is_empty() {
                    return Some(take(&mut state));
                }
                if state.phase == Phase::Finishing {
                    return None;
                }
            }
            // A notification sent since the check above is kept as a permit.
            self.notify.notified().await;
        }
    }

    /// Takes what is queued now, without waiting.
    pub fn take(&self) -> Vec<Item> {
        let mut state = self.lock();
        match state.phase {
            Phase::Closed => Vec::new(),
            Phase::Open | Phase::Finishing => take(&mut state),
        }
    }

    /// Takes nothing new; [`drain`](Self::drain) hands out what is queued,
    /// then ends.
    pub fn finish(&self) {
        let mut state = self.lock();
        if state.phase == Phase::Open {
            state.phase = Phase::Finishing;
        }
        self.notify.notify_one();
    }

    /// Drops everything queued; [`drain`](Self::drain) ends at once.
    pub fn close(&self) {
        let mut state = self.lock();
        state.phase = Phase::Closed;
        state.items.clear();
        state.live_bytes = 0;
        self.notify.notify_one();
    }
}

/// What the writer takes no longer counts against the budget.
fn take(state: &mut State) -> Vec<Item> {
    state.live_bytes = 0;
    state.items.drain(..).collect()
}

#[cfg(test)]
mod tests {
    use cupper_protocol::frame::FrameDecoder;
    use cupper_protocol::ids::SessionId;
    use serde_json::{Value, json};

    use super::*;
    use crate::store::testutil::{project, published};

    fn session(n: u128) -> StreamId {
        StreamId::Session(SessionId::from_uuid(n))
    }

    /// A live event of `bytes` bytes.
    fn live(bytes: usize) -> Arc<Published> {
        let event = Arc::into_inner(published(1, 0, project(1))).unwrap().event;
        let json = "x".repeat(bytes).into_boxed_str();
        Arc::new(Published { event, json })
    }

    /// What was taken: `frame <bytes>`, `live <feed> <bytes>`, or the
    /// method of a JSON frame.
    fn seen(items: Vec<Item>) -> Vec<String> {
        let frame = |frame: Vec<u8>| {
            let mut decoder = FrameDecoder::new();
            decoder.push(&frame);
            match decoder.next_frame() {
                Ok(Some(frame)) => {
                    let message: Value = serde_json::from_slice(&frame.payload).unwrap();
                    format!("{}", message["params"])
                }
                _ => format!("frame {}", frame.len()),
            }
        };
        let item = |item| match item {
            Item::Frame(bytes) => frame(bytes),
            Item::Live { feed, event, .. } => format!("live {feed} {}", event.json.len()),
            Item::Snapshot { feed, .. } => format!("snapshot {feed}"),
            Item::Replay { feed, .. } => format!("replay {feed}"),
        };
        items.into_iter().map(item).collect()
    }

    #[tokio::test]
    async fn outbox_live_events_stay_within_the_budget_and_frames_always_queue() {
        let outbox = Arc::new(Outbox::new(10));
        let (a, b) = (outbox.open_feed(), outbox.open_feed());
        assert_eq!(a.push(&StreamId::Index, &live(6)), Push::Queued);
        assert_eq!(a.push(&StreamId::Index, &live(5)), Push::Full);
        outbox.push_frame(vec![3; 100]);
        assert_eq!(b.push(&session(1), &live(4)), Push::Queued);
        let sent = seen(outbox.drain().await.unwrap());
        assert_eq!(sent, ["live 0 6", "frame 100", "live 1 4"]);
        // What the writer took no longer counts.
        assert_eq!(a.push(&StreamId::Index, &live(10)), Push::Queued);
    }

    #[tokio::test]
    async fn outbox_ending_a_feed_drops_what_it_queued_and_may_say_so() {
        let (slow, other) = (session(1), session(2));
        let outbox = Arc::new(Outbox::new(10));
        let (a, b) = (outbox.open_feed(), outbox.open_feed());
        let snapshot = |stream: &StreamId, feed: &Feed| Item::Snapshot {
            stream: stream.clone(),
            feed: feed.id,
            at_seq: 0,
            epoch: 1,
            ops: Vec::new(),
        };
        outbox.push(snapshot(&slow, &a));
        a.push(&slow, &live(4));
        b.push(&other, &live(3));
        a.push(&slow, &live(3));
        assert_eq!(a.push(&slow, &live(4)), Push::Full);
        a.reset(&slow);
        // The slow feed's bytes are free again, and it takes nothing more.
        assert_eq!(b.push(&other, &live(7)), Push::Queued);
        assert_eq!(a.push(&slow, &live(1)), Push::Closed);
        outbox.push(snapshot(&slow, &a));
        assert!(!outbox.end_feed(&slow, a.id, Some(ResetReason::Rebuilt)));
        let reset = json!({"stream": slow.to_string(), "reason": "slow_consumer"});
        let sent = seen(outbox.drain().await.unwrap());
        assert_eq!(sent, ["live 1 3", &reset.to_string(), "live 1 7"]);

        // Ended without a reason, as on unsubscribe: nothing is said.
        outbox.push(snapshot(&other, &b));
        assert!(outbox.end_feed(&other, b.id, None));
        assert!(!outbox.feed_live(b.id));
        outbox.push_frame(vec![1]);
        assert_eq!(seen(outbox.take()), ["frame 1"]);
    }

    #[tokio::test]
    async fn outbox_finish_sends_the_rest_and_close_drops_it() {
        let outbox = Arc::new(Outbox::new(10));
        let feed = outbox.open_feed();
        outbox.push_frame(vec![1]);
        outbox.finish();
        outbox.push_frame(vec![2]);
        assert_eq!(feed.push(&StreamId::Index, &live(1)), Push::Closed);
        assert_eq!(seen(outbox.drain().await.unwrap()), ["frame 1"]);
        assert!(outbox.drain().await.is_none());

        let outbox = Outbox::new(10);
        outbox.push_frame(vec![1]);
        outbox.close();
        outbox.push_frame(vec![2]);
        assert!(outbox.drain().await.is_none());
        assert!(outbox.take().is_empty());
    }
}
