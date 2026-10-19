//! Session actors (design 5.2.6): one [`SessionActor`](actor) per session
//! runs its pure cores ([`core`]) on the session's mailbox, its agent's
//! events, the clock and the store; the [`SessionManager`] starts them and
//! serves the gateway's session port.

mod actor;
pub mod core;
mod manager;
mod rows;

use cupper_protocol::error::{ErrorCode, INVALID_PARAMS, RpcError};
use serde::de::DeserializeOwned;
use serde_json::Value;

pub use manager::{SESSION, SessionDeps, SessionManager};

/// `params` as `T`; the gateway checked them against the method already.
fn parse<T: DeserializeOwned>(params: &Value) -> Result<T, RpcError> {
    T::deserialize(params).map_err(|err| RpcError::new(INVALID_PARAMS, err.to_string()))
}

fn not_found(what: &str) -> RpcError {
    RpcError::app(ErrorCode::NotFound, format!("no such {what}"))
}

fn internal(err: impl std::fmt::Display) -> RpcError {
    RpcError::app(ErrorCode::Internal, err.to_string())
}
