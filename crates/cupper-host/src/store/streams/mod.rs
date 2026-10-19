//! Streams (design 5.2.3, 5.2.8): every stream's seqs, view and subscribers
//! over the event log, reworked from SP-4a's `hub`, whose invariant holds: a
//! subscriber sees every seq after its start once, in order.

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the stream registry, next, puts it to use")
)]
mod state;
mod sync;
mod view;

pub use state::{Published, Push, Sink, Start};
pub use view::StreamView;
