//! cupper host protocol: the contract shared by the host, the CLI, the desktop
//! app and, later, iOS. No I/O, so the library also builds for iOS (ADR-02, ADR-04).
//!
//! The Rust types are the source of truth; the JSON Schema exported from them is
//! what other languages generate from.

pub mod annotations;
pub mod canonical;
pub mod capabilities;
pub mod coalesce;
pub mod domain;
pub mod envelope;
pub mod error;
pub mod event;
pub mod frame;
pub mod ids;
pub mod index;
pub mod interaction;
pub mod item;
pub mod limits;
pub mod open_enum;
pub mod open_union;
pub mod rpc;
pub mod schema;
pub mod session;
pub mod sync;
#[cfg(test)]
mod testutil;
pub mod text;
pub mod transport;
pub mod view;
