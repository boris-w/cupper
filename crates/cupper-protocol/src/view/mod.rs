//! Views (design 5.3.3, 5.2.8, 5.7.6): the state of a stream, folded from its
//! event bodies by one pure `apply`. The host keeps them to serve snapshots;
//! clients fold the same bodies into the same views. A snapshot is a list of
//! bodies ([`View::snapshot_ops`]), so live events, snapshots and history
//! pages all go through the same `apply`.
//!
//! The serialized form of [`IndexView`] and [`SessionView`] is part of the
//! schema: the TS views must produce the same JSON (test #16). Absent optional
//! values are left out; maps, lists and strings are always present.

#[cfg(test)]
mod fixtures;
mod index;
mod ops;
#[cfg(test)]
mod props;
mod session;

use std::ops::Range;

use thiserror::Error;

use crate::event::EventBody;
use crate::limits::V1;
use crate::text::MAX_DELTA_ESCAPED;

pub use index::IndexView;
pub use session::{Entry, ItemEnd, ItemView, RequestView, SessionView, TurnView, WorkspaceView};

/// A body that does not fit the view. The client resubscribes, as on a gap.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ViewError {
    #[error("{0} is not in the view")]
    Missing(String),
    #[error("{0} is already in the view")]
    Duplicate(String),
    #[error("`{0}` belongs to the other stream")]
    OtherStream(String),
}

/// The state of one stream.
pub trait View: Default {
    /// Applies a live event, a snapshot op or a history op. Unknown event
    /// types change nothing.
    fn apply(&mut self, body: &EventBody) -> Result<(), ViewError>;

    /// Bodies that rebuild this view from empty, each within [`OpLimits::V1`].
    fn snapshot_ops(&self) -> Vec<EventBody>;
}

/// What a session snapshot covers besides the running turn, the entries
/// after the last turn and the pending requests (design 5.2.8, 5.3.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    /// Finished turns.
    pub turns: usize,
    /// Items of those turns together. The newest finished turn stays even
    /// when it alone has more, so that a history page is never mistaken for
    /// a new turn (see [`SessionView::trim`]).
    pub items: usize,
}

impl Window {
    pub const V1: Self = Self {
        turns: V1.snapshot_turns as usize,
        items: V1.snapshot_items as usize,
    };
}

/// How snapshot ops are cut: every op fits one history page, which also
/// leaves room for the `stream/snapshot` envelope in a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpLimits {
    /// Escaped text of one `item.delta`; at least 6, the longest escaped char.
    pub delta: usize,
    /// One serialized op. An `item.started` above it moves its optional
    /// fields into `item.updated` ops; a single field that is larger still
    /// arrived in one live event, so it fits a frame.
    pub op: usize,
}

impl OpLimits {
    pub const V1: Self = Self {
        delta: MAX_DELTA_ESCAPED,
        op: V1.max_history_page_bytes as usize,
    };
}

/// Bytes of `op` as JSON.
pub fn op_bytes(op: &EventBody) -> usize {
    serde_json::to_vec(op)
        .expect("event bodies serialize")
        .len()
}

/// Cuts `ops` into pages whose JSON array stays within `max_bytes`; an op
/// larger than that gets a page of its own. Snapshot pages and `history/page`
/// (whose cursor counts ops) both page this way.
pub fn paginate(ops: &[EventBody], max_bytes: usize) -> Vec<Range<usize>> {
    let mut pages = Vec::new();
    let (mut start, mut size) = (0, 2);
    for (i, op) in ops.iter().enumerate() {
        let bytes = op_bytes(op);
        if i > start && size + 1 + bytes > max_bytes {
            pages.push(start..i);
            (start, size) = (i, 2);
        }
        size += bytes + usize::from(i > start);
    }
    if start < ops.len() {
        pages.push(start..ops.len());
    }
    pages
}
