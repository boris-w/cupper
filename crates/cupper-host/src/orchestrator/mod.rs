//! Session orchestration (design 5.2.6). So far its pure cores: the turn
//! state machine, the queue of messages it drains, and the interaction broker.

pub mod interaction;
#[cfg(test)]
mod props;
pub mod queue;
#[cfg(test)]
mod testkit;
pub mod turn;
