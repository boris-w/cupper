//! The library of `cupper`, the command-line client (design 5.2.14): finding
//! and verifying the host, the connection with its handshake and request
//! correlation, and [`Mirror`](mirror::Mirror), the client side of stream
//! sync. The host's integration tests use it as their client.

pub mod build_kind;
