//! `host/hello` and `initialize` (design 5.3.2, 5.8.1, 5.8.2), and which
//! authentications each build offers (design 5.3.11, rulings before T030):
//!
//! | auth | release | `dev-auth` | `test-harness` | both (e2e) |
//! |---|---|---|---|---|
//! | `codesign` | yes | no | yes | no |
//! | `peer` | yes | yes | yes | yes |
//! | `dev_key` | no | yes | no | yes |
//! | `harness` | no | no | yes | yes |
//!
//! `dev_key` and `harness` prove the startup payload's key, bound to the
//! channel ([`proof`]), and the host proves it back in `host_proof`. A wrong
//! or missing proof is refused, never taken for `peer`; `dev_key` is only
//! accepted from the process that started the host. The code of a key
//! authentication exists only in the builds that offer it.

use cupper_protocol::domain::{ClientRole, PrincipalKind};
use cupper_protocol::error::{ErrorCode, ErrorReason, INVALID_PARAMS, RpcError};
use cupper_protocol::limits::V1;
#[cfg(any(feature = "dev-auth", feature = "test-harness"))]
use cupper_protocol::methods::handshake::KeyProof;
use cupper_protocol::methods::handshake::{
    Auth, ClientIdentity, Feature, HostHelloParams, HostIdentity, InitializeParams,
    InitializeResult, PROTOCOL, Principal,
};
use cupper_protocol::proof;
use cupper_protocol::rpc::Scope;
use cupper_sys::AuditToken;
use serde::Deserialize;
use serde_json::Value;

use super::{GatewayConfig, Verdict};

/// `h_nonce`: 32 random bytes, sent as 64 hex digits.
pub type Nonce = proof::Bytes32;

/// `dev-auth` builds authenticate the UI by key, not by signature.
const CODESIGN_OFFERED: bool = !cfg!(feature = "dev-auth");
/// Degraded local authentication (ADR-05).
const WEAK_AUTH: bool = cfg!(feature = "dev-auth");
/// The key authentications this build offers.
const KEY_AUTHS: &[&str] = &[
    #[cfg(feature = "dev-auth")]
    "dev_key",
    #[cfg(feature = "test-harness")]
    "harness",
];
/// The optional features this host supports; each comes with the task that
/// implements it.
const HOST_FEATURES: &[Feature] = &[
    Feature::StreamCoalesce,
    Feature::HistoryPage,
    Feature::BlobGet,
    Feature::BlobPut,
];

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
        nonce: proof::to_hex(nonce),
        protocol: PROTOCOL,
        host_version: env!("CARGO_PKG_VERSION").to_owned(),
    }
}

/// Handles the connection's `initialize`: `nonce` is the one its
/// `host/hello` carried, `peer` its pinned token and `verdict` the one on
/// its signature.
#[cfg_attr(
    not(any(feature = "dev-auth", feature = "test-harness")),
    expect(
        unused_variables,
        reason = "only key authentications look at the token"
    )
)]
pub(super) fn initialize(
    config: &GatewayConfig,
    nonce: &Nonce,
    peer: &AuditToken,
    verdict: Verdict,
    params: Value,
) -> Result<(InitializeResult, Session, ClientIdentity), RpcError> {
    let params = parse_params(&params)?;
    if proof::parse_hex32(&params.nonce) != Some(*nonce) {
        return Err(unauthenticated(
            ErrorReason::BadNonce,
            "the nonce is not the one of host/hello",
        ));
    }
    let protocol = PROTOCOL.negotiate(&params.protocol).ok_or_else(|| {
        RpcError::app(ErrorCode::VersionUnsupported, "no common protocol version")
            .with_data(|data| data.supported = Some(PROTOCOL))
    })?;
    let (kind, host_proof) = match &params.auth {
        #[cfg(feature = "dev-auth")]
        Auth::DevKey(proof) => dev_key(config, proof, nonce, peer)?,
        #[cfg(feature = "test-harness")]
        Auth::Harness(proof) => harness(config, proof, nonce, peer)?,
        auth => (authenticate(auth, verdict)?, None),
    };
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
        host_proof,
        principal: session.principal.clone(),
        features: session.features.clone(),
        limits: V1,
    };
    Ok((result, session, params.client))
}

/// `initialize`'s params. An offered key authentication with its proof
/// missing or malformed is refused as such, not as invalid params.
fn parse_params(params: &Value) -> Result<InitializeParams, RpcError> {
    InitializeParams::deserialize(params).map_err(|err| {
        let auth = &params["auth"];
        let key_auth = auth["type"]
            .as_str()
            .is_some_and(|tag| KEY_AUTHS.contains(&tag));
        match key_auth && Auth::deserialize(auth).is_err() {
            true => unauthenticated(ErrorReason::BadProof, "the key proof is missing"),
            false => RpcError::new(INVALID_PARAMS, err.to_string()),
        }
    })
}

/// What an authentication other than a key proves the peer to be.
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

/// `dev_key`: the desktop UI, as long as it is the process that started the
/// host (design 5.8.1).
#[cfg(feature = "dev-auth")]
fn dev_key(
    config: &GatewayConfig,
    proof: &KeyProof,
    h_nonce: &Nonce,
    peer: &AuditToken,
) -> Result<(PrincipalKind, Option<String>), RpcError> {
    let dev = config.dev_key.as_ref().ok_or_else(no_key)?;
    if peer.unique_id() != Ok(dev.launcher) {
        return Err(unauthenticated(
            ErrorReason::BadKey,
            "dev_key is only accepted from the process that started the host",
        ));
    }
    let host_proof = check_key(dev.key.expose(), proof, h_nonce, peer)?;
    Ok((PrincipalKind::DesktopUi, Some(host_proof)))
}

/// `harness`: the test principal.
#[cfg(feature = "test-harness")]
fn harness(
    config: &GatewayConfig,
    proof: &KeyProof,
    h_nonce: &Nonce,
    peer: &AuditToken,
) -> Result<(PrincipalKind, Option<String>), RpcError> {
    let key = config.harness_key.as_ref().ok_or_else(no_key)?;
    let host_proof = check_key(key.expose(), proof, h_nonce, peer)?;
    Ok((PrincipalKind::TestHarness, Some(host_proof)))
}

#[cfg(any(feature = "dev-auth", feature = "test-harness"))]
fn no_key() -> RpcError {
    unauthenticated(
        ErrorReason::BadKey,
        "no key was loaded for this authentication",
    )
}

/// Checks the client's proof of `key`, made by the process of the pinned
/// token; the host's proof in return.
#[cfg(any(feature = "dev-auth", feature = "test-harness"))]
fn check_key(
    key: &proof::Bytes32,
    auth: &KeyProof,
    h_nonce: &Nonce,
    peer: &AuditToken,
) -> Result<String, RpcError> {
    use proof::{Channel, Direction};

    let bad = || unauthenticated(ErrorReason::BadProof, "the key proof does not hold");
    let c_nonce = proof::parse_hex32(&auth.c_nonce).ok_or_else(bad)?;
    let channel = Channel {
        h_nonce: *h_nonce,
        c_nonce,
    };
    channel
        .check(key, Direction::ClientToHost, peer.pid(), &auth.proof)
        .map_err(|_| bad())?;
    let pid = std::process::id() as i32;
    Ok(channel.prove(key, Direction::HostToClient, pid))
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
        #[cfg(feature = "test-harness")]
        (PrincipalKind::TestHarness, None) => (
            "pr_test",
            vec![Read, Write, Approve, Privileged, UiLocal, Scope::Test],
        ),
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
    fn it_handshake_nonce_is_fresh_hex() {
        let nonce = new_nonce();
        assert_eq!(proof::parse_hex32(&hello(&nonce).nonce), Some(nonce));
        assert_ne!(new_nonce(), nonce);
    }
}
