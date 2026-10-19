//! Views (design 5.3.3, 5.2.8, 5.7.6): the state of a stream, folded from its
//! event bodies by one pure `apply`. The host keeps them to serve snapshots;
//! clients fold the same bodies into the same views. A snapshot is a list of
//! bodies ([`View::snapshot_ops`]), so live events, snapshots and history
//! pages all go through the same `apply`.
//!
//! The serialized form of [`IndexView`] and `SessionView` is part of the
//! schema: the TS views must produce the same JSON (test #16). Absent optional
//! values are left out; maps, lists and strings are always present.

mod index;

use std::ops::Range;

use thiserror::Error;

use crate::event::EventBody;

pub use index::IndexView;

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

    /// Bodies that rebuild this view from empty.
    fn snapshot_ops(&self) -> Vec<EventBody>;
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
