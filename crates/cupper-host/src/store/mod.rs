//! The event store's storage layer (design 5.2.8): the connections to
//! `host.db` and `raw.db`, their durability, the [`writer`] thread that does
//! all the writing, the [`Readers`] pool, and the [`epoch`] decision. Before
//! the writer starts, [`migrate`] brings `host.db` to this host's schema and
//! [`prepare_raw`] does the same for `raw.db`.
//!
//! SQLite runs only on the store's own threads or under `spawn_blocking`, never
//! on an async worker (SP-4a).
//!
//! On top of them, [`streams`] keeps every stream's seqs, view and
//! subscribers over the event [`log`], and [`history`] reads the turn
//! snapshots that `history/page` serves.

mod connection;
pub mod epoch;
pub mod history;
pub mod log;
pub mod meta;
mod migrate;
mod raw;
mod readers;
pub mod streams;
#[cfg(test)]
mod testutil;
pub mod upcast;
pub mod writer;

pub use connection::{BUSY_TIMEOUT, Database, Durability, OpenError, open_reader, open_writer};
pub use migrate::{LAYOUT_VERSION, MIGRATIONS, MigrateError, Migrated, Migration, migrate};
pub use raw::{RAW_SCHEMA, RawDb, RawSchema, prepare_raw};
pub use readers::{ReadError, Readers};
pub use writer::{MAX_BATCH, Receipt, WRITER, WriteError, Writer, WriterConfig, WriterThread};
