//! Principals and the policy check every request passes through.
//!
//! This spike implements two principals: the read-only `cli` (same-uid peer)
//! and `test_harness` (only compiled with the `test-harness` feature; the key
//! arrives over an inherited fd). Code-signature checks are SP-4b.

use cupper_protocol::rpc::{ErrorCode, RpcError};
use cupper_protocol::{AuthParams, PrincipalInfo, PrincipalKind, scopes};

#[derive(Debug, Clone, PartialEq)]
pub struct Principal {
    pub id: String,
    pub kind: PrincipalKind,
    pub scopes: Vec<&'static str>,
}

impl Principal {
    pub fn allows(&self, scope: &str) -> bool {
        self.scopes.iter().any(|s| *s == scopes::ALL || *s == scope)
    }

    /// Only the desktop UI may pick `allow_always` or switch to full access (ADR-05).
    pub fn may_allow_always(&self) -> bool {
        self.kind == PrincipalKind::DesktopUi
    }

    pub fn info(&self) -> PrincipalInfo {
        PrincipalInfo { id: self.id.clone(), kind: self.kind.clone(), scopes: self.scopes.iter().map(|s| (*s).to_owned()).collect() }
    }
}

/// Authenticate a connection that already passed the same-uid peer check.
pub fn authenticate(auth: &AuthParams, harness_key: Option<&[u8]>) -> Result<Principal, RpcError> {
    match auth {
        AuthParams::Peer => Ok(Principal { id: "p_cli".into(), kind: PrincipalKind::Cli, scopes: vec![scopes::READ] }),
        AuthParams::Harness { token } => harness(token, harness_key),
        AuthParams::Unknown(u) => Err(RpcError::app(ErrorCode::Unauthenticated, format!("unsupported auth type {}", u.kind))),
    }
}

#[cfg(feature = "test-harness")]
fn harness(token: &str, key: Option<&[u8]>) -> Result<Principal, RpcError> {
    let ok = key.is_some_and(|k| {
        let given = token.as_bytes();
        // Constant-time comparison.
        given.len() == k.len() && given.iter().zip(k).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
    });
    if ok {
        Ok(Principal { id: "p_test_harness".into(), kind: PrincipalKind::TestHarness, scopes: vec![scopes::ALL] })
    } else {
        Err(RpcError::app(ErrorCode::Unauthenticated, "bad harness token"))
    }
}

#[cfg(not(feature = "test-harness"))]
fn harness(_token: &str, _key: Option<&[u8]>) -> Result<Principal, RpcError> {
    Err(RpcError::app(ErrorCode::Unauthenticated, "harness auth is not available in this build"))
}

/// Scope a method needs.
pub fn required_scope(method: &str) -> &'static str {
    match method {
        "stream/subscribe" | "blob/get" => scopes::READ,
        "interaction/respond" => scopes::APPROVE,
        _ => scopes::WRITE,
    }
}

pub fn authorize(p: &Principal, method: &str) -> Result<(), RpcError> {
    let scope = required_scope(method);
    if p.allows(scope) {
        Ok(())
    } else {
        Err(RpcError::app(ErrorCode::Forbidden, format!("{method} needs scope {scope}")).with_extra("scope", scope.into()))
    }
}
