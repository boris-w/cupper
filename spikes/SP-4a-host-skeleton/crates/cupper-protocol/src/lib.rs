//! cupper host protocol (spike): types, framing and sync rules. No I/O.
//!
//! The Rust types are the source of truth; [`schema::protocol_schema`] exports
//! the JSON Schema contract that other languages generate from.

pub mod coalesce;
pub mod events;
pub mod frame;
pub mod messages;
mod open_enum;
pub mod rpc;
pub mod schema;
pub mod sync;
pub mod text;
pub mod transport;
pub mod view;

pub use events::*;
pub use messages::*;
