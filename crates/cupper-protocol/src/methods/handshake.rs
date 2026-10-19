//! Handshake, lifecycle and presence (design 5.3.2, 5.3.4).

use schemars::{JsonSchema, SchemaGenerator};
use serde::{Deserialize, Serialize};

use super::Empty;
use super::catalog::Catalog;
use crate::domain::{ClientKind, ClientRole, PrincipalKind};
use crate::ids::{CommandId, HostId, PrincipalId, RequestId, SessionId};
use crate::index::ClientInfo;
use crate::limits::Limits;
use crate::rpc::Scope;
use crate::{open_enum, open_union};

/// A range of protocol major versions, both ends included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VersionRange {
    pub min: u32,
    pub max: u32,
}

/// The major versions this crate speaks.
pub const PROTOCOL: VersionRange = VersionRange { min: 1, max: 1 };

impl VersionRange {
    /// The largest version in both ranges; `None` means `version_unsupported`.
    pub fn negotiate(&self, other: &VersionRange) -> Option<u32> {
        let max = self.max.min(other.max);
        (max >= self.min.max(other.min)).then_some(max)
    }
}

method! {
    /// `host/hello`: the host's first frame once it has checked the peer.
    host_notification HostHello = "host/hello" (HostHelloParams) { milestone: M1a }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HostHelloParams {
    /// 32 random bytes as 64 hex digits; `initialize` echoes it (`h_nonce`, design 5.8.1).
    #[schemars(extend("x-id" = true))]
    pub nonce: String,
    pub protocol: VersionRange,
    pub host_version: String,
}

method! {
    /// `initialize`: the client's first message, accepted once per connection.
    request Initialize = "initialize" (InitializeParams) -> InitializeResult {
        scope: None,
        idempotency: None,
        milestone: M1a,
        errors: [VersionUnsupported, Unauthenticated, Forbidden / AlreadyInitialized],
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InitializeParams {
    pub protocol: VersionRange,
    /// The `nonce` of `host/hello`, echoed.
    #[schemars(extend("x-id" = true))]
    pub nonce: String,
    pub client: ClientIdentity,
    /// `desktop_ui` only; fixed for the connection (design 5.7.1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<ClientRole>,
    pub auth: Auth,
    pub features: Vec<Feature>,
    /// The send-time coalescing window asked for; the host clamps it to
    /// `limits.max_coalesce_ms`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coalesce_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ClientIdentity {
    pub kind: ClientKind,
    pub name: String,
    pub version: String,
}

open_union! {
    /// How the client proves who it is (design 5.3.2, 5.8.1).
    pub enum Auth in tag "type" {
        /// The code signature the host checked on accept.
        Codesign => "codesign",
        /// Same user only: the read-only `cli` principal.
        Peer => "peer",
        /// `dev-auth` builds only.
        DevKey(KeyProof) => "dev_key",
        /// `test-harness` builds only.
        Harness(KeyProof) => "harness",
    }
}

/// A proof bound to the channel (design 5.8.1). The gateway checks the format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct KeyProof {
    /// The client's 32 random bytes as 64 hex digits.
    #[schemars(extend("x-id" = true))]
    pub c_nonce: String,
    /// HMAC-SHA256 as 64 hex digits.
    #[schemars(extend("x-id" = true))]
    pub proof: String,
}

open_enum! {
    /// An optional feature; the host answers with those both sides support
    /// (design 5.3.2). A new interaction request type needs one first.
    pub enum Feature {
        /// Range events with `from`.
        StreamCoalesce => "stream.coalesce",
        BlobGet => "blob.get",
        BlobPut => "blob.put",
        HistoryPage => "history.page",
        InteractionPermission => "interaction.permission",
        InteractionQuestion => "interaction.question",
        InteractionPlanConfirm => "interaction.plan_confirm",
        /// Not produced in M1.
        InteractionAuthUrl => "interaction.auth_url",
        /// Remote connections (M3).
        StreamAck => "stream.ack",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InitializeResult {
    /// The chosen major version (see [`VersionRange::negotiate`]).
    pub protocol: u32,
    pub host: HostIdentity,
    /// Degraded authentication only: the host's proof, checked by the client.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("x-id" = true))]
    pub host_proof: Option<String>,
    pub principal: Principal,
    pub features: Vec<Feature>,
    pub limits: Limits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HostIdentity {
    pub host_id: HostId,
    pub version: String,
    /// Degraded local authentication (ADR-05): release clients refuse the host.
    pub weak_auth: bool,
    /// Raised after a likely power loss; see `stream/subscribe` (design 5.2.8).
    pub data_epoch: u64,
}

/// Who the connection acts as, fixed at `initialize` (design 5.8.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Principal {
    pub id: PrincipalId,
    pub kind: PrincipalKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<ClientRole>,
    pub scopes: Vec<Scope>,
}

method! {
    /// `ping`.
    request Ping = "ping" (Empty) -> Empty {
        scope: None,
        idempotency: Natural,
        milestone: M1a,
        errors: [],
    }
}

method! {
    /// `client/presence`: whether the client is in front, and on which session.
    client_notification ClientPresence = "client/presence" (PresenceParams) {
        scope: Read,
        idempotency: Natural,
        milestone: M1a,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PresenceParams {
    pub foreground: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
}

method! {
    /// `host/status`: what quitting would interrupt.
    request HostStatus = "host/status" (Empty) -> HostStatusResult {
        scope: Read,
        idempotency: Natural,
        milestone: M1a,
        errors: [],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HostStatusResult {
    pub version: String,
    pub pending_upgrade: bool,
    pub running_turns: u32,
    pub pending_interactions: u32,
    /// As `clients.updated` lists them.
    pub clients: Vec<ClientInfo>,
    /// `test-harness` builds only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
}

method! {
    /// `host/prepare_upgrade`: stop taking turns and exit for an update.
    request PrepareUpgrade = "host/prepare_upgrade" (PrepareUpgradeParams) -> PrepareUpgradeResult {
        scope: Privileged,
        idempotency: Command,
        milestone: M1b,
        errors: [Forbidden],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PrepareUpgradeParams {
    pub command_id: CommandId,
    pub mode: UpgradeMode,
}

open_enum! {
    pub enum UpgradeMode {
        /// Exit once the running turns end.
        Wait => "wait",
        /// Exit now; running turns end as `interrupted`.
        Force => "force",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PrepareUpgradeResult {
    pub state: UpgradeState,
    pub running_turns: u32,
}

open_enum! {
    pub enum UpgradeState {
        Waiting => "waiting",
        Exiting => "exiting",
    }
}

method! {
    /// `host/exit`: an orderly exit; the result comes before it.
    request HostExit = "host/exit" (HostExitParams) -> Empty {
        scope: Privileged,
        idempotency: Command,
        milestone: M1a,
        errors: [Conflict / TurnRunning],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HostExitParams {
    pub command_id: CommandId,
    /// Exit even while turns run; they end as `interrupted`.
    pub force: bool,
}

method! {
    /// `host/shutdown`: sent to every connection before an orderly exit.
    host_notification HostShutdown = "host/shutdown" (ShutdownParams) { milestone: M1a }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ShutdownParams {
    pub reason: ShutdownReason,
    /// The host will be back soon, e.g. after an upgrade.
    pub restart_expected: bool,
}

open_enum! {
    pub enum ShutdownReason {
        UserQuit => "user_quit",
        Upgrade => "upgrade",
        Idle => "idle",
    }
}

method! {
    /// `ui/navigate`: the user clicked a notification of the host; sent on
    /// the `control` connection only (AP-3).
    host_notification UiNavigate = "ui/navigate" (NavigateParams) { milestone: M1b }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NavigateParams {
    pub session_id: SessionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<RequestId>,
}

pub fn register(c: &mut Catalog, g: &mut SchemaGenerator) {
    c.host_notification::<HostHello>(g);
    c.request::<Initialize>(g);
    c.request::<Ping>(g);
    c.client_notification::<ClientPresence>(g);
    c.request::<HostStatus>(g);
    c.request::<PrepareUpgrade>(g);
    c.request::<HostExit>(g);
    c.host_notification::<HostShutdown>(g);
    c.host_notification::<UiNavigate>(g);
}

#[cfg(test)]
pub(super) mod tests {
    use serde_json::json;

    use super::*;
    use crate::error::{ErrorCode, RpcError};
    use crate::limits::V1;
    use crate::methods::testkit::{notification, request};
    use crate::testutil::{open_values, roundtrip};

    /// Typical values of every method in this group (design 5.3.2, 5.3.4).
    pub fn samples() -> Vec<&'static str> {
        let hex = |c: &str| c.repeat(64);
        let limits = serde_json::to_value(V1).unwrap();
        vec![
            notification::<HostHello>(json!({"nonce": hex("a"),
                "protocol": {"min": 1, "max": 1}, "host_version": "0.1.0"})),
            request::<Initialize>(
                json!({"protocol": {"min": 1, "max": 2}, "nonce": hex("a"),
                    "client": {"kind": "desktop", "name": "Cupper", "version": "0.1.0"}, "role": "window",
                    "auth": {"type": "dev_key", "c_nonce": hex("b"), "proof": hex("c")},
                    "features": ["stream.coalesce", "blob.get", "interaction.permission", "canvas.v2"],
                    "coalesce_ms": 40}),
                json!({"protocol": 1, "host": {"host_id": "h_1", "version": "0.1.0", "weak_auth": true,
                    "data_epoch": 3}, "host_proof": hex("d"),
                    "principal": {"id": "pr_desktop", "kind": "desktop_ui", "role": "window",
                        "scopes": ["read", "write", "approve", "ui_local"]},
                    "features": ["stream.coalesce", "blob.get"], "limits": limits}),
            ),
            request::<Initialize>(
                json!({"protocol": {"min": 1, "max": 1}, "nonce": hex("a"),
                    "client": {"kind": "cli", "name": "cupper-cli", "version": "0.1.0"},
                    "auth": {"type": "peer"}, "features": []}),
                json!({"protocol": 1, "host": {"host_id": "h_1", "version": "0.1.0", "weak_auth": false,
                    "data_epoch": 1}, "principal": {"id": "pr_cli", "kind": "cli", "scopes": ["read"]},
                    "features": [], "limits": limits}),
            ),
            request::<Ping>(json!({}), json!({})),
            notification::<ClientPresence>(json!({"foreground": true, "session_id": "s_1"})),
            notification::<ClientPresence>(json!({"foreground": false})),
            request::<HostStatus>(
                json!({}),
                json!({"version": "0.1.0", "pending_upgrade": false, "running_turns": 2,
                    "pending_interactions": 1, "pid": 4242, "clients": [{"conn_id": "c_1",
                    "principal_kind": "desktop_ui", "role": "control", "client_name": "Cupper",
                    "client_version": "0.1.0", "since": 3, "auth": "verified"}]}),
            ),
            request::<PrepareUpgrade>(
                json!({"command_id": "c-1", "mode": "wait"}),
                json!({"state": "waiting", "running_turns": 1}),
            ),
            request::<HostExit>(json!({"command_id": "c-2", "force": true}), json!({})),
            notification::<HostShutdown>(json!({"reason": "upgrade", "restart_expected": true})),
            notification::<UiNavigate>(json!({"session_id": "s_1", "request_id": "r_1"})),
            notification::<UiNavigate>(json!({"session_id": "s_1"})),
        ]
    }

    #[test]
    fn unit_proto_methods_core_stream_auth_and_enums_are_open() {
        let auth: Vec<Auth> = roundtrip(&json!([{"type": "codesign"},
            {"type": "harness", "c_nonce": "b", "proof": "c"}, {"type": "passkey", "id": 1}]));
        let tags: Vec<&str> = auth.iter().map(Auth::tag).collect();
        assert_eq!(tags, ["codesign", "harness", "passkey"]);
        let err = serde_json::from_value::<Auth>(json!({"type": "dev_key", "proof": "c"}));
        assert!(
            err.unwrap_err()
                .to_string()
                .contains("`dev_key`: missing field `c_nonce`")
        );
        open_values::<Feature>(Feature::KNOWN);
        open_values::<UpgradeMode>(UpgradeMode::KNOWN);
        open_values::<UpgradeState>(UpgradeState::KNOWN);
        open_values::<ShutdownReason>(ShutdownReason::KNOWN);
    }

    #[test]
    fn unit_proto_methods_core_stream_version_negotiation() {
        let range = |min, max| VersionRange { min, max };
        assert_eq!(PROTOCOL.negotiate(&range(1, 1)), Some(1));
        assert_eq!(range(1, 2).negotiate(&range(1, 3)), Some(2));
        assert_eq!(range(2, 3).negotiate(&range(1, 2)), Some(2));
        assert_eq!(range(2, 3).negotiate(&range(1, 1)), None);
        assert_eq!(range(1, 1).negotiate(&range(2, 2)), None);

        // `version_unsupported` names the host's range in `data.supported`.
        let err = RpcError::app(ErrorCode::VersionUnsupported, "no common version")
            .with_data(|data| data.supported = Some(PROTOCOL));
        let raw = json!({"code": -32000, "message": "no common version",
            "data": {"code": "version_unsupported", "supported": {"min": 1, "max": 1}}});
        assert_eq!(serde_json::to_value(&err).unwrap(), raw);
        let parsed: RpcError = roundtrip(&raw);
        let data = parsed.data.unwrap();
        assert_eq!(data.supported, Some(PROTOCOL));
        assert!(data.details.is_empty());
    }
}
