//! Client side of the host protocol: a socket client and a stream mirror that
//! enforces the contiguity invariant. Used by the CLI and by the host's tests.

pub mod client;
pub mod mirror;

pub use client::{Client, ClientError, Incoming};
pub use mirror::{Mirror, SyncError};

/// A fresh UUIDv7 command id.
pub fn command_id() -> String {
    uuid::Uuid::now_v7().to_string()
}
