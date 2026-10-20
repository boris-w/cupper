//! Session actors (design 5.2.6): one [`SessionActor`](actor) per session
//! runs its pure cores ([`core`]) on the session's mailbox, its agent's
//! events, the clock and the store; the [`SessionManager`] starts them and
//! serves the gateway's session port.

mod actor;
pub mod core;
#[cfg(test)]
mod interaction_tests;
mod manager;
mod rows;
#[cfg(test)]
pub(super) mod tests;

use std::fmt;

use cupper_protocol::capabilities::{ConfigOption, ConfigOptionId, SessionCapabilities};
use cupper_protocol::domain::Tier;
use cupper_protocol::error::{ErrorCode, INVALID_PARAMS, RpcError};
use cupper_protocol::ids::SessionId;
use cupper_protocol::index::PendingKind;
use serde::de::DeserializeOwned;
use serde_json::Value;

pub use manager::{SESSION, SessionDeps, SessionManager};

/// What the index (T034's `IndexActor`) hears from the sessions.
pub trait IndexNotes: Send + Sync + fmt::Debug {
    /// The session's pending requests changed (design 5.2.7, step 4): how
    /// many there are, and the class of the oldest.
    fn pending(&self, session: &SessionId, count: u32, oldest: Option<PendingKind>);
}

/// The index until T034: it hears nothing.
#[derive(Debug)]
pub struct NoIndex;

impl IndexNotes for NoIndex {
    fn pending(&self, _: &SessionId, _: u32, _: Option<PendingKind>) {}
}

/// The tier of permission mode `mode` by the agent's mapping.
fn tier_of(capabilities: Option<&SessionCapabilities>, mode: &str) -> Option<Tier> {
    risk(&capabilities?.config, mode)
}

fn risk(options: &[ConfigOption], mode: &str) -> Option<Tier> {
    let option = options
        .iter()
        .find(|o| o.id == ConfigOptionId::PermissionMode)?;
    let value = option.values.iter().find(|v| v.value == mode)?;
    Some(value.risk.clone())
}

/// The permission mode `options` have now and its tier: one the mapping
/// does not know is full access (design 5.6.5).
fn mode_tier(options: &[ConfigOption]) -> Option<(String, Tier)> {
    let option = options
        .iter()
        .find(|o| o.id == ConfigOptionId::PermissionMode)?;
    let tier = risk(options, &option.current).unwrap_or(Tier::FullAccess);
    Some((option.current.clone(), tier))
}

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
