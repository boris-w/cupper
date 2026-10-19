//! The library of `cupper`, the command-line client (design 5.2.14): finding
//! and verifying the host, and the connection with its handshake and request
//! correlation. The client side of stream sync, the mirror, comes with T049.
//! The host's integration tests use this library as their client.

pub mod build_kind;
pub mod client;
#[cfg(feature = "test-harness")]
pub mod harness;
pub mod verify;
