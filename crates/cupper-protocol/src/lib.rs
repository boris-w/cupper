//! cupper host protocol: the contract shared by the host, the CLI, the desktop
//! app and, later, iOS. No I/O, so the library also builds for iOS (ADR-02, ADR-04).
//!
//! The Rust types are the source of truth; the JSON Schema exported from them is
//! what other languages generate from.

pub mod annotations;
pub mod coalesce;
pub mod envelope;
pub mod error;
pub mod frame;
pub mod ids;
pub mod limits;
pub mod open_enum;
pub mod rpc;
pub mod sync;
pub mod text;
