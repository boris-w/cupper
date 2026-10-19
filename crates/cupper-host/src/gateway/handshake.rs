//! `host/hello` and `initialize` (design 5.3.2, 5.8.1, 5.8.2), and which
//! authentications each build offers (design 5.3.11, rulings before T030):
//!
//! | auth | release | `dev-auth` | `test-harness` | both (e2e) |
//! |---|---|---|---|---|
//! | `codesign` | yes | no | yes | no |
//! | `peer` | yes | yes | yes | yes |
//! | `dev_key` | no | yes (T039) | no | yes (T039) |
//! | `harness` | no | no | yes (T039) | yes (T039) |
//!
//! Until T039 brings them, `dev_key` and `harness` are refused like any
//! authentication a build does not offer.

use cupper_protocol::domain::{ClientRole, PrincipalKind};
use cupper_protocol::error::{ErrorCode, ErrorReason, INVALID_PARAMS, RpcError};
use cupper_protocol::limits::V1;
use cupper_protocol::methods::handshake::{
    Auth, ClientIdentity, Feature, HostHelloParams, HostIdentity, InitializeParams,
    InitializeResult, PROTOCOL, Principal,
};
use cupper_protocol::rpc::Scope;
use serde_json::Value;

use super::{GatewayConfig, Verdict};

/// `h_nonce`: 32 random bytes, sent as 64 hex digits.
pub type Nonce = [u8; 32];

/// `dev-auth` builds authenticate the UI by key, not by signature.
const CODESIGN_OFFERED: bool = !cfg!(feature = "dev-auth");
/// Degraded local authentication (ADR-05).
const WEAK_AUTH: bool = cfg!(feature = "dev-auth");
/// The optional features this host supports; each comes with the task that
/// implements it (T033 and later).
const HOST_FEATURES: &[Feature] = &[];

/// What `initialize` settled for the connection's lifetime.
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub principal: Principal,
    pub features: Vec<Feature>,
    /// The send-time coalescing window, clamped to `limits.max_coalesce_ms`.
    pub coalesce_ms: u32,
}

pub(super) fn new_nonce() -> Nonce {
    let mut nonce = [0; 32];
    getrandom::fill(&mut nonce).expect("the operating system's random source is available");
    nonce
}

pub(super) fn hello(nonce: &Nonce) -> HostHelloParams {
    HostHelloParams {
        nonce: nonce.iter().map(|byte| format!("{byte:02x}")).collect(),
        protocol: PROTOCOL,
        host_version: env!("CARGO_PKG_VERSION").to_owned(),
    }
}

/// Handles the connection's `initialize`: `nonce` is the one its
/// `host/hello` carried, `verdict` the one on its signature.
pub(super) fn initialize(
    config: &GatewayConfig,
    nonce: &Nonce,
    verdict: Verdict,
    params: Value,
) -> Result<(InitializeResult, Session, ClientIdentity), RpcError> {
    let params: InitializeParams = serde_json::from_value(params)
        .map_err(|err| RpcError::new(INVALID_PARAMS, err.to_string()))?;
    if parse_nonce(&params.nonce) != Some(*nonce) {
        return Err(unauthenticated(
            ErrorReason::BadNonce,
            "the nonce is not the one of host/hello",
        ));
    }
    let protocol = PROTOCOL.negotiate(&params.protocol).ok_or_else(|| {
        RpcError::app(ErrorCode::VersionUnsupported, "no common protocol version")
            .with_data(|data| data.supported = Some(PROTOCOL))
    })?;
    let kind = authenticate(&params.auth, verdict)?;
    let session = Session {
        principal: principal(kind, params.role)?,
        features: HOST_FEATURES
            .iter()
            .filter(|feature| params.features.contains(feature))
            .cloned()
            .collect(),
        coalesce_ms: params.coalesce_ms.unwrap_or(0).min(V1.max_coalesce_ms),
    };
    let result = InitializeResult {
        protocol,
        host: HostIdentity {
            host_id: config.host_id.clone(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            weak_auth: WEAK_AUTH,
            data_epoch: config.data_epoch,
        },
        host_proof: None,
        principal: session.principal.clone(),
        features: session.features.clone(),
        limits: V1,
    };
    Ok((result, session, params.client))
}

/// What `auth` proves the peer to be. Every authentication is decided here;
/// T039 adds the `dev_key` and `harness` arms, with their keys in
/// [`GatewayConfig`], the pinned token for the proof's pid, and `host_proof`
/// in the result.
fn authenticate(auth: &Auth, verdict: Verdict) -> Result<PrincipalKind, RpcError> {
    match auth {
        Auth::Peer => Ok(PrincipalKind::Cli),
        Auth::Codesign if CODESIGN_OFFERED => match verdict {
            Verdict::DesktopUi => Ok(PrincipalKind::DesktopUi),
            Verdict::Other(_) => Err(unauthenticated(
                ErrorReason::SignatureInvalid,
                "the peer does not satisfy the desktop UI's code requirement",
            )),
        },
        _ => Err(unauthenticated(
            ErrorReason::BadKey,
            "this build does not offer this authentication",
        )),
    }
}

/// The principal of `kind` in `role`. Only the desktop UI has roles, and it
/// must name one (design 5.8.1, 5.8.2).
fn principal(kind: PrincipalKind, role: Option<ClientRole>) -> Result<Principal, RpcError> {
    use Scope::{Approve, Privileged, Read, UiLocal, Write};
    let (id, scopes) = match (&kind, &role) {
        (PrincipalKind::DesktopUi, Some(ClientRole::Control)) => (
            "pr_desktop",
            vec![Read, Write, Approve, Privileged, UiLocal],
        ),
        (PrincipalKind::DesktopUi, Some(ClientRole::Window)) => {
            ("pr_desktop", vec![Read, Write, Approve, UiLocal])
        }
        (PrincipalKind::Cli, None) => ("pr_cli", vec![Read]),
        _ => {
            return Err(
                RpcError::app(ErrorCode::Forbidden, "this role is not for this client")
                    .with_reason(ErrorReason::Role),
            );
        }
    };
    Ok(Principal {
        id: id.parse().expect("a well-formed principal id"),
        kind,
        role,
        scopes,
    })
}

fn unauthenticated(reason: ErrorReason, message: &str) -> RpcError {
    RpcError::app(ErrorCode::Unauthenticated, message).with_reason(reason)
}

/// 64 hex digits, either case.
fn parse_nonce(text: &str) -> Option<Nonce> {
    let digits = text.as_bytes();
    if digits.len() != 64 || !digits.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    let mut nonce = [0; 32];
    for (byte, pair) in nonce.iter_mut().zip(digits.chunks_exact(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(nonce)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reason(err: RpcError) -> Option<ErrorReason> {
        err.data.and_then(|data| data.reason)
    }

    #[test]
    fn it_handshake_principal_and_scopes_follow_kind_and_role() {
        let desktop = PrincipalKind::DesktopUi;
        let control = principal(desktop.clone(), Some(ClientRole::Control)).unwrap();
        let window = principal(desktop.clone(), Some(ClientRole::Window)).unwrap();
        let cli = principal(PrincipalKind::Cli, None).unwrap();
        assert_eq!(
            (control.id.as_str(), window.id.as_str(), cli.id.as_str()),
            ("pr_desktop", "pr_desktop", "pr_cli")
        );
        assert!(control.scopes.contains(&Scope::Privileged));
        assert!(!window.scopes.contains(&Scope::Privileged));
        assert!(window.scopes.contains(&Scope::UiLocal));
        assert_eq!(cli.scopes, [Scope::Read]);

        let refused = [
            (desktop, None),
            (
                PrincipalKind::DesktopUi,
                Some(ClientRole::Unknown("tab".into())),
            ),
            (PrincipalKind::Cli, Some(ClientRole::Window)),
        ];
        for (kind, role) in refused {
            let err = principal(kind, role).unwrap_err();
            assert_eq!(reason(err), Some(ErrorReason::Role));
        }
    }

    #[test]
    fn it_handshake_codesign_trusts_only_the_ui_verdict_where_offered() {
        let ui = authenticate(&Auth::Codesign, Verdict::DesktopUi);
        let other = authenticate(&Auth::Codesign, Verdict::Other(None));
        if CODESIGN_OFFERED {
            assert_eq!(ui.unwrap(), PrincipalKind::DesktopUi);
            assert_eq!(
                reason(other.unwrap_err()),
                Some(ErrorReason::SignatureInvalid)
            );
        } else {
            assert_eq!(reason(ui.unwrap_err()), Some(ErrorReason::BadKey));
            assert_eq!(reason(other.unwrap_err()), Some(ErrorReason::BadKey));
        }
    }

    #[test]
    fn it_handshake_nonce_is_64_hex_digits() {
        let nonce: Nonce = std::array::from_fn(|i| i as u8 * 7);
        let hex = hello(&nonce).nonce;
        assert_eq!(parse_nonce(&hex), Some(nonce));
        assert_eq!(parse_nonce(&hex.to_uppercase()), Some(nonce));
        for bad in [&hex[2..], "+1".repeat(32).as_str(), &"é".repeat(32)] {
            assert_eq!(parse_nonce(bad), None, "{bad}");
        }
        assert_ne!(new_nonce(), new_nonce());
    }
}
