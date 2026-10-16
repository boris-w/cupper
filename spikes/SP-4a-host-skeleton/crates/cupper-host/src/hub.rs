//! Streams: contiguous seq assignment, persistence, in-memory views and fan-out.
//!
//! Each stream has one lock. Appending (persist, apply to the view, push to
//! subscribers) and subscribing (pick replay or snapshot, register) both happen
//! under it, so a subscriber sees every event exactly once and in order.
//! Nothing under the lock waits on a socket.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use cupper_protocol::rpc::{ErrorCode, RpcError};
use cupper_protocol::sync::StreamId;
use cupper_protocol::view::StreamView;
use cupper_protocol::{Cause, EVENT_VERSION, Event, EventBody, ResetReason, SubscribeMode, SubscribeResult};
use thiserror::Error;

use crate::outbox::{Outbox, Push};
use crate::store::{Store, StoreError};
use crate::util::now_ms;
use crate::wire;

/// Largest seq a JSON number represents exactly.
pub const MAX_SEQ: u64 = 1 << 53;

#[derive(Debug, Error)]
pub enum HubError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Frame(#[from] cupper_protocol::frame::FrameError),
}

impl From<HubError> for RpcError {
    fn from(e: HubError) -> Self {
        RpcError::app(ErrorCode::Internal, e.to_string())
    }
}

#[derive(Debug)]
struct StreamState {
    name: Arc<str>,
    head: u64,
    /// First seq still in the log; `head + 1` when nothing is retained.
    first_retained: u64,
    view: StreamView,
    subs: Vec<Arc<Outbox>>,
}

#[derive(Debug)]
pub struct Hub {
    store: Arc<Store>,
    retain: u64,
    streams: Mutex<HashMap<StreamId, Arc<Mutex<StreamState>>>>,
}

impl Hub {
    pub fn new(store: Arc<Store>, retain: u64) -> Self {
        Self { store, retain, streams: Mutex::default() }
    }

    /// Load (checkpoint + log replay) or fetch a stream.
    fn stream(&self, id: &StreamId) -> Result<Arc<Mutex<StreamState>>, HubError> {
        let mut map = self.streams.lock().expect("hub mutex poisoned");
        if let Some(s) = map.get(id) {
            return Ok(s.clone());
        }
        let name: Arc<str> = id.to_string().into();
        let (mut at, mut view) = match self.store.load_checkpoint(&name)? {
            Some((at, json)) => (at, serde_json::from_str(&json)?),
            None => (0, StreamView::empty_for(id)),
        };
        loop {
            let page = self.store.read_events(&name, at, MAX_SEQ, 1000)?;
            if page.is_empty() {
                break;
            }
            for (seq, json) in page {
                let ev: Event = serde_json::from_str(&json)?;
                if let Err(e) = view.apply(&ev.body) {
                    tracing::warn!(stream = %name, seq, "view apply failed during load: {e}");
                }
                at = seq;
            }
        }
        let first_retained = self.store.event_bounds(&name)?.map_or(at + 1, |(first, _)| first);
        let state = Arc::new(Mutex::new(StreamState { name, head: at, first_retained, view, subs: Vec::new() }));
        map.insert(id.clone(), state.clone());
        Ok(state)
    }

    /// Append events to a stream. They are persisted before any subscriber sees them.
    pub fn append(&self, id: &StreamId, bodies: Vec<EventBody>, cause: Option<&Cause>) -> Result<u64, HubError> {
        let st = self.stream(id)?;
        let mut st = st.lock().expect("stream mutex poisoned");
        if bodies.is_empty() {
            return Ok(st.head);
        }
        let ts = now_ms();
        let mut events = Vec::with_capacity(bodies.len());
        let mut rows = Vec::with_capacity(bodies.len());
        for (i, body) in bodies.into_iter().enumerate() {
            let ev = Event { seq: st.head + 1 + i as u64, from: None, v: EVENT_VERSION, ts, cause: cause.cloned(), fallback: None, body };
            let json = serde_json::to_string(&ev)?;
            rows.push((ev.seq, json));
            events.push(ev);
        }
        self.store.append_events(&st.name, &rows)?;
        let name = st.name.clone();
        for (ev, (_, json)) in events.into_iter().zip(rows) {
            if let Err(e) = st.view.apply(&ev.body) {
                tracing::warn!(stream = %name, seq = ev.seq, "view apply failed: {e}");
            }
            st.head = ev.seq;
            let (ev, json): (Arc<Event>, Arc<str>) = (Arc::new(ev), json.into());
            st.subs.retain(|sub| match sub.push_live(&name, &ev, &json) {
                Push::Queued => true,
                Push::Closed => false,
                Push::Overflow => {
                    tracing::info!(stream = %name, conn = sub.id, "send queue over budget; resetting stream");
                    sub.reset(&name, wire::reset(&name, ResetReason::SlowConsumer));
                    false
                }
            });
        }
        Ok(st.head)
    }

    /// Register `outbox` on a stream. `respond` builds the response frame, which
    /// is queued before the snapshot or replay it announces.
    pub fn subscribe(
        &self,
        outbox: &Arc<Outbox>,
        id: &StreamId,
        after_seq: Option<u64>,
        respond: impl FnOnce(&SubscribeResult) -> Vec<u8>,
    ) -> Result<SubscribeResult, HubError> {
        let st = self.stream(id)?;
        let mut st = st.lock().expect("stream mutex poisoned");
        st.subs.retain(|s| s.id != outbox.id && !s.is_closed());
        let replay_from = after_seq.filter(|a| *a <= st.head && a + 1 >= st.first_retained);
        let result = SubscribeResult {
            stream: st.name.to_string(),
            mode: if replay_from.is_some() { SubscribeMode::Replay } else { SubscribeMode::Snapshot },
            at_seq: replay_from.unwrap_or(st.head),
            head_seq: st.head,
        };
        let frames = match replay_from {
            Some(_) => Vec::new(),
            None => wire::snapshot(&st.name, st.head, &st.view.to_ops())?,
        };
        outbox.push_frame(respond(&result));
        match replay_from {
            Some(after) if after < st.head => outbox.push_replay(st.name.clone(), after, st.head),
            Some(_) => {}
            None => frames.into_iter().for_each(|f| outbox.push_frame(f)),
        }
        st.subs.push(outbox.clone());
        Ok(result)
    }

    /// Read access to a stream's current view.
    pub fn with_view<T>(&self, id: &StreamId, f: impl FnOnce(&StreamView, u64) -> T) -> Result<T, HubError> {
        let st = self.stream(id)?;
        let st = st.lock().expect("stream mutex poisoned");
        Ok(f(&st.view, st.head))
    }

    /// Persist the view at the head and drop log entries beyond the retention window.
    pub fn checkpoint_and_prune(&self, id: &StreamId) -> Result<(), HubError> {
        let st = self.stream(id)?;
        let mut st = st.lock().expect("stream mutex poisoned");
        self.store.save_checkpoint(&st.name, st.head, &serde_json::to_string(&st.view)?)?;
        if st.head > self.retain {
            let cut = st.head - self.retain;
            if cut >= st.first_retained {
                self.store.prune_events(&st.name, cut)?;
                st.first_retained = cut + 1;
            }
        }
        Ok(())
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }
}
