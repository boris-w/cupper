//! Session orchestration (design 5.2.6). So far its pure cores: the turn
//! state machine and the queue of messages it drains.

pub mod coalescer;
pub mod grouping;
#[cfg(test)]
mod props;
pub mod queue;
#[cfg(test)]
mod testkit;
pub mod turn;
