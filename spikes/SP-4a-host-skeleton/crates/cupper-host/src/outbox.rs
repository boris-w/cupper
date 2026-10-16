//! Per-connection send queue.
//!
//! Producers (the event log fan-out, request handlers) only ever lock a mutex
//! and push; they never wait for the socket. Live events count against a byte
//! budget; when a push would exceed it, the hub resets that stream for this
//! connection instead of waiting. The connection's writer task drains the queue.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cupper_protocol::Event;
use tokio::sync::Notify;

#[derive(Debug)]
pub enum OutItem {
    /// A ready frame (response, snapshot page, reset). Never dropped.
    Frame(Vec<u8>),
    /// A live event; `json` is its serialized form.
    Event { stream: Arc<str>, event: Arc<Event>, json: Arc<str> },
    /// Events `after+1 ..= up_to` to be read from the log by the writer.
    Replay { stream: Arc<str>, after: u64, up_to: u64 },
}

#[derive(Debug, PartialEq, Eq)]
pub enum Push {
    Queued,
    /// The live backlog would exceed the budget; nothing was queued.
    Overflow,
    Closed,
}

#[derive(Debug, Default)]
struct State {
    items: VecDeque<OutItem>,
    live_bytes: usize,
    closed: bool,
}

#[derive(Debug)]
pub struct Outbox {
    pub id: u64,
    max_live_bytes: usize,
    state: Mutex<State>,
    notify: Notify,
}

impl Outbox {
    pub fn new(id: u64, max_live_bytes: usize) -> Arc<Self> {
        Arc::new(Self { id, max_live_bytes, state: Mutex::default(), notify: Notify::new() })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("outbox mutex poisoned")
    }

    pub fn push_frame(&self, frame: Vec<u8>) {
        let mut s = self.lock();
        if !s.closed {
            s.items.push_back(OutItem::Frame(frame));
            self.notify.notify_one();
        }
    }

    pub fn push_replay(&self, stream: Arc<str>, after: u64, up_to: u64) {
        let mut s = self.lock();
        if !s.closed {
            s.items.push_back(OutItem::Replay { stream, after, up_to });
            self.notify.notify_one();
        }
    }

    pub fn push_live(&self, stream: &Arc<str>, event: &Arc<Event>, json: &Arc<str>) -> Push {
        let mut s = self.lock();
        if s.closed {
            return Push::Closed;
        }
        if s.live_bytes + json.len() > self.max_live_bytes {
            return Push::Overflow;
        }
        s.live_bytes += json.len();
        s.items.push_back(OutItem::Event { stream: stream.clone(), event: event.clone(), json: json.clone() });
        self.notify.notify_one();
        Push::Queued
    }

    /// Drop this stream's queued live events and queue `reset_frame` after
    /// everything already queued.
    pub fn reset(&self, stream: &str, reset_frame: Vec<u8>) {
        let mut s = self.lock();
        let mut freed = 0;
        s.items.retain(|it| match it {
            OutItem::Event { stream: st, json, .. } if &**st == stream => {
                freed += json.len();
                false
            }
            _ => true,
        });
        s.live_bytes -= freed;
        if !s.closed {
            s.items.push_back(OutItem::Frame(reset_frame));
            self.notify.notify_one();
        }
    }

    /// Wait for and take everything queued. `None` once closed.
    pub async fn drain(&self) -> Option<Vec<OutItem>> {
        loop {
            {
                let mut s = self.lock();
                if s.closed {
                    return None;
                }
                if !s.items.is_empty() {
                    s.live_bytes = 0;
                    return Some(s.items.drain(..).collect());
                }
            }
            self.notify.notified().await;
        }
    }

    /// Take whatever is queued right now without waiting.
    pub fn try_drain(&self) -> Option<Vec<OutItem>> {
        let mut s = self.lock();
        if s.closed || s.items.is_empty() {
            return None;
        }
        s.live_bytes = 0;
        Some(s.items.drain(..).collect())
    }

    pub fn close(&self) {
        let mut s = self.lock();
        s.closed = true;
        s.items.clear();
        s.live_bytes = 0;
        self.notify.notify_one();
    }

    pub fn is_closed(&self) -> bool {
        self.lock().closed
    }
}
