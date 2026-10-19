//! The event store's storage layer (design 5.2.8): the connections to
//! `host.db` and `raw.db` and their durability.
//!
//! SQLite runs only on the store's own threads or under `spawn_blocking`, never
//! on an async worker (SP-4a). The tables and migrations are T025's.

mod connection;
#[cfg(test)]
mod testutil;

pub use connection::{BUSY_TIMEOUT, Database, Durability, OpenError, open_reader, open_writer};
