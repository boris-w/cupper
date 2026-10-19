//! Session orchestration (design 5.2.6): the pure cores (the turn state
//! machine, the queue of messages it drains, the interaction broker, the
//! autonomous judge, and the coalescer that writes the agent's updates as
//! items, with their activity groups and the final answer), and the
//! [`session`] actors that run them.

pub mod autonomous;
pub mod coalescer;
pub mod grouping;
pub mod interaction;
#[cfg(test)]
mod props;
pub mod queue;
pub mod session;
#[cfg(test)]
mod session_props;
#[cfg(test)]
mod testkit;
pub mod turn;
