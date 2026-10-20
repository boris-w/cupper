//! Session actors (design 5.2.6): one `SessionActor` per session
//! runs its pure cores ([`core`]) on the session's mailbox, its agent's
//! events, the clock and the store; the [`SessionManager`] starts them and
//! serves the gateway's session port.

mod actor;
pub mod core;
#[cfg(test)]
mod gating_tests;
#[cfg(test)]
mod interaction_tests;
mod manager;
mod rows;
#[cfg(test)]
pub(super) mod tests;
#[cfg(test)]
mod trust_tests;

use std::fmt;

use cupper_protocol::capabilities::{ConfigOption, ConfigOptionId, SessionCapabilities};
use cupper_protocol::domain::FailureCode;
use cupper_protocol::domain::Tier;
use cupper_protocol::error::{ErrorCode, INVALID_PARAMS, RpcError};
use cupper_protocol::ids::{ProjectId, RequestId, SessionId};
use cupper_protocol::index::{PendingKind, SessionSummary};
use cupper_protocol::session::TurnStatus;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::trust::Scan;

pub use manager::{SESSION, SessionDeps, SessionManager};

/// What the index (`crate::index::IndexHandle`) hears from the sessions:
/// what a committed step changed, for the status (design 5.2.9), and the
/// trust rescans.
pub trait IndexNotes: Send + Sync + fmt::Debug {
    /// A new session, before its first turn.
    fn created(&self, summary: SessionSummary);
    /// `notes` were committed at `at` (wall ms), the session's latest
    /// activity.
    fn note(&self, session: &SessionId, at: u64, notes: Vec<Note>);
    /// A session rescanned its project's trust range (design 5.2.6); the
    /// index records a trusted project that became `stale`.
    fn rescanned(&self, project: &ProjectId, scan: Scan);
}

/// One change of a session the index computes its status from.
#[derive(Debug, Clone, PartialEq)]
pub enum Note {
    TurnStarted,
    /// A turn ended with the event at `seq`.
    TurnEnded {
        seq: u64,
        status: TurnStatus,
        error: Option<FailureCode>,
    },
    /// The pending requests (design 5.2.7, step 4): how many, and the
    /// oldest one's class and ID.
    Pending {
        count: u32,
        oldest: Option<(PendingKind, RequestId)>,
    },
    /// `workspace.ready`'s branch.
    Branch(Option<String>),
    /// `session.configured`'s `read_only`.
    ReadOnly(bool),
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
