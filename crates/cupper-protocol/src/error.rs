//! Errors (design 5.3.8). Application errors use JSON-RPC code [`APP_ERROR`]
//! and branch on `error.data.code`; `data.reason` narrows it down. `message`
//! is English for developers and never shown as UI text.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::annotations::DurationMs;
use crate::open_enum;
use crate::rpc::Scope;

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const APP_ERROR: i64 = -32000;

open_enum! {
    /// `error.data.code`.
    pub enum ErrorCode {
        VersionUnsupported => "version_unsupported",
        NotInitialized => "not_initialized",
        Unauthenticated => "unauthenticated",
        Forbidden => "forbidden",
        NotFound => "not_found",
        Conflict => "conflict",
        Expired => "expired",
        Unsupported => "unsupported",
        Unavailable => "unavailable",
        AgentError => "agent_error",
        GitError => "git_error",
        IdempotencyMismatch => "idempotency_mismatch",
        TooLarge => "too_large",
        RateLimited => "rate_limited",
        Timeout => "timeout",
        ShuttingDown => "shutting_down",
        Internal => "internal",
    }
}

open_enum! {
    /// `error.data.reason`. Each known reason belongs to one code, see [`ErrorReason::code`].
    pub enum ErrorReason {
        BadNonce => "bad_nonce",
        SignatureInvalid => "signature_invalid",
        BadKey => "bad_key",
        BadProof => "bad_proof",
        Scope => "scope",
        Lineage => "lineage",
        Role => "role",
        AlreadyInitialized => "already_initialized",
        UserActivationRequired => "user_activation_required",
        AlreadyResolved => "already_resolved",
        TurnMismatch => "turn_mismatch",
        DigestMismatch => "digest_mismatch",
        TooEarly => "too_early",
        VersionMismatch => "version_mismatch",
        StaleDiff => "stale_diff",
        OutcomeUnknown => "outcome_unknown",
        TurnRunning => "turn_running",
        ConfigUnconfirmed => "config_unconfirmed",
        AdapterExited => "adapter_exited",
        HostRestarted => "host_restarted",
        NotARepo => "not_a_repo",
        CliMissing => "cli_missing",
        CliNotLoggedIn => "cli_not_logged_in",
        CliTooOld => "cli_too_old",
        RuntimeInstalling => "runtime_installing",
        RuntimeFailed => "runtime_failed",
        AdapterStarting => "adapter_starting",
        AdapterCrashed => "adapter_crashed",
        PendingUpgrade => "pending_upgrade",
        QuotaExhausted => "quota_exhausted",
        AppNotFound => "app_not_found",
        SnapshotMissing => "snapshot_missing",
        IndexUnmerged => "index_unmerged",
        ProjectUntrusted => "project_untrusted",
        ScanFailed => "scan_failed",
        HookFailed => "hook_failed",
        NothingToCommit => "nothing_to_commit",
        AuthFailed => "auth_failed",
        Rejected => "rejected",
        LockHeld => "lock_held",
        UnsafeRepoConfig => "unsafe_repo_config",
        RepoConfigChanged => "repo_config_changed",
        FilterUnsupported => "filter_unsupported",
        Steer => "steer",
        Cancel => "cancel",
        Upgrade => "upgrade",
        UserQuit => "user_quit",
        Idle => "idle",
    }
}

impl ErrorReason {
    /// The code this reason refines; `None` for a reason this version does not know.
    pub fn code(&self) -> Option<ErrorCode> {
        use ErrorReason as R;
        Some(match self {
            R::BadNonce | R::SignatureInvalid | R::BadKey | R::BadProof => {
                ErrorCode::Unauthenticated
            }
            R::Scope | R::Lineage | R::Role | R::AlreadyInitialized | R::UserActivationRequired => {
                ErrorCode::Forbidden
            }
            R::AlreadyResolved
            | R::TurnMismatch
            | R::DigestMismatch
            | R::TooEarly
            | R::VersionMismatch
            | R::StaleDiff
            | R::OutcomeUnknown
            | R::TurnRunning
            | R::ConfigUnconfirmed => ErrorCode::Conflict,
            R::AdapterExited | R::HostRestarted => ErrorCode::Expired,
            R::NotARepo => ErrorCode::Unsupported,
            R::CliMissing
            | R::CliNotLoggedIn
            | R::CliTooOld
            | R::RuntimeInstalling
            | R::RuntimeFailed
            | R::AdapterStarting
            | R::AdapterCrashed
            | R::PendingUpgrade
            | R::QuotaExhausted
            | R::AppNotFound
            | R::SnapshotMissing
            | R::IndexUnmerged
            | R::ProjectUntrusted
            | R::ScanFailed => ErrorCode::Unavailable,
            R::HookFailed
            | R::NothingToCommit
            | R::AuthFailed
            | R::Rejected
            | R::LockHeld
            | R::UnsafeRepoConfig
            | R::RepoConfigChanged
            | R::FilterUnsupported => ErrorCode::GitError,
            R::Steer | R::Cancel => ErrorCode::Timeout,
            R::Upgrade | R::UserQuit | R::Idle => ErrorCode::ShuttingDown,
            R::Unknown(_) => return None,
        })
    }
}

/// `error.data` of an application error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ErrorData {
    pub code: ErrorCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<ErrorReason>,
    /// `forbidden/scope`: the scope the caller lacks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<Scope>,
    /// `unsupported`: the missing capability.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
    /// `agent_error`: the agent's own code and message (truncated and redacted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_message: Option<String>,
    /// `rate_limited`: when to try again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<DurationMs>,
    /// Other code-specific details, e.g. `resolved_by` or `supported`.
    #[serde(flatten)]
    pub details: Map<String, Value>,
}

impl ErrorData {
    pub fn new(code: ErrorCode) -> Self {
        Self {
            code,
            reason: None,
            scope: None,
            capability: None,
            agent_code: None,
            agent_message: None,
            retry_after_ms: None,
            details: Map::new(),
        }
    }
}

/// A JSON-RPC error object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, thiserror::Error)]
#[error("rpc error {code}: {message}")]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    /// Boxed to keep `Result<_, RpcError>` small.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Box<ErrorData>>,
}

impl RpcError {
    /// A protocol-level error such as [`INVALID_PARAMS`], without `data`.
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// An application error with the given `data.code`.
    pub fn app(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code: APP_ERROR,
            message: message.into(),
            data: Some(Box::new(ErrorData::new(code))),
        }
    }

    /// Sets `data.reason`; the reason must belong to the error's code.
    pub fn with_reason(self, reason: ErrorReason) -> Self {
        debug_assert!(
            reason
                .code()
                .is_none_or(|code| self.app_code() == Some(&code)),
            "reason {reason} does not belong to {:?}",
            self.app_code()
        );
        self.with_data(|data| data.reason = Some(reason))
    }

    /// Edits `data`; no effect on an error without it.
    pub fn with_data(mut self, edit: impl FnOnce(&mut ErrorData)) -> Self {
        if let Some(data) = self.data.as_deref_mut() {
            edit(data);
        }
        self
    }

    pub fn app_code(&self) -> Option<&ErrorCode> {
        self.data.as_ref().map(|data| &data.code)
    }

    pub fn reason(&self) -> Option<&ErrorReason> {
        self.data.as_ref().and_then(|data| data.reason.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_proto_base_error_codes_roundtrip() {
        for wire in ErrorCode::KNOWN {
            let code: ErrorCode = serde_json::from_value(Value::from(*wire)).unwrap();
            assert!(!matches!(code, ErrorCode::Unknown(_)), "{wire}");
            assert_eq!(serde_json::to_value(&code).unwrap(), *wire);
        }
        for wire in ErrorReason::KNOWN {
            let reason = ErrorReason::from(*wire);
            assert!(reason.code().is_some(), "{wire} has no code");
            assert_eq!(serde_json::to_value(&reason).unwrap(), *wire);
        }
        assert_eq!(ErrorReason::Scope.code(), Some(ErrorCode::Forbidden));
        assert_eq!(
            ErrorReason::UnsafeRepoConfig.code(),
            Some(ErrorCode::GitError)
        );
        assert_eq!(ErrorReason::from("later").code(), None);
    }

    #[test]
    fn unit_proto_base_error_object_roundtrip() {
        let err = RpcError::app(ErrorCode::Forbidden, "missing scope")
            .with_reason(ErrorReason::Scope)
            .with_data(|data| data.scope = Some(Scope::Write));
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"code": -32000, "message": "missing scope",
                "data": {"code": "forbidden", "reason": "scope", "scope": "write"}})
        );
        assert_eq!(serde_json::from_value::<RpcError>(json).unwrap(), err);

        // Unknown codes, reasons and details survive a roundtrip.
        let raw = serde_json::json!({"code": -32000, "message": "x", "data": {
            "code": "future_code", "reason": "later", "resolved_by": {"principal_id": "pr_cli"}}});
        let err: RpcError = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(
            err.app_code(),
            Some(&ErrorCode::Unknown("future_code".into()))
        );
        assert_eq!(err.reason(), Some(&ErrorReason::Unknown("later".into())));
        assert_eq!(serde_json::to_value(&err).unwrap(), raw);
    }
}
