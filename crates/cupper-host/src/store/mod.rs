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
//!
//! Raw frames reach `raw.db` through the bounded queue of a [`RawWriter`],
//! a thread of its own. [`BlobStore`] keeps the content-addressed blobs, and
//! [`retention`] cleans up after all of them.

pub mod blobs;
mod connection;
pub mod epoch;
pub mod history;
pub mod log;
pub mod meta;
mod migrate;
mod raw;
mod raw_writer;
mod readers;
pub mod retention;
pub mod streams;
#[cfg(test)]
mod testutil;
pub mod upcast;
pub mod writer;

pub use blobs::{BlobError, BlobStore};
pub use connection::{BUSY_TIMEOUT, Database, Durability, OpenError, open_reader, open_writer};
pub use migrate::{LAYOUT_VERSION, MIGRATIONS, MigrateError, Migrated, Migration, migrate};
pub use raw::{RAW_SCHEMA, RawDb, RawSchema, prepare_raw};
#[cfg(test)]
pub(crate) use raw_writer::Unattended;
pub use raw_writer::{
    Dir, RAW_QUEUE_CAPACITY, RAW_WRITER, RawFrame, RawPruned, RawRetention, RawSink, RawWriter,
    RawWriterThread,
};
pub use readers::{ReadError, Readers};
pub use writer::{MAX_BATCH, Receipt, WRITER, WriteError, Writer, WriterConfig, WriterThread};
