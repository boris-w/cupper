//! A client-side copy of one stream: snapshot assembly, the contiguity check
//! (`from` or `seq` must be the previous seq + 1) and the folded view.

use cupper_protocol::sync::{GapError, StreamCursor, StreamId};
use cupper_protocol::view::{StreamView, ViewError};
use cupper_protocol::{StreamEventsParams, StreamSnapshotParams, SubscribeMode, SubscribeResult};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SyncError {
    #[error(transparent)]
    Gap(#[from] GapError),
    #[error(transparent)]
    View(#[from] ViewError),
    #[error("data for a stream that is not subscribed or still loading")]
    NotReady,
}

#[derive(Debug, Clone)]
pub struct Mirror {
    pub stream: StreamId,
    pub view: StreamView,
    cursor: Option<StreamCursor>,
    building: Option<(u64, StreamView)>,
    /// Events received (a range event counts once).
    pub events: u64,
    /// Range events (with `from`) received.
    pub ranges: u64,
    pub snapshots: u64,
    pub resets: u64,
}

impl Mirror {
    pub fn new(stream: StreamId) -> Self {
        Self { view: StreamView::empty_for(&stream), stream, cursor: None, building: None, events: 0, ranges: 0, snapshots: 0, resets: 0 }
    }

    /// Last applied seq; what to pass as `after_seq` when resubscribing.
    pub fn last_seq(&self) -> Option<u64> {
        self.cursor.map(|c| c.last_seq())
    }

    pub fn on_subscribed(&mut self, r: &SubscribeResult) {
        match r.mode {
            SubscribeMode::Replay => self.cursor = Some(StreamCursor::at(r.at_seq)),
            _ => {
                self.cursor = None;
                self.building = Some((r.at_seq, StreamView::empty_for(&self.stream)));
            }
        }
    }

    pub fn on_snapshot(&mut self, p: &StreamSnapshotParams) -> Result<(), SyncError> {
        let (at, view) = self.building.as_mut().ok_or(SyncError::NotReady)?;
        for op in &p.ops {
            view.apply(op)?;
        }
        if p.last {
            let at = *at;
            let (_, view) = self.building.take().expect("present");
            self.view = view;
            self.cursor = Some(StreamCursor::at(at));
            self.snapshots += 1;
        }
        Ok(())
    }

    pub fn on_events(&mut self, p: &StreamEventsParams) -> Result<(), SyncError> {
        let cursor = self.cursor.as_mut().ok_or(SyncError::NotReady)?;
        for e in &p.events {
            cursor.accept(e)?;
            self.view.apply(&e.body)?;
            self.events += 1;
            if e.from.is_some() {
                self.ranges += 1;
            }
        }
        Ok(())
    }

    /// The host dropped the subscription; keep the cursor for resubscribing.
    pub fn on_reset(&mut self) {
        self.resets += 1;
        self.building = None;
    }

    pub fn session(&self) -> Option<&cupper_protocol::view::SessionView> {
        match &self.view {
            StreamView::Session(s) => Some(s),
            _ => None,
        }
    }

    pub fn index(&self) -> Option<&cupper_protocol::view::IndexView> {
        match &self.view {
            StreamView::Index(i) => Some(i),
            _ => None,
        }
    }
}
