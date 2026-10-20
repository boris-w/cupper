//! The library of `cupper`, the command-line client (design 5.2.14): finding
//! and verifying the host, the connection with its handshake and request
//! correlation, and [`mirror`], the client side of stream sync. The host's
//! integration tests use this library as their client.

pub mod args;
pub mod build_kind;
pub mod client;
#[cfg(feature = "test-harness")]
pub mod harness;
pub mod mirror;
pub mod verify;
