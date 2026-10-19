//! A connection's send queue (design 5.2.3, 5.2.4).
//!
//! Producers lock, push and leave; they never wait for the socket. The
//! connection's writer task takes everything queued at once and writes it.
//! Frames such as responses are never dropped. Live events count against the
//! connection's budget (`limits.max_queue_bytes`, design 5.3.9): a push that
//! would exceed it is refused with [`Push::Full`], and the stream then resets
//! the subscription for this connection only ([`Outbox::reset`], design
//! 5.3.3). Coalescing live events at send time and replaying from the log are
//! T033's.

use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard, PoisonError};

use cupper_protocol::methods::stream::{ResetReason, StreamReset, StreamResetParams};
use cupper_protocol::sync::StreamId;
use tokio::sync::Notify;

use super::wire;

/// What became of a live event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Push {
    Queued,
    /// Over the budget; nothing was queued.
    Full,
    /// The connection is closing.
    Closed,
}

#[derive(Debug)]
pub struct Outbox {
    max_live_bytes: usize,
    state: Mutex<State>,
    notify: Notify,
}

#[derive(Debug, Default)]
struct State {
    items: VecDeque<Item>,
    /// Bytes of the live frames in `items`.
    live_bytes: usize,
    phase: Phase,
}

#[derive(Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Open,
    /// Takes nothing new; the writer sends what is queued, then stops.
    Finishing,
    Closed,
}

#[derive(Debug)]
enum Item {
    Frame(Vec<u8>),
    Live { stream: StreamId, frame: Vec<u8> },
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
        let mut state = self.lock();
        if state.phase == Phase::Open {
            state.items.push_back(Item::Frame(frame));
            self.notify.notify_one();
        }
    }

    /// Queues a `stream/events` frame of `stream` within the budget.
    pub fn push_live(&self, stream: &StreamId, frame: Vec<u8>) -> Push {
        let mut state = self.lock();
        if state.phase != Phase::Open {
            return Push::Closed;
        }
        if state.live_bytes + frame.len() > self.max_live_bytes {
            return Push::Full;
        }
        state.live_bytes += frame.len();
        state.items.push_back(Item::Live {
            stream: stream.clone(),
            frame,
        });
        self.notify.notify_one();
        Push::Queued
    }

    /// Drops the queued live frames of `stream` and queues `stream/reset
    /// {reason: slow_consumer}` after everything else queued.
    pub fn reset(&self, stream: &StreamId) {
        let mut state = self.lock();
        let mut freed = 0;
        state.items.retain(|item| match item {
            Item::Live { stream: of, frame } if of == stream => {
                freed += frame.len();
                false
            }
            _ => true,
        });
        state.live_bytes -= freed;
        drop(state);
        self.push_frame(wire::notification::<StreamReset>(&StreamResetParams {
            stream: stream.clone(),
            reason: ResetReason::SlowConsumer,
        }));
    }

    /// Waits for frames and takes all of them; `None` once there are no more
    /// to send.
    pub async fn drain(&self) -> Option<Vec<Vec<u8>>> {
        loop {
            {
                let mut state = self.lock();
                if state.phase == Phase::Closed {
                    return None;
                }
                if !state.items.is_empty() {
                    state.live_bytes = 0;
                    let items = state.items.drain(..).map(|item| match item {
                        Item::Frame(frame) | Item::Live { frame, .. } => frame,
                    });
                    return Some(items.collect());
                }
                if state.phase == Phase::Finishing {
                    return None;
                }
            }
            // A notification sent since the check above is kept as a permit.
            self.notify.notified().await;
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

#[cfg(test)]
mod tests {
    use cupper_protocol::frame::FrameDecoder;
    use cupper_protocol::ids::SessionId;
    use serde_json::{Value, json};

    use super::*;

    fn session(n: u128) -> StreamId {
        StreamId::Session(SessionId::from_uuid(n))
    }

    fn json_of(frame: &[u8]) -> Value {
        let mut decoder = FrameDecoder::new();
        decoder.push(frame);
        serde_json::from_slice(&decoder.next_frame().unwrap().unwrap().payload).unwrap()
    }

    #[tokio::test]
    async fn outbox_live_frames_stay_within_the_budget_and_frames_always_queue() {
        let outbox = Outbox::new(10);
        assert_eq!(outbox.push_live(&StreamId::Index, vec![1; 6]), Push::Queued);
        assert_eq!(outbox.push_live(&StreamId::Index, vec![2; 5]), Push::Full);
        outbox.push_frame(vec![3; 100]);
        assert_eq!(outbox.push_live(&session(1), vec![4; 4]), Push::Queued);
        let sent = outbox.drain().await.unwrap();
        assert_eq!(sent, [vec![1; 6], vec![3; 100], vec![4; 4]]);
        // What the writer took no longer counts.
        assert_eq!(
            outbox.push_live(&StreamId::Index, vec![5; 10]),
            Push::Queued
        );
    }

    #[tokio::test]
    async fn outbox_reset_drops_one_streams_live_frames_and_says_so() {
        let (slow, other) = (session(1), session(2));
        let outbox = Outbox::new(10);
        outbox.push_live(&slow, vec![1; 4]);
        outbox.push_live(&other, vec![2; 3]);
        outbox.push_live(&slow, vec![3; 3]);
        assert_eq!(outbox.push_live(&slow, vec![4; 4]), Push::Full);
        outbox.reset(&slow);
        // The slow stream's bytes are free again.
        assert_eq!(outbox.push_live(&other, vec![5; 7]), Push::Queued);
        let sent = outbox.drain().await.unwrap();
        assert_eq!(sent.len(), 3);
        assert_eq!((&sent[0], &sent[2]), (&vec![2; 3], &vec![5; 7]));
        let reset = json!({"jsonrpc": "2.0", "method": "stream/reset",
            "params": {"stream": slow.to_string(), "reason": "slow_consumer"}});
        assert_eq!(json_of(&sent[1]), reset);
    }

    #[tokio::test]
    async fn outbox_finish_sends_the_rest_and_close_drops_it() {
        let outbox = Outbox::new(10);
        outbox.push_frame(vec![1]);
        outbox.finish();
        outbox.push_frame(vec![2]);
        assert_eq!(outbox.push_live(&StreamId::Index, vec![3]), Push::Closed);
        assert_eq!(outbox.drain().await, Some(vec![vec![1]]));
        assert_eq!(outbox.drain().await, None);

        let outbox = Outbox::new(10);
        outbox.push_frame(vec![1]);
        outbox.close();
        outbox.push_frame(vec![2]);
        assert_eq!(outbox.drain().await, None);
    }
}
