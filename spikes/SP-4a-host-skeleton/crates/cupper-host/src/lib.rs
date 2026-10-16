//! cupper session host (spike).
//!
//! Data flow: adapter stdout -> [`driver`] (ACP SDK, normalized updates) ->
//! [`session`] actor ([`normalize`], interactions) -> [`hub`] (seq, SQLite,
//! views, fan-out) -> per-connection [`outbox`] -> [`conn`] writer -> socket.
//! The ACP read path ends at an unbounded channel send; slow clients are reset,
//! never waited for.

pub mod auth;
pub mod commands;
pub mod conn;
pub mod driver;
pub mod host;
pub mod hub;
pub mod normalize;
pub mod outbox;
pub mod session;
pub mod store;
pub mod sys;
pub mod util;
pub mod wire;
